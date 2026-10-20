use wiremock::MockServer;

use super::ApiV3Coletor;

pub(crate) fn coletor(servidor: &MockServer, max_depth: i32) -> ApiV3Coletor {
    let base_url = reqwest::Url::parse(&format!("{}/youtube/v3/", servidor.uri()))
        .expect("URL do servidor wiremock");
    ApiV3Coletor::with_base_url("chave-teste", base_url, max_depth).expect("coletor de teste")
}
