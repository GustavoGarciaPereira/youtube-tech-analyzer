use analysis::AnalysisError;
use domain::ErroDominio;
use thiserror::Error;

/// Erros da camada de aplicação.
#[derive(Debug, Error)]
pub enum ErroAplicacao {
    /// Erro propagado de uma porta do domínio.
    #[error("erro de domínio: {0}")]
    Dominio(#[from] ErroDominio),

    /// Erro devolvido pelas primitivas estatísticas.
    #[error("erro de análise: {0}")]
    Analise(#[from] AnalysisError),

    /// O canal ou o período não tem dados suficientes para um relatório.
    #[error("dados insuficientes: {0}")]
    SemDados(String),

    /// A coleta foi cancelada pelo utilizador.
    #[error("coleta cancelada pelo utilizador")]
    Cancelada,

    /// O canal não tem vídeos publicados.
    #[error("canal sem vídeos publicados")]
    CanalSemVideos,

    /// A configuração fornecida é inválida.
    #[error("configuração inválida: {0}")]
    ConfigInvalida(String),
}
