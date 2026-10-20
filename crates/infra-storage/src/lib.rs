//! Adaptador SQLite para a porta [`domain::ports::RepositorioDeCanais`].
//!
//! O schema é criado de forma idempotente a partir de
//! `migrations/schema.sql`. A conexão é protegida por um `Mutex` porque
//! `rusqlite::Connection` não é `Sync`; os métodos da porta são síncronos e
//! podem ser chamados a partir de `tokio::task::spawn_blocking`.

#![forbid(unsafe_code)]

/// Versão do schema SQLite. Incrementar quando `migrations/schema.sql`
/// mudar de forma incompatível.
pub const VERSAO_SCHEMA: &str = "1";

mod error;
mod mapeamento;
mod repositorio;

pub use error::Error;
pub use repositorio::SqliteRepositorio;
