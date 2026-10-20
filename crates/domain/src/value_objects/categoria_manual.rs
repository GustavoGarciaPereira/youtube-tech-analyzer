use serde::{Deserialize, Serialize};

/// Categoria manual atribuída a um canal.
///
/// É independente do nicho classificado por LLM; serve para organizar a
/// comparação de pares no relatório.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoriaManual {
    /// Programação e desenvolvimento de software.
    Programacao,
    /// Programação com foco educativo.
    ProgramacaoEducacao,
    /// Tecnologia geral.
    Tecnologia,
    /// Fora das categorias acompanhadas.
    Outro,
}

const TODAS: [CategoriaManual; 4] = [
    CategoriaManual::Programacao,
    CategoriaManual::ProgramacaoEducacao,
    CategoriaManual::Tecnologia,
    CategoriaManual::Outro,
];

impl CategoriaManual {
    /// Representação snake_case canónica.
    #[must_use]
    pub const fn como_str(&self) -> &'static str {
        match self {
            Self::Programacao => "programacao",
            Self::ProgramacaoEducacao => "programacao_educacao",
            Self::Tecnologia => "tecnologia",
            Self::Outro => "outro",
        }
    }

    /// Interpreta apenas os valores snake_case canónicos.
    #[must_use]
    pub fn de_str(valor: &str) -> Option<Self> {
        match valor {
            "programacao" => Some(Self::Programacao),
            "programacao_educacao" => Some(Self::ProgramacaoEducacao),
            "tecnologia" => Some(Self::Tecnologia),
            "outro" => Some(Self::Outro),
            _ => None,
        }
    }

    /// Todas as categorias válidas.
    #[must_use]
    pub fn todas() -> &'static [Self] {
        &TODAS
    }
}

#[cfg(test)]
mod tests {
    use super::CategoriaManual;

    #[test]
    fn todas_as_variantes_tem_ida_e_volta() {
        for categoria in CategoriaManual::todas() {
            assert_eq!(
                CategoriaManual::de_str(categoria.como_str()),
                Some(*categoria)
            );
        }
        assert_eq!(CategoriaManual::todas().len(), 4);
    }

    #[test]
    fn rejeita_valores_nao_canonicos() {
        assert_eq!(CategoriaManual::de_str("Programação"), None);
        assert_eq!(CategoriaManual::de_str("programacao "), None);
        assert_eq!(CategoriaManual::de_str(""), None);
        assert_eq!(CategoriaManual::de_str("programação"), None);
    }
}
