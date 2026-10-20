//! Portas de entrada e saída do domínio.
//!
//! As portas são traits puros. As implementações concretas vivem nos crates
//! `infra-*` e são injetadas pelo *composition root* (`desktop`).

pub mod classificador;
pub mod coletor;
pub mod repositorio;

pub use classificador::ClassificadorDeNicho;
pub use coletor::ColetorDeCanal;
pub use repositorio::RepositorioDeCanais;
