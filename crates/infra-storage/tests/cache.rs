use domain::entities::canal::Canal;
use domain::entities::nicho::{ClassificacaoNicho, Nicho};
use domain::entities::video::Video;
use domain::ids::{CanalId, VideoId};
use domain::ports::{CacheDeClassificacoes, RepositorioDeCanais};
use infra_storage::SqliteRepositorio;

type Resultado = Result<(), Box<dyn std::error::Error>>;

fn repositorio() -> Result<SqliteRepositorio, infra_storage::Error> {
    SqliteRepositorio::em_memoria()
}

fn canal(id: &str) -> Canal {
    Canal {
        id: CanalId::new(id),
        nome: "Canal".to_owned(),
        handle: None,
        url: format!("https://www.youtube.com/channel/{id}"),
        descricao: None,
        inscritos: 0,
        total_videos: 1,
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
        visualizacoes: 0,
        curtidas: 0,
        total_comentarios: 0,
    }
}

fn classificacao(video_id: &str) -> ClassificacaoNicho {
    ClassificacaoNicho {
        video_id: VideoId::new(video_id),
        nicho: Nicho::Programacao,
        confianca: 0.9,
        metodo: "fake-model".to_owned(),
        data_classificacao: "2024-01-01T00:00:00Z".to_owned(),
    }
}

#[tokio::test]
async fn cache_grava_obtem_e_isola_hash_versao() -> Resultado {
    let repo = repositorio()?;
    repo.guardar_canal(&canal("canal-1"))?;
    repo.guardar_video(&video("video-1", "canal-1"))?;

    let video_id = VideoId::new("video-1");
    let classificacao = classificacao("video-1");

    repo.gravar(&video_id, "hash-a", "v1", &classificacao)
        .await?;

    assert_eq!(
        repo.obter(&video_id, "hash-a", "v1").await?,
        Some(classificacao)
    );
    assert_eq!(repo.obter(&video_id, "hash-b", "v1").await?, None);
    assert_eq!(repo.obter(&video_id, "hash-a", "v2").await?, None);

    assert_eq!(repo.contar_por_versao("v1").await?, 1);
    assert_eq!(repo.contar_por_versao("v2").await?, 0);
    Ok(())
}
