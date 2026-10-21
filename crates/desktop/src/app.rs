use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use analysis::Direcao;
use application::relatorio::export::{
    media_sentimento, para_csv, para_json, para_markdown, resumo_sentimento,
};
use application::{
    AnalisarConfig, AnalisarSentimentosUseCase, ColetaConfig, ColetarCanalUseCase,
    GerarRelatorioUseCase, MetadadosReproducao, ModoAnalise, RelatorioCanal, RelatorioConfig,
    TendenciaMetrica,
};
use chrono::NaiveDate;
use domain::ports::AnalisadorDeSentimento;
use domain::{CanalId, CanalRef, CategoriaManual, Metrica};
use eframe::egui;
use egui::{Color32, RichText};
use egui_plot::{Legend, Line, Plot, Polygon};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::state::{
    AcaoNlp, AppState, Ecra, EstadoColeta, EstadoRelatorio, EventoNlp, FormatoExport,
    ProgressoColeta, acao_nlp,
};
use crate::wiring::Dependencias;

const COR_ACCENT: Color32 = Color32::from_rgb(64, 120, 220);
const COR_SUCESSO: Color32 = Color32::from_rgb(80, 190, 120);
const COR_ERRO: Color32 = Color32::from_rgb(220, 90, 90);
const COR_AVISO: Color32 = Color32::from_rgb(230, 190, 80);

/// Estado do bloco de NLP apresentado no relatório.
#[derive(Debug, Clone)]
struct NlpUi {
    modelo_pronto: bool,
    em_progresso: bool,
    progresso_msg: Option<String>,
    pendentes: usize,
}

/// Aplicação desktop.
pub struct DesktopApp {
    state: AppState,
}

