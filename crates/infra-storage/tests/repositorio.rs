use domain::ports::RepositorioDeCanais;
use domain::{
    Canal, CanalId, CategoriaManual, ClassificacaoNicho, Comentario, ComentarioId, EntidadeTipo,
    ErroDominio, Metrica, Nicho, Snapshot, Video, VideoId,
};
use infra_storage::SqliteRepositorio;

type Resultado = Result<(), Box<dyn std::error::Error>>;

fn repositorio() -> Result<SqliteRepositorio, infra_storage::Error> {
    SqliteRepositorio::em_memoria()
}

fn canal(id: &str) -> Canal {
    Canal {
        id: CanalId::new(id),
        nome: "Canal de Tecnologia".to_owned(),
        handle: Some(format!("@{id}")),
        url: format!("https://www.youtube.com/@{id}"),
        descricao: Some("Descrição do canal".to_owned()),
        inscritos: 10_000,
        total_videos: 3,
        topic_categories: vec!["Technology".to_owned()],
        criado_em: Some("2020-01-01T00:00:00Z".to_owned()),
    }
}

fn video(id: &str, canal_id: &str, publicado_em: &str) -> Video {
    Video {
        id: domain::ids::VideoId::new(id),
        canal_id: CanalId::new(canal_id),
        titulo: format!("Vídeo {id}"),
        descricao: Some("Descrição do vídeo".to_owned()),
        url: format!("https://www.youtube.com/watch?v={id}"),
        tags: vec!["rust".to_owned(), "programação".to_owned()],
        topic_categories: vec!["Technology".to_owned()],
        publicado_em: publicado_em.to_owned(),
        duracao_segundos: 600,
        visualizacoes: 1_000,
        curtidas: 100,
        total_comentarios: 10,
    }
}

fn comentario(id: &str, video_id: &str) -> Comentario {
    Comentario {
        id: domain::ids::ComentarioId::new(id),
        video_id: domain::ids::VideoId::new(video_id),
        autor: "Autor".to_owned(),
        texto: "Comentário".to_owned(),
        publicado_em: "2024-01-02T00:00:00Z".to_owned(),
        curtidas: 5,
        eh_resposta: false,
        nivel_resposta: 0,
        parent_id: None,
    }
}

fn classificacao(video_id: &str, nicho: Nicho) -> ClassificacaoNicho {
    ClassificacaoNicho {
        video_id: domain::ids::VideoId::new(video_id),
        nicho,
        confianca: 0.9,
        metodo: "teste".to_owned(),
        data_classificacao: "2024-01-03T00:00:00Z".to_owned(),
    }
}

#[test]
fn canal_roundtrip_preserva_campos_json() -> Resultado {
    let repo = repositorio()?;
    let canal = canal("canal-1");

    repo.guardar_canal(&canal)?;
    let lido = repo.obter_canal(&canal.id)?.expect("canal existente");

    assert_eq!(lido, canal);
    Ok(())
}

#[test]
fn inserir_mesmo_video_duas_vezes_nao_duplica_e_preserva_comentarios() -> Resultado {
    let repo = repositorio()?;
    let canal = canal("canal-1");
    repo.guardar_canal(&canal)?;

    let mut video = video("video-1", "canal-1", "2024-01-01T00:00:00Z");
    repo.guardar_video(&video)?;
    repo.guardar_comentario(&comentario("comentario-1", "video-1"))?;

    video.titulo = "Título atualizado".to_owned();
    repo.guardar_video(&video)?;

    let videos = repo.listar_videos(&canal.id)?;
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0].titulo, "Título atualizado");
    assert_eq!(repo.listar_comentarios(&video.id)?.len(), 1);
    Ok(())
}

#[test]
fn comentarios_preservam_relacao_de_resposta() -> Resultado {
    let repo = repositorio()?;
    let canal = canal("canal-1");
    repo.guardar_canal(&canal)?;

    let video = video("video-1", "canal-1", "2024-01-01T00:00:00Z");
    repo.guardar_video(&video)?;

    let principal = comentario("comentario-1", "video-1");
    repo.guardar_comentario(&principal)?;

    let mut resposta = comentario("comentario-2", "video-1");
    resposta.eh_resposta = true;
    resposta.nivel_resposta = 1;
    resposta.parent_id = Some(ComentarioId::new("comentario-1"));
    repo.guardar_comentario(&resposta)?;

    let lidos = repo.listar_comentarios(&video.id)?;
    assert_eq!(lidos.len(), 2);
    let resposta_lida = lidos
        .iter()
        .find(|comentario| comentario.id == resposta.id)
        .expect("resposta persistida");
    assert!(resposta_lida.eh_resposta);
    assert_eq!(resposta_lida.nivel_resposta, 1);
    assert_eq!(resposta_lida.parent_id, resposta.parent_id);
    Ok(())
}

