use domain::ErroDominio;
use thiserror::Error;

/// Erros do adaptador YouTube.
#[derive(Debug, Error)]
pub enum ColetaError {
    /// A variável de ambiente `YOUTUBE_API_KEY` não está definida.
    #[error("chave de API do YouTube ausente (YOUTUBE_API_KEY)")]
    ChaveApiAusente,

    /// O recurso pedido não existe.
    #[error("canal não encontrado")]
    NaoEncontrado,

    /// O recurso existe mas não está disponível para consulta.
    #[error("recurso não disponível")]
    Indisponivel,

    /// A quota diária da API foi esgotada.
    #[error("quota da API esgotada")]
    QuotaExcedida,

    /// A resposta da API não respeita o formato esperado.
    #[error("resposta inválida da API: {0}")]
    RespostaInvalida(String),

    /// Falha de transporte ou de construção do cliente HTTP.
    #[error("erro HTTP: {0}")]
    Http(#[from] reqwest::Error),

    /// Falha ao interpretar uma data ISO 8601.
    #[error("erro de parse de data: {0}")]
    Data(#[from] chrono::ParseError),

    /// A referência de canal fornecida não é reconhecida.
    #[error("URL de canal não reconhecida: {0}")]
    UrlInvalida(String),
}

impl From<ColetaError> for ErroDominio {
    fn from(error: ColetaError) -> Self {
        match error {
            ColetaError::UrlInvalida(mensagem) => Self::DadosInvalidos(mensagem),
            outro => Self::Coleta(outro.to_string()),
        }
    }
}
