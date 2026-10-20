use std::error::Error;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use domain::CategoriaManual;
use domain::entities::canal::Canal;
use domain::entities::snapshot::{EntidadeTipo, Metrica, Snapshot};
use domain::ids::CanalId;
use domain::ports::RepositorioDeCanais;
use infra_storage::SqliteRepositorio;
use tempfile::TempDir;

fn yta(db: &Path) -> Command {
    let mut comando = Command::cargo_bin("yta").expect("binário yta");
    comando.env("YTA_DB_PATH", db);
    comando.env_remove("YOUTUBE_API_KEY");
    // Evita que o `.env` do repositório seja carregado nos testes.
    if let Some(diretorio) = db.parent() {
        comando.current_dir(diretorio);
    }
    comando
}

fn canal(id: &str, nome: &str) -> Canal {
    Canal {
        id: CanalId::new(id),
        nome: nome.to_owned(),
        handle: Some(format!("@{id}")),
        url: format!("https://www.youtube.com/@{id}"),
        descricao: Some("Descrição do canal".to_owned()),
        inscritos: 10_000,
        total_videos: 30,
        topic_categories: Vec::new(),
        criado_em: None,
    }
}

fn snapshots(canal_id: &str) -> Vec<Snapshot> {
    (0..30)
        .map(|indice| {
            Snapshot::novo(
                EntidadeTipo::Canal,
                canal_id,
                Metrica::Visualizacoes,
                10 + indice as i64,
                format!("2024-01-{:02}", indice + 1),
            )
        })
        .collect()
}

fn seed(
    caminho: &Path,
    canal: &Canal,
    snapshots: &[Snapshot],
    categoria: Option<CategoriaManual>,
) -> Result<(), Box<dyn Error>> {
    let repositorio = SqliteRepositorio::new(
        caminho
            .to_str()
            .ok_or_else(|| std::io::Error::other("caminho não UTF-8"))?,
    )?;
    repositorio.guardar_canal(canal)?;
    if !snapshots.is_empty() {
        repositorio.upsert_snapshots_batch(snapshots)?;
    }
    if let Some(categoria) = categoria {
        repositorio.definir_categoria_manual(&canal.id, categoria)?;
    }
    Ok(())
}

fn stdout(assert: &assert_cmd::assert::Assert) -> Result<String, Box<dyn Error>> {
    Ok(String::from_utf8(assert.get_output().stdout.clone())?)
}

fn stderr(assert: &assert_cmd::assert::Assert) -> Result<String, Box<dyn Error>> {
    Ok(String::from_utf8(assert.get_output().stderr.clone())?)
}

#[test]
fn categoriza_e_lista_canal() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");
    let canal = canal("canal-1", "Canal de Tecnologia");
    seed(&db, &canal, &[], None)?;

    yta(&db)
        .args([
            "canal",
            "categorizar",
            "canal-1",
            "--categoria",
            "programacao",
        ])
        .assert()
        .success();

    let assert = yta(&db).args(["canal", "listar"]).assert().success();
    let saida = stdout(&assert)?;
    assert!(saida.contains("Canal de Tecnologia"));
    assert!(saida.contains("programacao"));
    Ok(())
}

#[test]
fn relatorio_sem_snapshots_mostra_avisos() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");
    let canal = canal("canal-1", "Canal");
    seed(&db, &canal, &[], Some(CategoriaManual::Programacao))?;

    let assert = yta(&db)
        .args(["relatorio", "canal-1", "--formato", "md"])
        .assert()
        .success();
    let saida = stdout(&assert)?;

    assert!(saida.contains("## Avisos"));
    assert!(saida.contains("amostra insuficiente"));
    Ok(())
}

#[test]
fn relatorio_com_snapshots_contem_tendencia_e_hash() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");
    let canal = canal("canal-1", "Canal");
    seed(
        &db,
        &canal,
        &snapshots("canal-1"),
        Some(CategoriaManual::Programacao),
    )?;

    let assert = yta(&db)
        .args(["relatorio", "canal-1", "--formato", "md"])
        .assert()
        .success();
    let saida = stdout(&assert)?;

    assert!(saida.contains("crescente"));
    let hash = StdCommand::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()?;
    let hash = String::from_utf8(hash.stdout)?;
    assert!(saida.contains(hash.trim()));
    Ok(())
}

#[test]
fn categoria_invalida_sai_com_codigo_do_clap() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");

    let assert = yta(&db)
        .args(["canal", "categorizar", "canal-1", "--categoria", "xpto"])
        .assert()
        .code(2);

    assert!(stderr(&assert)?.contains("categoria inválida"));
    Ok(())
}

#[test]
fn db_caminho_respeita_variavel_de_ambiente() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");

    let assert = yta(&db).args(["db", "caminho"]).assert().success();
    let saida = stdout(&assert)?;

    assert_eq!(saida.trim(), db.display().to_string());
    Ok(())
}

#[test]
fn canal_adicionar_sem_api_key_falha_com_mensagem_clara() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");

    let assert = yta(&db).args(["canal", "adicionar", "@x"]).assert().code(1);
    let erro = stderr(&assert)?;

    assert!(erro.contains("YOUTUBE_API_KEY"));
    Ok(())
}

#[test]
fn ajuda_de_coleta_mostra_recoletar_comentarios() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");

    let assert = yta(&db)
        .args(["canal", "coletar", "--help"])
        .assert()
        .success();

    assert!(stdout(&assert)?.contains("--recoletar-comentarios"));
    Ok(())
}

#[test]
fn relatorio_json_faz_round_trip() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let db = dir.path().join("yta.db");
    let canal = canal("canal-1", "Canal");
    seed(
        &db,
        &canal,
        &snapshots("canal-1"),
        Some(CategoriaManual::Programacao),
    )?;

    let assert = yta(&db)
        .args(["relatorio", "canal-1", "--formato", "json"])
        .assert()
        .success();
    let saida = stdout(&assert)?;
    let relatorio: application::RelatorioCanal = serde_json::from_str(&saida)?;

    assert_eq!(relatorio.canal.id.as_str(), "canal-1");
    assert_eq!(relatorio.periodo_fim.to_string(), "2024-01-30");
    Ok(())
}