impl DesktopApp {
    /// Cria a aplicação a partir das dependências e do runtime tokio.
    #[must_use]
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        rt: tokio::runtime::Handle,
        deps: Dependencias,
        caminho_db: PathBuf,
    ) -> Self {
        aplicar_tema(&cc.egui_ctx);
        Self {
            state: AppState::novo(deps, rt, caminho_db),
        }
    }

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        self.poll_add(ctx);
        self.poll_coleta(ctx);
        self.poll_relatorio(ctx);
        self.poll_nlp(ctx);
    }

    fn poll_add(&mut self, ctx: &egui::Context) {
        let Some(mut rx) = self.state.add_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(id)) => {
                if let Err(erro) = self.state.refresh_canais() {
                    self.state.toast(erro);
                }
                self.state.selecionar_canal(id);
                self.state.formulario_novo_canal.clear();
                self.state.toast("Canal adicionado");
                ctx.request_repaint();
            }
            Ok(Err(erro)) => {
                self.state.toast(format!("Erro ao adicionar canal: {erro}"));
                ctx.request_repaint();
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                self.state.add_rx = Some(rx);
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.state
                    .toast("A tarefa de adição terminou sem resultado");
            }
        }
    }

    fn poll_coleta(&mut self, ctx: &egui::Context) {
        let estado = std::mem::replace(&mut self.state.coleta, EstadoColeta::Ocioso);
        match estado {
            EstadoColeta::EmCurso {
                cancelamento,
                mut eventos_rx,
                mut resultado_rx,
                mut progresso,
                iniciado_em,
            } => {
                while let Ok(evento) = eventos_rx.try_recv() {
                    let _ = progresso.aplicar(evento);
                }

                match resultado_rx.try_recv() {
                    Ok(Ok(mensagem)) => {
                        if let Err(erro) = self.state.refresh_canais() {
                            self.state.toast(erro);
                        }
                        self.state.coleta = EstadoColeta::Concluida(mensagem.clone());
                        self.state.toast(mensagem);
                        ctx.request_repaint();
                    }
                    Ok(Err(erro)) => {
                        self.state.coleta = EstadoColeta::Ocioso;
                        self.state.toast(format!("Coleta falhou: {erro}"));
                        ctx.request_repaint();
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        self.state.coleta = EstadoColeta::EmCurso {
                            cancelamento,
                            eventos_rx,
                            resultado_rx,
                            progresso,
                            iniciado_em,
                        };
                        ctx.request_repaint_after(Duration::from_millis(100));
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        self.state.coleta = EstadoColeta::Ocioso;
                        self.state
                            .toast("A tarefa de coleta terminou sem resultado");
                    }
                }
            }
            outro => self.state.coleta = outro,
        }
    }

    fn poll_relatorio(&mut self, ctx: &egui::Context) {
        // Detecta um modelo descarregado pela CLI sem reiniciar a GUI.
        self.state.refresh_nlp_modelo_pronto();
        let estado = std::mem::replace(&mut self.state.relatorio, EstadoRelatorio::Ocioso);
        match estado {
            EstadoRelatorio::EmGeracao { mut rx } => match rx.try_recv() {
                Ok(Ok(relatorio)) => {
                    self.state.metrica_plot = relatorio
                        .tendencias
                        .first()
                        .map(|tendencia| tendencia.metrica);
                    self.state.relatorio = EstadoRelatorio::Pronto(Box::new(relatorio));
                    self.state.refresh_nlp_pendentes();
                    self.state.toast("Relatório gerado");
                    ctx.request_repaint();
                }
                Ok(Err(erro)) => {
                    self.state.relatorio = EstadoRelatorio::Erro(erro.clone());
                    self.state.toast(format!("Relatório falhou: {erro}"));
                    ctx.request_repaint();
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    self.state.relatorio = EstadoRelatorio::EmGeracao { rx };
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.state.relatorio =
                        EstadoRelatorio::Erro("a tarefa terminou sem resultado".to_owned());
                }
            },
            outro => self.state.relatorio = outro,
        }
    }

    fn poll_nlp(&mut self, ctx: &egui::Context) {
        let Some(mut rx) = self.state.nlp_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(evento)) => {
                self.state.nlp_em_progresso = false;
                self.state.nlp_progresso_msg = None;
                match evento {
                    EventoNlp::ModeloPronto => {
                        self.state.nlp_modelo_pronto = true;
                        self.state.refresh_nlp_pendentes();
                        self.state.toast("Modelo NLP pronto");
                    }
                    EventoNlp::AnaliseConcluida {
                        analisados,
                        falhados,
                        duracao_ms,
                    } => {
                        self.state.nlp_pendentes =
                            self.state.nlp_pendentes.saturating_sub(analisados);
                        self.state.refresh_nlp_pendentes();
                        self.state.toast(format!(
                            "Sentimentos: {analisados} analisados, {falhados} falhas em {:.1}s",
                            duracao_ms as f64 / 1000.0
                        ));
                        // O relatório em curso pode ter começado antes de a
                        // análise terminar; descartá-lo garante que a série
                        // nova aparece no relatório final.
                        if matches!(self.state.relatorio, EstadoRelatorio::EmGeracao { .. }) {
                            self.state.relatorio = EstadoRelatorio::Ocioso;
                        }
                        self.iniciar_relatorio(ctx);
                    }
                }
                ctx.request_repaint();
            }
            Ok(Err(erro)) => {
                self.state.nlp_em_progresso = false;
                self.state.nlp_progresso_msg = None;
                self.state.toast(format!("NLP falhou: {erro}"));
                ctx.request_repaint();
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                self.state.nlp_rx = Some(rx);
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.state.nlp_em_progresso = false;
                self.state.nlp_progresso_msg = None;
                self.state.toast("A tarefa de NLP terminou sem resultado");
                ctx.request_repaint();
            }
        }
    }

    fn iniciar_adicionar(&mut self, ctx: &egui::Context) {
        if self.state.add_rx.is_some() {
            self.state.toast("Já está a adicionar um canal");
            return;
        }
        let Some(coletor) = self.state.deps.coletor.clone() else {
            self.state
                .toast("YOUTUBE_API_KEY ausente; não é possível adicionar canais");
            return;
        };
        let referencia = self.state.formulario_novo_canal.trim().to_owned();
        if referencia.is_empty() {
            self.state.toast("Indica uma URL ou @handle");
            return;
        }

        let (tx, rx) = mpsc::channel(1);
        self.state.add_rx = Some(rx);
        let repositorio = Arc::clone(&self.state.deps.repositorio);
        self.state.rt.spawn(async move {
            let resultado = match infra_youtube::parse_canal_ref(&referencia) {
                Ok(canal_ref) => match coletor.coletar_canal(&canal_ref).await {
                    Ok(canal) => match repositorio.guardar_canal(&canal) {
                        Ok(()) => Ok(canal.id),
                        Err(erro) => Err(erro.to_string()),
                    },
                    Err(erro) => Err(erro.to_string()),
                },
                Err(erro) => Err(erro.to_string()),
            };
            let _ = tx.send(resultado).await;
        });
        ctx.request_repaint();
    }

    fn iniciar_coleta(&mut self, ctx: &egui::Context, canal_id: CanalId) {
        if matches!(self.state.coleta, EstadoColeta::EmCurso { .. }) {
            self.state.toast("Já há uma coleta em curso");
            return;
        }
        let Some(coletor) = self.state.deps.coletor.clone() else {
            self.state
                .toast("YOUTUBE_API_KEY ausente; não é possível coletar");
            return;
        };
        let (tx, rx) = mpsc::channel(256);
        let (resultado_tx, resultado_rx) = mpsc::channel(1);
        let cancelamento = CancellationToken::new();
        self.state.coleta = EstadoColeta::EmCurso {
            cancelamento: cancelamento.clone(),
            eventos_rx: rx,
            resultado_rx,
            progresso: ProgressoColeta::default(),
            iniciado_em: Instant::now(),
        };

        let repositorio = Arc::clone(&self.state.deps.repositorio);
        let rt = self.state.rt.clone();
        rt.spawn(async move {
            let caso = ColetarCanalUseCase::new(coletor, repositorio, ColetaConfig::default());
            let resultado = caso
                .executar(
                    CanalRef::ChannelId(canal_id.as_str().to_owned()),
                    Some(tx),
                    cancelamento,
                )
                .await;
            let mensagem = match resultado {
                Ok(relatorio) => Ok(format!(
                    "Coleta concluída: {} novos, {} já coletados, {} falhas, {} comentários",
                    relatorio.videos_novos,
                    relatorio.videos_ja_coletados,
                    relatorio.videos_falhados,
                    relatorio.comentarios_total
                )),
                Err(erro) => Err(erro.to_string()),
            };
            let _ = resultado_tx.send(mensagem).await;
        });
        ctx.request_repaint();
    }

    fn iniciar_coleta_todos(&mut self, ctx: &egui::Context) {
        if matches!(self.state.coleta, EstadoColeta::EmCurso { .. }) {
            self.state.toast("Já há uma coleta em curso");
            return;
        }
        let Some(coletor) = self.state.deps.coletor.clone() else {
            self.state
                .toast("YOUTUBE_API_KEY ausente; não é possível coletar");
            return;
        };
        let ids: Vec<CanalId> = self
            .state
            .canais
            .iter()
            .map(|canal| canal.id.clone())
            .collect();
        if ids.is_empty() {
            self.state.toast("Não há canais para coletar");
            return;
        }

        let (tx, rx) = mpsc::channel(256);
        let (resultado_tx, resultado_rx) = mpsc::channel(1);
        let cancelamento = CancellationToken::new();
        self.state.coleta = EstadoColeta::EmCurso {
            cancelamento: cancelamento.clone(),
            eventos_rx: rx,
            resultado_rx,
            progresso: ProgressoColeta::default(),
            iniciado_em: Instant::now(),
        };

        let repositorio = Arc::clone(&self.state.deps.repositorio);
        let rt = self.state.rt.clone();
        rt.spawn(async move {
            let mut processados = 0_usize;
            let mut falhas = 0_usize;
            for id in ids {
                if cancelamento.is_cancelled() {
                    break;
                }
                let caso = ColetarCanalUseCase::new(
                    Arc::clone(&coletor),
                    Arc::clone(&repositorio),
                    ColetaConfig::default(),
                );
                match caso
                    .executar(
                        CanalRef::ChannelId(id.as_str().to_owned()),
                        Some(tx.clone()),
                        cancelamento.clone(),
                    )
                    .await
                {
                    Ok(_) => processados += 1,
                    Err(_) => falhas += 1,
                }
            }
            drop(tx);
            let mensagem = if cancelamento.is_cancelled() {
                Err("Coleta cancelada".to_owned())
            } else if falhas > 0 {
                Err(format!(
                    "{processados} canais concluídos; {falhas} falharam"
                ))
            } else {
                Ok(format!("{processados} canais coletados"))
            };
            let _ = resultado_tx.send(mensagem).await;
        });
        ctx.request_repaint();
    }

    fn iniciar_relatorio(&mut self, ctx: &egui::Context) {
        if matches!(self.state.relatorio, EstadoRelatorio::EmGeracao { .. }) {
            self.state.toast("Já está a gerar um relatório");
            return;
        }
        let Some(canal_id) = self.state.canal_selecionado.clone() else {
            self.state.toast("Seleciona um canal primeiro");
            return;
        };
        let (tx, rx) = mpsc::channel(1);
        self.state.relatorio = EstadoRelatorio::EmGeracao { rx };

        let repositorio = Arc::clone(&self.state.deps.repositorio);
        let config = RelatorioConfig {
            periodo_dias: self.state.periodo_dias,
            modo: self.state.modo_analise,
            ..RelatorioConfig::default()
        };
        self.state.rt.spawn(async move {
            let caso = GerarRelatorioUseCase::new(repositorio, config);
            let metadados = MetadadosReproducao::new(
                infra_storage::VERSAO_SCHEMA,
                Some("desktop GUI".to_owned()),
            );
            let resultado = caso
                .executar(&canal_id, metadados)
                .await
                .map_err(|erro| erro.to_string());
            let _ = tx.send(resultado).await;
        });
        ctx.request_repaint();
    }

    fn iniciar_download_modelo(&mut self, ctx: &egui::Context) {
        if self.state.nlp_em_progresso {
            self.state.toast("Já há uma tarefa de NLP em curso");
            return;
        }
        let (tx, rx) = mpsc::channel(1);
        self.state.nlp_rx = Some(rx);
        self.state.nlp_em_progresso = true;
        self.state.nlp_progresso_msg = Some("A descarregar modelo NLP (~143 MB)...".to_owned());

        let ctx_tarefa = ctx.clone();
        let rt = self.state.rt.clone();
        ctx.request_repaint();
        rt.spawn(async move {
            let resultado = infra_nlp::ModelManager::novo()
                .garantir_modelo_baixado()
                .await
                .map(|_| EventoNlp::ModeloPronto)
                .map_err(|erro| erro.to_string());
            let _ = tx.send(resultado).await;
            ctx_tarefa.request_repaint();
        });
    }

    fn iniciar_analise_sentimentos(&mut self, ctx: &egui::Context) {
        if self.state.nlp_em_progresso {
            self.state.toast("Já há uma tarefa de NLP em curso");
            return;
        }
        if !self.state.nlp_modelo_pronto {
            self.state.toast("O modelo NLP ainda não está descarregado");
            return;
        }
        let Some(canal_id) = self.state.canal_selecionado.clone() else {
            self.state.toast("Seleciona um canal primeiro");
            return;
        };
        let pendentes = self.state.nlp_pendentes;

        let (tx, rx) = mpsc::channel(1);
        self.state.nlp_rx = Some(rx);
        self.state.nlp_em_progresso = true;
        self.state.nlp_progresso_msg = Some(format!(
            "A analisar {pendentes} comentários (inferência local)..."
        ));

        let repositorio = Arc::clone(&self.state.deps.repositorio);
        let ctx_tarefa = ctx.clone();
        let rt = self.state.rt.clone();
        ctx.request_repaint();
        rt.spawn(async move {
            let resultado = async {
                let pasta = infra_nlp::ModelManager::novo().base().to_path_buf();
                let analisador =
                    tokio::task::spawn_blocking(move || infra_nlp::RobertaAnalisador::new(&pasta))
                        .await
                        .map_err(|erro| format!("tarefa de carregamento do modelo falhou: {erro}"))?
                        .map_err(|erro| erro.to_string())?;
                let analisador: Arc<dyn AnalisadorDeSentimento> = Arc::new(analisador);
                let caso = AnalisarSentimentosUseCase::new(
                    analisador,
                    repositorio,
                    AnalisarConfig::default(),
                );
                let relatorio = caso
                    .executar(&canal_id)
                    .await
                    .map_err(|erro| erro.to_string())?;
                Ok(EventoNlp::AnaliseConcluida {
                    analisados: relatorio.analisados,
                    falhados: relatorio.falhados,
                    duracao_ms: u64::try_from(relatorio.duracao.as_millis()).unwrap_or(u64::MAX),
                })
            }
            .await;
            let _ = tx.send(resultado).await;
            ctx_tarefa.request_repaint();
        });
    }

    fn aplicar_categoria(&mut self, id: &CanalId, categoria: CategoriaManual, ctx: &egui::Context) {
        match self
            .state
            .deps
            .repositorio
            .definir_categoria_manual(id, categoria)
        {
            Ok(()) => {
                if let Some(canal) = self.state.canais.iter_mut().find(|canal| &canal.id == id) {
                    canal.categoria = Some(categoria);
                }
                self.state.toast("Categoria atualizada");
                ctx.request_repaint();
            }
            Err(erro) => self.state.toast(format!("Erro ao categorizar: {erro}")),
        }
    }

    fn topo(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("YouTube Tech Analyzer");
            ui.separator();
            ui.label(match self.state.ecra {
                Ecra::Canais => "Canais",
                Ecra::Relatorio => "Relatório",
                Ecra::Definicoes => "Definições",
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.state.deps.coletor.is_none() {
                    ui.colored_label(COR_AVISO, "YOUTUBE_API_KEY ausente");
                }
            });
        });
    }

    fn navegacao(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let mut destino = None;
        if ui
            .selectable_label(self.state.ecra == Ecra::Canais, "Canais")
            .clicked()
        {
            destino = Some(Ecra::Canais);
        }
        let tem_canal = self.state.canal_selecionado.is_some();
        ui.add_enabled_ui(tem_canal, |ui| {
            if ui
                .selectable_label(self.state.ecra == Ecra::Relatorio, "Relatório")
                .clicked()
            {
                destino = Some(Ecra::Relatorio);
            }
        });
        if ui
            .selectable_label(self.state.ecra == Ecra::Definicoes, "Definições")
            .clicked()
        {
            destino = Some(Ecra::Definicoes);
        }
        if let Some(destino) = destino {
            self.state.mudar_ecra(destino);
        }

        ui.separator();
        if let Some(canal) = self.state.canal_atual() {
            ui.label(RichText::new("Canal selecionado").small().weak());
            ui.label(&canal.nome);
            if let Some(categoria) = canal.categoria {
                ui.label(RichText::new(categoria.como_str()).color(COR_ACCENT));
            }
        } else {
            ui.label(RichText::new("Nenhum canal selecionado").weak());
        }
    }

    fn ecra_canais(&mut self, ui: &mut egui::Ui) {
        ui.heading("Canais");
        // TODO: filtro por texto quando a lista ultrapassar ~50 canais.
        let mut adicionar = false;
        ui.horizontal(|ui| {
            ui.label("URL ou @handle:");
            let resposta = ui.text_edit_singleline(&mut self.state.formulario_novo_canal);
            let enter =
                resposta.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button("Resolver e adicionar").clicked() || enter {
                adicionar = true;
            }
        });
        if self.state.deps.coletor.is_none() {
            ui.colored_label(
                COR_AVISO,
                "Sem YOUTUBE_API_KEY: listar e gerar relatórios continua disponível.",
            );
        }
        if adicionar {
            self.iniciar_adicionar(ui.ctx());
        }
        ui.separator();

        let mut categoria_alterada: Option<(CanalId, CategoriaManual)> = None;
        let mut coletar: Option<CanalId> = None;
        let mut ver_relatorio: Option<CanalId> = None;

        egui::ScrollArea::vertical()
            .max_height((ui.available_height() - 80.0).max(120.0))
            .show(ui, |ui| {
                egui::Grid::new("tabela_canais")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Nome");
                        ui.strong("Categoria");
                        ui.strong("Inscritos");
                        ui.strong("Vídeos");
                        ui.strong("Ações");
                        ui.end_row();

                        for canal in &self.state.canais {
                            ui.label(&canal.nome);
                            let mut categoria = canal.categoria;
                            egui::ComboBox::from_id_salt(format!("categoria-{}", canal.id))
                                .selected_text(
                                    categoria.map(|valor| valor.como_str()).unwrap_or("—"),
                                )
                                .show_ui(ui, |ui| {
                                    for opcao in CategoriaManual::todas() {
                                        ui.selectable_value(
                                            &mut categoria,
                                            Some(*opcao),
                                            opcao.como_str(),
                                        );
                                    }
                                });
                            if categoria != canal.categoria
                                && let Some(categoria) = categoria
                            {
                                categoria_alterada = Some((canal.id.clone(), categoria));
                            }
                            ui.label(
                                canal
                                    .inscritos
                                    .map(|valor| valor.to_string())
                                    .unwrap_or_else(|| "—".to_owned()),
                            );
                            ui.label(
                                canal
                                    .total_videos
                                    .map(|valor| valor.to_string())
                                    .unwrap_or_else(|| "—".to_owned()),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("Coletar").clicked() {
                                    coletar = Some(canal.id.clone());
                                }
                                if ui.button("Ver relatório").clicked() {
                                    ver_relatorio = Some(canal.id.clone());
                                }
                                ui.add_enabled(false, egui::Button::new("Remover"));
                            });
                            ui.end_row();
                        }
                    });
            });

        if let Some((id, categoria)) = categoria_alterada {
            self.aplicar_categoria(&id, categoria, ui.ctx());
        }
        if let Some(id) = coletar {
            self.state.selecionar_canal(id.clone());
            self.iniciar_coleta(ui.ctx(), id);
        }
        if let Some(id) = ver_relatorio {
            self.state.selecionar_canal(id);
            self.state.mudar_ecra(Ecra::Relatorio);
        }

        ui.separator();
        if ui.button("Coletar todos").clicked() {
            self.iniciar_coleta_todos(ui.ctx());
        }
    }

    fn ecra_relatorio(&mut self, ui: &mut egui::Ui) {
        let Some(canal_id) = self.state.canal_selecionado.clone() else {
            ui.label("Seleciona um canal no ecrã Canais.");
            return;
        };
        let Some(canal) = self.state.canal_atual().cloned() else {
            ui.label("O canal selecionado já não existe.");
            return;
        };

        if ui.input(|input| input.key_pressed(egui::Key::R) && input.modifiers.command) {
            self.iniciar_relatorio(ui.ctx());
        }

        ui.horizontal(|ui| {
            ui.heading(&canal.nome);
            let mut categoria = canal.categoria;
            egui::ComboBox::from_id_salt("categoria-relatorio")
                .selected_text(categoria.map(|valor| valor.como_str()).unwrap_or("—"))
                .show_ui(ui, |ui| {
                    for opcao in CategoriaManual::todas() {
                        ui.selectable_value(&mut categoria, Some(*opcao), opcao.como_str());
                    }
                });
            if categoria != canal.categoria
                && let Some(categoria) = categoria
            {
                self.aplicar_categoria(&canal_id, categoria, ui.ctx());
            }
        });

        let relatorio = match &self.state.relatorio {
            EstadoRelatorio::Pronto(relatorio) => Some(relatorio.as_ref().clone()),
            _ => None,
        };

        let mut metrica = self.state.metrica_plot;
        let mut periodo = self.state.periodo_dias;
        let mut modo = self.state.modo_analise;
        let mut escala_log = self.state.escala_log;
        let nlp_ui = NlpUi {
            modelo_pronto: self.state.nlp_modelo_pronto,
            em_progresso: self.state.nlp_em_progresso,
            progresso_msg: self.state.nlp_progresso_msg.clone(),
            pendentes: self.state.nlp_pendentes,
        };
        let metrica_plot = self.state.metrica_plot;
        let escala_log_plot = self.state.escala_log;
        let mut gerar = false;
        let mut copiar: Option<FormatoExport> = None;

        ui.horizontal(|ui| {
            if let Some(relatorio) = &relatorio {
                egui::ComboBox::from_id_salt("metrica-relatorio")
                    .selected_text(
                        self.state
                            .metrica_plot
                            .map(|valor| valor.as_str())
                            .unwrap_or("—"),
                    )
                    .show_ui(ui, |ui| {
                        for tendencia in &relatorio.tendencias {
                            ui.selectable_value(
                                &mut metrica,
                                Some(tendencia.metrica),
                                tendencia.metrica.as_str(),
                            );
                        }
                    });
            }
            ui.checkbox(&mut escala_log, "Escala Log")
                .on_hover_text("Eixo Y logarítmico; útil quando há outliers de views/dia.");
            ui.label("Análise:");
            egui::ComboBox::from_id_salt("modo-relatorio")
                .selected_text(match modo {
                    ModoAnalise::PorColeta => "Por coleta",
                    ModoAnalise::PorPublicacao => "Por publicação",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut modo,
                        ModoAnalise::PorColeta,
                        "Por coleta (série temporal)",
                    );
                    ui.selectable_value(
                        &mut modo,
                        ModoAnalise::PorPublicacao,
                        "Por publicação (cross-section)",
                    );
                });
            ui.label("Período:");
            egui::ComboBox::from_id_salt("periodo-relatorio")
                .selected_text(rotulo_periodo(periodo))
                .show_ui(ui, |ui| {
                    for (valor, rotulo) in OPCOES_PERIODO {
                        ui.selectable_value(&mut periodo, *valor, *rotulo);
                    }
                });
            if ui.button("Gerar relatório").clicked() {
                gerar = true;
            }
            if relatorio.is_some() {
                ui.separator();
                if ui.button("Markdown").clicked() {
                    copiar = Some(FormatoExport::Md);
                }
                if ui.button("CSV").clicked() {
                    copiar = Some(FormatoExport::Csv);
                }
                if ui.button("JSON").clicked() {
                    copiar = Some(FormatoExport::Json);
                }
            }
        });

        if metrica != self.state.metrica_plot {
            self.state.metrica_plot = metrica;
        }
        if periodo != self.state.periodo_dias {
            self.state.periodo_dias = periodo;
            self.state.marcar_prefs();
        }
        if escala_log != self.state.escala_log {
            self.state.escala_log = escala_log;
            self.state.marcar_prefs();
            ui.ctx().request_repaint();
        }
        if modo != self.state.modo_analise {
            self.state.modo_analise = modo;
            if modo == ModoAnalise::PorPublicacao {
                // Em cross-section a primeira impressão deve ser o rácio de
                // engagement, não views/dia dominado por outliers.
                self.state.metrica_plot = Some(Metrica::Curtidas);
            }
            self.state.marcar_prefs();
        }
        if modo == ModoAnalise::PorPublicacao {
            ui.colored_label(
                COR_AVISO,
                "Análise cross-section: usa a data de publicação de cada vídeo. Complementa a série temporal, não a substitui.",
            );
        }
        if gerar {
            self.iniciar_relatorio(ui.ctx());
        }
        if let Some(formato) = copiar
            && let Some(relatorio) = &relatorio
        {
            let texto = match formato {
                FormatoExport::Md => Ok(para_markdown(relatorio)),
                FormatoExport::Csv => Ok(para_csv(relatorio)),
                FormatoExport::Json => para_json(relatorio),
            };
            match texto {
                Ok(texto) => {
                    ui.ctx().copy_text(texto);
                    self.state.toast("Relatório copiado para o clipboard");
                }
                Err(erro) => self.state.toast(format!("Erro ao exportar: {erro}")),
            }
        }

        ui.separator();
        match &self.state.relatorio {
            EstadoRelatorio::EmGeracao { .. } => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label("A gerar relatório...");
                });
            }
            EstadoRelatorio::Erro(erro) => {
                ui.colored_label(COR_ERRO, erro);
                if ui.button("Tentar de novo").clicked() {
                    self.iniciar_relatorio(ui.ctx());
                }
            }
            EstadoRelatorio::Ocioso => {
                ui.label("Clica em \"Gerar relatório\" para analisar o canal.");
            }
            EstadoRelatorio::Pronto(relatorio) => {
                // O relatório é mais alto que a janela; sem scroll a secção
                // de diagnóstico e os avisos ficavam inalcançáveis.
                let acao = egui::ScrollArea::vertical()
                    .show(ui, |ui| {
                        render_relatorio(
                            ui,
                            relatorio.as_ref(),
                            metrica_plot,
                            escala_log_plot,
                            &nlp_ui,
                        )
                    })
                    .inner;
                match acao {
                    Some(AcaoNlp::BaixarModelo) => self.iniciar_download_modelo(ui.ctx()),
                    Some(AcaoNlp::Analisar { .. }) => {
                        self.iniciar_analise_sentimentos(ui.ctx());
                    }
                    None => {}
                }
            }
        }
    }

    fn ecra_definicoes(&mut self, ui: &mut egui::Ui) {
        ui.heading("Definições");
        ui.label(format!(
            "Base de dados: {}",
            self.state.caminho_db.display()
        ));
        let metadados = MetadadosReproducao::new(infra_storage::VERSAO_SCHEMA, None);
        ui.label(format!("Versão: {}", env!("CARGO_PKG_VERSION")));
        ui.label(format!("Commit: {}", metadados.hash_commit));
        ui.label(format!("Schema: {}", metadados.versao_schema));
        if ui.button("Abrir pasta da BD").clicked()
            && let Some(parent) = self.state.caminho_db.parent()
        {
            match std::process::Command::new("xdg-open").arg(parent).spawn() {
                Ok(_) => self.state.toast("A abrir a pasta da base de dados"),
                Err(erro) => self.state.toast(format!("Erro ao abrir a pasta: {erro}")),
            }
        }
        ui.separator();

        let mut formato = self.state.formato_export;
        ui.horizontal(|ui| {
            ui.label("Formato de exportação por omissão:");
            egui::ComboBox::from_id_salt("formato-definicoes")
                .selected_text(formato.como_str())
                .show_ui(ui, |ui| {
                    for opcao in FormatoExport::TODOS {
                        ui.selectable_value(&mut formato, opcao, opcao.como_str());
                    }
                });
        });
        if formato != self.state.formato_export {
            self.state.formato_export = formato;
            self.state.marcar_prefs();
        }

        let mut periodo = self.state.periodo_dias;
        ui.horizontal(|ui| {
            ui.label("Período por omissão:");
            egui::ComboBox::from_id_salt("periodo-definicoes")
                .selected_text(rotulo_periodo(periodo))
                .show_ui(ui, |ui| {
                    for (valor, rotulo) in OPCOES_PERIODO {
                        ui.selectable_value(&mut periodo, *valor, *rotulo);
                    }
                });
        });
        if periodo != self.state.periodo_dias {
            self.state.periodo_dias = periodo;
            self.state.marcar_prefs();
        }
    }

    fn render_toast(&mut self, ctx: &egui::Context) {
        let expirado = self
            .state
            .mensagem_toast
            .as_ref()
            .is_some_and(|(_, instante)| instante.elapsed() > Duration::from_secs(4));
        if expirado {
            self.state.mensagem_toast = None;
            return;
        }
        let Some((mensagem, _)) = self.state.mensagem_toast.clone() else {
            return;
        };
        ctx.request_repaint_after(Duration::from_secs(4));
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(mensagem);
                });
            });
    }

    fn render_modal_coleta(&mut self, ctx: &egui::Context) {
        let mut cancelar = false;
        if let EstadoColeta::EmCurso {
            progresso,
            iniciado_em,
            ..
        } = &self.state.coleta
        {
            egui::Window::new("Coleta em curso")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Canal: {}",
                        progresso.canal.as_deref().unwrap_or("—")
                    ));
                    ui.label(format!("Tempo: {}s", iniciado_em.elapsed().as_secs()));
                    ui.label(format!(
                        "Vídeo {}/{} · {} novos · {} já coletados · {} falhas",
                        progresso.video_atual,
                        progresso.total,
                        progresso.videos_novos,
                        progresso.videos_ja_coletados,
                        progresso.videos_falhados
                    ));
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for linha in &progresso.linhas {
                                if linha.erro {
                                    ui.colored_label(COR_ERRO, &linha.texto);
                                } else {
                                    ui.label(&linha.texto);
                                }
                            }
                        });
                    if ui.button("Cancelar").clicked() {
                        cancelar = true;
                    }
                });
        }
        if cancelar {
            if let EstadoColeta::EmCurso { cancelamento, .. } = &self.state.coleta {
                cancelamento.cancel();
            }
            self.state.toast("Cancelamento pedido");
        }

        let mut fechar = false;
        if let EstadoColeta::Concluida(mensagem) = &self.state.coleta {
            egui::Window::new("Coleta concluída")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(mensagem);
                    if ui.button("Fechar").clicked() {
                        fechar = true;
                    }
                });
        }
        if fechar {
            self.state.coleta = EstadoColeta::Ocioso;
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_tasks(ctx);
        self.state.guardar_prefs_se_preciso();
        if self.state.prefs_pendente_desde.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        self.render_toast(ctx);

        egui::TopBottomPanel::top("topo").show(ctx, |ui| self.topo(ui));
        egui::SidePanel::left("navegacao")
            .resizable(false)
            .default_width(180.0)
            .show(ctx, |ui| self.navegacao(ui));
        egui::CentralPanel::default().show(ctx, |ui| match self.state.ecra {
            Ecra::Canais => self.ecra_canais(ui),
            Ecra::Relatorio => self.ecra_relatorio(ui),
            Ecra::Definicoes => self.ecra_definicoes(ui),
        });
        self.render_modal_coleta(ctx);
    }
}

