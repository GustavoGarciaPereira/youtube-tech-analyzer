use std::sync::Arc;

use analysis::{AnalysisError, benjamini_hochberg, mann_kendall, theil_sen};
use chrono::{Days, NaiveDate, Utc};
use domain::entities::snapshot::{EntidadeTipo, Metrica, Snapshot};
use domain::errors::ErroDominio;
use domain::ids::CanalId;
use domain::ports::RepositorioDeCanais;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::errors::ErroAplicacao;
use crate::relatorio::{
    ComparacaoPares, MetadadosReproducao, RelatorioCanal, RelatorioConfig, TendenciaMetrica,
};

const CONCORRENCIA_LOTE: usize = 4;

/// Caso de uso que gera relatórios estatísticos de canais.
pub struct GerarRelatorioUseCase {
    repositorio: Arc<dyn RepositorioDeCanais>,
    config: RelatorioConfig,
}

impl GerarRelatorioUseCase {
    /// Cria o caso de uso.
    #[must_use]
    pub fn new(repositorio: Arc<dyn RepositorioDeCanais>, config: RelatorioConfig) -> Self {
        Self {
            repositorio,
            config,
        }
    }

    /// Gera o relatório de um canal.
    pub async fn executar(
        &self,
        canal_id: &CanalId,
        metadados: MetadadosReproducao,
    ) -> Result<RelatorioCanal, ErroAplicacao> {
        let mut relatorio = gerar_relatorio(
            Arc::clone(&self.repositorio),
            self.config.clone(),
            canal_id,
            metadados,
        )
        .await?;
        aplicar_fdr(
            std::slice::from_mut(&mut relatorio),
            self.config.nivel_significancia,
        )?;
        Ok(relatorio)
    }

    /// Gera relatórios para vários canais, com uma única correção FDR.
    ///
    /// O `Result` externo representa falhas fatais do lote; cada canal tem o
    /// seu próprio `Result` para que uma falha não aborte os restantes.
    pub async fn executar_lote(
        &self,
        canal_ids: &[CanalId],
        metadados: MetadadosReproducao,
    ) -> Result<Vec<Result<RelatorioCanal, ErroAplicacao>>, ErroAplicacao> {
        if canal_ids.is_empty() {
            return Ok(Vec::new());
        }

        let semaforo = Arc::new(Semaphore::new(CONCORRENCIA_LOTE));
        let mut tarefas = JoinSet::new();

        for (indice, canal_id) in canal_ids.iter().enumerate() {
            let repositorio = Arc::clone(&self.repositorio);
            let config = self.config.clone();
            let metadados = metadados.clone();
            let semaforo = Arc::clone(&semaforo);
            let canal_id = canal_id.clone();

            tarefas.spawn(async move {
                let _permit = match semaforo.acquire_owned().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        return (
                            indice,
                            Err(ErroAplicacao::SemDados(
                                "semáforo de relatórios fechado".to_owned(),
                            )),
                        );
                    }
                };
                let resultado = gerar_relatorio(repositorio, config, &canal_id, metadados).await;
                (indice, resultado)
            });
        }

        let mut resultados = Vec::with_capacity(canal_ids.len());
        while let Some(join) = tarefas.join_next().await {
            match join {
                Ok(resultado) => resultados.push(resultado),
                Err(erro) => resultados.push((
                    usize::MAX,
                    Err(ErroAplicacao::Dominio(ErroDominio::Coleta(format!(
                        "tarefa de relatório falhou: {erro}"
                    )))),
                )),
            }
        }
        resultados.sort_by_key(|(indice, _)| *indice);

        // BH é aplicado UMA vez sobre todos os p-values dos canais com sucesso.
        let mut p_values = Vec::new();
        for (_, resultado) in &resultados {
            if let Ok(relatorio) = resultado {
                for tendencia in &relatorio.tendencias {
                    p_values.push(tendencia.p_value);
                }
            }
        }

        if !p_values.is_empty() {
            let ajustados = benjamini_hochberg(&p_values, self.config.nivel_significancia)?;
            let mut indice = 0;
            for (_, resultado) in &mut resultados {
                if let Ok(relatorio) = resultado {
                    for tendencia in &mut relatorio.tendencias {
                        tendencia.p_ajustado_fdr = ajustados[indice];
                        indice += 1;
                    }
                }
            }
        }

        Ok(resultados
            .into_iter()
            .map(|(_, resultado)| resultado)
            .collect())
    }
}

