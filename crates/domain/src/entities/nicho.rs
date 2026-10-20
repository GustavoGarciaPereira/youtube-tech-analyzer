use serde::{Deserialize, Serialize};

use crate::errors::ErroDominio;
use crate::ids::VideoId;

/// Nicho atribuído a um vídeo pelo classificador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nicho {
    /// Conteúdo centrado em programação e desenvolvimento de software.
    Programacao,
    /// Conteúdo educativo, tutoriais e explicações.
    Educacao,
    /// Conteúdo tecnológico geral que não se enquadra nos nichos anteriores.
    Tecnologia,
    /// Conteúdo fora dos nichos acompanhados pelo projeto.
    Outro,
}

impl Nicho {
    /// Representação estável usada na persistência e nos relatórios.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Programacao => "programacao",
            Self::Educacao => "educacao",
            Self::Tecnologia => "tecnologia",
            Self::Outro => "outro",
        }
    }
}

impl TryFrom<&str> for Nicho {
    type Error = ErroDominio;

    fn try_from(valor: &str) -> Result<Self, Self::Error> {
        match valor {
            "programacao" => Ok(Self::Programacao),
            "educacao" => Ok(Self::Educacao),
            "tecnologia" => Ok(Self::Tecnologia),
            "outro" => Ok(Self::Outro),
            outro => Err(ErroDominio::DadosInvalidos(format!(
                "nicho desconhecido: {outro}"
            ))),
        }
    }
}

/// Resultado da classificação de um vídeo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificacaoNicho {
    /// Vídeo classificado.
    pub video_id: VideoId,
    /// Nicho atribuído.
    pub nicho: Nicho,
    /// Confiança do classificador, entre `0.0` e `1.0`.
    pub confianca: f32,
    /// Método ou modelo usado na classificação.
    pub metodo: String,
    /// Data da classificação em formato RFC 3339.
    pub data_classificacao: String,
}