fn aplicar_tema(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(24, 24, 28);
    visuals.window_fill = Color32::from_rgb(32, 32, 38);
    visuals.selection.bg_fill = COR_ACCENT;
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    });
}

fn render_relatorio(
    ui: &mut egui::Ui,
    relatorio: &RelatorioCanal,
    metrica: Option<Metrica>,
    escala_log: bool,
    nlp: &NlpUi,
) -> Option<AcaoNlp> {
    ui.label(RichText::new(formatar_sumario(relatorio)).size(16.0));
    ui.add_space(8.0);

    if let Some(tendencia) = metrica
        .and_then(|metrica| relatorio.tendencias.iter().find(|t| t.metrica == metrica))
        .or_else(|| relatorio.tendencias.first())
    {
        render_plot(ui, tendencia, escala_log);
        ui.add_space(8.0);
    }

    let acao = render_nlp_bloco(ui, relatorio, nlp);
    ui.add_space(8.0);

    ui.heading("Tendências");
    egui::Grid::new("tendencias")
        .num_columns(7)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Métrica");
            ui.strong("N");
            ui.strong("Direção");
            ui.strong("τ");
            ui.strong("p");
            ui.strong("p (FDR)");
            ui.strong("Slope (IC95)");
            ui.end_row();
            for tendencia in &relatorio.tendencias {
                let significativa = tendencia.p_ajustado_fdr < relatorio.nivel_significancia;
                ui.label(tendencia.metrica.as_str());
                ui.label(tendencia.n_snapshots.to_string());
                ui.colored_label(
                    cor_direcao(tendencia.direcao, significativa),
                    direcao_str(tendencia.direcao),
                );
                ui.label(format!("{:.4}", tendencia.tau));
                ui.label(format!("{:.4}", tendencia.p_value));
                ui.label(format!("{:.4}", tendencia.p_ajustado_fdr));
                ui.label(format!(
                    "{} ({}, {}) {}",
                    application::relatorio::export::formatar_numero(tendencia.slope_theil_sen),
                    application::relatorio::export::formatar_numero(tendencia.slope_ic95.0),
                    application::relatorio::export::formatar_numero(tendencia.slope_ic95.1),
                    tendencia.unidade_slope
                ));
                ui.end_row();
            }
        });

    ui.add_space(8.0);
    ui.heading("Comparação com pares");
    if relatorio.comparacoes.is_empty() {
        ui.label("Sem pares comparáveis.");
    } else {
        for comparacao in &relatorio.comparacoes {
            ui.label(application::relatorio::export::texto_comparacao(comparacao));
        }
    }

    if let Some(janelas) = &relatorio.diagnostico_janelas {
        ui.add_space(8.0);
        ui.heading("Diagnóstico de Robustez (views/dia)");
        for linha in application::relatorio::export::linhas_diagnostico(janelas) {
            ui.label(format!("• {linha}"));
        }
        ui.colored_label(
            COR_AVISO,
            application::relatorio::export::nota_diagnostico(janelas),
        );
    }

    if relatorio.modo == ModoAnalise::PorPublicacao {
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Nota: views/dia é um proxy com viés conhecido — vídeos recentes concentram as visualizações nos primeiros dias, o que infla o slope. O rácio likes/views é independente da idade e é o sinal mais robusto.",
            )
            .small()
            .weak(),
        );
    }

    if !relatorio.avisos.is_empty() {
        ui.add_space(8.0);
        ui.heading("Avisos");
        for aviso in &relatorio.avisos {
            ui.colored_label(COR_AVISO, format!("• {aviso}"));
        }
    }

    acao
}

