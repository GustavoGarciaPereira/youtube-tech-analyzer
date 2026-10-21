use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Preferências persistidas da GUI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Preferencias {
    /// Último canal selecionado.
    pub ultimo_canal: Option<String>,
    /// Último ecrã aberto.
    pub ultimo_ecra: Option<String>,
    /// Formato de exportação por omissão.
    pub formato_export_default: Option<String>,
    /// Período de análise por omissão.
    pub periodo_dias_default: Option<u32>,
    /// Modo de análise por omissão (`por_coleta` ou `por_publicacao`).
    pub modo_analise_default: Option<String>,
    /// Escala logarítmica no gráfico de tendência.
    #[serde(default)]
    pub log_scale: bool,
}

/// Caminho de `prefs.toml`.
#[must_use]
pub fn caminho() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("yta").join("prefs.toml");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("yta")
            .join("prefs.toml");
    }
    PathBuf::from("prefs.toml")
}

/// Carrega as preferências; qualquer erro devolve os valores por omissão.
#[must_use]
pub fn carregar() -> Preferencias {
    let caminho = caminho();
    let Ok(texto) = std::fs::read_to_string(&caminho) else {
        return Preferencias::default();
    };
    toml::from_str(&texto).unwrap_or_default()
}

impl Preferencias {
    /// Guarda as preferências no caminho configurado.
    pub fn guardar(&self) -> anyhow::Result<()> {
        let caminho = caminho();
        if let Some(parent) = caminho.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("criar diretório {}", parent.display()))?;
        }
        let texto = toml::to_string_pretty(self).context("serializar preferências")?;
        std::fs::write(&caminho, texto)
            .with_context(|| format!("escrever {}", caminho.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Preferencias;

    #[test]
    fn log_scale_faz_round_trip() {
        let prefs = Preferencias {
            log_scale: true,
            ..Preferencias::default()
        };
        let texto = toml::to_string_pretty(&prefs).expect("serializar preferências");
        let lido: Preferencias = toml::from_str(&texto).expect("desserializar preferências");
        assert!(lido.log_scale);
    }

    #[test]
    fn prefs_antigas_sem_log_scale_continuam_validas() {
        let lido: Preferencias = toml::from_str(
            "periodo_dias_default = 365
",
        )
        .expect("prefs antigas");
        assert!(!lido.log_scale);
        assert_eq!(lido.periodo_dias_default, Some(365));
    }
}
