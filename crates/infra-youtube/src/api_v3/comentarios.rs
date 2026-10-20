use domain::entities::comentario::Comentario;
use domain::ids::{ComentarioId, VideoId};

use super::client::ApiV3Client;
use super::data_rfc3339;
use super::dto::{Comment, CommentThread, ListResponse};
use crate::errors::ColetaError;

const MAX_RESULTADOS: &str = "100";

pub(crate) async fn listar_comentarios(
    client: &ApiV3Client,
    video_id: &VideoId,
    max_depth: i32,
) -> Result<Vec<Comentario>, ColetaError> {
    if max_depth < -1 {
        return Err(ColetaError::RespostaInvalida(
            "max_depth deve ser -1 ou maior ou igual a 0".to_owned(),
        ));
    }

    let mut page_token: Option<String> = None;
    let mut comentarios = Vec::new();

    loop {
        let url = {
            let mut parametros = vec![
                ("part", "snippet,replies"),
                ("videoId", video_id.as_str()),
                ("maxResults", MAX_RESULTADOS),
                ("textFormat", "plainText"),
            ];
            if let Some(token) = page_token.as_deref() {
                parametros.push(("pageToken", token));
            }
            client.url("commentThreads", &parametros)?
        };

        let resposta: ListResponse<CommentThread> = client.get_json(url).await?;
        let ListResponse {
            items,
            next_page_token,
        } = resposta;

        for thread in items.unwrap_or_default() {
            let total_respostas = thread.snippet.total_reply_count;
            let top_level = thread.snippet.top_level_comment;
            let top_level_id = ComentarioId::new(top_level.id.clone());

            comentarios.push(mapear_comentario(&top_level, video_id, false, 0, None)?);

            if total_respostas > 0 && max_depth != 0 {
                let respostas =
                    buscar_respostas(client, video_id, &top_level_id, 0, max_depth).await?;
                comentarios.extend(respostas);
            }
        }

        page_token = next_page_token;
        if page_token.is_none() {
            break;
        }
    }

    comentarios.sort_by(|a, b| {
        a.publicado_em
            .cmp(&b.publicado_em)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    Ok(comentarios)
}

async fn buscar_respostas(
    client: &ApiV3Client,
    video_id: &VideoId,
    parent_id: &ComentarioId,
    parent_level: i32,
    max_depth: i32,
) -> Result<Vec<Comentario>, ColetaError> {
    let child_level = parent_level + 1;
    if max_depth >= 0 && child_level > max_depth {
        return Ok(Vec::new());
    }

    let mut page_token: Option<String> = None;
    let mut respostas = Vec::new();

    loop {
        let url = {
            let mut parametros = vec![
                ("part", "snippet"),
                ("parentId", parent_id.as_str()),
                ("maxResults", MAX_RESULTADOS),
                ("textFormat", "plainText"),
            ];
            if let Some(token) = page_token.as_deref() {
                parametros.push(("pageToken", token));
            }
            client.url("comments", &parametros)?
        };

        let resposta: ListResponse<Comment> = client.get_json(url).await?;
        let ListResponse {
            items,
            next_page_token,
        } = resposta;
        let itens = items.unwrap_or_default();
        if itens.is_empty() {
            break;
        }

        for item in itens {
            let resposta_id = ComentarioId::new(item.id.clone());
            respostas.push(mapear_comentario(
                &item,
                video_id,
                true,
                child_level,
                Some(parent_id.clone()),
            )?);

            if max_depth == -1 || child_level < max_depth {
                let filhos = Box::pin(buscar_respostas(
                    client,
                    video_id,
                    &resposta_id,
                    child_level,
                    max_depth,
                ))
                .await?;
                respostas.extend(filhos);
            }
        }

        page_token = next_page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(respostas)
}

fn mapear_comentario(
    item: &Comment,
    video_id: &VideoId,
    eh_resposta: bool,
    nivel: i32,
    parent_id: Option<ComentarioId>,
) -> Result<Comentario, ColetaError> {
    Ok(Comentario {
        id: ComentarioId::new(item.id.clone()),
        video_id: video_id.clone(),
        autor: item.snippet.author_display_name.clone(),
        texto: item.snippet.text_display.clone(),
        publicado_em: data_rfc3339(&item.snippet.published_at)?,
        curtidas: item.snippet.like_count,
        eh_resposta,
        nivel_resposta: u32::try_from(nivel).map_err(|_| {
            ColetaError::RespostaInvalida(format!("nível de resposta inválido: {nivel}"))
        })?,
        parent_id,
    })
}

#[cfg(test)]
mod tests {
    use domain::ids::VideoId;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::api_v3::test_support::coletor;

    fn thread_json(id: &str, total_respostas: u32, publicado_em: &str) -> Value {
        json!({
            "snippet": {
                "totalReplyCount": total_respostas,
                "topLevelComment": {
                    "id": id,
                    "snippet": {
                        "authorDisplayName": "Autor",
                        "textDisplay": format!("Comentário {id}"),
                        "likeCount": 1,
                        "publishedAt": publicado_em
                    }
                }
            }
        })
    }

    fn reply_json(id: &str, publicado_em: &str) -> Value {
        json!({
            "id": id,
            "snippet": {
                "authorDisplayName": "Autor",
                "textDisplay": format!("Resposta {id}"),
                "likeCount": 1,
                "publishedAt": publicado_em
            }
        })
    }

    async fn pedidos_para(servidor: &MockServer, caminho: &str) -> usize {
        servidor
            .received_requests()
            .await
            .expect("pedidos registados")
            .iter()
            .filter(|pedido| pedido.url.path() == caminho)
            .count()
    }

    #[tokio::test]
    async fn max_depth_zero_nao_pede_respostas() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/commentThreads"))
            .and(query_param("part", "snippet,replies"))
            .and(query_param("textFormat", "plainText"))
            .and(query_param("maxResults", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [thread_json("c1", 3, "2024-01-03T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        let coletor = coletor(&servidor, 2);
        let comentarios = coletor
            .coletar_comentarios_com_profundidade(&VideoId::new("video-1"), 0)
            .await
            .expect("comentários");

        assert_eq!(comentarios.len(), 1);
        assert!(!comentarios[0].eh_resposta);
        assert_eq!(comentarios[0].nivel_resposta, 0);
        assert_eq!(pedidos_para(&servidor, "/youtube/v3/comments").await, 0);
    }

    #[tokio::test]
    async fn total_reply_count_da_thread_dispara_busca_de_respostas() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/commentThreads"))
            .and(query_param("part", "snippet,replies"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [thread_json("c1", 3, "2024-01-03T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/comments"))
            .and(query_param("parentId", "c1"))
            .and(query_param("part", "snippet"))
            .and(query_param("textFormat", "plainText"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    reply_json("c2", "2024-01-01T00:00:00Z"),
                    reply_json("c3", "2024-01-02T00:00:00Z")
                ]
            })))
            .expect(1)
            .mount(&servidor)
            .await;

        let coletor = coletor(&servidor, 2);
        let comentarios = coletor
            .coletar_comentarios_com_profundidade(&VideoId::new("video-1"), 1)
            .await
            .expect("comentários");

        assert_eq!(comentarios.len(), 3);
        assert_eq!(comentarios[0].id.as_str(), "c2");
        assert_eq!(comentarios[0].nivel_resposta, 1);
        assert_eq!(
            comentarios[0].parent_id.as_ref().map(|id| id.as_str()),
            Some("c1")
        );
        assert_eq!(comentarios[1].id.as_str(), "c3");
        assert_eq!(comentarios[2].id.as_str(), "c1");
        assert_eq!(pedidos_para(&servidor, "/youtube/v3/comments").await, 1);
    }

    #[tokio::test]
    async fn max_depth_dois_recursiona_na_arvore() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/commentThreads"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [thread_json("c1", 1, "2024-01-01T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/comments"))
            .and(query_param("parentId", "c1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [reply_json("c2", "2024-01-02T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/comments"))
            .and(query_param("parentId", "c2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [reply_json("c3", "2024-01-03T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        let coletor = coletor(&servidor, 2);
        let comentarios = coletor
            .coletar_comentarios_com_profundidade(&VideoId::new("video-1"), 2)
            .await
            .expect("comentários");

        assert_eq!(comentarios.len(), 3);
        assert_eq!(comentarios[0].nivel_resposta, 0);
        assert_eq!(comentarios[1].nivel_resposta, 1);
        assert_eq!(comentarios[2].nivel_resposta, 2);
        // c1 (nível 0) e c2 (nível 1) são consultados; c3 já está no limite.
        assert_eq!(pedidos_para(&servidor, "/youtube/v3/comments").await, 2);
    }

    #[tokio::test]
    async fn pagina_threads_com_next_page_token() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/commentThreads"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [thread_json("c1", 0, "2024-01-01T00:00:00Z")],
                "nextPageToken": "threads-2"
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/commentThreads"))
            .and(query_param("pageToken", "threads-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [thread_json("c2", 0, "2024-01-02T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        let coletor = coletor(&servidor, 2);
        let comentarios = coletor
            .coletar_comentarios_com_profundidade(&VideoId::new("video-1"), 2)
            .await
            .expect("comentários");

        assert_eq!(comentarios.len(), 2);
    }

    #[tokio::test]
    async fn pagina_respostas_com_next_page_token() {
        let servidor = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/commentThreads"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [thread_json("c1", 2, "2024-01-01T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/comments"))
            .and(query_param("parentId", "c1"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [reply_json("c2", "2024-01-02T00:00:00Z")],
                "nextPageToken": "respostas-2"
            })))
            .mount(&servidor)
            .await;

        Mock::given(method("GET"))
            .and(path("/youtube/v3/comments"))
            .and(query_param("parentId", "c1"))
            .and(query_param("pageToken", "respostas-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [reply_json("c3", "2024-01-03T00:00:00Z")]
            })))
            .mount(&servidor)
            .await;

        let coletor = coletor(&servidor, 2);
        let comentarios = coletor
            .coletar_comentarios_com_profundidade(&VideoId::new("video-1"), 1)
            .await
            .expect("comentários");

        assert_eq!(comentarios.len(), 3);
        assert_eq!(pedidos_para(&servidor, "/youtube/v3/comments").await, 2);
    }
}