fn render_nlp_bloco(ui: &mut egui::Ui, relatorio: &RelatorioCanal, nlp: &NlpUi) -> Option<AcaoNlp> {
    ui.heading("Análise de Sentimento");
    if nlp.em_progresso {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(
                nlp.progresso_msg
                    .as_deref()
                    .unwrap_or("A processar pedidos de NLP..."),
            );
        });
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }

    if let Some(serie) = relatorio
        .serie_sentimento
        .as_deref()
        .filter(|serie| !serie.is_empty())
    {
        if let Some(resumo) = resumo_sentimento(serie) {
            ui.label(resumo);
        }
        render_plot_sentimento(ui, serie);
    } else {
        ui.label("Ainda não há comentários analisados para este canal.");
    }

    let mut pedido = None;
    if !nlp.em_progresso {
        let acao = acao_nlp(nlp.modelo_pronto, nlp.em_progresso, nlp.pendentes);
        if let Some(acao) = acao {
            if ui.button(acao.rotulo()).clicked() {
                pedido = Some(acao);
            }
        } else if nlp.modelo_pronto {
            ui.label(RichText::new("Todos os comentários já têm sentimento.").weak());
        }
    }

    ui.label(
        RichText::new(
            "Modelo: CardiffNLP twitter-roberta-base-sentiment-latest (Xenova, INT8, CC-BY-4.0). O processamento é local.",
        )
        .small()
        .weak(),
    );
    pedido
}

