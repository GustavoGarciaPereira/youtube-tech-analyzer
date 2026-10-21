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
    ComparacaoPares, DiagnosticoJanela, MetadadosReproducao, ModoAnalise, RelatorioCanal,
    RelatorioConfig, TendenciaMetrica,
};

const CONCORRENCIA_LOTE: usize = 4;

/// Abaixo deste número de pares o percentil não é fiável.
const MIN_PARES_COMPARACAO: usize = 10;

/// Série de pontos `(data, valor)` acompanhada dos avisos de qualidade.
type SerieCarregada = (Vec<(NaiveDate, f64)>, Vec<String>);

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
    let nome_pontos = match config.modo {
        ModoAnalise::PorColeta => "snapshots",
        ModoAnalise::PorPublicacao => "vídeos",
    };

    // Metodologia do modo cross-section: repetida uma vez, não por métrica.
    if config.modo == ModoAnalise::PorPublicacao {
        avisos.push(
            "análise por publicação (cross-section): cada vídeo é um ponto; o slope é medido por vídeo publicado, não por dia de coleta."
                .to_owned(),
        );
        avisos.push(
            "cada vídeo contribui com as métricas do seu último snapshot; vídeos nunca re-coletados mantêm valores antigos."
                .to_owned(),
        );
        if config.views_por_dia {
            avisos.push(
                "normalização por idade ativa: visualizações em views/dia desde a publicação e engagement em rácio sobre visualizações."
                    .to_owned(),
            );
        }
        avisos.push(format!(
            "vídeos com menos de {} dias são excluídos (idade_minima_dias).",
            config.idade_minima_dias
        ));
    }

    for metrica in &config.metricas {
        let (serie, avisos_serie) = match config.modo {
            ModoAnalise::PorColeta => {
                let snapshots = repositorio.listar_serie_temporal(
                    EntidadeTipo::Canal,
                    canal.id.as_str(),
                    *metrica,
                )?;
                let (mut serie, avisos) = converter_serie(&snapshots, *metrica);
                aplicar_periodo(&mut serie, config.periodo_dias)?;
                (serie, avisos)
            }
            ModoAnalise::PorPublicacao => {
                carregar_por_publicacao(repositorio.as_ref(), &canal.id, *metrica, &config)?
            }
        };
        for aviso in avisos_serie {
            if !avisos.contains(&aviso) {
                avisos.push(aviso);
            }
        }

        if serie.len() < config.min_snapshots {
            avisos.push(format!(
                "métrica {}: apenas {} {nome_pontos}, mínimo {}",
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
            intercept_theil_sen: ts.intercept,
            slope_ic95: ts.slope_ic95,
            pontos: serie.clone(),
            amostra_suficiente: serie.len() >= config.aviso_amostra_minima,
            eixo_temporal: config.modo.eixo_temporal().to_owned(),
            unidade_slope: unidade_slope(config.modo, *metrica, config.views_por_dia),
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

    if config.modo == ModoAnalise::PorPublicacao
        && config.periodo_dias.is_none()
        && periodo_fim.signed_duration_since(periodo_inicio).num_days() > 730
    {
        avisos.push(
            "os vídeos abrangem mais de dois anos de publicações: parte do slope pode vir da maturação dos vídeos, não do canal. Considera restringir com --periodo-dias 365."
                .to_owned(),
        );
    }

    let diagnostico_janelas = construir_diagnostico(repositorio.as_ref(), &canal.id, &config)?;
    let comparacoes =
        construir_comparacoes(&repositorio, &canal.id, &tendencias, &config, &mut avisos)?;

    let serie_sentimento =
        repositorio.obter_serie_sentimento(&canal.id, &config.versao_modelo_sentimento)?;
    let serie_sentimento = (!serie_sentimento.is_empty()).then_some(serie_sentimento);

    Ok(RelatorioCanal {
        canal,
        periodo_inicio,
        periodo_fim,
        modo: config.modo,
        tendencias,
        diagnostico_janelas,
        comparacoes,
        serie_sentimento,
        avisos,
        metadados,
        nivel_significancia: config.nivel_significancia,
        aviso_amostra_minima: config.aviso_amostra_minima,
    })
}

/// Carrega a cross-section de um canal usando `publicado_em` como eixo.
///
/// Aplica `idade_minima_dias` e `periodo_dias` (este último sobre a data de
/// publicação) e normaliza os valores quando `views_por_dia` está ativo.
fn carregar_por_publicacao(
    repositorio: &dyn RepositorioDeCanais,
    canal_id: &CanalId,
    metrica: Metrica,
    config: &RelatorioConfig,
) -> Result<SerieCarregada, ErroAplicacao> {
    if !metrica_por_publicacao(metrica) {
        return Err(ErroAplicacao::ConfigInvalida(format!(
            "métrica {} não é analisável por publicação (só visualizações, curtidas e total de comentários); usa o modo por-coleta",
            metrica.as_str()
        )));
    }

    let videos = repositorio.listar_videos(canal_id)?;
    let hoje = Utc::now().date_naive();
    let corte = corte_periodo(config.periodo_dias)?;

    let mut serie = Vec::with_capacity(videos.len());
    let mut avisos = Vec::new();
    let mut recentes = 0_usize;
    let mut fora_periodo = 0_usize;
    let mut sem_visualizacoes = 0_usize;
    let mut datas_invalidas = 0_usize;

    for video in &videos {
        let Some(data) = parse_data(&video.publicado_em) else {
            datas_invalidas += 1;
            continue;
        };
        let idade = hoje.signed_duration_since(data).num_days();

        if idade < i64::from(config.idade_minima_dias) {
            recentes += 1;
            continue;
        }
        if corte.is_some_and(|corte| data < corte) {
            fora_periodo += 1;
            continue;
        }

        let valor = match metrica {
            Metrica::Visualizacoes => {
                let views = video.visualizacoes as f64;
                if config.views_por_dia {
                    views / idade.max(1) as f64
                } else {
                    views
                }
            }
            Metrica::Curtidas | Metrica::TotalComentarios => {
                let bruto = if metrica == Metrica::Curtidas {
                    video.curtidas as f64
                } else {
                    video.total_comentarios as f64
                };
                if config.views_por_dia {
                    if video.visualizacoes == 0 {
                        sem_visualizacoes += 1;
                        continue;
                    }
                    bruto / video.visualizacoes as f64
                } else {
                    bruto
                }
            }
            _ => return Err(metrica_invalida(metrica)),
        };
        serie.push((data, valor));
    }

    // Datas iguais (dois vídeos no mesmo dia) ordenam por valor para que a
    // série seja determinística entre execuções.
    serie.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));

    if datas_invalidas > 0 {
        avisos.push(format!(
            "métrica {}: {datas_invalidas} vídeos com data de publicação inválida ignorados",
            metrica.as_str()
        ));
    }
    if recentes > 0 {
        avisos.push(format!(
            "métrica {}: {} por terem menos de {} dias",
            metrica.as_str(),
            contar(recentes, "vídeo ignorado", "vídeos ignorados"),
            config.idade_minima_dias
        ));
    }
    if fora_periodo > 0 {
        avisos.push(format!(
            "métrica {}: {} fora dos últimos {} dias de publicação",
            metrica.as_str(),
            contar(fora_periodo, "vídeo", "vídeos"),
            config.periodo_dias.unwrap_or_default()
        ));
    }
    if sem_visualizacoes > 0 {
        avisos.push(format!(
            "métrica {}: {} no rácio de engagement por não terem visualizações",
            metrica.as_str(),
            contar(sem_visualizacoes, "vídeo ignorado", "vídeos ignorados")
        ));
    }

    Ok((serie, avisos))
}

/// Auto-auditoria do viés de idade: reanalisa a série de views/dia com
/// janelas progressivamente mais curtas.
///
/// Só corre em PorPublicacao, com normalização ativa e quando a métrica
/// 'visualizacoes' faz parte do relatório.
fn construir_diagnostico(
    repositorio: &dyn RepositorioDeCanais,
    canal_id: &CanalId,
    config: &RelatorioConfig,
) -> Result<Option<Vec<DiagnosticoJanela>>, ErroAplicacao> {
    if config.modo != ModoAnalise::PorPublicacao
        || !config.views_por_dia
        || !config.metricas.contains(&Metrica::Visualizacoes)
    {
        return Ok(None);
    }

    let mut janelas = Vec::new();
    for janela in [None, Some(365), Some(180)] {
        let mut config_janela = config.clone();
        config_janela.periodo_dias = janela;
        let (serie, _) = carregar_por_publicacao(
            repositorio,
            canal_id,
            Metrica::Visualizacoes,
            &config_janela,
        )?;
        if serie.len() < 3 {
            continue;
        }
        let valores: Vec<f64> = serie.iter().map(|(_, valor)| *valor).collect();
        let Ok(resultado) = theil_sen(&valores) else {
            continue;
        };
        janelas.push(DiagnosticoJanela {
            janela_dias: janela,
            n_snapshots: serie.len(),
            slope_views_dia: resultado.slope,
            multiplicador_slope: 1.0,
        });
    }

    if janelas.is_empty() {
        return Ok(None);
    }

    // A referência é a janela completa; sem ela, os multiplicadores ficam 1.0.
    let base = janelas
        .iter()
        .find(|janela| janela.janela_dias.is_none())
        .map(|janela| janela.slope_views_dia);
    if let Some(base) = base
        && base.abs() > f64::MIN_POSITIVE
    {
        for janela in &mut janelas {
            janela.multiplicador_slope = janela.slope_views_dia / base;
        }
    }

    Ok(Some(janelas))
}

const fn metrica_por_publicacao(metrica: Metrica) -> bool {
    matches!(
        metrica,
        Metrica::Visualizacoes | Metrica::Curtidas | Metrica::TotalComentarios
    )
}

fn contar(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("1 {singular}")
    } else {
        format!("{n} {plural}")
    }
}