async fn gerar_relatorio(
    repositorio: Arc<dyn RepositorioDeCanais>,
    config: RelatorioConfig,
    canal_id: &CanalId,
    metadados: MetadadosReproducao,
) -> Result<RelatorioCanal, ErroAplicacao> {
    let canal = repositorio
        .obter_canal(canal_id)?
        .ok_or_else(|| ErroAplicacao::SemDados(format!("canal {canal_id} não encontrado")))?;

    let mut avisos = Vec::new();
    let mut tendencias = Vec::new();
    let mut periodo_inicio: Option<NaiveDate> = None;
    let mut periodo_fim: Option<NaiveDate> = None;

    for metrica in &config.metricas {
        let snapshots =
            repositorio.listar_serie_temporal(EntidadeTipo::Canal, canal.id.as_str(), *metrica)?;
        let (mut serie, avisos_serie) = converter_serie(&snapshots, *metrica);
        avisos.extend(avisos_serie);
        aplicar_periodo(&mut serie, config.periodo_dias)?;

        if serie.len() < config.min_snapshots {
            avisos.push(format!(
                "métrica {}: apenas {} snapshots, mínimo {}",
                metrica.as_str(),
                serie.len(),
                config.min_snapshots
            ));
            continue;
        }

        let valores: Vec<f64> = serie.iter().map(|(_, valor)| *valor).collect();

        let mk = match mann_kendall(&valores, config.nivel_significancia) {
            Ok(resultado) => resultado,
            Err(AnalysisError::SerieCurta { n }) => {
                avisos.push(format!(
                    "métrica {}: série demasiado curta ({n} pontos)",
                    metrica.as_str()
                ));
                continue;
            }
            Err(erro) => return Err(ErroAplicacao::Analise(erro)),
        };

        let ts = match theil_sen(&valores) {
            Ok(resultado) => resultado,
            Err(AnalysisError::SerieCurta { n }) => {
                avisos.push(format!(
                    "métrica {}: série demasiado curta para Theil-Sen ({n} pontos)",
                    metrica.as_str()
                ));
                continue;
            }
            Err(erro) => return Err(ErroAplicacao::Analise(erro)),
        };

        if let (Some(primeira), Some(ultima)) = (serie.first(), serie.last()) {
            periodo_inicio = Some(periodo_inicio.map_or(primeira.0, |atual| atual.min(primeira.0)));
            periodo_fim = Some(periodo_fim.map_or(ultima.0, |atual| atual.max(ultima.0)));
        }

        if serie.len() < config.aviso_amostra_minima {
            avisos.push(format!(
                "métrica {}: amostra insuficiente para inferência preditiva robusta (N<{})",
                metrica.as_str(),
                config.aviso_amostra_minima
            ));
        }

        tendencias.push(TendenciaMetrica {
            metrica: *metrica,
            n_snapshots: serie.len(),
            direcao: mk.direcao,
            tau: mk.tau,
            p_value: mk.p_value,
            p_ajustado_fdr: mk.p_value,
            slope_theil_sen: ts.slope,
            slope_ic95: ts.slope_ic95,
            amostra_suficiente: serie.len() >= config.aviso_amostra_minima,
        });
    }

    if tendencias.is_empty() {
        avisos
            .push("amostra insuficiente para gerar tendências; ver avisos por métrica".to_owned());
    }

    // Sem tendências não há período real; usamos a data de geração para que o
    // relatório ainda possa ser exportado com os avisos.
    let periodo_inicio = periodo_inicio.unwrap_or_else(|| Utc::now().date_naive());
    let periodo_fim = periodo_fim.unwrap_or(periodo_inicio);

    let comparacoes =
        construir_comparacoes(&repositorio, &canal.id, &tendencias, &config, &mut avisos)?;

    Ok(RelatorioCanal {
        canal,
        periodo_inicio,
        periodo_fim,
        tendencias,
        comparacoes,
        avisos,
        metadados,
        nivel_significancia: config.nivel_significancia,
        aviso_amostra_minima: config.aviso_amostra_minima,
    })
}

