use thiserror::Error;

/// Erros de domínio e de adaptação das portas.
///
/// As implementações de infraestrutura devem traduzir os seus erros concretos
/// (HTTP, SQLite, etc.) para uma destas variantes, mantendo o domínio livre de
/// dependências técnicas.
#[derive(Debug, Error)]
pub enum ErroDominio {
    /// Os dados recebidos não respeitam as regras do domínio.
    #[error("dados inválidos: {0}")]
    DadosInvalidos(String),

    /// O recurso pedido não existe.
    #[error("{recurso} não encontrado: {id}")]
    NaoEncontrado {
        /// Tipo de recurso procurado (por exemplo, `canal`).
        recurso: &'static str,
        /// Identificador usado na procura.
        id: String,
    },

    /// A recolha de dados a partir de uma fonte externa falhou.
    #[error("falha na recolha de dados: {0}")]
    Coleta(String),

    /// A persistência ou leitura de dados falhou.
    #[error("falha no repositório: {0}")]
    Repositorio(String),

    /// A classificação de nicho falhou.
    #[error("falha na classificação de nicho: {0}")]
    Classificacao(String),
}
