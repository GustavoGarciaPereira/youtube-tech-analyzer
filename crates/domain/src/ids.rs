use std::fmt;

use serde::{Deserialize, Serialize};

/// Define um identificador opaco baseado em `String`.
macro_rules! definir_id {
    ($(#[$meta:meta])* $nome:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $nome(String);

        impl $nome {
            /// Cria um identificador a partir de um valor textual.
            #[must_use]
            pub fn new(valor: impl Into<String>) -> Self {
                Self(valor.into())
            }

            /// Devolve o valor textual do identificador.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $nome {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $nome {
            fn from(valor: String) -> Self {
                Self(valor)
            }
        }

        impl From<&str> for $nome {
            fn from(valor: &str) -> Self {
                Self(valor.to_owned())
            }
        }

        impl AsRef<str> for $nome {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

definir_id! {
    /// Identificador único de um canal do YouTube.
    CanalId
}

definir_id! {
    /// Identificador único de um vídeo do YouTube.
    VideoId
}

definir_id! {
    /// Identificador único de um comentário do YouTube.
    ComentarioId
}
