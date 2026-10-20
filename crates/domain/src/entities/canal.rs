use serde::{Deserialize, Serialize};

use crate::ids::CanalId;

/// Canal de tecnologia monitorizado pelo pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canal {
    /// Identificador estável do canal.
    pub id: CanalId,
    /// Nome público do canal.
    pub nome: String,
    /// URL canónica do canal.
    pub url: String,
    /// Descrição pública do canal, quando disponível.
    pub descricao: Option<String>,
    /// Número de subscritores no momento da recolha.
    pub inscritos: u64,
    /// Número total de vídeos públicos no momento da recolha.
    pub total_videos: u64,
}
