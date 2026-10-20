mod fakes;

use std::sync::Arc;

use analysis::Direcao;
use application::relatorio::export::{para_csv, para_json, para_markdown};
use application::{
    ComparacaoPares, ErroAplicacao, GerarRelatorioUseCase, MetadadosReproducao, RelatorioCanal,
    RelatorioConfig,
};
use chrono::{Days, NaiveDate, Utc};
use domain::Nicho;
use domain::entities::canal::Canal;
use domain::entities::snapshot::{EntidadeTipo, Metrica, Snapshot};
use domain::ids::CanalId;
use domain::ports::RepositorioDeCanais;
use fakes::FakeRepositorio;

fn canal(id: &str, nome: &str) -> Canal {
    Canal {
        id: CanalId::new(id),
        nome: nome.to_owned(),
        handle: Some(format!("@{id}")),
        url: format!("https://www.youtube.com/@{id}"),
        descricao: None,
        inscritos: 1_000,
        total_videos: 30,
        topic_categories: Vec::new(),
        criado_em: None,
    }
}

fn metadados() -> MetadadosReproducao {
    MetadadosReproducao {
        hash_commit: "abc12345".to_owned(),
        versao_schema: "schema-teste".to_owned(),
        comando: Some("cargo test --relatorio".to_owned()),
        gerado_em: Utc::now(),
        versao_app: "0.1.0".to_owned(),
    }
}

fn snapshot(canal_id: &str, metrica: Metrica, data: NaiveDate, valor: i64) -> Snapshot {
    Snapshot::novo(
        EntidadeTipo::Canal,
        canal_id,
        metrica,
        valor,
        data.to_string(),
    )
}

fn serie_linear(
    canal_id: &str,
    metrica: Metrica,
    n: usize,
    slope: i64,
    intercept: i64,
) -> Vec<Snapshot> {
    let inicio = NaiveDate::from_ymd_opt(2024, 1, 1).expect("data válida");
    (0..n)
        .map(|indice| {
            let data = inicio
                .checked_add_days(Days::new(indice as u64))
                .expect("data válida");
            snapshot(canal_id, metrica, data, intercept + slope * indice as i64)
        })
        .collect()
}

fn serie_terminando_ontem(canal_id: &str, metrica: Metrica, n: usize) -> Vec<Snapshot> {
    let hoje = Utc::now().date_naive();
    let fim = hoje.checked_sub_days(Days::new(1)).expect("data válida");
    let inicio = fim
        .checked_sub_days(Days::new((n - 1) as u64))
        .expect("data válida");
    (0..n)
        .map(|indice| {
            let data = inicio
                .checked_add_days(Days::new(indice as u64))
                .expect("data válida");
            snapshot(canal_id, metrica, data, indice as i64 + 1)
        })
        .collect()
}

async fn executar(
    repositorio: Arc<FakeRepositorio>,
    config: RelatorioConfig,
    canal_id: &str,
) -> Result<RelatorioCanal, ErroAplicacao> {
    let repositorio: Arc<dyn RepositorioDeCanais> = repositorio;
    let use_case = GerarRelatorioUseCase::new(repositorio, config);
    use_case
        .executar(&CanalId::new(canal_id), metadados())
        .await
}

#[tokio::test]
async fn happy_path_gera_tres_tendencias_crescentes() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");

    let metricas = [
        Metrica::Visualizacoes,
        Metrica::Curtidas,
        Metrica::TotalComentarios,
    ];
    let mut snapshots = Vec::new();
    for (indice, metrica) in metricas.iter().enumerate() {
        snapshots.extend(serie_linear("canal-1", *metrica, 30, indice as i64 + 1, 10));
    }
    repo.upsert_snapshots_batch(&snapshots)
        .expect("snapshots guardados");

    let relatorio = executar(repo, RelatorioConfig::default(), "canal-1")
        .await
        .expect("relatório gerado");

    assert_eq!(relatorio.tendencias.len(), 3);
    assert!(
        relatorio
            .tendencias
            .iter()
            .all(|tendencia| tendencia.direcao == Direcao::Crescente)
    );
    assert!(
        relatorio
            .tendencias
            .iter()
            .all(|tendencia| tendencia.amostra_suficiente)
    );
    assert!(
        relatorio.avisos.is_empty(),
        "avisos: {:?}",
        relatorio.avisos
    );
}

