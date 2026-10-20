use domain::ErroDominio;
use thiserror::Error;

/// Erros internos do adaptador SQLite.
///
/// Na fronteira da porta, estes erros são convertidos para [`ErroDominio`] por
/// [`From`], mantendo o domínio independente de `rusqlite`.
#[derive(Debug, Error)]
pub enum Error {
    /// Falha devolvida pelo SQLite.
    #[error("falha na base de dados SQLite: {0}")]
    Database(#[from] rusqlite::Error),

    /// Falha ao serializar ou desserializar um campo JSON.
    #[error("falha ao processar JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// Erro de domínio produzido durante o mapeamento de uma linha.
    #[error(transparent)]
    Domain(#[from] ErroDominio),

    /// O `Mutex` da conexão ficou envenenado após um panic.
    #[error("falha ao obter o lock da conexão SQLite")]
    Lock,

    /// Um valor persistido não respeita o tipo ou o intervalo esperado.
    #[error("dados inválidos na base de dados: {0}")]
    InvalidData(String),
}

impl From<Error> for ErroDominio {
    fn from(error: Error) -> Self {
        match error {
            Error::Domain(error) => error,
            Error::Database(error) => Self::Repositorio(error.to_string()),
            Error::Json(error) => Self::Repositorio(error.to_string()),
            Error::Lock => {
                Self::Repositorio("conexão SQLite bloqueada por um panic anterior".to_owned())
            }
            Error::InvalidData(message) => Self::DadosInvalidos(message),
        }
    }
}