fn construir_comparacoes(
    repositorio: &Arc<dyn RepositorioDeCanais>,
    canal_id: &CanalId,
    tendencias: &[TendenciaMetrica],
    config: &RelatorioConfig,
    avisos: &mut Vec<String>,
) -> Result<Vec<ComparacaoPares>, ErroAplicacao> {
    let mut todos = repositorio.listar_canais()?;
    todos.retain(|canal| &canal.id != canal_id);
    let ha_outros_canais = !todos.is_empty();

    // O baseline usa a categoria manual. `nicho_do_canal` fica reservado ao
    // futuro classificador de nicho e não é consultado neste caminho.
    let categoria = repositorio.categoria_manual_do_canal(canal_id)?;
    let (pares, grupo) = match categoria {
        Some(categoria) => {
            let mut pares = repositorio.canais_por_categoria(categoria)?;
            pares.retain(|canal| &canal.id != canal_id);
            if pares.is_empty() {
                if ha_outros_canais {
                    avisos.push(format!(
                        "sem pares na categoria '{}'; comparação feita contra todos os canais",
                        categoria.como_str()
                    ));
                } else {
                    avisos.push("não há outros canais na base de dados para comparação".to_owned());
                }
                (todos, "todos".to_owned())
            } else {
                (pares, format!("categoria:{}", categoria.como_str()))
            }
        }
        None => {
            if ha_outros_canais {
                avisos.push(
                    "canal sem categoria manual; comparação feita contra todos os canais"
                        .to_owned(),
                );
            } else {
                avisos.push("não há outros canais na base de dados para comparação".to_owned());
            }
            (todos, "todos".to_owned())
        }
    };

    let mut comparacoes = Vec::new();
    for tendencia in tendencias {
        let mut slopes = Vec::new();
        for par in &pares {
            let snapshots = repositorio.listar_serie_temporal(
                EntidadeTipo::Canal,
                par.id.as_str(),
                tendencia.metrica,
            )?;
            let (mut serie, _) = converter_serie(&snapshots, tendencia.metrica);
            aplicar_periodo(&mut serie, config.periodo_dias)?;
            if serie.len() < 3 {
                continue;
            }
            let valores: Vec<f64> = serie.iter().map(|(_, valor)| *valor).collect();
            match theil_sen(&valores) {
                Ok(resultado) => slopes.push(resultado.slope),
                Err(AnalysisError::SerieCurta { .. }) => {}
                Err(erro) => return Err(ErroAplicacao::Analise(erro)),
            }
        }

        if slopes.is_empty() {
            continue;
        }

        let n_pares = slopes.len();
        let menores = slopes
            .iter()
            .filter(|slope| **slope < tendencia.slope_theil_sen)
            .count();
        let mediana = mediana(&mut slopes).ok_or_else(|| {
            ErroAplicacao::SemDados("sem declives de pares para comparar".to_owned())
        })?;

        comparacoes.push(ComparacaoPares {
            grupo: grupo.clone(),
            n_pares,
            slope_mediano_pares: mediana,
            slope_canal: tendencia.slope_theil_sen,
            percentil_canal_entre_pares: (menores as f64 / n_pares as f64) * 100.0,
        });
    }

    Ok(comparacoes)
}

fn converter_serie(
    snapshots: &[Snapshot],
    metrica: Metrica,
) -> (Vec<(NaiveDate, f64)>, Vec<String>) {
    let mut serie = Vec::new();
    let mut avisos = Vec::new();

    for snapshot in snapshots {
        if snapshot.metrica != metrica {
            continue;
        }
        match parse_data(&snapshot.data_coleta) {
            Some(data) => serie.push((data, snapshot.valor as f64)),
            None => avisos.push(format!(
                "métrica {}: snapshot com data inválida '{}' ignorado",
                metrica.as_str(),
                snapshot.data_coleta
            )),
        }
    }

    serie.sort_by_key(|(data, _)| *data);
    (serie, avisos)
}

fn parse_data(valor: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(valor, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(valor)
                .ok()
                .map(|data| data.date_naive())
        })
}

fn aplicar_periodo(
    serie: &mut Vec<(NaiveDate, f64)>,
    periodo_dias: Option<u32>,
) -> Result<(), ErroAplicacao> {
    if let Some(dias) = periodo_dias {
        let hoje = Utc::now().date_naive();
        let corte = hoje
            .checked_sub_days(Days::new(u64::from(dias)))
            .ok_or_else(|| ErroAplicacao::ConfigInvalida("periodo_dias inválido".to_owned()))?;
        serie.retain(|(data, _)| *data >= corte);
    }
    Ok(())
}

