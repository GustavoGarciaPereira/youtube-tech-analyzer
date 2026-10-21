//! Modelo do relatório e exportadores.

pub mod export;

use analysis::Direcao;
use chrono::{DateTime, NaiveDate, Utc};
use domain::entities::{canal::Canal, snapshot::Metrica};
use serde::{Deserialize, Serialize};

/// Eixo temporal usado na análise de tendência.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModoAnalise {
    /// Série temporal por dia de coleta (`snapshots.data_coleta`).
    ///
    /// É o modo por omissão e responde a "o canal está a crescer por dia?".
    #[default]
    PorColeta,
    /// Cross-section por data de publicação de cada vídeo (`videos.publicado_em`).
    ///
    /// Cada vídeo é um ponto. Responde à mesma pergunta — "o canal está a
    /// crescer ou a cair?" — mas com um único snapshot, desde que existam
    /// vídeos publicados em datas diferentes.
    PorPublicacao,
}

impl ModoAnalise {
    /// Representação estável usada no relatório e na CLI.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::PorColeta => "por_coleta",
            Self::PorPublicacao => "por_publicacao",
        }
    }

    /// Nome do eixo temporal usado no relatório.
    #[must_use]
    pub const fn eixo_temporal(self) -> &'static str {
        match self {
            Self::PorColeta => "data_coleta",
            Self::PorPublicacao => "data_publicacao",
        }
    }

    /// Interpreta a representação persistida.
    #[must_use]
    pub fn de_str(valor: &str) -> Option<Self> {
        match valor {
            "por_coleta" => Some(Self::PorColeta),
            "por_publicacao" => Some(Self::PorPublicacao),
            _ => None,
        }
    }
}

impl std::fmt::Display for ModoAnalise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.como_str())
    }
}

/// Tendência de uma métrica de um canal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TendenciaMetrica {
    /// Métrica analisada.
    pub metrica: Metrica,
    /// Número de pontos usados: snapshots (`PorColeta`) ou vídeos (`PorPublicacao`).
    pub n_snapshots: usize,
    /// Direção inferida pelo Mann-Kendall.
    pub direcao: Direcao,
    /// Tau de Kendall.
    pub tau: f64,
    /// p-value do Mann-Kendall.
    pub p_value: f64,
    /// p-value ajustado por Benjamini-Hochberg.
    pub p_ajustado_fdr: f64,
    /// Declive Theil-Sen na unidade indicada por `unidade_slope`.
    pub slope_theil_sen: f64,
    /// Intercepto Theil-Sen no índice dos pontos.
    pub intercept_theil_sen: f64,
    /// IC95 do declive.
    pub slope_ic95: (f64, f64),
    /// Série efetivamente usada, para visualização.
    pub pontos: Vec<(NaiveDate, f64)>,
    /// `true` quando `n_snapshots >= aviso_amostra_minima`.
    pub amostra_suficiente: bool,
    /// Eixo temporal dos pontos: `"data_coleta"` ou `"data_publicacao"`.
    pub eixo_temporal: String,
    /// Unidade do declive, legível (ex.: `"por dia"`, `"views/dia por vídeo"`).
    pub unidade_slope: String,
}

/// Comparação do declive do canal com os pares.
///
/// Os declives absolutos são dominados pelo tamanho do canal; o percentil é
/// calculado sobre declives relativos (declive / mediana da série do próprio
/// canal), para comparar trajectórias e não escalas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparacaoPares {
    /// "todos" ou o nome do nicho usado no filtro.
    pub grupo: String,
    /// Número de canais pares com série utilizável.
    pub n_pares: usize,
    /// Mediana dos declives absolutos dos pares (contexto, não usado no percentil).
    pub slope_mediano_pares: f64,
    /// Declive absoluto do canal alvo.
    pub slope_canal: f64,
    /// Mediana da série do canal alvo, usada como factor de normalização.
    pub fator_normalizacao: f64,
    /// Declive do canal dividido por fator_normalizacao.
    pub slope_canal_relativo: f64,
    /// Mediana dos declives relativos dos pares.
    pub slope_mediano_pares_relativo: f64,
    /// Percentil do canal entre os pares, de 0.0 a 100.0.
    ///
    /// None quando há menos de 10 pares: com amostras menores o percentil
    /// só pode assumir uns poucos valores e não é fiável.
    pub percentil_canal_entre_pares: Option<f64>,
}

/// Relatório completo de um canal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelatorioCanal {
    /// Canal analisado.
    pub canal: Canal,
    /// Primeira data efetivamente usada (coleta ou publicação, conforme o modo).
    pub periodo_inicio: NaiveDate,
    /// Última data efetivamente usada (coleta ou publicação, conforme o modo).
    pub periodo_fim: NaiveDate,
    /// Modo de análise usado.
    pub modo: ModoAnalise,
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
    ///
    /// Em `PorColeta` filtra por `data_coleta`; em `PorPublicacao` filtra por
    /// `publicado_em`. Por omissão não há corte.
    pub periodo_dias: Option<u32>,
    /// Mínimo de pontos para incluir uma métrica.
    pub min_snapshots: usize,
    /// Nível de significância do MK e do FDR.
    pub nivel_significancia: f64,
    /// Abaixo deste N, o relatório emite aviso de amostra insuficiente.
    pub aviso_amostra_minima: usize,
    /// Eixo temporal da análise.
    pub modo: ModoAnalise,
    /// Idade mínima, em dias, para um vídeo entrar na cross-section.
    ///
    /// Só se aplica a `PorPublicacao`. Evita vídeos que ainda não
    /// estabilizaram as métricas.
    pub idade_minima_dias: u32,
    /// Normaliza os valores pela idade do vídeo em `PorPublicacao`.
    ///
    /// Visualizações passam a views/dia desde a publicação; curtidas e
    /// comentários passam a rácio de engagement sobre as visualizações.
    pub views_por_dia: bool,
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
            modo: ModoAnalise::PorColeta,
            idade_minima_dias: 7,
            views_por_dia: true,
        }
    }
}
