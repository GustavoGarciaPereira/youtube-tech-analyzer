use std::collections::HashSet;

use domain::entities::video::Video;
use domain::ids::{CanalId, VideoId};

use super::client::ApiV3Client;
use super::dto::{ListResponse, PlaylistItem, VideoItem};
use super::{data_rfc3339, numero_u64, texto_opcional};
use crate::errors::ColetaError;

const PART_VIDEOS: &str = "snippet,statistics,topicDetails,contentDetails";
const MAX_IDS_POR_LOTE: usize = 50;

pub(crate) async fn listar_videos(
    client: &ApiV3Client,
    canal_id: &CanalId,
    uploads_playlist: &str,
) -> Result<Vec<Video>, ColetaError> {
    let ids = coletar_ids(client, uploads_playlist).await?;
    let mut videos = Vec::new();

    for lote in ids.chunks(MAX_IDS_POR_LOTE) {
        let ids_param = lote.join(",");
        let parametros = [
            ("part", PART_VIDEOS),
            ("id", ids_param.as_str()),
            ("maxResults", "50"),
        ];
        let url = client.url("videos", &parametros)?;
        let resposta: ListResponse<VideoItem> = client.get_json(url).await?;

        for item in resposta.items.unwrap_or_default() {
            videos.push(mapear_video(item, canal_id)?);
        }
    }

    videos.sort_by(|a, b| {
        a.publicado_em
            .cmp(&b.publicado_em)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    Ok(videos)
}

async fn coletar_ids(
    client: &ApiV3Client,
    uploads_playlist: &str,
) -> Result<Vec<String>, ColetaError> {
    let mut page_token: Option<String> = None;
    let mut ids = Vec::new();

    loop {
        let url = {
            let mut parametros = vec![
                ("part", "snippet,contentDetails"),
                ("playlistId", uploads_playlist),
                ("maxResults", "50"),
            ];
            if let Some(token) = page_token.as_deref() {
                parametros.push(("pageToken", token));
            }
            client.url("playlistItems", &parametros)?
        };

        let resposta: ListResponse<PlaylistItem> = client.get_json(url).await?;
        let ListResponse {
            items,
            next_page_token,
        } = resposta;
        ids.extend(
            items
                .unwrap_or_default()
                .into_iter()
                .map(|item| item.content_details.video_id),
        );

        page_token = next_page_token;
        if page_token.is_none() {
            break;
        }
    }

    let mut vistos = HashSet::new();
    ids.retain(|id| vistos.insert(id.clone()));
    Ok(ids)
}

fn mapear_video(item: VideoItem, canal_id: &CanalId) -> Result<Video, ColetaError> {
    let VideoItem {
        id,
        snippet,
        statistics,
        topic_details,
        content_details,
    } = item;

    let duracao_segundos = match content_details.and_then(|detalhes| detalhes.duration) {
        Some(duracao) => parse_duracao(&duracao)?,
        None => 0,
    };
    let url = format!("https://www.youtube.com/watch?v={id}");

    Ok(Video {
        id: VideoId::new(id),
        canal_id: canal_id.clone(),
        titulo: snippet.title,
        descricao: texto_opcional(snippet.description),
        url,
        tags: snippet.tags.unwrap_or_default(),
        topic_categories: topic_details
            .and_then(|detalhes| detalhes.topic_categories)
            .unwrap_or_default(),
        publicado_em: data_rfc3339(&snippet.published_at)?,
        duracao_segundos,
        visualizacoes: numero_u64(
            statistics
                .as_ref()
                .and_then(|estatisticas| estatisticas.view_count.as_deref()),
            "viewCount",
        )?,
        curtidas: numero_u64(
            statistics
                .as_ref()
                .and_then(|estatisticas| estatisticas.like_count.as_deref()),
            "likeCount",
        )?,
        total_comentarios: numero_u64(
            statistics
                .as_ref()
                .and_then(|estatisticas| estatisticas.comment_count.as_deref()),
            "commentCount",
        )?,
    })
}

fn parse_duracao(valor: &str) -> Result<u32, ColetaError> {
    if valor == "P0D" || valor == "PT0S" {
        return Ok(0);
    }

    let valor = valor.strip_prefix("PT").ok_or_else(|| {
        ColetaError::RespostaInvalida(format!("duração ISO 8601 inválida: {valor}"))
    })?;
    if valor.is_empty() {
        return Ok(0);
    }

    let mut total = 0_u64;
    let mut numero = String::new();
    for caractere in valor.chars() {
        if caractere.is_ascii_digit() {
            numero.push(caractere);
            continue;
        }

        let quantidade: u64 = numero.parse().map_err(|_| {
            ColetaError::RespostaInvalida(format!("duração ISO 8601 inválida: {valor}"))
        })?;
        let multiplicador = match caractere {
            'H' => 3_600,
            'M' => 60,
            'S' => 1,
            _ => {
                return Err(ColetaError::RespostaInvalida(format!(
                    "duração ISO 8601 inválida: {valor}"
                )));
            }
        };
        total += quantidade * multiplicador;
        numero.clear();
    }

    if !numero.is_empty() {
        return Err(ColetaError::RespostaInvalida(format!(
            "duração ISO 8601 inválida: {valor}"
        )));
    }

    u32::try_from(total).map_err(|_| {
        ColetaError::RespostaInvalida(format!("duração excede o limite de u32: {total}"))
    })
}

#[cfg(test)]
mod tests {
    use domain::ids::CanalId;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{listar_videos, parse_duracao};
    use crate::api_v3::test_support::coletor;

    fn video_json(
        id: &str,
        publicado_em: &str,
        visualizacoes: &str,
        duracao: &str,
    ) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": {
                "title": format!("Vídeo {id}"),
                "description": "Descrição",
                "tags": ["rust"],
                "publishedAt": publicado_em
            },
            "statistics": {
                "viewCount": visualizacoes,
                "likeCount": "10",
                "commentCount": "2"
            },
            "topicDetails": {
                "topicCategories": ["https://en.wikipedia.org/wiki/Technology"]
            },
            "contentDetails": {
                "duration": duracao
            }
        })
    }

    #[test]
    fn converte_duracoes_iso_8601() {
        assert_eq!(parse_duracao("PT10M").expect("duração"), 600);
        assert_eq!(parse_duracao("PT1H2M3S").expect("duração"), 3_723);
        assert_eq!(parse_duracao("PT30S").expect("duração"), 30);
        assert_eq!(parse_duracao("P0D").expect("duração"), 0);
        assert!(parse_duracao("10M").is_err());
    }

    #[tokio::test]
    async fn pagina_playlist_e_batcha_videos() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/playlistItems"))
            .and(query_param("playlistId", "UU123"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    { "contentDetails": { "videoId": "a" } },
                    { "contentDetails": { "videoId": "b" } }
                ],
                "nextPageToken": "pagina-2"
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/playlistItems"))
            .and(query_param("playlistId", "UU123"))
            .and(query_param("pageToken", "pagina-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    { "contentDetails": { "videoId": "c" } }
                ]
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/videos"))
            .and(query_param("id", "a,b,c"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    video_json("a", "2024-01-01T00:00:00Z", "100", "PT10M"),
                    video_json("b", "2024-01-02T00:00:00Z", "200", "PT20M"),
                    video_json("c", "2024-01-03T00:00:00Z", "300", "PT30M")
                ]
            })))
            .expect(1)
            .mount(&servidor)
            .await;

        let coletor = coletor(&servidor, 2);
        let videos = listar_videos(
            coletor.client(),
            &CanalId::new("UC1234567890123456789012"),
            "UU123",
        )
        .await
        .expect("vídeos recolhidos");

        assert_eq!(videos.len(), 3);
        assert_eq!(videos[0].id.as_str(), "a");
        assert_eq!(videos[0].duracao_segundos, 600);
        assert_eq!(videos[1].id.as_str(), "b");
        assert_eq!(videos[2].id.as_str(), "c");
    }
}
