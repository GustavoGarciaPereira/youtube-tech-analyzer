use domain::ErroDominio;

/// Erros do motor de NLP.
#[derive(Debug, thiserror::Error)]
pub enum ErroNlp {
    /// Falha de leitura ou escrita de ficheiros.
    #[error("falha de IO: {0}")]
    Io(#[from] std::io::Error),

    /// Falha de rede durante o download.
    #[error("falha de rede ao obter {url}: {fonte}")]
    Rede {
        /// URL pedido.
        url: String,
        /// Descrição do erro de rede.
        fonte: String,
    },

    /// Resposta HTTP não bem-sucedida.
    #[error("resposta HTTP {status} ao obter {url}")]
    Http {
        /// URL pedido.
        url: String,
        /// Código de estado.
        status: u16,
    },

    /// O ficheiro descarregado ou existente não é utilizável.
    #[error("ficheiro inválido em {caminho}: {motivo}")]
    FicheiroInvalido {
        /// Caminho do ficheiro ou pasta.
        caminho: String,
        /// Motivo da rejeição.
        motivo: String,
    },

    /// Falha do tokenizer.
    #[error("erro do tokenizer: {0}")]
    Tokenizer(String),

    /// Falha do ONNX Runtime.
    #[error("erro do ONNX Runtime: {0}")]
    Ort(String),

    /// O modelo não expõe as entradas/saídas esperadas.
    #[error("contrato do modelo inválido: {0}")]
    ContratoModelo(String),

    /// Falha durante a inferência.
    #[error("falha na inferência: {0}")]
    Inferencia(String),
}

impl From<ErroNlp> for ErroDominio {
    fn from(erro: ErroNlp) -> Self {
        Self::Sentimento(erro.to_string())
    }
}
