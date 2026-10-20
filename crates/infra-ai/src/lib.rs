//! Adaptador de classificação de nicho com cache.
//!
//! O transporte LLM é abstraído por [`ClienteLLM`]. A implementação concreta
//! usa o CLI do OpenCode como subprocesso; o cache chega como
//! `Arc<dyn CacheDeClassificacoes>` e não acopla este crate a `infra-storage`.

#![forbid(unsafe_code)]

pub mod classificador;
pub mod cliente_llm;
pub mod errors;
pub mod hash;
pub mod opencode;
pub mod prompt;

pub use classificador::ClassificadorComCache;
pub use cliente_llm::{ClienteLLM, PedidoClassificacao, RespostaBruta};
pub use errors::ErroIA;
pub use hash::hash_conteudo;
pub use opencode::ClienteOpenCode;
pub use prompt::{Taxonomia, VERSAO_PROMPT};

/// Hash do commit usado no build, útil para logs e diagnóstico.
pub const GIT_HASH: &str = env!("GIT_HASH");
