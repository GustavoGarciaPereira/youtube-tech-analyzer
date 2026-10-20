use std::future::Future;

use crate::entities::{comentario::Comentario, nicho::ClassificacaoNicho, video::Video};
use crate::errors::ErroDominio;

/// Porta de saída para classificar o nicho de um vídeo.
///
/// A implementação concreta pertence ao crate `infra-ai` e pode usar metadados
/// do vídeo e comentários como contexto.
pub trait ClassificadorDeNicho: Send + Sync {
    /// Classifica um vídeo, devolvendo o nicho e a confiança associada.
    fn classificar(
        &self,
        video: &Video,
        comentarios: &[Comentario],
    ) -> impl Future<Output = Result<ClassificacaoNicho, ErroDominio>> + Send;
}
