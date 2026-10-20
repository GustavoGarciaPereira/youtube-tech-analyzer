//! Adaptador YouTube Data API v3 para a porta
//! [`domain::ports::ColetorDeCanal`].
//!
//! O MVP usa exclusivamente a API v3. A porta fica aberta para uma segunda
//! implementação baseada em InnerTube, sem alterar o domínio nem a aplicação.
//! A persistência não pertence a este crate: os métodos apenas devolvem as
//! entidades recolhidas.

#![forbid(unsafe_code)]

mod api_v3;
mod errors;
mod url;

pub use api_v3::{ApiV3Coletor, MAX_DEPTH_PADRAO};
pub use domain::CanalRef;
pub use errors::ColetaError;
pub use url::parse as parse_canal_ref;
