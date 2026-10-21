mod fakes;

use std::sync::Arc;

use analysis::Direcao;
use application::relatorio::export::{para_csv, para_json, para_markdown};
use application::{
    ComparacaoPares, ErroAplicacao, GerarRelatorioUseCase, MetadadosReproducao, ModoAnalise,
    RelatorioCanal, RelatorioConfig,
};
use chrono::{Days, NaiveDate, Utc};
use domain::CategoriaManual;
use domain::entities::canal::Canal;
use domain::entities::snapshot::{EntidadeTipo, Metrica, Snapshot};
use domain::entities::video::Video;
use domain::ids::{CanalId, VideoId};
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

fn video(
    canal_id: &str,
    id: &str,
    dias_atras: u64,
    visualizacoes: u64,
    curtidas: u64,
    total_comentarios: u64,
) -> Video {
    let data = Utc::now()
        .date_naive()
        .checked_sub_days(Days::new(dias_atras))
        .expect("data válida");
    Video {
        id: VideoId::new(id),
        canal_id: CanalId::new(canal_id),
        titulo: format!("Vídeo {id}"),
        descricao: None,
        url: format!("https://www.youtube.com/watch?v={id}"),
        tags: Vec::new(),
        topic_categories: Vec::new(),
        publicado_em: format!("{data}T12:00:00Z"),
        duracao_segundos: 600,
        visualizacoes,
        curtidas,
        total_comentarios,
    }
}

fn config_publicacao(metricas: Vec<Metrica>) -> RelatorioConfig {
    RelatorioConfig {
        metricas,
        modo: ModoAnalise::PorPublicacao,
        ..RelatorioConfig::default()
    }
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
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("não há outros canais na base de dados")),
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
async fn baseline_com_poucos_pares_nao_calcula_percentil() {
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
    assert!(
        comparacao.percentil_canal_entre_pares.is_none(),
        "com 2 pares o percentil não é fiável"
    );
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("mínimo 10")),
        "avisos: {:?}",
        relatorio.avisos
    );
}

#[tokio::test]
async fn baseline_normaliza_slopes_pelo_tamanho_do_canal() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("alvo", "Alvo"))
        .expect("canal guardado");

    // Canal com slope 6 e mediana 97 -> relativo 6/97.
    let mut snapshots = serie_linear("alvo", Metrica::Visualizacoes, 30, 6, 10);
    for indice in 0..11 {
        let id = format!("par-{indice}");
        repo.guardar_canal(&canal(&id, &id))
            .expect("canal guardado");
        // Par com slope k e mediana 10 + 14.5k -> relativo k/(10+14.5k).
        snapshots.extend(serie_linear(
            &id,
            Metrica::Visualizacoes,
            30,
            indice as i64 + 1,
            10,
        ));
    }
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
    assert_eq!(comparacao.n_pares, 11);
    assert!((comparacao.fator_normalizacao - 97.0).abs() < 1e-12);
    assert!((comparacao.slope_canal_relativo - 6.0 / 97.0).abs() < 1e-12);
    // O par com slope 6 tem exactamente o mesmo slope relativo (6/97); o
    // percentil de meio-rank coloca-o em 50.
    let percentil = comparacao
        .percentil_canal_entre_pares
        .expect("11 pares permitem percentil");
    assert!((percentil - 50.0).abs() < 1e-12, "percentil: {percentil}");
}

#[tokio::test]
async fn markdown_com_poucos_pares_nao_mostra_percentil() {
    let repo = Arc::new(FakeRepositorio::new());
    for (id, nome) in [("alvo", "Alvo"), ("par-1", "Par 1"), ("par-2", "Par 2")] {
        repo.guardar_canal(&canal(id, nome))
            .expect("canal guardado");
    }
    let mut snapshots = serie_linear("alvo", Metrica::Visualizacoes, 30, 2, 10);
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
    let markdown = para_markdown(&relatorio);

    assert!(!markdown.contains("no percentil"));
    assert!(!markdown.contains("percentil 100.0"));
    assert!(markdown.contains("não tem percentil fiável"));
    assert!(markdown.contains("mínimo 10"));
}

