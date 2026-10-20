use serde::Deserialize;

/// Resposta paginada genérica da API v3.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListResponse<T> {
    pub(crate) items: Option<Vec<T>>,
    pub(crate) next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChannelItem {
    pub(crate) id: String,
    pub(crate) snippet: Option<ChannelSnippet>,
    pub(crate) statistics: Option<ChannelStatistics>,
    pub(crate) topic_details: Option<TopicDetails>,
    pub(crate) content_details: Option<ContentDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChannelSnippet {
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) custom_url: Option<String>,
    pub(crate) published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChannelStatistics {
    pub(crate) subscriber_count: Option<String>,
    pub(crate) video_count: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TopicDetails {
    pub(crate) topic_categories: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentDetails {
    pub(crate) related_playlists: Option<RelatedPlaylists>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelatedPlaylists {
    pub(crate) uploads: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoItem {
    pub(crate) id: String,
    pub(crate) snippet: VideoSnippet,
    pub(crate) statistics: Option<VideoStatistics>,
    pub(crate) topic_details: Option<TopicDetails>,
    pub(crate) content_details: Option<VideoContentDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoSnippet {
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) published_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoStatistics {
    pub(crate) view_count: Option<String>,
    pub(crate) like_count: Option<String>,
    pub(crate) comment_count: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoContentDetails {
    pub(crate) duration: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlaylistItem {
    pub(crate) content_details: PlaylistItemContentDetails,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlaylistItemContentDetails {
    pub(crate) video_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommentThread {
    pub(crate) snippet: CommentThreadSnippet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommentThreadSnippet {
    pub(crate) top_level_comment: Comment,
    #[serde(default)]
    pub(crate) total_reply_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Comment {
    pub(crate) id: String,
    pub(crate) snippet: CommentSnippet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommentSnippet {
    pub(crate) author_display_name: String,
    pub(crate) text_display: String,
    #[serde(default)]
    pub(crate) like_count: u64,
    pub(crate) published_at: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ErrorResponse {
    pub(crate) error: ErrorBody,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ErrorBody {
    pub(crate) errors: Option<Vec<ErrorDetail>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ErrorDetail {
    pub(crate) reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::CommentThread;

    #[test]
    fn total_reply_count_vem_do_snippet_da_thread() {
        let thread: CommentThread = serde_json::from_value(serde_json::json!({
            "snippet": {
                "totalReplyCount": 3,
                "topLevelComment": {
                    "id": "c1",
                    "snippet": {
                        "authorDisplayName": "Autor",
                        "textDisplay": "Texto",
                        "likeCount": 1,
                        "publishedAt": "2024-01-01T00:00:00Z"
                    }
                }
            }
        }))
        .expect("JSON de thread válido");

        assert_eq!(thread.snippet.total_reply_count, 3);
        assert_eq!(thread.snippet.top_level_comment.id, "c1");
    }
}
