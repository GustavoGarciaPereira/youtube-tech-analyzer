use serde::{Deserialize, Serialize};

use crate::entities::{
    canal::Canal, comentario::Comentario, nicho::ClassificacaoNicho, video::Video,
};
use crate::ids::CanalId;

/// Captura pontual do estado de um canal e do seu conteúdo.
///
/// A chave lógica de um snapshot é o par (`canal.id`, `data_coleta`), o que
/// permite que a persistência seja idempotente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Data da recolha em formato RFC 3339.
    pub data_coleta: String,
    /// Metadados do canal no momento da recolha.
    pub canal: Canal,
    /// Vídeos observados no momento da recolha.
    pub videos: Vec<Video>,
    /// Comentários observados no momento da recolha.
    pub comentarios: Vec<Comentario>,
    /// Classificações de nicho atribuídas aos vídeos.
    pub classificacoes: Vec<ClassificacaoNicho>,
}

impl Snapshot {
    /// Identificador do canal capturado.
    #[must_use]
    pub fn canal_id(&self) -> &CanalId {
        &self.canal.id
    }
}