/// Rótulo do eixo X para uma marca inteira da série.
///
/// O `egui_plot` também gera marcas fraccionárias em séries curtas; sem esta
/// verificação, `0.5` arredondaria para o mesmo índice que `0.0` ou `1.0` e
/// duplicaria os rótulos.
fn rotulo_eixo_x(datas: &[NaiveDate], valor: f64, formato: &str) -> String {
    let indice = valor.round();
    if (valor - indice).abs() > 1e-6 || indice < 0.0 {
        return String::new();
    }
    datas
        .get(indice as usize)
        .map(|data| data.format(formato).to_string())
        .unwrap_or_default()
}

fn render_plot_sentimento(ui: &mut egui::Ui, serie: &[(NaiveDate, f64, f64)]) {
    ui.label(RichText::new("Evolução do Sentimento da Comunidade (Mensal)").strong());
    if let Some(media) = media_sentimento(serie) {
        ui.label(format!(
            "Score médio ponderado: {media:.2} ({}).",
            application::relatorio::export::classificacao_sentimento(media)
        ));
    }

    let n = serie.len();
    let max_contagem = serie
        .iter()
        .map(|(_, _, contagem)| *contagem)
        .fold(0.0, f64::max);
    let max_contagem = if max_contagem > 0.0 {
        max_contagem
    } else {
        1.0
    };
    let datas: Vec<NaiveDate> = serie.iter().map(|(data, _, _)| *data).collect();
    let x_final = n.saturating_sub(1) as f64;
    let pontos: Vec<[f64; 2]> = serie
        .iter()
        .enumerate()
        .map(|(indice, (_, score, _))| [indice as f64, score.clamp(-1.0, 1.0)])
        .collect();

    // A contagem não tem eixo próprio no egui_plot 0.29: é desenhada como
    // área sombreada no fundo, com altura relativa ao máximo do período.
    let mut area: Vec<[f64; 2]> = serie
        .iter()
        .enumerate()
        .map(|(indice, (_, _, contagem))| [indice as f64, -1.0 + 0.3 * (contagem / max_contagem)])
        .collect();
    area.extend((0..n).rev().map(|indice| [indice as f64, -1.0]));

    let plot = Plot::new("grafico-sentimento")
        .legend(Legend::default())
        .height(260.0)
        .include_y(-1.0)
        .include_y(1.0)
        .x_axis_formatter(move |mark, _| rotulo_eixo_x(&datas, mark.value, "%Y-%m"));

    plot.show(ui, |plot_ui| {
        if n >= 2 {
            plot_ui.polygon(
                Polygon::new(area)
                    .name("N comentários (escala relativa)")
                    .fill_color(Color32::from_rgba_unmultiplied(230, 190, 80, 35))
                    .stroke(egui::Stroke::new(0.0_f32, Color32::TRANSPARENT)),
            );
        }
        plot_ui.line(
            Line::new(vec![[0.0, 0.0], [x_final, 0.0]])
                .name("Neutro")
                .color(Color32::GRAY)
                .width(1.0_f32),
        );
        plot_ui.line(
            Line::new(pontos)
                .name("Score médio")
                .color(COR_ACCENT)
                .width(2.0_f32),
        );
    });
}

