use serde::{Deserialize, Serialize};

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

/// Resultado da classificação de um vídeo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificacaoNicho {
    /// Vídeo classificado.
    pub video_id: VideoId,
    /// Nicho atribuído.
    pub nicho: Nicho,
    /// Confiança do classificador, entre `0.0` e `1.0`.
    pub confianca: f32,
}
