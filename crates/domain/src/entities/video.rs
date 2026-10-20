use serde::{Deserialize, Serialize};

use crate::ids::{CanalId, VideoId};

/// Vídeo publicado por um canal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Video {
    /// Identificador estável do vídeo.
    pub id: VideoId,
    /// Canal a que o vídeo pertence.
    pub canal_id: CanalId,
    /// Título do vídeo.
    pub titulo: String,
    /// Descrição pública do vídeo, quando disponível.
    pub descricao: Option<String>,
    /// URL canónica do vídeo.
    pub url: String,
    /// Tags públicas associadas ao vídeo.
    pub tags: Vec<String>,
    /// Categorias de tópico atribuídas pelo YouTube.
    pub topic_categories: Vec<String>,
    /// Data de publicação em formato RFC 3339.
    pub publicado_em: String,
    /// Duração do vídeo em segundos.
    pub duracao_segundos: u32,
    /// Número de visualizações no momento da recolha.
    pub visualizacoes: u64,
    /// Número de gostos no momento da recolha.
    pub curtidas: u64,
    /// Número total de comentários no momento da recolha.
    pub total_comentarios: u64,
}