fn render_plot(ui: &mut egui::Ui, tendencia: &TendenciaMetrica, escala_log: bool) {
    let n = tendencia.pontos.len();
    if n < 2 {
        return;
    }

    // O egui_plot 0.29 não tem eixo logarítmico nativo: transformamos os
    // valores para ln antes de plotar e revertemos no formatter do eixo Y.
    // O piso é o menor valor positivo da série (não 1e-9): um único zero em
    // likes/views esticaria o eixo até ln(1e-9) e esmagaria tudo o resto.
    let piso = tendencia
        .pontos
        .iter()
        .map(|(_, valor)| *valor)
        .filter(|valor| *valor > 0.0)
        .fold(f64::INFINITY, f64::min);
    let piso = if piso.is_finite() && piso > 0.0 {
        piso
    } else {
        1e-9
    };
    let transformar = |valor: f64| {
        if escala_log {
            valor.max(piso).ln()
        } else {
            valor
        }
    };

    let pontos: Vec<[f64; 2]> = tendencia
        .pontos
        .iter()
        .enumerate()
        .filter_map(|(indice, (_, valor))| {
            if escala_log && *valor <= 0.0 {
                // Um zero não tem log: fica fora do gráfico em vez de virar pico.
                return None;
            }
            Some([indice as f64, transformar(*valor)])
        })
        .collect();
    let datas: Vec<NaiveDate> = tendencia.pontos.iter().map(|(data, _)| *data).collect();
    let x_final = (n - 1) as f64;

    let mut banda: Vec<[f64; 2]> = (0..n)
        .map(|indice| {
            let x = indice as f64;
            [
                x,
                transformar(tendencia.intercept_theil_sen + tendencia.slope_ic95.0 * x),
            ]
        })
        .collect();
    banda.extend((0..n).rev().map(|indice| {
        let x = indice as f64;
        [
            x,
            transformar(tendencia.intercept_theil_sen + tendencia.slope_ic95.1 * x),
        ]
    }));

    let inicio = transformar(tendencia.intercept_theil_sen);
    let fim = transformar(tendencia.intercept_theil_sen + tendencia.slope_theil_sen * x_final);

    let mut plot = Plot::new("grafico-serie")
        .legend(Legend::default())
        .height(300.0)
        .x_axis_formatter(move |mark, _| rotulo_eixo_x(&datas, mark.value, "%Y-%m-%d"));
    if escala_log {
        plot = plot.y_axis_label("log").y_axis_formatter(|mark, _| {
            application::relatorio::export::formatar_numero(mark.value.exp())
        });
    }

    plot.show(ui, |plot_ui| {
        plot_ui.polygon(
            Polygon::new(banda)
                .name("IC95")
                .fill_color(Color32::from_rgba_unmultiplied(64, 120, 220, 40))
                .stroke(egui::Stroke::new(0.0_f32, Color32::TRANSPARENT)),
        );
        plot_ui.line(
            Line::new(pontos)
                .name("Série")
                .color(Color32::from_rgb(90, 160, 255))
                .width(2.0_f32),
        );
        plot_ui.line(
            Line::new(vec![[0.0, inicio], [x_final, fim]])
                .name("Theil-Sen")
                .color(COR_SUCESSO)
                .width(2.0_f32),
        );
    });
}

