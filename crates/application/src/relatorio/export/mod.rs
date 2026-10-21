use std::fmt::Write as _;

use analysis::Direcao;
use domain::entities::snapshot::Metrica;

use super::{ComparacaoPares, ModoAnalise, RelatorioCanal, TendenciaMetrica};

/// Exporta o relatório em Markdown.
#[must_use]
pub fn para_markdown(relatorio: &RelatorioCanal) -> String {
    let mut saida = String::new();

    let _ = writeln!(saida, "# Relatório: {}", relatorio.canal.nome);
    let _ = writeln!(saida);
    let _ = writeln!(
        saida,
        "> Gerado em {} · commit `{}` · schema `{}`",
        relatorio.metadados.gerado_em.format("%Y-%m-%d %H:%M UTC"),
        relatorio.metadados.hash_commit,
        relatorio.metadados.versao_schema
    );
    let comando = relatorio
        .metadados
        .comando
        .as_deref()
        .unwrap_or("não disponível");
    let _ = writeln!(saida, "> Reproduzir: `{comando}`");
    let _ = writeln!(saida);
    let _ = writeln!(
        saida,
        "> Eixo temporal: `{}` (modo `{}`)",
        relatorio.modo.eixo_temporal(),
        relatorio.modo.como_str()
    );
    let _ = writeln!(saida);

    let _ = writeln!(saida, "## Sumário");
    let _ = writeln!(saida);
    let _ = writeln!(saida, "{}", texto_sumario(relatorio));
    let _ = writeln!(saida);

    let _ = writeln!(saida, "## Tendência por Métrica");
    let _ = writeln!(saida);
    let _ = writeln!(
        saida,
        "| Métrica | N | Direção | τ (Kendall) | p | p (FDR) | Slope (IC95, {}) |",
        unidade_cabecalho(relatorio.modo)
    );
    let _ = writeln!(saida, "|---|---|---|---|---|---|---|");
    for tendencia in &relatorio.tendencias {
        let _ = writeln!(
            saida,
            "| {} | {} | {} | {:.4} | {:.4} | {:.4} | {} ({}, {}) {} |",
            tendencia.metrica.as_str(),
            tendencia.n_snapshots,
            direcao_str(tendencia.direcao),
            tendencia.tau,
            tendencia.p_value,
            tendencia.p_ajustado_fdr,
            formatar_numero(tendencia.slope_theil_sen),
            formatar_numero(tendencia.slope_ic95.0),
            formatar_numero(tendencia.slope_ic95.1),
            tendencia.unidade_slope
        );
    }
    let _ = writeln!(saida);

    if relatorio.modo == ModoAnalise::PorPublicacao
        && relatorio
            .tendencias
            .iter()
            .any(|tendencia| tendencia.metrica == Metrica::Visualizacoes)
    {
        let _ = writeln!(
            saida,
            "> Nota: views/dia é um proxy com viés conhecido — vídeos recentes têm as visualizações concentradas nos primeiros dias, o que infla o slope. O rácio likes/views é independente da idade e é o sinal mais robusto numa cross-section."
        );
        let _ = writeln!(saida);
    }

    let _ = writeln!(saida, "## Comparação com Pares");
    let _ = writeln!(saida);
    if relatorio.comparacoes.is_empty() {
        let _ = writeln!(saida, "Sem pares disponíveis para comparação.");
    } else {
        for comparacao in &relatorio.comparacoes {
            let _ = writeln!(saida, "{}", texto_comparacao(comparacao));
        }
    }
    let _ = writeln!(saida);

    let _ = writeln!(saida, "## Avisos");
    let _ = writeln!(saida);
    if relatorio.avisos.is_empty() {
        let _ = writeln!(saida, "Nenhum.");
    } else {
        for aviso in &relatorio.avisos {
            let _ = writeln!(saida, "- {aviso}");
        }
    }
    let _ = writeln!(saida);

    let _ = writeln!(saida, "## Limitações");
    let _ = writeln!(saida);
    let _ = writeln!(saida, "- Fonte: YouTube Data API v3.");
    let _ = writeln!(
        saida,
        "- Teste estatístico: Mann-Kendall bicaudal (não paramétrico, robusto a outliers)."
    );
    let _ = writeln!(
        saida,
        "- Correção de comparações múltiplas: Benjamini-Hochberg (FDR), α = {}.",
        relatorio.nivel_significancia
    );
    let _ = writeln!(
        saida,
        "- Intervalo de confiança do slope: aproximação analítica de Sen (determinística, não bootstrap)."
    );
    let _ = writeln!(
        saida,
        "- N mínimo de pontos para inferência preditiva: {}.",
        relatorio.aviso_amostra_minima
    );
    match relatorio.modo {
        ModoAnalise::PorColeta => {
            let _ = writeln!(
                saida,
                "- O eixo X é a data de coleta: o slope mede a variação por dia de observação."
            );
        }
        ModoAnalise::PorPublicacao => {
            let _ = writeln!(
                saida,
                "- O eixo X é a data de publicação: o slope mede a variação por vídeo publicado, não por dia."
            );
            let _ = writeln!(
                saida,
                "- Cross-section: vídeos de idades diferentes têm maturações diferentes. A normalização por idade (views/dia, rácio de engagement) reduz, mas não elimina, este viés."
            );
            let _ = writeln!(
                saida,
                "- As métricas de cada vídeo são as do último snapshot; vídeos nunca re-coletados mantêm valores antigos."
            );
        }
    }

    saida
}

