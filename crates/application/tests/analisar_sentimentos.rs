mod fakes;

use std::sync::Arc;
use std::time::Duration;

use application::{AnalisarConfig, AnalisarSentimentosUseCase, ErroAplicacao};
use domain::entities::canal::Canal;
use domain::entities::comentario::Comentario;
use domain::entities::video::Video;
use domain::ids::{CanalId, ComentarioId, VideoId};
use domain::ports::RepositorioDeCanais;
use fakes::{FakeAnalisador, FakeRepositorio};

fn canal(id: &str) -> Canal {
    Canal {
        id: CanalId::new(id),
        nome: "Canal".to_owned(),
        handle: Some(format!("@{id}")),
        url: format!("https://www.youtube.com/@{id}"),
        descricao: None,
        inscritos: 1_000,
        total_videos: 10,
        topic_categories: Vec::new(),
        criado_em: None,
    }
}

fn video(id: &str, canal_id: &str) -> Video {
    Video {
        id: VideoId::new(id),
        canal_id: CanalId::new(canal_id),
        titulo: "Vídeo".to_owned(),
        descricao: None,
        url: format!("https://www.youtube.com/watch?v={id}"),
        tags: Vec::new(),
        topic_categories: Vec::new(),
        publicado_em: "2024-01-01T00:00:00Z".to_owned(),
        duracao_segundos: 60,
        visualizacoes: 1_000,
        curtidas: 10,
        total_comentarios: 10,
    }
}

fn comentario(id: &str, texto: &str) -> Comentario {
    Comentario {
        id: ComentarioId::new(id),
        video_id: VideoId::new("video-1"),
        autor: "Autor".to_owned(),
        texto: texto.to_owned(),
        publicado_em: "2024-01-02T00:00:00Z".to_owned(),
        curtidas: 0,
        eh_resposta: false,
        nivel_resposta: 0,
        parent_id: None,
    }
}

fn semear(repo: &FakeRepositorio, textos: &[&str]) {
    repo.guardar_canal(&canal("canal-1")).expect("canal");
    repo.guardar_video(&video("video-1", "canal-1"))
        .expect("vídeo");
    for (indice, texto) in textos.iter().enumerate() {
        repo.guardar_comentario(&comentario(&format!("c{indice:02}"), texto))
            .expect("comentário");
    }
}

fn config(concorrencia: usize) -> AnalisarConfig {
    AnalisarConfig { concorrencia }
}

#[tokio::test]
async fn analisa_todos_os_comentarios_pendentes() {
    let repo = Arc::new(FakeRepositorio::new());
    semear(&repo, &["bom vídeo"; 10]);
    let analisador = Arc::new(FakeAnalisador::novo());

    let caso = AnalisarSentimentosUseCase::new(analisador.clone(), repo.clone(), config(3));
    let relatorio = caso
        .executar(&CanalId::new("canal-1"))
        .await
        .expect("análise");

    assert_eq!(relatorio.total_comentarios, 10);
    assert_eq!(relatorio.analisados, 10);
    assert_eq!(relatorio.falhados, 0);
    assert_eq!(analisador.chamadas(), 10);
    assert_eq!(
        repo.comentarios_sem_sentimento(&CanalId::new("canal-1"), "fake-v1")
            .expect("pendentes")
            .len(),
        0
    );
}

#[tokio::test]
async fn falha_individual_nao_aborta_o_lote() {
    let repo = Arc::new(FakeRepositorio::new());
    let mut textos = vec!["bom vídeo"; 9];
    textos.push("texto problemático");
    semear(&repo, &textos);
    let analisador = Arc::new(FakeAnalisador::novo().com_falha_para("texto problemático"));

    let caso = AnalisarSentimentosUseCase::new(analisador, repo.clone(), config(2));
    let relatorio = caso
        .executar(&CanalId::new("canal-1"))
        .await
        .expect("análise");

    assert_eq!(relatorio.total_comentarios, 10);
    assert_eq!(relatorio.analisados, 9);
    assert_eq!(relatorio.falhados, 1);

    let pendentes = repo
        .comentarios_sem_sentimento(&CanalId::new("canal-1"), "fake-v1")
        .expect("pendentes");
    assert_eq!(pendentes.len(), 1);
    assert_eq!(pendentes[0].texto, "texto problemático");
}

#[tokio::test]
async fn segunda_execucao_nao_reanalisa_a_mesma_versao() {
    let repo = Arc::new(FakeRepositorio::new());
    semear(&repo, &["bom vídeo"; 10]);
    let analisador = Arc::new(FakeAnalisador::novo());

    let caso = AnalisarSentimentosUseCase::new(analisador.clone(), repo.clone(), config(3));
    caso.executar(&CanalId::new("canal-1"))
        .await
        .expect("primeira");
    let segunda = caso
        .executar(&CanalId::new("canal-1"))
        .await
        .expect("segunda");

    assert_eq!(segunda.total_comentarios, 0);
    assert_eq!(segunda.analisados, 0);
    assert_eq!(segunda.falhados, 0);
    assert_eq!(analisador.chamadas(), 10);
}

#[tokio::test]
async fn respeita_o_limite_de_concorrencia() {
    let repo = Arc::new(FakeRepositorio::new());
    semear(&repo, &["bom vídeo"; 12]);
    let analisador = Arc::new(FakeAnalisador::novo().com_atraso(Duration::from_millis(40)));

    let caso = AnalisarSentimentosUseCase::new(analisador.clone(), repo, config(3));
    let relatorio = caso
        .executar(&CanalId::new("canal-1"))
        .await
        .expect("análise");

    assert_eq!(relatorio.analisados, 12);
    assert!(
        analisador.max_ativos() <= 3,
        "pico {} excedeu o limite",
        analisador.max_ativos()
    );
    assert!(
        analisador.max_ativos() >= 2,
        "sem paralelismo real (pico {})",
        analisador.max_ativos()
    );
}

#[tokio::test]
async fn concorrencia_zero_e_rejeitada() {
    let repo = Arc::new(FakeRepositorio::new());
    semear(&repo, &["bom vídeo"]);
    let analisador = Arc::new(FakeAnalisador::novo());

    let caso = AnalisarSentimentosUseCase::new(analisador.clone(), repo, config(0));
    let erro = caso
        .executar(&CanalId::new("canal-1"))
        .await
        .expect_err("configuração inválida");

    assert!(matches!(erro, ErroAplicacao::ConfigInvalida(_)));
    assert_eq!(analisador.chamadas(), 0);
}

#[tokio::test]
async fn canal_inexistente_devolve_erro_de_dados() {
    let repo = Arc::new(FakeRepositorio::new());
    let analisador = Arc::new(FakeAnalisador::novo());

    let caso = AnalisarSentimentosUseCase::new(analisador, repo, config(2));
    let erro = caso
        .executar(&CanalId::new("nao-existe"))
        .await
        .expect_err("canal inexistente");

    assert!(matches!(erro, ErroAplicacao::SemDados(_)));
}

#[test]
fn concorrencia_por_omissao_deixa_um_nucleo_livre() {
    let config = AnalisarConfig::default();
    assert!(config.concorrencia >= 2);
}
