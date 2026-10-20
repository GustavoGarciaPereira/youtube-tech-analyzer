mod fakes;

use std::sync::Arc;

use domain::entities::nicho::{ClassificacaoNicho, Nicho};
use domain::entities::video::Video;
use domain::errors::ErroDominio;
use domain::ids::{CanalId, VideoId};
use domain::ports::{CacheDeClassificacoes, ClassificadorDeNicho};
use fakes::{FakeCache, FakeClienteLLM};
use infra_ai::cliente_llm::ClienteLLM;
use infra_ai::hash_conteudo;
use infra_ai::prompt::formatar_template;
use infra_ai::{ClassificadorComCache, Taxonomia};

fn video(id: &str, titulo: &str) -> Video {
    Video {
        id: VideoId::new(id),
        canal_id: CanalId::new("canal-1"),
        titulo: titulo.to_owned(),
        descricao: Some("Descrição do vídeo".to_owned()),
        url: format!("https://www.youtube.com/watch?v={id}"),
        tags: vec!["rust".to_owned(), "programação".to_owned()],
        topic_categories: Vec::new(),
        publicado_em: "2024-01-01T00:00:00Z".to_owned(),
        duracao_segundos: 600,
        visualizacoes: 100,
        curtidas: 10,
        total_comentarios: 2,
    }
}

fn json(nicho: &str, confianca: f64) -> String {
    format!(r#"{{"nicho":"{nicho}","confianca":{confianca},"justificativa":"teste"}}"#)
}

fn criar(
    cliente: &Arc<FakeClienteLLM>,
    cache: &Arc<FakeCache>,
    versao: &str,
) -> ClassificadorComCache {
    let cliente: Arc<dyn ClienteLLM> = cliente.clone();
    let cache: Arc<dyn CacheDeClassificacoes> = cache.clone();
    ClassificadorComCache::new(cliente, cache, Taxonomia::default(), versao)
}

fn classificacao(video_id: &str) -> ClassificacaoNicho {
    ClassificacaoNicho {
        video_id: VideoId::new(video_id),
        nicho: Nicho::Programacao,
        confianca: 0.9,
        metodo: "cache".to_owned(),
        data_classificacao: "2024-01-01T00:00:00Z".to_owned(),
    }
}

#[tokio::test]
async fn cache_hit_nao_chama_o_llm() {
    let video = video("v1", "Rust para iniciantes");
    let taxonomia = Taxonomia::default();
    let template = formatar_template(&taxonomia);
    let hash = hash_conteudo(
        &video.titulo,
        video.descricao.as_deref().unwrap_or(""),
        &video.tags,
        &template,
    );
    let classificacao = classificacao("v1");

    let cache = Arc::new(FakeCache::new());
    cache.inserir(
        video.id.clone(),
        hash,
        "v1".to_owned(),
        classificacao.clone(),
    );
    let cliente = Arc::new(FakeClienteLLM::novo(Vec::new()));
    let classificador = criar(&cliente, &cache, "v1");

    let resultado = classificador
        .classificar(&video, &[])
        .await
        .expect("cache hit");

    assert_eq!(resultado, classificacao);
    assert_eq!(cliente.chamadas(), 0);
}

#[tokio::test]
async fn cache_miss_chama_e_grava() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![FakeClienteLLM::resposta(&json(
        "programacao",
        0.9,
    ))]));
    let cache = Arc::new(FakeCache::new());
    let classificador = criar(&cliente, &cache, "v1");

    let resultado = classificador
        .classificar(&video("v1", "Rust"), &[])
        .await
        .expect("classificação");

    assert_eq!(resultado.nicho, Nicho::Programacao);
    assert_eq!(resultado.metodo, "fake-model");
    assert_eq!(cliente.chamadas(), 1);
    assert_eq!(cache.total_por_versao("v1"), 1);
}

#[tokio::test]
async fn detalhe_inclui_justificativa_do_llm() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![FakeClienteLLM::resposta(&json(
        "programacao",
        0.9,
    ))]));
    let cache = Arc::new(FakeCache::new());
    let classificador = criar(&cliente, &cache, "v1");

    let detalhe = classificador
        .classificar_detalhado(&video("v1", "Rust"))
        .await
        .expect("classificação detalhada");

    assert_eq!(detalhe.justificativa.as_deref(), Some("teste"));
    assert_eq!(detalhe.classificacao.nicho, Nicho::Programacao);
}

#[tokio::test]
async fn segunda_chamada_usa_o_cache() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![FakeClienteLLM::resposta(&json(
        "programacao",
        0.9,
    ))]));
    let cache = Arc::new(FakeCache::new());
    let classificador = criar(&cliente, &cache, "v1");
    let video = video("v1", "Rust");

    let primeiro = classificador
        .classificar(&video, &[])
        .await
        .expect("primeira");
    let segundo = classificador
        .classificar(&video, &[])
        .await
        .expect("segunda");

    assert_eq!(primeiro, segundo);
    assert_eq!(cliente.chamadas(), 1);
}