#[test]
fn categorias_manuais_sao_persistidas_e_filtradas() -> Resultado {
    let repo = repositorio()?;
    repo.guardar_canal(&canal("canal-1"))?;
    repo.guardar_canal(&canal("canal-2"))?;
    repo.guardar_canal(&canal("canal-3"))?;

    assert_eq!(
        repo.categoria_manual_do_canal(&CanalId::new("canal-1"))?,
        None
    );

    repo.definir_categoria_manual(&CanalId::new("canal-1"), CategoriaManual::Programacao)?;
    repo.definir_categoria_manual(&CanalId::new("canal-2"), CategoriaManual::Programacao)?;
    repo.definir_categoria_manual(&CanalId::new("canal-3"), CategoriaManual::Tecnologia)?;

    assert_eq!(
        repo.categoria_manual_do_canal(&CanalId::new("canal-1"))?,
        Some(CategoriaManual::Programacao)
    );
    assert_eq!(
        repo.canais_por_categoria(CategoriaManual::Programacao)?
            .len(),
        2
    );

    repo.definir_categoria_manual(&CanalId::new("canal-1"), CategoriaManual::Outro)?;
    assert_eq!(
        repo.categoria_manual_do_canal(&CanalId::new("canal-1"))?,
        Some(CategoriaManual::Outro)
    );

    let erro = repo
        .definir_categoria_manual(&CanalId::new("canal-inexistente"), CategoriaManual::Outro)
        .expect_err("canal inexistente");
    assert!(matches!(erro, ErroDominio::NaoEncontrado { .. }));
    Ok(())
}

#[test]
fn lista_canais_e_calcula_nicho_predominante() -> Resultado {
    let repo = repositorio()?;
    repo.guardar_canal(&canal("canal-1"))?;
    repo.guardar_canal(&canal("canal-2"))?;
    assert_eq!(repo.listar_canais()?.len(), 2);

    let videos = [
        video("video-1", "canal-1", "2024-01-01T00:00:00Z"),
        video("video-2", "canal-1", "2024-01-02T00:00:00Z"),
        video("video-3", "canal-1", "2024-01-03T00:00:00Z"),
    ];
    repo.upsert_videos_batch(&videos)?;
    repo.guardar_classificacao(&classificacao("video-1", Nicho::Programacao))?;
    repo.guardar_classificacao(&classificacao("video-2", Nicho::Programacao))?;
    repo.guardar_classificacao(&classificacao("video-3", Nicho::Educacao))?;

    assert_eq!(
        repo.nicho_do_canal(&CanalId::new("canal-1"))?,
        Some(Nicho::Programacao)
    );
    assert_eq!(repo.nicho_do_canal(&CanalId::new("canal-2"))?, None);
    Ok(())
}

#[test]
fn upsert_videos_batch_e_idempotente() -> Resultado {
    let repo = repositorio()?;
    let canal = canal("canal-1");
    repo.guardar_canal(&canal)?;

    let videos = vec![
        video("video-1", "canal-1", "2024-01-01T00:00:00Z"),
        video("video-2", "canal-1", "2024-01-02T00:00:00Z"),
        video("video-3", "canal-1", "2024-01-03T00:00:00Z"),
    ];
    repo.upsert_videos_batch(&videos)?;
    assert_eq!(repo.listar_videos(&canal.id)?.len(), 3);

    repo.upsert_videos_batch(&videos)?;
    assert_eq!(repo.listar_videos(&canal.id)?.len(), 3);
    Ok(())
}

#[test]
fn marcador_de_comentarios_e_one_time_independente_da_data() -> Resultado {
    let repo = repositorio()?;
    let video_sem_marcador = VideoId::new("video-sem-marcador");
    let video_hoje = VideoId::new("video-hoje");
    let video_ontem = VideoId::new("video-ontem");

    assert!(!repo.comentarios_do_video_ja_coletados(&video_sem_marcador)?);

    let hoje = chrono::Utc::now().date_naive();
    let ontem = (hoje - chrono::Days::new(1)).to_string();
    let hoje = hoje.to_string();
    repo.upsert_snapshots_batch(&[
        Snapshot::novo(
            EntidadeTipo::Video,
            video_hoje.as_str(),
            Metrica::ComentariosColetados,
            2,
            hoje,
        ),
        Snapshot::novo(
            EntidadeTipo::Video,
            video_ontem.as_str(),
            Metrica::ComentariosColetados,
            2,
            ontem,
        ),
    ])?;

    assert!(repo.comentarios_do_video_ja_coletados(&video_hoje)?);
    assert!(repo.comentarios_do_video_ja_coletados(&video_ontem)?);
    Ok(())
}

