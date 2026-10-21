//! Portas de entrada e saída do domínio.
//!
//! As portas são traits puros. As implementações concretas vivem nos crates
//! `infra-*` e são injetadas pelo *composition root* (`desktop`).

use std::future::Future;
use std::pin::Pin;

pub mod analisador_sentimento;
pub mod cache_classificacoes;
pub mod classificador;
pub mod coletor;
pub mod repositorio;

/// Future boxed usado nas portas assíncronas object-safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub use analisador_sentimento::{AnalisadorDeSentimento, VERSAO_MODELO_SENTIMENTO};
pub use cache_classificacoes::CacheDeClassificacoes;
pub use classificador::ClassificadorDeNicho;
pub use coletor::ColetorDeCanal;
pub use repositorio::RepositorioDeCanais;
