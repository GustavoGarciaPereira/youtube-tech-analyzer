use crate::entities::{
    canal::Canal, comentario::Comentario, nicho::ClassificacaoNicho, nicho::Nicho,
    snapshot::EntidadeTipo, snapshot::Metrica, snapshot::Snapshot, video::Video,
};
use crate::errors::ErroDominio;
use crate::ids::{CanalId, ComentarioId, VideoId};
use crate::value_objects::{CategoriaManual, ScoreSentimento};

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

    /// Define ou substitui a categoria manual de um canal.
    fn definir_categoria_manual(
        &self,
        canal_id: &CanalId,
        categoria: CategoriaManual,
    ) -> Result<(), ErroDominio>;

    /// Obtém a categoria manual de um canal.
    fn categoria_manual_do_canal(
        &self,
        canal_id: &CanalId,
    ) -> Result<Option<CategoriaManual>, ErroDominio>;

    /// Lista os canais de uma categoria manual.
    fn canais_por_categoria(&self, categoria: CategoriaManual) -> Result<Vec<Canal>, ErroDominio>;

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

    /// Guarda o resultado da análise de sentimento de um comentário.
    ///
    /// A chave lógica é (comentario_id, versao_modelo): cada versão do modelo
    /// tem a sua linha, pelo que re-analisar não apaga o histórico anterior.
    /// A operação é idempotente para o mesmo par.
    fn guardar_sentimento(
        &self,
        comentario_id: &ComentarioId,
        score: &ScoreSentimento,
        versao_modelo: &str,
    ) -> Result<(), ErroDominio>;

    /// Lista comentários de um canal ainda sem análise para esta versão.
    ///
    /// Um comentário analisado com outra versão do modelo volta a aparecer:
    /// é isso que permite re-analisar quando o modelo muda.
    fn comentarios_sem_sentimento(
        &self,
        canal_id: &CanalId,
        versao_modelo: &str,
    ) -> Result<Vec<Comentario>, ErroDominio>;

    /// Regista uma observação de métrica.
    fn guardar_snapshot(&self, snapshot: &Snapshot) -> Result<(), ErroDominio>;

    /// Regista várias observações de métricas numa única transação.
    fn upsert_snapshots_batch(&self, snapshots: &[Snapshot]) -> Result<(), ErroDominio>;

    /// Indica se os comentários do vídeo já foram recolhidos alguma vez,
    /// independentemente da data.
    fn comentarios_do_video_ja_coletados(&self, id: &VideoId) -> Result<bool, ErroDominio>;

    /// Indica se as métricas do vídeo já foram registadas hoje (UTC).
    ///
    /// Serve apenas para evitar re-coletar no mesmo dia; no dia seguinte
    /// deve devolver `false` e permitir novos snapshots.
    fn video_metricas_hoje_ja_coletadas(&self, id: &VideoId) -> Result<bool, ErroDominio>;

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
