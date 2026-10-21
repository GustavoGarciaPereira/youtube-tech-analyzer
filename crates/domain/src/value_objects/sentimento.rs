use serde::{Deserialize, Serialize};

use crate::errors::ErroDominio;

/// Polaridade de sentimento atribuída a um texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentimento {
    /// Texto com valência positiva.
    Positivo,
    /// Texto sem valência clara ou misto.
    Neutro,
    /// Texto com valência negativa.
    Negativo,
}

impl Sentimento {
    /// Representação snake_case canónica, usada na persistência.
    #[must_use]
    pub const fn como_str(&self) -> &'static str {
        match self {
            Self::Positivo => "positivo",
            Self::Neutro => "neutro",
            Self::Negativo => "negativo",
        }
    }

    /// Interpreta apenas os valores snake_case canónicos.
    #[must_use]
    pub fn de_str(valor: &str) -> Option<Self> {
        match valor {
            "positivo" => Some(Self::Positivo),
            "neutro" => Some(Self::Neutro),
            "negativo" => Some(Self::Negativo),
            _ => None,
        }
    }
}

impl TryFrom<&str> for Sentimento {
    type Error = ErroDominio;

    fn try_from(valor: &str) -> Result<Self, Self::Error> {
        Self::de_str(valor)
            .ok_or_else(|| ErroDominio::DadosInvalidos(format!("sentimento desconhecido: {valor}")))
    }
}

/// Resultado da análise de sentimento de um comentário.
///
/// O score é contínuo em [-1, 1] e permite distinguir, por exemplo, um
/// "positivo forte" de um "positivo ligeiro". A polaridade discreta é a
/// classificação que o relatório usa; o score fica para agregações.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScoreSentimento {
    /// Polaridade discreta.
    pub sentimento: Sentimento,
    /// Intensidade de -1.0 (muito negativo) a 1.0 (muito positivo).
    pub score: f64,
}

impl ScoreSentimento {
    /// Cria um resultado de análise.
    #[must_use]
    pub fn novo(sentimento: Sentimento, score: f64) -> Self {
        Self { sentimento, score }
    }

    /// Indica se o score é finito e está dentro de [-1, 1].
    #[must_use]
    pub fn e_valido(&self) -> bool {
        self.score.is_finite() && (-1.0..=1.0).contains(&self.score)
    }
}

#[cfg(test)]
mod tests {
    use super::{ScoreSentimento, Sentimento};

    #[test]
    fn sentimento_faz_round_trip() {
        for sentimento in [
            Sentimento::Positivo,
            Sentimento::Neutro,
            Sentimento::Negativo,
        ] {
            assert_eq!(Sentimento::de_str(sentimento.como_str()), Some(sentimento));
            assert_eq!(
                Sentimento::try_from(sentimento.como_str()).expect("variante canónica"),
                sentimento
            );
        }
    }

    #[test]
    fn sentimento_rejeita_valores_desconhecidos() {
        assert_eq!(Sentimento::de_str("Positivo"), None);
        assert_eq!(Sentimento::de_str("positivo "), None);
        assert_eq!(Sentimento::de_str(""), None);
        assert!(Sentimento::try_from("mixed").is_err());
    }

    #[test]
    fn score_fora_do_intervalo_e_invalido() {
        let valido = ScoreSentimento::novo(Sentimento::Positivo, 0.5);
        assert!(valido.e_valido());

        let fora = ScoreSentimento::novo(Sentimento::Negativo, -1.5);
        assert!(!fora.e_valido());

        let nan = ScoreSentimento::novo(Sentimento::Neutro, f64::NAN);
        assert!(!nan.e_valido());
    }
}
