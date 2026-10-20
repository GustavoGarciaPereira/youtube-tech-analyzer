use serde::{Deserialize, Serialize};

use crate::ids::{ComentarioId, VideoId};

/// Comentário público associado a um vídeo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comentario {
    /// Identificador estável do comentário.
    pub id: ComentarioId,
    /// Vídeo a que o comentário pertence.
    pub video_id: VideoId,
    /// Nome do autor do comentário.
    pub autor: String,
    /// Texto do comentário.
    pub texto: String,
    /// Data de publicação em formato RFC 3339.
    pub publicado_em: String,
    /// Número de gostos no momento da recolha.
    pub curtidas: u64,
    /// Indica se o comentário é uma resposta a outro comentário.
    pub eh_resposta: bool,
    /// Nível na árvore: `0` para comentário principal, `1` para resposta
    /// direta, e assim sucessivamente.
    pub nivel_resposta: u32,
    /// Comentário pai, quando este comentário é uma resposta.
    pub parent_id: Option<ComentarioId>,
}
