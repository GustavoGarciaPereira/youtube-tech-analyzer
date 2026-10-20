use std::future::Future;
use std::pin::Pin;

use crate::canal_ref::CanalRef;
use crate::entities::{canal::Canal, comentario::Comentario, video::Video};
use crate::errors::ErroDominio;
use crate::ids::{CanalId, VideoId};

/// Porta de saída para recolher dados de canais do YouTube.
///
/// A implementação concreta pertence ao crate `infra-youtube`. Os futures são
/// boxed para manter a trait *dyn-compatible*: a camada `application` injeta
/// `Arc<dyn ColetorDeCanal>`.
///
/// A referência de canal já chega normalizada; o parsing de URLs/handles é
/// responsabilidade da infraestrutura.
pub trait ColetorDeCanal: Send + Sync {
    /// Recolhe os metadados de um canal.
    fn coletar_canal<'a>(
        &'a self,
        canal_ref: &CanalRef,
    ) -> Pin<Box<dyn Future<Output = Result<Canal, ErroDominio>> + Send + 'a>>;

    /// Recolhe os vídeos publicados por um canal.
    fn coletar_videos<'a>(
        &'a self,
        canal_id: &CanalId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Video>, ErroDominio>> + Send + 'a>>;

    /// Recolhe os comentários públicos de um vídeo.
    fn coletar_comentarios<'a>(
        &'a self,
        video_id: &VideoId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Comentario>, ErroDominio>> + Send + 'a>>;
}
