use serde::{Deserialize, Serialize};

/// Referência normalizada a um canal do YouTube.
///
/// O parsing de URLs e handles pertence à infraestrutura; este value object
/// existe para que a camada `application` não dependa de `infra-youtube`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanalRef {
    /// Handle moderno, sem o prefixo `@`.
    Handle(String),
    /// URL personalizada legada (`/c/Nome`).
    Custom(String),
    /// Nome de utilizador legado (`/user/Nome`).
    Username(String),
    /// Identificador de canal (`UC...`).
    ChannelId(String),
}