/// Sumário curto do relatório para a GUI.
#[must_use]
pub fn formatar_sumario(relatorio: &RelatorioCanal) -> String {
    application::relatorio::export::texto_sumario(relatorio)
}

/// Opções de período disponíveis na GUI (dias, rótulo).
const OPCOES_PERIODO: &[(Option<u32>, &str)] = &[
    (None, "Tudo"),
    (Some(30), "30d"),
    (Some(60), "60d"),
    (Some(90), "90d"),
    (Some(180), "180d"),
    (Some(365), "365d"),
    (Some(730), "730d"),
];

fn rotulo_periodo(periodo: Option<u32>) -> &'static str {
    OPCOES_PERIODO
        .iter()
        .find(|(valor, _)| *valor == periodo)
        .map(|(_, rotulo)| *rotulo)
        .unwrap_or("Tudo")
}

/// Cor associada a uma direção.
#[must_use]
pub fn cor_direcao(direcao: Direcao, significativa: bool) -> Color32 {
    if !significativa {
        return Color32::GRAY;
    }
    match direcao {
        Direcao::Crescente => COR_SUCESSO,
        Direcao::Decrescente => COR_ERRO,
        Direcao::Estavel => Color32::GRAY,
    }
}

fn direcao_str(direcao: Direcao) -> &'static str {
    match direcao {
        Direcao::Crescente => "crescente",
        Direcao::Decrescente => "decrescente",
        Direcao::Estavel => "estável",
    }
}