#[tokio::test]
async fn serie_curta_gera_aviso_de_amostra() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    repo.upsert_snapshots_batch(&serie_linear("canal-1", Metrica::Visualizacoes, 8, 1, 10))
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório gerado");

    assert_eq!(relatorio.tendencias.len(), 1);
    assert!(!relatorio.tendencias[0].amostra_suficiente);
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("amostra insuficiente"))
    );
}

#[tokio::test]
async fn metrica_sem_snapshots_nao_entra_e_gera_aviso() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    repo.upsert_snapshots_batch(&serie_linear("canal-1", Metrica::Visualizacoes, 10, 1, 10))
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes, Metrica::Curtidas],
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório gerado");

    assert_eq!(relatorio.tendencias.len(), 1);
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("apenas 0 snapshots"))
    );
}

#[tokio::test]
async fn baseline_de_pares_calcula_percentil() {
    let repo = Arc::new(FakeRepositorio::new());
    for (id, nome) in [("alvo", "Alvo"), ("par-1", "Par 1"), ("par-2", "Par 2")] {
        repo.guardar_canal(&canal(id, nome))
            .expect("canal guardado");
    }

    let mut snapshots = Vec::new();
    snapshots.extend(serie_linear("alvo", Metrica::Visualizacoes, 30, 2, 10));
    snapshots.extend(serie_linear("par-1", Metrica::Visualizacoes, 30, 1, 10));
    snapshots.extend(serie_linear("par-2", Metrica::Visualizacoes, 30, 3, 10));
    repo.upsert_snapshots_batch(&snapshots)
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "alvo")
        .await
        .expect("relatório gerado");

    let comparacao: &ComparacaoPares = relatorio.comparacoes.first().expect("comparação com pares");
    assert_eq!(comparacao.grupo, "todos");
    assert_eq!(comparacao.n_pares, 2);
    assert!((comparacao.slope_mediano_pares - 2.0).abs() < 1e-12);
    assert!((comparacao.percentil_canal_entre_pares - 50.0).abs() < 1e-12);
}

#[tokio::test]
async fn baseline_por_nicho_filtra_pares() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("alvo", "Alvo"))
        .expect("canal guardado");
    repo.guardar_canal(&canal("mesmo-nicho", "Mesmo nicho"))
        .expect("canal guardado");
    for indice in 0..5 {
        repo.guardar_canal(&canal(
            &format!("outro-{indice}"),
            &format!("Outro {indice}"),
        ))
        .expect("canal guardado");
        repo.definir_nicho(CanalId::new(format!("outro-{indice}")), Nicho::Tecnologia);
    }
    repo.definir_nicho(CanalId::new("alvo"), Nicho::Programacao);
    repo.definir_nicho(CanalId::new("mesmo-nicho"), Nicho::Programacao);

    let mut snapshots = Vec::new();
    snapshots.extend(serie_linear("alvo", Metrica::Visualizacoes, 30, 1, 10));
    snapshots.extend(serie_linear(
        "mesmo-nicho",
        Metrica::Visualizacoes,
        30,
        2,
        10,
    ));
    repo.upsert_snapshots_batch(&snapshots)
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "alvo")
        .await
        .expect("relatório gerado");

    let comparacao = relatorio.comparacoes.first().expect("comparação com pares");
    assert_eq!(comparacao.grupo, "programacao");
    assert_eq!(comparacao.n_pares, 1);
}

