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
}
