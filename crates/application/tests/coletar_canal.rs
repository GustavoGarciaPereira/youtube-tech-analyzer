mod fakes;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use application::{ColetaConfig, ColetarCanalUseCase, ErroAplicacao, EventoProgresso};
use chrono::{Days, Utc};
use domain::CanalRef;
use domain::entities::snapshot::{EntidadeTipo, Metrica, Snapshot};
use domain::entities::{canal::Canal, comentario::Comentario, video::Video};
use domain::ids::{CanalId, ComentarioId, VideoId};
use domain::ports::{ColetorDeCanal, RepositorioDeCanais};
use fakes::{FakeColetor, FakeRepositorio};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn canal() -> Canal {
    Canal {
        id: CanalId::new("canal-1"),
        nome: "Canal de Tecnologia".to_owned(),
        handle: Some("@canal".to_owned()),
        url: "https://www.youtube.com/@canal".to_owned(),
        descricao: Some("Descrição".to_owned()),
        inscritos: 10_000,
        total_videos: 5,
        topic_categories: vec!["Technology".to_owned()],
        criado_em: Some("2020-01-01T00:00:00Z".to_owned()),
    }
}

fn video(id: &str, publicado_em: &str) -> Video {
    Video {
        id: VideoId::new(id),
        canal_id: CanalId::new("canal-1"),
        titulo: format!("Vídeo {id}"),
        descricao: Some("Descrição".to_owned()),
        url: format!("https://www.youtube.com/watch?v={id}"),
        tags: vec!["rust".to_owned()],
        topic_categories: vec!["Technology".to_owned()],
        publicado_em: publicado_em.to_owned(),
        duracao_segundos: 600,
        visualizacoes: 1_000,
        curtidas: 100,
        total_comentarios: 2,
    }
}

fn comentario(id: &str, video_id: &str) -> Comentario {
    Comentario {
        id: ComentarioId::new(id),
        video_id: VideoId::new(video_id),
        autor: "Autor".to_owned(),
        texto: "Comentário".to_owned(),
        publicado_em: "2024-01-01T00:00:00Z".to_owned(),
        curtidas: 1,
        eh_resposta: false,
        nivel_resposta: 0,
        parent_id: None,
    }
}

fn mapa_comentarios(videos: &[Video], por_video: usize) -> HashMap<VideoId, Vec<Comentario>> {
    videos
        .iter()
        .map(|video| {
            let comentarios = (0..por_video)
                .map(|indice| comentario(&format!("{}-c{indice}", video.id), video.id.as_str()))
                .collect();
            (video.id.clone(), comentarios)
        })
        .collect()
}

fn criar_use_case(
    coletor: Arc<dyn ColetorDeCanal>,
    repositorio: Arc<dyn RepositorioDeCanais>,
    config: ColetaConfig,
) -> ColetarCanalUseCase {
    ColetarCanalUseCase::new(coletor, repositorio, config)
}

fn referencia() -> CanalRef {
    CanalRef::Handle("canal".to_owned())
}

