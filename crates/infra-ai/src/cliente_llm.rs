use domain::BoxFuture;

use crate::errors::ErroIA;

/// Pedido de classificação enviado a um cliente LLM.
#[derive(Debug, Clone)]
pub struct PedidoClassificacao {
    /// Texto completo do prompt.
    pub texto: String,
    /// Nichos válidos.
    pub taxonomia: Vec<String>,
    /// Versão do template do prompt.
    pub versao_prompt: String,
}

/// Resposta bruta de um cliente LLM.
#[derive(Debug, Clone)]
pub struct RespostaBruta {
    /// Conteúdo textual devolvido pelo modelo.
    pub conteudo: String,
    /// Identificador do modelo usado.
    pub modelo: String,
}

/// Abstração do transporte LLM.
pub trait ClienteLLM: Send + Sync {
    /// Envia o pedido e devolve a resposta bruta.
    fn classificar(
        &self,
        pedido: PedidoClassificacao,
    ) -> BoxFuture<'_, Result<RespostaBruta, ErroIA>>;
}
