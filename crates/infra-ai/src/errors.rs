use domain::ErroDominio;
use thiserror::Error;

/// Erros do adaptador de IA.
#[derive(Debug, Error)]
pub enum ErroIA {
    /// Falha de transporte ou de configuração do cliente LLM.
    #[error("cliente LLM falhou: {0}")]
    Cliente(String),

    /// O LLM devolveu conteúdo que não respeita o contrato.
    #[error("resposta inválida: {0}")]
    RespostaInvalida(String),

    /// O nicho devolvido não pertence à taxonomia.
    #[error("nicho '{0}' fora da taxonomia")]
    NichoInvalido(String),

    /// O LLM não respondeu dentro do timeout.
    #[error("timeout ao contactar o LLM")]
    Timeout,

    /// A taxonomia configurada está vazia.
    #[error("taxonomia vazia")]
    TaxonomiaVazia,
}

impl From<ErroIA> for ErroDominio {
    fn from(error: ErroIA) -> Self {
        match error {
            ErroIA::TaxonomiaVazia => Self::DadosInvalidos("taxonomia de nichos vazia".to_owned()),
            ErroIA::NichoInvalido(nicho) => {
                Self::Classificacao(format!("nicho fora da taxonomia: {nicho}"))
            }
            outro => Self::Classificacao(outro.to_string()),
        }
    }
}
