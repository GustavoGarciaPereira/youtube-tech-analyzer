use crate::entities::{
    canal::Canal, comentario::Comentario, nicho::ClassificacaoNicho, nicho::Nicho,
    snapshot::EntidadeTipo, snapshot::Metrica, snapshot::Snapshot, video::Video,
};
use crate::errors::ErroDominio;
use crate::ids::{CanalId, VideoId};

/// Porta de saída para persistência e consulta de dados.
///
/// A implementação concreta pertence ao crate `infra-storage`. `canais`,
/// `videos` e `comentarios` são upserts idempotentes pela chave primária.
/// `snapshots` é histórico: a chave composta
/// (`entidade_tipo`, `entidade_id`, `metrica`, `data_coleta`) evita duplicar a
/// mesma observação no mesmo dia, mas preserva dias diferentes.
pub trait RepositorioDeCanais: Send + Sync {
    /// Insere ou atualiza os metadados de um canal.
    fn guardar_canal(&self, canal: &Canal) -> Result<(), ErroDominio>;

    /// Obtém um canal pelo seu identificador.
    fn obter_canal(&self, id: &CanalId) -> Result<Option<Canal>, ErroDominio>;

    /// Lista todos os canais conhecidos.
    fn listar_canais(&self) -> Result<Vec<Canal>, ErroDominio>;

    /// Devolve o nicho predominante de um canal, calculado a partir das
    /// classificações dos seus vídeos.
    fn nicho_do_canal(&self, id: &CanalId) -> Result<Option<Nicho>, ErroDominio>;

    /// Insere ou atualiza um vídeo.
    fn guardar_video(&self, video: &Video) -> Result<(), ErroDominio>;

    /// Insere ou atualiza vários vídeos numa única transação.
    fn upsert_videos_batch(&self, videos: &[Video]) -> Result<(), ErroDominio>;

    /// Lista os vídeos de um canal, ordenados por data de publicação.
    fn listar_videos(&self, canal_id: &CanalId) -> Result<Vec<Video>, ErroDominio>;

    /// Insere ou atualiza um comentário.
    fn guardar_comentario(&self, comentario: &Comentario) -> Result<(), ErroDominio>;

    /// Insere ou atualiza vários comentários numa única transação.
    fn upsert_comentarios_batch(&self, comentarios: &[Comentario]) -> Result<(), ErroDominio>;

    /// Lista os comentários de um vídeo, ordenados por data de publicação.
    fn listar_comentarios(&self, video_id: &VideoId) -> Result<Vec<Comentario>, ErroDominio>;

    /// Regista uma observação de métrica.
    fn guardar_snapshot(&self, snapshot: &Snapshot) -> Result<(), ErroDominio>;

    /// Regista várias observações de métricas numa única transação.
    fn upsert_snapshots_batch(&self, snapshots: &[Snapshot]) -> Result<(), ErroDominio>;

    /// Indica se os comentários do vídeo já foram recolhidos hoje (UTC).
    fn video_ja_coletado(&self, id: &VideoId) -> Result<bool, ErroDominio>;

    /// Lista as observações de métricas de um canal.
    fn listar_snapshots(&self, canal_id: &CanalId) -> Result<Vec<Snapshot>, ErroDominio>;

    /// Lista a série temporal de uma métrica para uma entidade.
    fn listar_serie_temporal(
        &self,
        entidade_tipo: EntidadeTipo,
        entidade_id: &str,
        metrica: Metrica,
    ) -> Result<Vec<Snapshot>, ErroDominio>;

    /// Insere ou atualiza a classificação de nicho de um vídeo.
    fn guardar_classificacao(&self, classificacao: &ClassificacaoNicho) -> Result<(), ErroDominio>;

    /// Obtém a classificação de nicho de um vídeo.
    fn obter_classificacao(
        &self,
        video_id: &VideoId,
    ) -> Result<Option<ClassificacaoNicho>, ErroDominio>;

    /// Lista os vídeos associados a um nicho.
    fn listar_videos_por_nicho(&self, nicho: Nicho) -> Result<Vec<Video>, ErroDominio>;
}