fn metrica_invalida(metrica: Metrica) -> ErroAplicacao {
    ErroAplicacao::ConfigInvalida(format!(
        "métrica {} não é analisável por publicação; usa o modo por-coleta",
        metrica.as_str()
    ))
}

fn unidade_slope(modo: ModoAnalise, metrica: Metrica, normalizado: bool) -> String {
    if modo == ModoAnalise::PorColeta {
        return "por dia".to_owned();
    }
    let base = match metrica {
        Metrica::Visualizacoes => "views",
        Metrica::Curtidas => "likes",
        Metrica::TotalComentarios => "comentários",
        outra => outra.as_str(),
    };
    if !normalizado {
        return format!("{base} por vídeo");
    }
    if metrica == Metrica::Visualizacoes {
        "views/dia por vídeo".to_owned()
    } else {
        format!("{base}/views por vídeo")
    }
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

    // O baseline usa a categoria manual. nicho_do_canal fica reservado ao
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
        // Sem uma mediana positiva não há forma de retirar a escala do canal.
        let Some(fator) = fator_normalizacao(&tendencia.pontos) else {
            aviso_unico(
                avisos,
                format!(
                    "métrica {}: mediana da série é zero; comparação com pares indisponível",
                    tendencia.metrica.as_str()
                ),
            );
            continue;
        };
        let slope_relativo_canal = tendencia.slope_theil_sen / fator;

        let mut absolutos = Vec::new();
        let mut relativos = Vec::new();
        for par in &pares {
            let serie = match config.modo {
                ModoAnalise::PorColeta => {
                    let snapshots = repositorio.listar_serie_temporal(
                        EntidadeTipo::Canal,
                        par.id.as_str(),
                        tendencia.metrica,
                    )?;
                    let (mut serie, _) = converter_serie(&snapshots, tendencia.metrica);
                    aplicar_periodo(&mut serie, config.periodo_dias)?;
                    serie
                }
                ModoAnalise::PorPublicacao => {
                    carregar_por_publicacao(
                        repositorio.as_ref(),
                        &par.id,
                        tendencia.metrica,
                        config,
                    )?
                    .0
                }
            };
            if serie.len() < 3 {
                continue;
            }
            let Some(fator_par) = fator_normalizacao(&serie) else {
                continue;
            };
            let valores: Vec<f64> = serie.iter().map(|(_, valor)| *valor).collect();
            match theil_sen(&valores) {
                Ok(resultado) => {
                    absolutos.push(resultado.slope);
                    relativos.push(resultado.slope / fator_par);
                }
                Err(AnalysisError::SerieCurta { .. }) => {}
                Err(erro) => return Err(ErroAplicacao::Analise(erro)),
            }
        }

        if relativos.is_empty() {
            continue;
        }

        let n_pares = relativos.len();
        let slope_mediano_pares = mediana(&mut absolutos).ok_or_else(|| {
            ErroAplicacao::SemDados("sem declives de pares para comparar".to_owned())
        })?;
        let slope_mediano_pares_relativo = mediana(&mut relativos)
            .ok_or_else(|| ErroAplicacao::SemDados("sem declives relativos de pares".to_owned()))?;

        let percentil_canal_entre_pares = if n_pares >= MIN_PARES_COMPARACAO {
            Some(percentil(&relativos, slope_relativo_canal))
        } else {
            aviso_unico(
                avisos,
                format!(
                    "apenas {n_pares} pares {} com série válida para '{}' (mínimo {MIN_PARES_COMPARACAO})",
                    grupo_legivel(&grupo),
                    tendencia.metrica.as_str()
                ),
            );
            None
        };

        comparacoes.push(ComparacaoPares {
            metrica: tendencia.metrica,
            grupo: grupo.clone(),
            n_pares,
            slope_mediano_pares,
            slope_canal: tendencia.slope_theil_sen,
            fator_normalizacao: fator,
            slope_canal_relativo: slope_relativo_canal,
            slope_mediano_pares_relativo,
            percentil_canal_entre_pares,
        });
    }

    Ok(comparacoes)
}