/// Exporta o relatório em CSV, com uma linha por métrica.
///
/// O ficheiro começa com um comentário que documenta o separador decimal.
#[must_use]
pub fn para_csv(relatorio: &RelatorioCanal) -> String {
    let mut saida = String::new();
    let _ = writeln!(saida, "# Valores decimais usam ponto (.).");
    let _ = writeln!(
        saida,
        "canal_id,canal_nome,metrica,n_snapshots,direcao,tau,p_value,p_ajustado_fdr,slope,slope_ic_low,slope_ic_high,amostra_suficiente,periodo_inicio,periodo_fim,eixo_temporal,unidade_slope"
    );

    for tendencia in &relatorio.tendencias {
        let _ = writeln!(
            saida,
            "{},{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{},{}",
            campo_csv(relatorio.canal.id.as_str()),
            campo_csv(&relatorio.canal.nome),
            tendencia.metrica.as_str(),
            tendencia.n_snapshots,
            direcao_str(tendencia.direcao),
            tendencia.tau,
            tendencia.p_value,
            tendencia.p_ajustado_fdr,
            tendencia.slope_theil_sen,
            tendencia.slope_ic95.0,
            tendencia.slope_ic95.1,
            tendencia.amostra_suficiente,
            relatorio.periodo_inicio,
            relatorio.periodo_fim,
            campo_csv(&tendencia.eixo_temporal),
            campo_csv(&tendencia.unidade_slope)
        );
    }

    saida
}

/// Exporta o relatório em JSON indentado.
pub fn para_json(relatorio: &RelatorioCanal) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(relatorio)
}

/// Sumário em texto, adaptado ao eixo temporal.
#[must_use]
pub fn texto_sumario(relatorio: &RelatorioCanal) -> String {
    let total = relatorio.tendencias.len();
    let significativas: Vec<&TendenciaMetrica> = relatorio
        .tendencias
        .iter()
        .filter(|tendencia| tendencia.p_ajustado_fdr < relatorio.nivel_significancia)
        .collect();

    match relatorio.modo {
        ModoAnalise::PorColeta => texto_sumario_coleta(relatorio, total, &significativas),
        ModoAnalise::PorPublicacao => texto_sumario_publicacao(relatorio, total, &significativas),
    }
}

fn texto_sumario_coleta(
    relatorio: &RelatorioCanal,
    total: usize,
    significativas: &[&TendenciaMetrica],
) -> String {
    if total == 0 {
        return format!(
            "No período de {} a {}, não foi possível analisar nenhuma métrica: são necessários pelo menos 5 snapshots em dias distintos por métrica (valor por omissão); snapshots de vídeos do mesmo dia não formam uma série temporal. Ver a secção Avisos.",
            relatorio.periodo_inicio, relatorio.periodo_fim
        );
    }
    if significativas.is_empty() {
        return format!(
            "No período de {} a {}, nenhuma das {} métricas analisadas apresenta tendência estatisticamente significativa (MK, p<{} após FDR).",
            relatorio.periodo_inicio, relatorio.periodo_fim, total, relatorio.nivel_significancia
        );
    }
    format!(
        "No período de {} a {}, {} das {} métricas analisadas apresentam tendência {} estatisticamente significativa (MK, p<{} após FDR).",
        relatorio.periodo_inicio,
        relatorio.periodo_fim,
        significativas.len(),
        total,
        direcao_predominante(significativas),
        relatorio.nivel_significancia
    )
}

