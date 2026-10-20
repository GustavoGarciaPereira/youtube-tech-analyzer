use domain::CanalRef;
use domain::entities::canal::Canal;
use domain::ids::CanalId;

use super::client::ApiV3Client;
use super::dto::{ChannelItem, ContentDetails, ListResponse};
use super::{data_rfc3339, numero_u64, texto_opcional};
use crate::errors::ColetaError;

const PART_CANAL: &str = "snippet,statistics,topicDetails,contentDetails";

pub(crate) async fn resolver_canal(
    client: &ApiV3Client,
    canal_ref: &CanalRef,
) -> Result<(Canal, String), ColetaError> {
    let consulta;
    let mut parametros = vec![("part", PART_CANAL)];
    match canal_ref {
        CanalRef::Handle(handle) => {
            consulta = format!("@{handle}");
            parametros.push(("forHandle", consulta.as_str()));
        }
        // A API v3 não expõe lookup por custom URL (/c/Nome). Usar
        // forUsername é uma aproximação de melhor esforço.
        CanalRef::Custom(nome) | CanalRef::Username(nome) => {
            consulta = nome.clone();
            parametros.push(("forUsername", consulta.as_str()));
        }
        CanalRef::ChannelId(id) => {
            consulta = id.clone();
            parametros.push(("id", consulta.as_str()));
        }
    }

    let url = client.url("channels", &parametros)?;
    let resposta: ListResponse<ChannelItem> = client.get_json(url).await?;
    let item = resposta
        .items
        .and_then(|itens| itens.into_iter().next())
        .ok_or(ColetaError::NaoEncontrado)?;

    mapear_canal(item)
}

pub(crate) async fn obter_uploads_playlist(
    client: &ApiV3Client,
    canal_id: &CanalId,
) -> Result<String, ColetaError> {
    let parametros = [("part", "contentDetails"), ("id", canal_id.as_str())];
    let url = client.url("channels", &parametros)?;
    let resposta: ListResponse<ChannelItem> = client.get_json(url).await?;
    let item = resposta
        .items
        .and_then(|itens| itens.into_iter().next())
        .ok_or(ColetaError::NaoEncontrado)?;

    extrair_uploads(item.content_details)
}

fn mapear_canal(item: ChannelItem) -> Result<(Canal, String), ColetaError> {
    let ChannelItem {
        id,
        snippet,
        statistics,
        topic_details,
        content_details,
    } = item;
    let snippet =
        snippet.ok_or_else(|| ColetaError::RespostaInvalida("canal sem snippet".to_owned()))?;
    let uploads = extrair_uploads(content_details)?;
    let handle = snippet.custom_url;
    let url = match handle.as_deref() {
        Some(handle) => format!("https://www.youtube.com/{handle}"),
        None => format!("https://www.youtube.com/channel/{id}"),
    };

    let canal = Canal {
        id: CanalId::new(id),
        nome: snippet.title,
        handle,
        url,
        descricao: texto_opcional(snippet.description),
        inscritos: numero_u64(
            statistics
                .as_ref()
                .and_then(|estatisticas| estatisticas.subscriber_count.as_deref()),
            "subscriberCount",
        )?,
        total_videos: numero_u64(
            statistics
                .as_ref()
                .and_then(|estatisticas| estatisticas.video_count.as_deref()),
            "videoCount",
        )?,
        topic_categories: topic_details
            .and_then(|detalhes| detalhes.topic_categories)
            .unwrap_or_default(),
        criado_em: snippet
            .published_at
            .as_deref()
            .map(data_rfc3339)
            .transpose()?,
    };

    Ok((canal, uploads))
}

fn extrair_uploads(content_details: Option<ContentDetails>) -> Result<String, ColetaError> {
    content_details
        .and_then(|detalhes| detalhes.related_playlists)
        .and_then(|playlists| playlists.uploads)
        .ok_or_else(|| ColetaError::RespostaInvalida("canal sem uploads playlist".to_owned()))
}

#[cfg(test)]
mod tests {
    use domain::CanalRef;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{PART_CANAL, resolver_canal};
    use crate::api_v3::test_support::coletor;

    #[tokio::test]
    async fn resolve_canal_com_topic_categories_e_uploads() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/channels"))
            .and(query_param("part", PART_CANAL))
            .and(query_param("forHandle", "@canal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "UC1234567890123456789012",
                    "snippet": {
                        "title": "Canal de Tecnologia",
                        "description": "Descrição",
                        "customUrl": "@canal",
                        "publishedAt": "2020-01-01T00:00:00Z"
                    },
                    "statistics": {
                        "subscriberCount": "1000",
                        "videoCount": "42"
                    },
                    "topicDetails": {
                        "topicCategories": ["https://en.wikipedia.org/wiki/Technology"]
                    },
                    "contentDetails": {
                        "relatedPlaylists": { "uploads": "UU123" }
                    }
                }]
            })))
            .mount(&servidor)
            .await;

        let coletor = coletor(&servidor, 2);
        let (canal, uploads) =
            resolver_canal(coletor.client(), &CanalRef::Handle("canal".to_owned()))
                .await
                .expect("canal resolvido");

        assert_eq!(canal.id.as_str(), "UC1234567890123456789012");
        assert_eq!(canal.nome, "Canal de Tecnologia");
        assert_eq!(canal.handle.as_deref(), Some("@canal"));
        assert_eq!(canal.inscritos, 1_000);
        assert_eq!(canal.total_videos, 42);
        assert_eq!(
            canal.topic_categories,
            vec!["https://en.wikipedia.org/wiki/Technology"]
        );
        assert_eq!(canal.criado_em.as_deref(), Some("2020-01-01T00:00:00Z"));
        assert_eq!(uploads, "UU123");
    }
}