#[test]
fn metricas_de_hoje_nao_incluem_ontem() -> Resultado {
    let repo = repositorio()?;
    let video_hoje = VideoId::new("video-hoje");
    let video_ontem = VideoId::new("video-ontem");

    let hoje = chrono::Utc::now().date_naive();
    let ontem = (hoje - chrono::Days::new(1)).to_string();
    let hoje = hoje.to_string();
    repo.upsert_snapshots_batch(&[
        Snapshot::novo(
            EntidadeTipo::Video,
            video_hoje.as_str(),
            Metrica::Visualizacoes,
            10,
            hoje,
        ),
        Snapshot::novo(
            EntidadeTipo::Video,
            video_ontem.as_str(),
            Metrica::Visualizacoes,
            10,
            ontem,
        ),
    ])?;

    assert!(repo.video_metricas_hoje_ja_coletadas(&video_hoje)?);
    assert!(!repo.video_metricas_hoje_ja_coletadas(&video_ontem)?);
    Ok(())
}

#[test]
fn snapshots_deduplicam_no_mesmo_dia_e_preservam_dias_diferentes() -> Resultado {
    let repo = repositorio()?;
    let canal_id = CanalId::new("canal-1");
    let snapshot = |data: &str, valor: i64| {
        Snapshot::novo(
            EntidadeTipo::Canal,
            "canal-1",
            Metrica::Inscritos,
            valor,
            data,
        )
    };

    repo.guardar_snapshot(&snapshot("2024-01-01", 100))?;
    repo.guardar_snapshot(&snapshot("2024-01-01", 999))?;
    repo.guardar_snapshot(&snapshot("2024-01-02", 200))?;

    let serie = repo.listar_serie_temporal(EntidadeTipo::Canal, "canal-1", Metrica::Inscritos)?;
    assert_eq!(serie.len(), 2);
    assert_eq!(serie[0].valor, 100);
    assert_eq!(serie[1].valor, 200);

    let snapshots = repo.listar_snapshots(&canal_id)?;
    assert_eq!(snapshots.len(), 2);
    Ok(())
}

#[test]
fn listar_videos_ordena_por_data_de_publicacao() -> Resultado {
    let repo = repositorio()?;
    let canal = canal("canal-1");
    repo.guardar_canal(&canal)?;

    for (id, data) in [
        ("video-2", "2023-01-01T00:00:00Z"),
        ("video-1", "2022-01-01T00:00:00Z"),
        ("video-3", "2024-01-01T00:00:00Z"),
    ] {
        repo.guardar_video(&video(id, "canal-1", data))?;
    }

    let videos = repo.listar_videos(&canal.id)?;
    let datas: Vec<&str> = videos
        .iter()
        .map(|video| video.publicado_em.as_str())
        .collect();
    assert_eq!(
        datas,
        vec![
            "2022-01-01T00:00:00Z",
            "2023-01-01T00:00:00Z",
            "2024-01-01T00:00:00Z"
        ]
    );
    Ok(())
}

#[test]
fn listar_videos_por_nicho_usa_a_classificacao_persistida() -> Resultado {
    let repo = repositorio()?;
    let canal = canal("canal-1");
    repo.guardar_canal(&canal)?;

    let programacao = video("video-1", "canal-1", "2024-01-01T00:00:00Z");
    let educacao = video("video-2", "canal-1", "2024-01-02T00:00:00Z");
    repo.guardar_video(&programacao)?;
    repo.guardar_video(&educacao)?;
    repo.guardar_classificacao(&classificacao("video-1", Nicho::Programacao))?;
    repo.guardar_classificacao(&classificacao("video-2", Nicho::Educacao))?;

    let videos = repo.listar_videos_por_nicho(Nicho::Programacao)?;
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0].id, programacao.id);

    let lida = repo
        .obter_classificacao(&programacao.id)?
        .expect("classificação existente");
    assert_eq!(lida.nicho, Nicho::Programacao);
    assert_eq!(lida.metodo, "teste");
    Ok(())
}
