use std::future::Future;

use crate::entities::{canal::Canal, comentario::Comentario, video::Video};
use crate::errors::ErroDominio;
use crate::ids::{CanalId, VideoId};

/// Porta de saída para recolher dados de canais do YouTube.
///
/// A implementação concreta pertence ao crate `infra-youtube`. O coletor deve
/// aplicar *backpressure* ao consumir os resultados, para não acumular
/// indefinidamente vídeos ou comentários em memória.
pub trait ColetorDeCanal: Send + Sync {
    /// Recolhe os metadados de um canal a partir de um handle ou URL.
    fn coletar_canal(
        &self,
        referencia: &str,
    ) -> impl Future<Output = Result<Canal, ErroDominio>> + Send;

    /// Recolhe os vídeos publicados por um canal.
    fn coletar_videos(
        &self,
        canal_id: &CanalId,
    ) -> impl Future<Output = Result<Vec<Video>, ErroDominio>> + Send;

    /// Recolhe os comentários públicos de um vídeo.
    fn coletar_comentarios(
        &self,
        video_id: &VideoId,
    ) -> impl Future<Output = Result<Vec<Comentario>, ErroDominio>> + Send;
}
