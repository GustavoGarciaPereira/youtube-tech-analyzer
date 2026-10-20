use serde::{Deserialize, Serialize};

use crate::errors::ErroDominio;

/// Tipo de entidade a que uma métrica se refere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntidadeTipo {
    /// Métrica agregada de um canal.
    Canal,
    /// Métrica de um vídeo.
    Video,
    /// Métrica de um comentário.
    Comentario,
}

impl EntidadeTipo {
    /// Representação estável usada na persistência e nos relatórios.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Canal => "canal",
            Self::Video => "video",
            Self::Comentario => "comentario",
        }
    }
}

impl TryFrom<&str> for EntidadeTipo {
    type Error = ErroDominio;

    fn try_from(valor: &str) -> Result<Self, Self::Error> {
        match valor {
            "canal" => Ok(Self::Canal),
            "video" => Ok(Self::Video),
            "comentario" => Ok(Self::Comentario),
            outro => Err(ErroDominio::DadosInvalidos(format!(
                "tipo de entidade desconhecido: {outro}"
            ))),
        }
    }
}

/// Métrica observada num snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metrica {
    /// Número de subscritores de um canal.
    Inscritos,
    /// Número total de vídeos de um canal.
    TotalVideos,
    /// Número de visualizações de um vídeo.
    Visualizacoes,
    /// Número de gostos de um vídeo ou comentário.
    Curtidas,
    /// Número total de comentários de um vídeo.
    TotalComentarios,
}

impl Metrica {
    /// Representação estável usada na persistência e nos relatórios.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inscritos => "inscritos",
            Self::TotalVideos => "total_videos",
            Self::Visualizacoes => "visualizacoes",
            Self::Curtidas => "curtidas",
            Self::TotalComentarios => "total_comentarios",
        }
    }
}

impl TryFrom<&str> for Metrica {
    type Error = ErroDominio;

    fn try_from(valor: &str) -> Result<Self, Self::Error> {
        match valor {
            "inscritos" => Ok(Self::Inscritos),
            "total_videos" => Ok(Self::TotalVideos),
            "visualizacoes" => Ok(Self::Visualizacoes),
            "curtidas" => Ok(Self::Curtidas),
            "total_comentarios" => Ok(Self::TotalComentarios),
            outra => Err(ErroDominio::DadosInvalidos(format!(
                "métrica desconhecida: {outra}"
            ))),
        }
    }
}

/// Observação pontual de uma métrica.
///
/// A chave lógica é o par composto
/// (`entidade_tipo`, `entidade_id`, `metrica`, `data_coleta`). Duas execuções
/// no mesmo dia para a mesma métrica representam a mesma observação; dias
/// diferentes produzem linhas históricas distintas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Tipo da entidade observada.
    pub entidade_tipo: EntidadeTipo,
    /// Identificador da entidade observada.
    pub entidade_id: String,
    /// Métrica observada.
    pub metrica: Metrica,
    /// Valor inteiro da métrica no momento da recolha.
    pub valor: i64,
    /// Data da recolha em formato RFC 3339.
    pub data_coleta: String,
}

impl Snapshot {
    /// Cria uma observação de métrica.
    #[must_use]
    pub fn novo(
        entidade_tipo: EntidadeTipo,
        entidade_id: impl Into<String>,
        metrica: Metrica,
        valor: i64,
        data_coleta: impl Into<String>,
    ) -> Self {
        Self {
            entidade_tipo,
            entidade_id: entidade_id.into(),
            metrica,
            valor,
            data_coleta: data_coleta.into(),
        }
    }
}