fn grupo_legivel(grupo: &str) -> String {
    if grupo == "todos" {
        return "na base de dados".to_owned();
    }
    if let Some(categoria) = grupo.strip_prefix("categoria:") {
        return format!("na categoria '{categoria}'");
    }
    format!("no grupo '{grupo}'")
}

fn fator_normalizacao(pontos: &[(NaiveDate, f64)]) -> Option<f64> {
    let mut valores: Vec<f64> = pontos.iter().map(|(_, valor)| *valor).collect();
    let mediana = mediana(&mut valores)?;
    if mediana.is_finite() && mediana > 0.0 {
        Some(mediana)
    } else {
        None
    }
}

fn percentil(valores: &[f64], alvo: f64) -> f64 {
    if valores.is_empty() {
        return 0.0;
    }
    let menores = valores.iter().filter(|valor| **valor < alvo).count();
    let iguais = valores.iter().filter(|valor| **valor == alvo).count();
    ((menores as f64 + iguais as f64 / 2.0) / valores.len() as f64) * 100.0
}

fn aviso_unico(avisos: &mut Vec<String>, texto: String) {
    if !avisos.contains(&texto) {
        avisos.push(texto);
    }
}

fn converter_serie(snapshots: &[Snapshot], metrica: Metrica) -> SerieCarregada {
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

fn corte_periodo(periodo_dias: Option<u32>) -> Result<Option<NaiveDate>, ErroAplicacao> {
    let Some(dias) = periodo_dias else {
        return Ok(None);
    };
    let hoje = Utc::now().date_naive();
    hoje.checked_sub_days(Days::new(u64::from(dias)))
        .map(Some)
        .ok_or_else(|| ErroAplicacao::ConfigInvalida("periodo_dias inválido".to_owned()))
}

fn aplicar_periodo(
    serie: &mut Vec<(NaiveDate, f64)>,
    periodo_dias: Option<u32>,
) -> Result<(), ErroAplicacao> {
    if let Some(corte) = corte_periodo(periodo_dias)? {
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
        MetadadosReproducao, ModoAnalise, RelatorioCanal, RelatorioConfig, TendenciaMetrica,
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
            intercept_theil_sen: 0.0,
            slope_ic95: (0.9, 1.1),
            pontos: Vec::new(),
            amostra_suficiente: true,
            eixo_temporal: "data_coleta".to_owned(),
            unidade_slope: "por dia".to_owned(),
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
            modo: ModoAnalise::PorColeta,
            tendencias: p_values.iter().map(|p| tendencia(*p)).collect(),
            diagnostico_janelas: None,
            comparacoes: Vec::new(),
            serie_sentimento: None,
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
        assert_eq!(config.modo, ModoAnalise::PorColeta);
        assert_eq!(config.idade_minima_dias, 7);
        assert!(config.views_por_dia);
    }
}
