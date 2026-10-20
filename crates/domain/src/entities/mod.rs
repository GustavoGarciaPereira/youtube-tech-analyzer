//! Entidades e objetos de valor do domínio.

pub mod canal;
pub mod comentario;
pub mod nicho;
pub mod snapshot;
pub mod video;

pub use canal::Canal;
pub use comentario::Comentario;
pub use nicho::{ClassificacaoNicho, Nicho};
pub use snapshot::{EntidadeTipo, Metrica, Snapshot};
pub use video::Video;
