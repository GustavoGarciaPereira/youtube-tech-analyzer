use thiserror::Error;

/// Erros do crate de análise.
#[derive(Debug, Error)]
pub enum AnalysisError {
    /// A série tem menos de três pontos.
    #[error("série demasiado curta: {n} pontos (mínimo 3)")]
    SerieCurta {
        /// Número de pontos recebidos.
        n: usize,
    },

    /// Um p-value está fora do intervalo válido ou não é finito.
    #[error("p-value inválido: {0}")]
    PValueInvalido(f64),
}