#[cfg(test)]
mod tests {
    use super::{cor_direcao, formatar_sumario, rotulo_eixo_x};
    use analysis::Direcao;
    use application::{
        ComparacaoPares, MetadadosReproducao, ModoAnalise, RelatorioCanal, TendenciaMetrica,
    };
    use chrono::{NaiveDate, Utc};
    use domain::entities::canal::Canal;
    use domain::ids::CanalId;
    use eframe::egui::Color32;

    fn relatorio(tendencias: Vec<TendenciaMetrica>) -> RelatorioCanal {
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
            periodo_inicio: NaiveDate::from_ymd_opt(2024, 1, 1).expect("data"),
            periodo_fim: NaiveDate::from_ymd_opt(2024, 1, 30).expect("data"),
            modo: ModoAnalise::PorColeta,
            tendencias,
            diagnostico_janelas: None,
            comparacoes: Vec::<ComparacaoPares>::new(),
            serie_sentimento: None,
            avisos: Vec::new(),
            metadados: MetadadosReproducao {
                hash_commit: "abc12345".to_owned(),
                versao_schema: "3".to_owned(),
                comando: None,
                gerado_em: Utc::now(),
                versao_app: "0.1.0".to_owned(),
            },
            nivel_significancia: 0.05,
            aviso_amostra_minima: 20,
        }
    }

    #[test]
    fn resumo_sem_metricas_nao_diz_zero_metricas() {
        let texto = formatar_sumario(&relatorio(Vec::new()));
        assert!(texto.contains("não foi possível analisar nenhuma métrica"));
        assert!(texto.contains("dias distintos"));
        assert!(!texto.contains("0 métricas"));
    }

    #[test]
    fn cores_de_direcao_respeitam_significancia() {
        assert_eq!(cor_direcao(Direcao::Crescente, false), Color32::GRAY);
        assert_eq!(cor_direcao(Direcao::Crescente, true), super::COR_SUCESSO);
        assert_eq!(cor_direcao(Direcao::Decrescente, true), super::COR_ERRO);
    }

    #[test]
    fn formatter_do_eixo_x_nao_duplica_rotulos_em_series_curtas() {
        let datas = [
            NaiveDate::from_ymd_opt(2024, 1, 1).expect("data"),
            NaiveDate::from_ymd_opt(2024, 2, 1).expect("data"),
        ];

        assert_eq!(rotulo_eixo_x(&datas, 0.0, "%Y-%m"), "2024-01");
        assert_eq!(rotulo_eixo_x(&datas, 1.0, "%Y-%m"), "2024-02");
        // Marcas fraccionárias não devem arredondar para um índice vizinho.
        assert_eq!(rotulo_eixo_x(&datas, 0.5, "%Y-%m"), "");
        assert_eq!(rotulo_eixo_x(&datas, 1.5, "%Y-%m"), "");
        // Fora da série também não há rótulo.
        assert_eq!(rotulo_eixo_x(&datas, 2.0, "%Y-%m"), "");
    }
}
