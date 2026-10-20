//! Modelo do relatório e exportadores.

pub mod export;

use analysis::Direcao;
use chrono::{DateTime, NaiveDate, Utc};
use domain::entities::{canal::Canal, snapshot::Metrica};
use serde::{Deserialize, Serialize};

/// Tendência de uma métrica de um canal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TendenciaMetrica {
    /// Métrica analisada.
    pub metrica: Metrica,
    /// Número de snapshots usados.
    pub n_snapshots: usize,
    /// Direção inferida pelo Mann-Kendall.
    pub direcao: Direcao,
    /// Tau de Kendall.
    pub tau: f64,
    /// p-value do Mann-Kendall.
    pub p_value: f64,
    /// p-value ajustado por Benjamini-Hochberg.
    pub p_ajustado_fdr: f64,
    /// Declive Theil-Sen em unidades por snapshot.
    pub slope_theil_sen: f64,
    /// IC95 do declive.
    pub slope_ic95: (f64, f64),
    /// `true` quando `n_snapshots >= aviso_amostra_minima`.
    pub amostra_suficiente: bool,
}

/// Comparação do declive do canal com os pares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparacaoPares {
    /// `"todos"` ou o nome do nicho usado no filtro.
    pub grupo: String,
    /// Número de canais pares com série utilizável.
    pub n_pares: usize,
    /// Mediana dos declives dos pares.
    pub slope_mediano_pares: f64,
    /// Declive do canal alvo.
    pub slope_canal: f64,
    /// Percentil do canal entre os pares, de `0.0` a `100.0`.
    pub percentil_canal_entre_pares: f64,
}

/// Relatório completo de um canal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelatorioCanal {
    /// Canal analisado.
    pub canal: Canal,
    /// Primeira data efetivamente usada.
    pub periodo_inicio: NaiveDate,
    /// Última data efetivamente usada.
    pub periodo_fim: NaiveDate,
    /// Tendências por métrica.
    pub tendencias: Vec<TendenciaMetrica>,
    /// Comparações com canais pares.
    pub comparacoes: Vec<ComparacaoPares>,
    /// Avisos metodológicos.
    pub avisos: Vec<String>,
    /// Metadados de reprodutibilidade.
    pub metadados: MetadadosReproducao,
    /// Nível de significância usado no relatório.
    pub nivel_significancia: f64,
    /// Limiar de amostra mínima usado no relatório.
    pub aviso_amostra_minima: usize,
}

/// Metadados para reproduzir o relatório.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadadosReproducao {
    /// Hash curto do commit que gerou o binário.
    pub hash_commit: String,
    /// Versão do schema SQLite.
    pub versao_schema: String,
    /// Comando exato usado para gerar o relatório.
    pub comando: Option<String>,
    /// Instante de geração.
    pub gerado_em: DateTime<Utc>,
    /// Versão da aplicação.
    pub versao_app: String,
}

impl MetadadosReproducao {
    /// Constrói metadados com o hash do build e a versão da aplicação.
    #[must_use]
    pub fn new(versao_schema: impl Into<String>, comando: Option<String>) -> Self {
        Self {
            hash_commit: env!("GIT_HASH").to_owned(),
            versao_schema: versao_schema.into(),
            comando,
            gerado_em: Utc::now(),
            versao_app: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// Configuração do relatório.
#[derive(Debug, Clone)]
pub struct RelatorioConfig {
    /// Métricas a analisar.
    pub metricas: Vec<Metrica>,
    /// Limita a série aos últimos N dias.
    pub periodo_dias: Option<u32>,
    /// Mínimo de snapshots para incluir uma métrica.
    pub min_snapshots: usize,
    /// Nível de significância do MK e do FDR.
    pub nivel_significancia: f64,
    /// Abaixo deste N, o relatório emite aviso de amostra insuficiente.
    pub aviso_amostra_minima: usize,
}

impl Default for RelatorioConfig {
    fn default() -> Self {
        Self {
            metricas: vec![
                Metrica::Visualizacoes,
                Metrica::Curtidas,
                Metrica::TotalComentarios,
            ],
            periodo_dias: None,
            min_snapshots: 5,
            nivel_significancia: 0.05,
            aviso_amostra_minima: 20,
        }
    }
}