#[tokio::test]
async fn happy_path_processa_tres_videos() {
    let videos = vec![
        video("v1", "2024-01-03T00:00:00Z"),
        video("v2", "2024-01-02T00:00:00Z"),
        video("v3", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(FakeColetor::novo(
        canal(),
        videos.clone(),
        mapa_comentarios(&videos, 2),
    ));
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake;
    let repositorio: Arc<dyn RepositorioDeCanais> = repo.clone();
    let use_case = criar_use_case(coletor, repositorio, ColetaConfig::default());

    let relatorio = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("coleta concluída");

    assert_eq!(relatorio.videos_novos, 3);
    assert_eq!(relatorio.videos_ja_coletados, 0);
    assert_eq!(relatorio.videos_falhados, 0);
    assert_eq!(relatorio.comentarios_total, 6);
    assert!(relatorio.falhas.is_empty());
    assert_eq!(repo.contar_videos(), 3);
    assert_eq!(repo.contar_comentarios(), 6);
}

#[tokio::test]
async fn falha_parcial_continua_e_reporta() {
    let videos = vec![
        video("v1", "2024-01-03T00:00:00Z"),
        video("v2", "2024-01-02T00:00:00Z"),
        video("v3", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(
        FakeColetor::novo(canal(), videos.clone(), mapa_comentarios(&videos, 2))
            .com_falhas([VideoId::new("v2")]),
    );
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake;
    let repositorio: Arc<dyn RepositorioDeCanais> = repo.clone();
    let use_case = criar_use_case(coletor, repositorio, ColetaConfig::default());

    let relatorio = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("falha parcial não propaga erro");

    assert_eq!(relatorio.videos_novos, 2);
    assert_eq!(relatorio.videos_falhados, 1);
    assert_eq!(relatorio.comentarios_total, 4);
    assert_eq!(relatorio.falhas.len(), 1);
    assert_eq!(relatorio.falhas[0].video_id.as_str(), "v2");
}

#[tokio::test]
async fn cancelamento_gracioso_preserva_o_que_ja_foi_salvo() {
    let videos = vec![
        video("v1", "2024-01-03T00:00:00Z"),
        video("v2", "2024-01-02T00:00:00Z"),
        video("v3", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(
        FakeColetor::novo(canal(), videos.clone(), mapa_comentarios(&videos, 2))
            .com_atraso(Duration::from_millis(10)),
    );
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake;
    let repositorio: Arc<dyn RepositorioDeCanais> = repo.clone();
    let config = ColetaConfig {
        concorrencia: 1,
        ..ColetaConfig::default()
    };
    let use_case = criar_use_case(coletor, repositorio, config);

    let (tx, mut rx) = mpsc::channel(64);
    let cancelamento = CancellationToken::new();
    let token_consumidor = cancelamento.clone();
    let consumidor = tokio::spawn(async move {
        while let Some(evento) = rx.recv().await {
            if matches!(evento, EventoProgresso::VideoConcluido { .. }) {
                token_consumidor.cancel();
            }
        }
    });

    let resultado = use_case
        .executar(referencia(), Some(tx), cancelamento)
        .await;

    assert!(matches!(resultado, Err(ErroAplicacao::Cancelada)));
    consumidor.abort();
    assert!(repo.contar_comentarios() > 0);
}

#[tokio::test]
async fn idempotencia_diaria_nao_recolhe_comentarios_duas_vezes() {
    let videos = vec![
        video("v1", "2024-01-03T00:00:00Z"),
        video("v2", "2024-01-02T00:00:00Z"),
        video("v3", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(FakeColetor::novo(
        canal(),
        videos.clone(),
        mapa_comentarios(&videos, 2),
    ));
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake.clone();
    let repositorio: Arc<dyn RepositorioDeCanais> = repo.clone();
    let use_case = criar_use_case(coletor, repositorio, ColetaConfig::default());

    let primeiro = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("primeira coleta");
    assert_eq!(primeiro.videos_novos, 3);

    let chamadas_antes = fake.chamadas_comentarios();
    let segundo = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("segunda coleta");

    assert_eq!(segundo.videos_ja_coletados, 3);
    assert_eq!(segundo.videos_novos, 0);
    assert_eq!(segundo.comentarios_total, 0);
    assert_eq!(fake.chamadas_comentarios(), chamadas_antes);
    assert_eq!(repo.contar_comentarios(), 6);
}

#[tokio::test]
async fn video_sem_comentarios_tambem_e_marcado_como_coletado() {
    let videos = vec![video("v1", "2024-01-01T00:00:00Z")];
    let fake = Arc::new(FakeColetor::novo(canal(), videos, HashMap::new()));
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake.clone();
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let use_case = criar_use_case(coletor, repositorio, ColetaConfig::default());

    let primeiro = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("primeira coleta");
    assert_eq!(primeiro.videos_novos, 1);
    assert_eq!(primeiro.comentarios_total, 0);
    assert_eq!(fake.chamadas_comentarios(), 1);

    let segundo = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("segunda coleta");
    assert_eq!(segundo.videos_ja_coletados, 1);
    assert_eq!(segundo.videos_novos, 0);
    assert_eq!(
        fake.chamadas_comentarios(),
        1,
        "o marcador deve existir mesmo com zero comentários"
    );
}

#[tokio::test]
async fn metricas_continuam_a_ser_gravadas_no_dia_seguinte() {
    let videos = vec![
        video("v1", "2024-01-02T00:00:00Z"),
        video("v2", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(FakeColetor::novo(
        canal(),
        videos.clone(),
        mapa_comentarios(&videos, 2),
    ));
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal()).expect("canal guardado");

    let hoje = Utc::now().date_naive();
    let ontem = (hoje - Days::new(1)).to_string();
    let hoje = hoje.to_string();
    repo.upsert_snapshots_batch(&[
        Snapshot::novo(
            EntidadeTipo::Video,
            "v1",
            Metrica::ComentariosColetados,
            2,
            ontem.clone(),
        ),
        Snapshot::novo(
            EntidadeTipo::Video,
            "v1",
            Metrica::Visualizacoes,
            100,
            ontem.clone(),
        ),
    ])
    .expect("marcadores antigos");

    let coletor: Arc<dyn ColetorDeCanal> = fake.clone();
    let repositorio: Arc<dyn RepositorioDeCanais> = repo.clone();
    let use_case = criar_use_case(coletor, repositorio, ColetaConfig::default());

    let relatorio = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("coleta do dia seguinte");

    assert_eq!(relatorio.videos_ja_coletados, 1);
    assert_eq!(relatorio.videos_novos, 1);
    assert_eq!(fake.chamadas_comentarios(), 1);

    let snapshots = repo.snapshots();
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.entidade_tipo == EntidadeTipo::Video
            && snapshot.entidade_id == "v1"
            && snapshot.metrica == Metrica::Visualizacoes
            && snapshot.data_coleta == hoje
    }));
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.entidade_tipo == EntidadeTipo::Video
            && snapshot.entidade_id == "v1"
            && snapshot.metrica == Metrica::Visualizacoes
            && snapshot.data_coleta == ontem
    }));
}

#[tokio::test]
async fn recoletar_comentarios_forca_nova_busca() {
    let videos = vec![video("v1", "2024-01-01T00:00:00Z")];
    let fake = Arc::new(FakeColetor::novo(
        canal(),
        videos,
        mapa_comentarios(&[video("v1", "2024-01-01T00:00:00Z")], 2),
    ));
    let repo = Arc::new(FakeRepositorio::new());
    repo.guardar_canal(&canal()).expect("canal guardado");

    let hoje = Utc::now().date_naive().to_string();
    repo.upsert_snapshots_batch(&[Snapshot::novo(
        EntidadeTipo::Video,
        "v1",
        Metrica::ComentariosColetados,
        2,
        hoje,
    )])
    .expect("marcador");

    let coletor: Arc<dyn ColetorDeCanal> = fake.clone();
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let config = ColetaConfig {
        recoletar_comentarios: true,
        ..ColetaConfig::default()
    };
    let use_case = criar_use_case(coletor, repositorio, config);

    let relatorio = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("refresh manual");

    assert_eq!(relatorio.videos_ja_coletados, 0);
    assert_eq!(relatorio.videos_novos, 1);
    assert_eq!(fake.chamadas_comentarios(), 1);
}

#[tokio::test]
async fn max_videos_limita_aos_mais_recentes() {
    let videos = vec![
        video("v1", "2024-01-05T00:00:00Z"),
        video("v2", "2024-01-04T00:00:00Z"),
        video("v3", "2024-01-03T00:00:00Z"),
        video("v4", "2024-01-02T00:00:00Z"),
        video("v5", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(FakeColetor::novo(
        canal(),
        videos.clone(),
        mapa_comentarios(&videos, 1),
    ));
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake.clone();
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let config = ColetaConfig {
        max_videos: Some(2),
        ..ColetaConfig::default()
    };
    let use_case = criar_use_case(coletor, repositorio, config);

    let relatorio = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("coleta limitada");

    assert_eq!(relatorio.videos_novos, 2);
    assert_eq!(fake.chamadas_comentarios(), 2);
    assert_eq!(relatorio.comentarios_total, 2);
}

#[tokio::test]
async fn emite_eventos_na_ordem_esperada() {
    let videos = vec![
        video("v1", "2024-01-03T00:00:00Z"),
        video("v2", "2024-01-02T00:00:00Z"),
        video("v3", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(FakeColetor::novo(
        canal(),
        videos.clone(),
        mapa_comentarios(&videos, 1),
    ));
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake;
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let config = ColetaConfig {
        concorrencia: 1,
        ..ColetaConfig::default()
    };
    let use_case = criar_use_case(coletor, repositorio, config);

    let (tx, mut rx) = mpsc::channel(64);
    let relatorio = use_case
        .executar(referencia(), Some(tx), CancellationToken::new())
        .await
        .expect("coleta concluída");
    assert_eq!(relatorio.videos_novos, 3);

    let mut eventos = Vec::new();
    while let Some(evento) = rx.recv().await {
        eventos.push(evento);
    }

    assert!(matches!(
        &eventos[0],
        EventoProgresso::CanalResolvido { .. }
    ));
    assert!(matches!(
        &eventos[1],
        EventoProgresso::VideosListados { total } if *total == 3
    ));
    let iniciados = eventos
        .iter()
        .filter(|evento| matches!(evento, EventoProgresso::VideoIniciado { .. }))
        .count();
    let concluidos = eventos
        .iter()
        .filter(|evento| matches!(evento, EventoProgresso::VideoConcluido { .. }))
        .count();
    assert_eq!(iniciados, 3);
    assert_eq!(concluidos, 3);
    assert!(matches!(
        eventos.last(),
        Some(EventoProgresso::Finalizado { .. })
    ));
}

#[tokio::test]
async fn canal_sem_videos_retorna_erro() {
    let fake = Arc::new(FakeColetor::novo(canal(), Vec::new(), HashMap::new()));
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake;
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let use_case = criar_use_case(coletor, repositorio, ColetaConfig::default());

    let resultado = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await;

    assert!(matches!(resultado, Err(ErroAplicacao::CanalSemVideos)));
}

#[tokio::test]
async fn respeita_o_limite_de_concorrencia() {
    let videos = vec![
        video("v1", "2024-01-04T00:00:00Z"),
        video("v2", "2024-01-03T00:00:00Z"),
        video("v3", "2024-01-02T00:00:00Z"),
        video("v4", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(
        FakeColetor::novo(canal(), videos.clone(), mapa_comentarios(&videos, 1))
            .com_atraso(Duration::from_millis(20)),
    );
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake.clone();
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let config = ColetaConfig {
        concorrencia: 2,
        ..ColetaConfig::default()
    };
    let use_case = criar_use_case(coletor, repositorio, config);

    let relatorio = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await
        .expect("coleta concorrente");

    assert_eq!(relatorio.videos_novos, 4);
    assert!(
        fake.max_ativos() <= 2,
        "pico observado: {}",
        fake.max_ativos()
    );
    assert!(fake.max_ativos() >= 1);
}

#[tokio::test]
async fn concorrencia_zero_e_configuracao_invalida() {
    let fake = Arc::new(FakeColetor::novo(canal(), Vec::new(), HashMap::new()));
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake;
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let config = ColetaConfig {
        concorrencia: 0,
        ..ColetaConfig::default()
    };
    let use_case = criar_use_case(coletor, repositorio, config);

    let resultado = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await;

    assert!(matches!(resultado, Err(ErroAplicacao::ConfigInvalida(_))));
}

#[tokio::test]
async fn falha_fatal_propaga_quando_continuar_em_falha_e_falso() {
    let videos = vec![
        video("v1", "2024-01-02T00:00:00Z"),
        video("v2", "2024-01-01T00:00:00Z"),
    ];
    let fake = Arc::new(
        FakeColetor::novo(canal(), videos.clone(), mapa_comentarios(&videos, 1))
            .com_falhas([VideoId::new("v2")]),
    );
    let repo = Arc::new(FakeRepositorio::new());
    let coletor: Arc<dyn ColetorDeCanal> = fake;
    let repositorio: Arc<dyn RepositorioDeCanais> = repo;
    let config = ColetaConfig {
        continuar_em_falha: false,
        ..ColetaConfig::default()
    };
    let use_case = criar_use_case(coletor, repositorio, config);

    let resultado = use_case
        .executar(referencia(), None, CancellationToken::new())
        .await;

    assert!(matches!(resultado, Err(ErroAplicacao::Dominio(_))));
}