#[tokio::test]
async fn lote_isola_erros_e_aplica_fdr_uma_vez() {
    let repo = Arc::new(FakeRepositorio::new());
    for id in ["a", "b", "c"] {
        repo.guardar_canal(&canal(id, id)).expect("canal guardado");
    }
    let mut snapshots = Vec::new();
    snapshots.extend(serie_linear("a", Metrica::Visualizacoes, 30, 1, 10));
    snapshots.extend(serie_linear("b", Metrica::Visualizacoes, 6, 1, 10));
    repo.upsert_snapshots_batch(&snapshots)
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        ..RelatorioConfig::default()
    };
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let use_case = GerarRelatorioUseCase::new(repositorio, config);
    let ids = vec![CanalId::new("a"), CanalId::new("b"), CanalId::new("c")];

    let resultados = use_case
        .executar_lote(&ids, metadados())
        .await
        .expect("lote executado");

    assert_eq!(resultados.len(), 3);
    assert_eq!(
        resultados
            .iter()
            .filter(|resultado| resultado.is_ok())
            .count(),
        2
    );
    assert_eq!(
        resultados
            .iter()
            .filter(|resultado| resultado.is_err())
            .count(),
        1
    );
    assert!(matches!(resultados[2], Err(ErroAplicacao::SemDados(_))));

    let relatorio_a = resultados[0].as_ref().expect("canal a com sucesso");
    let relatorio_b = resultados[1].as_ref().expect("canal b com sucesso");

    // Com BH sobre os 2 canais com sucesso (m=2), o menor p-value é ajustado
    // para 2p. Se a correção fosse feita por canal (m=1), ficaria igual a p.
    assert!(relatorio_a.tendencias[0].p_ajustado_fdr > relatorio_a.tendencias[0].p_value);
    assert!(relatorio_b.tendencias[0].p_ajustado_fdr >= relatorio_b.tendencias[0].p_value);
}

#[tokio::test]
async fn markdown_contem_metadados_de_reproducao() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    repo.upsert_snapshots_batch(&serie_linear("canal-1", Metrica::Visualizacoes, 30, 1, 10))
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório gerado");
    let markdown = para_markdown(&relatorio);

    assert!(markdown.contains("abc12345"));
    assert!(markdown.contains("schema-teste"));
    assert!(markdown.contains("cargo test --relatorio"));
    assert!(markdown.contains("## Tendência por Métrica"));
    assert!(markdown.contains("## Limitações"));
}

#[tokio::test]
async fn csv_tem_cabecalho_exato_e_uma_linha_por_metrica() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal, com vírgula"))
        .expect("canal guardado");
    repo.upsert_snapshots_batch(&serie_linear("canal-1", Metrica::Visualizacoes, 30, 1, 10))
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório gerado");
    let csv = para_csv(&relatorio);
    let linhas: Vec<&str> = csv.lines().collect();

    assert_eq!(linhas[0], "# Valores decimais usam ponto (.).");
    assert_eq!(
        linhas[1],
        "canal_id,canal_nome,metrica,n_snapshots,direcao,tau,p_value,p_ajustado_fdr,slope,slope_ic_low,slope_ic_high,amostra_suficiente,periodo_inicio,periodo_fim"
    );
    assert_eq!(linhas.len(), 2 + relatorio.tendencias.len());
    assert!(linhas[2].contains("\"Canal, com vírgula\""));
    assert!(linhas[2].contains("0."));
}

#[tokio::test]
async fn json_faz_round_trip_do_relatorio() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    repo.upsert_snapshots_batch(&serie_linear("canal-1", Metrica::Visualizacoes, 30, 1, 10))
        .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório gerado");
    let json = para_json(&relatorio).expect("JSON válido");
    let lido: RelatorioCanal = serde_json::from_str(&json).expect("JSON legível");

    assert_eq!(lido, relatorio);
}

#[tokio::test]
async fn periodo_dias_respeita_o_corte_e_a_data_do_ultimo_snapshot() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    repo.upsert_snapshots_batch(&serie_terminando_ontem(
        "canal-1",
        Metrica::Visualizacoes,
        90,
    ))
    .expect("snapshots guardados");

    let config = RelatorioConfig {
        metricas: vec![Metrica::Visualizacoes],
        periodo_dias: Some(30),
        ..RelatorioConfig::default()
    };
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório gerado");

    let hoje = Utc::now().date_naive();
    let ontem = hoje.checked_sub_days(Days::new(1)).expect("data válida");
    let inicio_esperado = hoje.checked_sub_days(Days::new(30)).expect("data válida");

    assert_eq!(relatorio.tendencias[0].n_snapshots, 30);
    assert_eq!(relatorio.periodo_fim, ontem);
    assert_eq!(relatorio.periodo_inicio, inicio_esperado);
}
