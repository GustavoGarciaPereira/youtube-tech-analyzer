use std::time::Duration;

use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;

use crate::api_v3::dto::ErrorResponse;
use crate::errors::ColetaError;

const API_BASE: &str = "https://www.googleapis.com/youtube/v3/";
const USER_AGENT: &str = "youtube-tech-analyzer/0.1";
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TENTATIVAS: usize = 5;
const BACKOFF_MS: [u64; MAX_TENTATIVAS - 1] = [250, 500, 1_000, 2_000];

/// Cliente HTTP fino, com retry e tratamento uniforme de erros da API v3.
pub(crate) struct ApiV3Client {
    http: Client,
    api_key: String,
    base_url: Url,
}

impl ApiV3Client {
    pub(crate) fn new(api_key: String) -> Result<Self, ColetaError> {
        let base_url = Url::parse(API_BASE)
            .map_err(|erro| ColetaError::RespostaInvalida(format!("URL base inválida: {erro}")))?;
        Self::with_base_url(api_key, base_url)
    }

    pub(crate) fn with_base_url(api_key: String, base_url: Url) -> Result<Self, ColetaError> {
        let http = Client::builder()
            .timeout(TIMEOUT)
            .user_agent(USER_AGENT)
            .build()?;
        Ok(Self {
            http,
            api_key,
            base_url,
        })
    }

    pub(crate) fn url(
        &self,
        caminho: &str,
        parametros: &[(&str, &str)],
    ) -> Result<Url, ColetaError> {
        let mut url = self.base_url.join(caminho).map_err(|erro| {
            ColetaError::RespostaInvalida(format!("URL da API inválida: {erro}"))
        })?;
        {
            let mut query = url.query_pairs_mut();
            for (chave, valor) in parametros {
                query.append_pair(chave, valor);
            }
            query.append_pair("key", &self.api_key);
        }
        Ok(url)
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, ColetaError> {
        let mut tentativa = 0;
        loop {
            match self.http.get(url.clone()).send().await {
                Ok(resposta) => {
                    let status = resposta.status();
                    if status.is_success() {
                        return resposta.json::<T>().await.map_err(ColetaError::Http);
                    }

                    let corpo = resposta.text().await.map_err(ColetaError::Http)?;
                    if status == StatusCode::FORBIDDEN && quota_excedida(&corpo) {
                        return Err(ColetaError::QuotaExcedida);
                    }

                    if status_retentavel(status) && tentativa + 1 < MAX_TENTATIVAS {
                        dormir(tentativa).await;
                        tentativa += 1;
                        continue;
                    }

                    return Err(mapear_status(status, &corpo));
                }
                Err(erro) => {
                    if erro_transitorio(&erro) && tentativa + 1 < MAX_TENTATIVAS {
                        dormir(tentativa).await;
                        tentativa += 1;
                        continue;
                    }
                    return Err(ColetaError::Http(erro));
                }
            }
        }
    }
}

async fn dormir(tentativa: usize) {
    if let Some(milissegundos) = BACKOFF_MS.get(tentativa) {
        tokio::time::sleep(Duration::from_millis(*milissegundos)).await;
    }
}

fn status_retentavel(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn erro_transitorio(erro: &reqwest::Error) -> bool {
    erro.is_timeout() || erro.is_connect() || erro.is_request() || erro.is_body()
}

fn quota_excedida(corpo: &str) -> bool {
    serde_json::from_str::<ErrorResponse>(corpo)
        .ok()
        .and_then(|resposta| resposta.error.errors)
        .is_some_and(|erros| {
            erros
                .iter()
                .any(|erro| erro.reason.as_deref() == Some("quotaExceeded"))
        })
}

fn mapear_status(status: StatusCode, corpo: &str) -> ColetaError {
    match status {
        StatusCode::NOT_FOUND => ColetaError::NaoEncontrado,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ColetaError::Indisponivel,
        _ => ColetaError::RespostaInvalida(format!("HTTP {status}: {corpo}")),
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Url;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{ApiV3Client, ColetaError};

    fn cliente(servidor: &MockServer) -> ApiV3Client {
        let base_url = Url::parse(&format!("{}/youtube/v3/", servidor.uri()))
            .expect("URL do servidor de teste");
        ApiV3Client::with_base_url("chave-teste".to_owned(), base_url).expect("cliente de teste")
    }

    fn url_teste(cliente: &ApiV3Client) -> Url {
        cliente.url("teste", &[]).expect("URL de teste")
    }

    #[tokio::test]
    async fn retry_429_depois_200() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/teste"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/teste"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .mount(&servidor)
            .await;

        let cliente = cliente(&servidor);
        let resposta: Value = cliente
            .get_json(url_teste(&cliente))
            .await
            .expect("resposta após retry");

        assert_eq!(resposta["ok"], true);
        assert_eq!(
            servidor
                .received_requests()
                .await
                .expect("pedidos registados")
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn quota_excedida_nao_faz_retry() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/teste"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "error": {
                    "errors": [{ "reason": "quotaExceeded" }]
                }
            })))
            .mount(&servidor)
            .await;

        let cliente = cliente(&servidor);
        let erro = cliente
            .get_json::<Value>(url_teste(&cliente))
            .await
            .expect_err("quota excedida");

        assert!(matches!(erro, ColetaError::QuotaExcedida));
        assert_eq!(
            servidor
                .received_requests()
                .await
                .expect("pedidos registados")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn erro_400_nao_faz_retry() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/teste"))
            .respond_with(ResponseTemplate::new(400).set_body_string("pedido inválido"))
            .mount(&servidor)
            .await;

        let cliente = cliente(&servidor);
        let erro = cliente
            .get_json::<Value>(url_teste(&cliente))
            .await
            .expect_err("erro 400");

        assert!(matches!(erro, ColetaError::RespostaInvalida(_)));
        assert_eq!(
            servidor
                .received_requests()
                .await
                .expect("pedidos registados")
                .len(),
            1
        );
    }
}
