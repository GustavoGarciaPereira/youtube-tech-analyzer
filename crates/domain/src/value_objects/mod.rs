//! Objetos de valor do domínio.

pub mod categoria_manual;
pub mod sentimento;

pub use categoria_manual::CategoriaManual;
pub use sentimento::{ScoreSentimento, Sentimento};