#[tokio::test]
async fn baseline_por_categoria_filtra_pares_e_e_deterministico() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("alvo", "Alvo"))
        .expect("canal guardado");
    repo.guardar_canal(&canal("mesma-categoria", "Mesma categoria"))
        .expect("canal guardado");
    for indice in 0..5 {
        let id = format!("outro-{indice}");
        repo.guardar_canal(&canal(&id, &format!("Outro {indice}")))
            .expect("canal guardado");
        repo.definir_categoria_manual(&CanalId::new(&id), CategoriaManual::Tecnologia)
            .expect("categoria");
    }
    repo.definir_categoria_manual(&CanalId::new("alvo"), CategoriaManual::Programacao)
        .expect("categoria");
    repo.definir_categoria_manual(
        &CanalId::new("mesma-categoria"),
        CategoriaManual::Programacao,
    )
    .expect("categoria");

    let mut snapshots = Vec::new();
    snapshots.extend(serie_linear("alvo", Metrica::Visualizacoes, 30, 1, 10));
    snapshots.extend(serie_linear(
        "mesma-categoria",
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
    let primeiro = executar(Arc::clone(&repo), config.clone(), "alvo")
        .await
        .expect("primeiro relatório");
    let segundo = executar(repo, config, "alvo")
        .await
        .expect("segundo relatório");

    let comparacao = primeiro.comparacoes.first().expect("comparação com pares");
    assert_eq!(comparacao.grupo, "categoria:programacao");
    assert_eq!(comparacao.n_pares, 1);
    assert_eq!(
        comparacao.percentil_canal_entre_pares,
        segundo.comparacoes[0].percentil_canal_entre_pares
    );
}

#[tokio::test]
async fn sem_categoria_usa_todos_e_avisa() {
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

    let comparacao = relatorio.comparacoes.first().expect("comparação com pares");
    assert_eq!(comparacao.grupo, "todos");
    assert_eq!(comparacao.n_pares, 2);
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("canal sem categoria manual"))
    );
}

#[tokio::test]
async fn categoria_sem_pares_faz_fallback_para_todos_e_avisa() {
    let repo = Arc::new(FakeRepositorio::new());
    for (id, nome) in [("alvo", "Alvo"), ("par-1", "Par 1"), ("par-2", "Par 2")] {
        repo.guardar_canal(&canal(id, nome))
            .expect("canal guardado");
    }
    repo.definir_categoria_manual(&CanalId::new("alvo"), CategoriaManual::Programacao)
        .expect("categoria");
    repo.definir_categoria_manual(&CanalId::new("par-1"), CategoriaManual::Tecnologia)
        .expect("categoria");
    repo.definir_categoria_manual(&CanalId::new("par-2"), CategoriaManual::Tecnologia)
        .expect("categoria");

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

    let comparacao = relatorio.comparacoes.first().expect("comparação com pares");
    assert_eq!(comparacao.grupo, "todos");
    assert_eq!(comparacao.n_pares, 2);
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("sem pares na categoria 'programacao'"))
    );
}

#[tokio::test]
async fn sem_outros_canais_avisa_de_forma_distinta() {
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

    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("não há outros canais na base de dados"))
    );
}

#[tokio::test]
async fn canal_sem_snapshots_gera_relatorio_com_avisos() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    repo.definir_categoria_manual(&CanalId::new("canal-1"), CategoriaManual::Programacao)
        .expect("categoria");

    let relatorio = executar(repo, RelatorioConfig::default(), "canal-1")
        .await
        .expect("relatório sem dados");

    assert!(relatorio.tendencias.is_empty());
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("amostra insuficiente"))
    );
}

#[tokio::test]
async fn lote_isola_erros_e_aplica_fdr_uma_vez() {
    let repo = Arc::new(FakeRepositorio::new());
    for id in ["a", "b"] {
        repo.guardar_canal(&canal(id, id)).expect("canal guardado");
    }
    // "c" não existe no repositório; é o erro isolado do lote.
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
        "canal_id,canal_nome,metrica,n_snapshots,direcao,tau,p_value,p_ajustado_fdr,slope,slope_ic_low,slope_ic_high,amostra_suficiente,periodo_inicio,periodo_fim,eixo_temporal,unidade_slope"
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

#[tokio::test]
async fn por_publicacao_gera_um_ponto_por_video() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    let idades = [400_u64, 350, 300, 250, 200, 150, 100, 60, 30, 10];
    for (indice, dias) in idades.into_iter().enumerate() {
        let views = 1_000 + (indice as u64) * 500;
        repo.guardar_video(&video("canal-1", &format!("v{indice}"), dias, views, 10, 2))
            .expect("vídeo guardado");
    }

    let relatorio = executar(
        repo,
        config_publicacao(vec![Metrica::Visualizacoes]),
        "canal-1",
    )
    .await
    .expect("relatório por publicação");

    assert_eq!(relatorio.modo, ModoAnalise::PorPublicacao);
    assert_eq!(relatorio.tendencias.len(), 1);
    let tendencia = &relatorio.tendencias[0];
    assert_eq!(tendencia.n_snapshots, 10);
    assert_eq!(tendencia.pontos.len(), 10);
    assert_eq!(tendencia.eixo_temporal, "data_publicacao");
    assert_eq!(tendencia.unidade_slope, "views/dia por vídeo");
    assert!(
        tendencia
            .pontos
            .windows(2)
            .all(|janela| janela[0].0 <= janela[1].0)
    );
    assert_eq!(relatorio.periodo_inicio, tendencia.pontos[0].0);
    assert_eq!(
        relatorio.periodo_fim,
        tendencia.pontos[tendencia.pontos.len() - 1].0
    );
}