fn mediana(valores: &mut [f64]) -> Option<f64> {
    if valores.is_empty() {
        return None;
    }
    valores.sort_by(f64::total_cmp);
    let meio = valores.len() / 2;
    if valores.len() % 2 == 1 {
        valores.get(meio).copied()
    } else {
        let esquerda = valores.get(meio - 1).copied()?;
        let direita = valores.get(meio).copied()?;
        Some((esquerda + direita) / 2.0)
    }
}

fn aplicar_fdr(
    relatorios: &mut [RelatorioCanal],
    nivel_significancia: f64,
) -> Result<(), ErroAplicacao> {
    let mut p_values = Vec::new();
    for relatorio in relatorios.iter() {
        for tendencia in &relatorio.tendencias {
            p_values.push(tendencia.p_value);
        }
    }
    if p_values.is_empty() {
        return Ok(());
    }

    let ajustados = benjamini_hochberg(&p_values, nivel_significancia)?;
    let mut indice = 0;
    for relatorio in relatorios {
        for tendencia in &mut relatorio.tendencias {
            tendencia.p_ajustado_fdr = ajustados[indice];
            indice += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, Utc};

    use super::aplicar_fdr;
    use crate::relatorio::{
        MetadadosReproducao, RelatorioCanal, RelatorioConfig, TendenciaMetrica,
    };
    use analysis::Direcao;
    use domain::entities::canal::Canal;
    use domain::entities::snapshot::Metrica;
    use domain::ids::CanalId;

    fn tendencia(p_value: f64) -> TendenciaMetrica {
        TendenciaMetrica {
            metrica: Metrica::Visualizacoes,
            n_snapshots: 30,
            direcao: Direcao::Crescente,
            tau: 0.8,
            p_value,
            p_ajustado_fdr: p_value,
            slope_theil_sen: 1.0,
            slope_ic95: (0.9, 1.1),
            amostra_suficiente: true,
        }
    }

    fn relatorio(p_values: &[f64]) -> RelatorioCanal {
        RelatorioCanal {
            canal: Canal {
                id: CanalId::new("canal-1"),
                nome: "Canal".to_owned(),
                handle: None,
                url: "https://www.youtube.com/channel/canal-1".to_owned(),
                descricao: None,
                inscritos: 0,
                total_videos: 0,
                topic_categories: Vec::new(),
                criado_em: None,
            },
            periodo_inicio: NaiveDate::from_ymd_opt(2024, 1, 1).expect("data válida"),
            periodo_fim: NaiveDate::from_ymd_opt(2024, 1, 30).expect("data válida"),
            tendencias: p_values.iter().map(|p| tendencia(*p)).collect(),
            comparacoes: Vec::new(),
            avisos: Vec::new(),
            metadados: MetadadosReproducao {
                hash_commit: "abc12345".to_owned(),
                versao_schema: "1".to_owned(),
                comando: Some("cargo test".to_owned()),
                gerado_em: Utc::now(),
                versao_app: "0.1.0".to_owned(),
            },
            nivel_significancia: 0.05,
            aviso_amostra_minima: 20,
        }
    }

    #[test]
    fn fdr_usa_todos_os_p_values_do_relatorio() {
        let mut relatorio = relatorio(&[0.001, 0.02, 0.04, 0.5]);
        aplicar_fdr(std::slice::from_mut(&mut relatorio), 0.05).expect("FDR válido");

        let ajustados: Vec<f64> = relatorio
            .tendencias
            .iter()
            .map(|tendencia| tendencia.p_ajustado_fdr)
            .collect();

        // BH com m=4: [0.004, 0.04, 0.053333..., 0.5].
        assert!((ajustados[0] - 0.004).abs() < 1e-12);
        assert!((ajustados[1] - 0.04).abs() < 1e-12);
        assert!((ajustados[2] - 0.053_333_333_333_333_33).abs() < 1e-12);
        assert!((ajustados[3] - 0.5).abs() < 1e-12);
        assert!(ajustados.windows(2).all(|janela| janela[0] <= janela[1]));
        assert_eq!(ajustados.iter().filter(|p| **p < 0.05).count(), 2);
    }

    #[test]
    fn config_default_tem_metricas_de_engagement() {
        let config = RelatorioConfig::default();
        assert_eq!(config.min_snapshots, 5);
        assert_eq!(config.aviso_amostra_minima, 20);
        assert_eq!(config.nivel_significancia, 0.05);
        assert_eq!(config.metricas.len(), 3);
    }
}
