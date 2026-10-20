use std::fmt::Write as _;

use analysis::Direcao;

use super::{ComparacaoPares, RelatorioCanal, TendenciaMetrica};

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

    let _ = writeln!(saida, "## Sumário");
    let _ = writeln!(saida);
    let significativas: Vec<&TendenciaMetrica> = relatorio
        .tendencias
        .iter()
        .filter(|tendencia| tendencia.p_ajustado_fdr < relatorio.nivel_significancia)
        .collect();
    let total = relatorio.tendencias.len();
    let direcao = direcao_predominante(&significativas);
    if total == 0 {
        let _ = writeln!(
            saida,
            "No período de {} a {}, não foi possível analisar nenhuma métrica; ver a secção Avisos.",
            relatorio.periodo_inicio, relatorio.periodo_fim
        );
    } else if significativas.is_empty() {
        let _ = writeln!(
            saida,
            "No período de {} a {}, nenhuma das {} métricas analisadas apresenta tendência estatisticamente significativa (MK, p<{} após FDR).",
            relatorio.periodo_inicio, relatorio.periodo_fim, total, relatorio.nivel_significancia
        );
    } else {
        let _ = writeln!(
            saida,
            "No período de {} a {}, {} das {} métricas analisadas apresentam tendência {} estatisticamente significativa (MK, p<{} após FDR).",
            relatorio.periodo_inicio,
            relatorio.periodo_fim,
            significativas.len(),
            total,
            direcao,
            relatorio.nivel_significancia
        );
    }
    let _ = writeln!(saida);

    let _ = writeln!(saida, "## Tendência por Métrica");
    let _ = writeln!(saida);
    let _ = writeln!(
        saida,
        "| Métrica | N | Direção | τ (Kendall) | p | p (FDR) | Slope (IC95) |"
    );
    let _ = writeln!(saida, "|---|---|---|---|---|---|---|");
    for tendencia in &relatorio.tendencias {
        let _ = writeln!(
            saida,
            "| {} | {} | {} | {:.4} | {:.4} | {:.4} | {:.4} ({:.4}, {:.4}) |",
            tendencia.metrica.as_str(),
            tendencia.n_snapshots,
            direcao_str(tendencia.direcao),
            tendencia.tau,
            tendencia.p_value,
            tendencia.p_ajustado_fdr,
            tendencia.slope_theil_sen,
            tendencia.slope_ic95.0,
            tendencia.slope_ic95.1
        );
    }
    let _ = writeln!(saida);

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
        "- N snapshots mínimo para inferência preditiva: {}.",
        relatorio.aviso_amostra_minima
    );

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
        "canal_id,canal_nome,metrica,n_snapshots,direcao,tau,p_value,p_ajustado_fdr,slope,slope_ic_low,slope_ic_high,amostra_suficiente,periodo_inicio,periodo_fim"
    );

    for tendencia in &relatorio.tendencias {
        let _ = writeln!(
            saida,
            "{},{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{}",
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
            relatorio.periodo_fim
        );
    }

    saida
}

/// Exporta o relatório em JSON indentado.
pub fn para_json(relatorio: &RelatorioCanal) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(relatorio)
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

fn texto_comparacao(comparacao: &ComparacaoPares) -> String {
    let grupo = if comparacao.grupo == "todos" {
        "na base de dados".to_owned()
    } else if let Some(categoria) = comparacao.grupo.strip_prefix("categoria:") {
        format!("da categoria `{categoria}`")
    } else {
        format!("do grupo `{}`", comparacao.grupo)
    };
    format!(
        "O slope do canal ({:.4}) situa-se no percentil {:.1} face a {} canais {}. Mediana dos pares: {:.4}.",
        comparacao.slope_canal,
        comparacao.percentil_canal_entre_pares,
        comparacao.n_pares,
        grupo,
        comparacao.slope_mediano_pares
    )
}

fn campo_csv(valor: &str) -> String {
    if valor.contains(',') || valor.contains('"') || valor.contains('\n') {
        format!("\"{}\"", valor.replace('"', "\"\""))
    } else {
        valor.to_owned()
    }
}
