//! Estatística pura para séries temporais.
//!
//! O crate é agnóstico a datas e a fontes de dados: recebe `&[f64]` em ordem
//! cronológica e devolve coeficientes em "unidades por passo". Não faz IO,
//! não usa rede, base de dados, `tokio` ou `chrono`.

#![forbid(unsafe_code)]

pub mod errors;
pub mod fdr;
pub mod mann_kendall;
pub mod theil_sen;

pub use errors::AnalysisError;
pub use fdr::benjamini_hochberg;
pub use mann_kendall::{Direcao, ResultadoMK, mann_kendall};
pub use theil_sen::{ResultadoTheilSen, theil_sen};
