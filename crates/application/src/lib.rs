//! Casos de uso da aplicação.
//!
//! Orquestram as portas definidas em [`domain`] sem conhecer as implementações
//! concretas de infraestrutura.

#![forbid(unsafe_code)]

pub mod config;
pub mod errors;
pub mod eventos;
pub mod relatorio;
pub mod use_cases;

pub use config::ColetaConfig;
pub use errors::ErroAplicacao;
pub use eventos::{EventoProgresso, FalhaVideo, RelatorioColeta};
pub use relatorio::{
    ComparacaoPares, MetadadosReproducao, ModoAnalise, RelatorioCanal, RelatorioConfig,
    TendenciaMetrica,
};
pub use use_cases::{ColetarCanalUseCase, GerarRelatorioUseCase};
