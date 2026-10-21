use std::fs;

use infra_nlp::{ErroNlp, ModelManager};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TAMANHO_MINIMO_MODELO: u64 = 50 * 1024 * 1024;
const TAMANHO_MINIMO_TOKENIZER: u64 = 100 * 1024;
const TAMANHO_MINIMO_RUNTIME: u64 = 1_000_000;

fn criar_com_tamanho(caminho: std::path::PathBuf, tamanho: u64) {
    fs::File::create(caminho)
        .expect("criar ficheiro")
        .set_len(tamanho)
        .expect("definir tamanho");
}

fn gestor(base: &std::path::Path, uri: &str) -> ModelManager {
    ModelManager::com_fontes(
        base.to_path_buf(),
        format!("{uri}/model.onnx"),
        format!("{uri}/tokenizer.json"),
    )
}

#[tokio::test]
async fn falha_http_devolve_erro_limpo() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&servidor)
        .await;

    let pasta = tempfile::tempdir().expect("pasta temporária");
    let erro = gestor(pasta.path(), &servidor.uri())
        .garantir_modelo_baixado()
        .await
        .expect_err("HTTP 500 deve falhar");

    assert!(
        matches!(erro, ErroNlp::Http { status: 500, .. }),
        "erro inesperado: {erro}"
    );
}

#[tokio::test]
async fn corpo_demasiado_pequeno_e_rejeitado() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0_u8; 100]))
        .mount(&servidor)
        .await;

    let pasta = tempfile::tempdir().expect("pasta temporária");
    let erro = gestor(pasta.path(), &servidor.uri())
        .garantir_modelo_baixado()
        .await
        .expect_err("corpo curto deve falhar");

    assert!(
        matches!(erro, ErroNlp::FicheiroInvalido { .. }),
        "erro inesperado: {erro}"
    );
    assert!(
        !gestor(pasta.path(), &servidor.uri())
            .caminho_modelo()
            .exists()
    );
}

#[tokio::test]
async fn descarrega_modelo_e_tokenizer() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/model.onnx"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7_u8; 4096]))
        .mount(&servidor)
        .await;
    Mock::given(method("GET"))
        .and(path("/tokenizer.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![3_u8; 2048]))
        .mount(&servidor)
        .await;

    let pasta = tempfile::tempdir().expect("pasta temporária");
    let gestor = gestor(pasta.path(), &servidor.uri());
    fs::write(gestor.caminho_runtime(), vec![0_u8; 1_000_001]).expect("runtime");
    let resultado = gestor
        .garantir_modelo_baixado()
        .await
        .expect("download completo");

    assert_eq!(resultado, pasta.path());
    assert_eq!(
        fs::metadata(gestor.caminho_modelo())
            .expect("modelo existe")
            .len(),
        4096
    );
    assert_eq!(
        fs::metadata(gestor.caminho_tokenizer())
            .expect("tokenizer existe")
            .len(),
        2048
    );
}

#[tokio::test]
async fn modelo_truncado_e_substituido_no_download() {
    let servidor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/model.onnx"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7_u8; 4096]))
        .mount(&servidor)
        .await;

    let pasta = tempfile::tempdir().expect("pasta temporária");
    let gestor = gestor(pasta.path(), &servidor.uri());
    fs::write(gestor.caminho_modelo(), vec![1_u8; 2048]).expect("modelo truncado");
    criar_com_tamanho(gestor.caminho_tokenizer(), TAMANHO_MINIMO_TOKENIZER);
    criar_com_tamanho(gestor.caminho_runtime(), TAMANHO_MINIMO_RUNTIME);

    gestor
        .garantir_modelo_baixado()
        .await
        .expect("download do modelo");

    assert_eq!(
        fs::metadata(gestor.caminho_modelo())
            .expect("modelo existe")
            .len(),
        4096,
        "o ficheiro truncado devia ter sido substituído"
    );
}

#[tokio::test]
async fn ficheiros_existentes_evitam_rede() {
    let pasta = tempfile::tempdir().expect("pasta temporária");
    // Porta 9 (discard) para garantir que não há download.
    let gestor = gestor(pasta.path(), "http://127.0.0.1:9");

    criar_com_tamanho(gestor.caminho_modelo(), TAMANHO_MINIMO_MODELO);
    criar_com_tamanho(gestor.caminho_tokenizer(), TAMANHO_MINIMO_TOKENIZER);
    criar_com_tamanho(gestor.caminho_runtime(), TAMANHO_MINIMO_RUNTIME);

    let resultado = gestor
        .garantir_modelo_baixado()
        .await
        .expect("não deve tentar a rede");
    assert_eq!(resultado, pasta.path());
}
