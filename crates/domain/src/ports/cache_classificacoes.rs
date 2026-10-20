use std::future::Future;
use std::pin::Pin;

use crate::entities::nicho::ClassificacaoNicho;
use crate::errors::ErroDominio;
use crate::ids::VideoId;

/// Future boxed usado nas portas assíncronas object-safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Porta de cache de classificações de nicho.
///
/// É uma porta separada de `RepositorioDeCanais` para que `infra-ai` não
/// dependa de `infra-storage`. O *composition root* injeta a implementação
/// concreta.
pub trait CacheDeClassificacoes: Send + Sync {
    /// Obtém uma classificação pelo vídeo, hash do conteúdo e versão.
    fn obter<'a>(
        &'a self,
        video_id: &'a VideoId,
        hash_conteudo: &'a str,
        versao_classificador: &'a str,
    ) -> BoxFuture<'a, Result<Option<ClassificacaoNicho>, ErroDominio>>;

    /// Grava ou atualiza uma classificação no cache.
    fn gravar<'a>(
        &'a self,
        video_id: &'a VideoId,
        hash_conteudo: &'a str,
        versao_classificador: &'a str,
        classificacao: &'a ClassificacaoNicho,
    ) -> BoxFuture<'a, Result<(), ErroDominio>>;

    /// Conta as entradas de cache de uma versão do classificador.
    fn contar_por_versao<'a>(
        &'a self,
        versao_classificador: &'a str,
    ) -> BoxFuture<'a, Result<usize, ErroDominio>>;
}
