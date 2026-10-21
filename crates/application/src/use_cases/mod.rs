//! Casos de uso da aplicação.

pub mod analisar_sentimentos;
pub mod coletar_canal;
pub mod gerar_relatorio;

pub use analisar_sentimentos::{AnalisarConfig, AnalisarSentimentosUseCase, RelatorioSentimentos};
pub use coletar_canal::ColetarCanalUseCase;
pub use gerar_relatorio::GerarRelatorioUseCase;
