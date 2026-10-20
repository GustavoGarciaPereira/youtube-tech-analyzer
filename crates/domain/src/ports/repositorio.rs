use crate::entities::{canal::Canal, comentario::Comentario, snapshot::Snapshot, video::Video};
use crate::errors::ErroDominio;
use crate::ids::{CanalId, VideoId};

/// Porta de saída para persistência e consulta de dados.
///
/// A implementação concreta pertence ao crate `infra-storage`. As operações de
/// escrita devem ser idempotentes para as chaves naturais de cada entidade
/// (`video_id`, `comentario_id` e o par `canal_id`/`data_coleta`).
pub trait RepositorioDeCanais: Send + Sync {
    /// Insere ou atualiza os metadados de um canal.
    fn guardar_canal(&self, canal: &Canal) -> Result<(), ErroDominio>;

    /// Obtém um canal pelo seu identificador.
    fn obter_canal(&self, id: &CanalId) -> Result<Option<Canal>, ErroDominio>;

    /// Insere ou atualiza um vídeo.
    fn guardar_video(&self, video: &Video) -> Result<(), ErroDominio>;

    /// Lista os vídeos de um canal.
    fn listar_videos(&self, canal_id: &CanalId) -> Result<Vec<Video>, ErroDominio>;

    /// Insere ou atualiza um comentário.
    fn guardar_comentario(&self, comentario: &Comentario) -> Result<(), ErroDominio>;

    /// Lista os comentários de um vídeo.
    fn listar_comentarios(&self, video_id: &VideoId) -> Result<Vec<Comentario>, ErroDominio>;

    /// Insere ou atualiza um snapshot completo.
    fn guardar_snapshot(&self, snapshot: &Snapshot) -> Result<(), ErroDominio>;

    /// Lista os snapshots de um canal, do mais recente para o mais antigo.
    fn listar_snapshots(&self, canal_id: &CanalId) -> Result<Vec<Snapshot>, ErroDominio>;
}