fn texto_sumario_publicacao(
    relatorio: &RelatorioCanal,
    total: usize,
    significativas: &[&TendenciaMetrica],
) -> String {
    let n_videos = relatorio
        .tendencias
        .iter()
        .map(|tendencia| tendencia.n_snapshots)
        .max()
        .unwrap_or_default();

    if total == 0 {
        return format!(
            "No período de {} a {}, não foi possível analisar nenhuma métrica por publicação: são necessários pelo menos 5 vídeos com idade suficiente e métricas de vídeo (visualizações, curtidas, comentários). Ver a secção Avisos.",
            relatorio.periodo_inicio, relatorio.periodo_fim
        );
    }
    if significativas.is_empty() {
        return format!(
            "No período de {} a {}, com {n_videos} vídeos publicados, nenhuma das {total} métricas analisadas apresenta tendência estatisticamente significativa (MK, p<{} após FDR).",
            relatorio.periodo_inicio, relatorio.periodo_fim, relatorio.nivel_significancia
        );
    }
    format!(
        "No período de {} a {}, com {n_videos} vídeos publicados, {} das {total} métricas analisadas apresentam tendência {} estatisticamente significativa (MK, p<{} após FDR).",
        relatorio.periodo_inicio,
        relatorio.periodo_fim,
        significativas.len(),
        direcao_predominante(significativas),
        relatorio.nivel_significancia
    )
}

/// Formata coeficientes pequenos sem os reduzir a `0.0000`.
///
/// Rácios de engagement (likes/views) têm declives na ordem de `1e-5`;
/// quatro casas decimais esconderiam o sinal.
#[must_use]
pub fn formatar_numero(valor: f64) -> String {
    let absoluto = valor.abs();
    if absoluto == 0.0 {
        "0".to_owned()
    } else if (0.001..100_000.0).contains(&absoluto) {
        format!("{valor:.4}")
    } else {
        format!("{valor:.4e}")
    }
}

fn unidade_cabecalho(modo: ModoAnalise) -> &'static str {
    match modo {
        ModoAnalise::PorColeta => "por dia",
        ModoAnalise::PorPublicacao => "por vídeo",
    }
}

fn direcao_str(direcao: Direcao) -> &'static str {
    match direcao {
        Direcao::Crescente => "crescente",
        Direcao::Decrescente => "decrescente",
        Direcao::Estavel => "estável",
    }
}

fn direcao_predominante(tendencias: &[&TendenciaMetrica]) -> &'static str {
    if tendencias.is_empty() {
        return "estável";
    }
    if tendencias
        .iter()
        .all(|tendencia| tendencia.direcao == Direcao::Crescente)
    {
        "crescente"
    } else if tendencias
        .iter()
        .all(|tendencia| tendencia.direcao == Direcao::Decrescente)
    {
        "decrescente"
    } else {
        "mista"
    }
}

/// Frase que descreve a comparação com pares (percentil relativo ou aviso).
#[must_use]
pub fn texto_comparacao(comparacao: &ComparacaoPares) -> String {
    let grupo = if comparacao.grupo == "todos" {
        "na base de dados".to_owned()
    } else if let Some(categoria) = comparacao.grupo.strip_prefix("categoria:") {
        format!("da categoria '{}'", categoria)
    } else {
        format!("do grupo '{}'", comparacao.grupo)
    };
    match comparacao.percentil_canal_entre_pares {
        Some(percentil) => format!(
            "O slope do canal ({:.4}) está no percentil {:.1} face a {} canais {}. Normalizado pela mediana da série ({:.4}), o slope do canal é {:.4} e a mediana dos pares é {:.4}.",
            comparacao.slope_canal,
            percentil,
            comparacao.n_pares,
            grupo,
            comparacao.fator_normalizacao,
            comparacao.slope_canal_relativo,
            comparacao.slope_mediano_pares_relativo
        ),
        None => format!(
            "O slope do canal ({:.4}) não tem percentil fiável: apenas {} canais {} com série utilizável (mínimo 10). Slope relativo do canal: {:.4}; mediana relativa dos pares: {:.4}.",
            comparacao.slope_canal,
            comparacao.n_pares,
            grupo,
            comparacao.slope_canal_relativo,
            comparacao.slope_mediano_pares_relativo
        ),
    }
}

fn campo_csv(valor: &str) -> String {
    if valor.contains(',') || valor.contains('"') || valor.contains('\n') {
        format!("\"{}\"", valor.replace('"', "\"\""))
    } else {
        valor.to_owned()
    }
}