#[tokio::test]
async fn titulo_diferente_invalida_o_cache() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![
        FakeClienteLLM::resposta(&json("programacao", 0.9)),
        FakeClienteLLM::resposta(&json("programacao", 0.9)),
    ]));
    let cache = Arc::new(FakeCache::new());
    let classificador = criar(&cliente, &cache, "v1");

    classificador
        .classificar(&video("v1", "Rust básico"), &[])
        .await
        .expect("primeiro título");
    classificador
        .classificar(&video("v1", "Rust avançado"), &[])
        .await
        .expect("segundo título");

    assert_eq!(cliente.chamadas(), 2);
    assert_eq!(cache.total_por_versao("v1"), 2);
}

#[tokio::test]
async fn versao_diferente_invalida_o_cache() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![
        FakeClienteLLM::resposta(&json("programacao", 0.9)),
        FakeClienteLLM::resposta(&json("programacao", 0.9)),
    ]));
    let cache = Arc::new(FakeCache::new());
    let video = video("v1", "Rust");

    let v1 = criar(&cliente, &cache, "v1");
    v1.classificar(&video, &[]).await.expect("v1");

    let v2 = criar(&cliente, &cache, "v2");
    v2.classificar(&video, &[]).await.expect("v2");

    assert_eq!(cliente.chamadas(), 2);
    assert_eq!(cache.total_por_versao("v1"), 1);
    assert_eq!(cache.total_por_versao("v2"), 1);
}

#[tokio::test]
async fn resposta_invalida_nao_polui_o_cache() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![
        FakeClienteLLM::resposta("isto não é json"),
        FakeClienteLLM::resposta(&json("programacao", 0.9)),
    ]));
    let cache = Arc::new(FakeCache::new());
    let classificador = criar(&cliente, &cache, "v1");
    let video = video("v1", "Rust");

    let primeiro = classificador.classificar(&video, &[]).await;
    assert!(matches!(primeiro, Err(ErroDominio::Classificacao(_))));
    assert!(cache.esta_vazio());

    let segundo = classificador
        .classificar(&video, &[])
        .await
        .expect("retry válido");
    assert_eq!(segundo.nicho, Nicho::Programacao);
    assert_eq!(cliente.chamadas(), 2);
    assert_eq!(cache.total_por_versao("v1"), 1);
}

#[tokio::test]
async fn nicho_fora_da_taxonomia_nao_polui_o_cache() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![
        FakeClienteLLM::resposta(&json("hardware", 0.9)),
        FakeClienteLLM::resposta(&json("programacao", 0.9)),
    ]));
    let cache = Arc::new(FakeCache::new());
    let classificador = criar(&cliente, &cache, "v1");
    let video = video("v1", "Rust");

    let primeiro = classificador.classificar(&video, &[]).await;
    assert!(matches!(primeiro, Err(ErroDominio::Classificacao(_))));
    assert!(cache.esta_vazio());

    classificador
        .classificar(&video, &[])
        .await
        .expect("retry válido");
    assert_eq!(cliente.chamadas(), 2);
    assert_eq!(cache.total_por_versao("v1"), 1);
}

#[tokio::test]
async fn confianca_fora_do_intervalo_nao_polui_o_cache() {
    let cliente = Arc::new(FakeClienteLLM::novo(vec![
        FakeClienteLLM::resposta(&json("programacao", 1.5)),
        FakeClienteLLM::resposta(&json("programacao", 0.9)),
    ]));
    let cache = Arc::new(FakeCache::new());
    let classificador = criar(&cliente, &cache, "v1");
    let video = video("v1", "Rust");

    let primeiro = classificador.classificar(&video, &[]).await;
    assert!(matches!(primeiro, Err(ErroDominio::Classificacao(_))));
    assert!(cache.esta_vazio());

    classificador
        .classificar(&video, &[])
        .await
        .expect("retry válido");
    assert_eq!(cliente.chamadas(), 2);
    assert_eq!(cache.total_por_versao("v1"), 1);
}

#[tokio::test]
async fn taxonomia_vazia_falha_antes_de_chamar_o_llm() {
    let taxonomia = Taxonomia {
        nichos: Vec::new(),
        instrucoes: String::new(),
    };
    let cliente: Arc<dyn ClienteLLM> = Arc::new(FakeClienteLLM::novo(Vec::new()));
    let cache: Arc<dyn CacheDeClassificacoes> = Arc::new(FakeCache::new());
    let classificador = ClassificadorComCache::new(cliente, cache, taxonomia, "v1");

    let resultado = classificador.classificar(&video("v1", "Rust"), &[]).await;

    assert!(matches!(resultado, Err(ErroDominio::DadosInvalidos(_))));
}