#[tokio::test]
async fn por_publicacao_filtra_idade_minima() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    for indice in 0..5 {
        repo.guardar_video(&video(
            "canal-1",
            &format!("recente-{indice}"),
            3,
            1_000,
            10,
            1,
        ))
        .expect("vídeo guardado");
        repo.guardar_video(&video(
            "canal-1",
            &format!("antigo-{indice}"),
            30 + indice,
            1_000,
            10,
            1,
        ))
        .expect("vídeo guardado");
    }

    let relatorio = executar(
        repo,
        config_publicacao(vec![Metrica::Visualizacoes]),
        "canal-1",
    )
    .await
    .expect("relatório por publicação");

    assert_eq!(relatorio.tendencias.len(), 1);
    assert_eq!(relatorio.tendencias[0].n_snapshots, 5);
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("5 vídeos ignorados por terem menos de 7 dias")),
        "avisos: {:?}",
        relatorio.avisos
    );
}

#[tokio::test]
async fn por_publicacao_normaliza_views_por_dia() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    for indice in 0..5 {
        repo.guardar_video(&video("canal-1", &format!("v{indice}"), 100, 1_000, 10, 1))
            .expect("vídeo guardado");
    }

    let relatorio = executar(
        repo,
        config_publicacao(vec![Metrica::Visualizacoes]),
        "canal-1",
    )
    .await
    .expect("relatório por publicação");

    let pontos = &relatorio.tendencias[0].pontos;
    assert_eq!(pontos.len(), 5);
    assert!(
        pontos
            .iter()
            .all(|(_, valor)| (*valor - 10.0).abs() < 1e-12)
    );
}

#[tokio::test]
async fn por_publicacao_usa_racio_de_engagement() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    for indice in 0..5 {
        repo.guardar_video(&video(
            "canal-1",
            &format!("v{indice}"),
            30 + indice,
            1_000,
            100,
            5,
        ))
        .expect("vídeo guardado");
    }

    let config = config_publicacao(vec![Metrica::Curtidas, Metrica::TotalComentarios]);
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório por publicação");

    let curtidas = relatorio
        .tendencias
        .iter()
        .find(|tendencia| tendencia.metrica == Metrica::Curtidas)
        .expect("tendência de curtidas");
    assert!(
        curtidas
            .pontos
            .iter()
            .all(|(_, valor)| (*valor - 0.1).abs() < 1e-12)
    );
    assert_eq!(curtidas.unidade_slope, "likes/views por vídeo");

    let comentarios = relatorio
        .tendencias
        .iter()
        .find(|tendencia| tendencia.metrica == Metrica::TotalComentarios)
        .expect("tendência de comentários");
    assert!(
        comentarios
            .pontos
            .iter()
            .all(|(_, valor)| (*valor - 0.005).abs() < 1e-12)
    );
}

#[tokio::test]
async fn por_publicacao_rejeita_metrica_de_canal() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");

    let erro = executar(repo, config_publicacao(vec![Metrica::Inscritos]), "canal-1")
        .await
        .expect_err("métrica de canal não é analisável por publicação");

    match erro {
        ErroAplicacao::ConfigInvalida(mensagem) => {
            assert!(mensagem.contains("não é analisável por publicação"));
        }
        outro => panic!("erro inesperado: {outro}"),
    }
}

#[tokio::test]
async fn markdown_por_publicacao_mostra_unidade_por_video() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    for (indice, dias) in [300_u64, 240, 180, 120, 60].into_iter().enumerate() {
        repo.guardar_video(&video(
            "canal-1",
            &format!("v{indice}"),
            dias,
            1_000 + (indice as u64) * 200,
            50,
            3,
        ))
        .expect("vídeo guardado");
    }

    let relatorio = executar(
        repo,
        config_publicacao(vec![Metrica::Visualizacoes]),
        "canal-1",
    )
    .await
    .expect("relatório por publicação");
    let markdown = para_markdown(&relatorio);

    assert!(markdown.contains("Slope (IC95, por vídeo)"));
    assert!(markdown.contains("views/dia por vídeo"));
    assert!(markdown.contains("vídeos publicados"));
}

#[tokio::test]
async fn por_publicacao_respeita_periodo_dias() {
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal("canal-1", "Canal"))
        .expect("canal guardado");
    for (indice, dias) in [400_u64, 350, 300, 250, 200, 150, 100]
        .into_iter()
        .enumerate()
    {
        repo.guardar_video(&video(
            "canal-1",
            &format!("v{indice}"),
            dias,
            1_000 + (indice as u64) * 100,
            10,
            1,
        ))
        .expect("vídeo guardado");
    }

    let config = RelatorioConfig {
        periodo_dias: Some(365),
        ..config_publicacao(vec![Metrica::Visualizacoes])
    };
    let relatorio = executar(repo, config, "canal-1")
        .await
        .expect("relatório por publicação");

    assert_eq!(relatorio.tendencias[0].n_snapshots, 6);
    assert!(
        relatorio
            .avisos
            .iter()
            .any(|aviso| aviso.contains("fora dos últimos 365 dias"))
    );
}
