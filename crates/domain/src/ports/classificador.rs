use std::future::Future;
use std::pin::Pin;

use crate::entities::{comentario::Comentario, nicho::ClassificacaoNicho, video::Video};
use crate::errors::ErroDominio;

/// Porta de saída para classificar o nicho de um vídeo.
///
/// A implementação concreta pertence ao crate `infra-ai` e pode usar metadados
/// do vídeo e comentários como contexto. O future é boxed para manter a trait
/// compatível com `Arc<dyn ClassificadorDeNicho>`.
pub trait ClassificadorDeNicho: Send + Sync {
    /// Classifica um vídeo, devolvendo o nicho e a confiança associada.
    fn classificar<'a>(
        &'a self,
        video: &'a Video,
        comentarios: &'a [Comentario],
    ) -> Pin<Box<dyn Future<Output = Result<ClassificacaoNicho, ErroDominio>> + Send + 'a>>;
}
