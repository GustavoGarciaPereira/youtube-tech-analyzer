//! Núcleo do domínio do YouTube Tech Analyzer.
//!
//! Este crate contém apenas entidades, objetos de valor, erros e portas.
//! Não conhece rede, base de dados, GUI ou runtime assíncrono concreto.

#![forbid(unsafe_code)]

pub mod entities;
pub mod errors;
pub mod ids;
pub mod ports;

pub use entities::{Canal, ClassificacaoNicho, Comentario, Nicho, Snapshot, Video};
pub use errors::ErroDominio;
pub use ids::{CanalId, ComentarioId, VideoId};
pub use ports::{ClassificadorDeNicho, ColetorDeCanal, RepositorioDeCanais};
