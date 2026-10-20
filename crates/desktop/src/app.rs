use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use analysis::Direcao;
use application::relatorio::export::{para_csv, para_json, para_markdown};
use application::{
    ColetaConfig, ColetarCanalUseCase, GerarRelatorioUseCase, MetadadosReproducao, RelatorioCanal,
    RelatorioConfig, TendenciaMetrica,
};
use domain::{CanalId, CanalRef, CategoriaManual, Metrica};
use eframe::egui;
use egui::{Color32, RichText};
use egui_plot::{Legend, Line, Plot, Polygon};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::state::{AppState, Ecra, EstadoColeta, EstadoRelatorio, FormatoExport, ProgressoColeta};
use crate::wiring::Dependencias;

const COR_ACCENT: Color32 = Color32::from_rgb(64, 120, 220);
const COR_SUCESSO: Color32 = Color32::from_rgb(80, 190, 120);
const COR_ERRO: Color32 = Color32::from_rgb(220, 90, 90);
const COR_AVISO: Color32 = Color32::from_rgb(230, 190, 80);

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
        let estado = std::mem::replace(&mut self.state.relatorio, EstadoRelatorio::Ocioso);
        match estado {
            EstadoRelatorio::EmGeracao { mut rx } => match rx.try_recv() {
                Ok(Ok(relatorio)) => {
                    self.state.metrica_plot = relatorio
                        .tendencias
                        .first()
                        .map(|tendencia| tendencia.metrica);
                    self.state.relatorio = EstadoRelatorio::Pronto(Box::new(relatorio));
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
            ui.label("Período:");
            egui::ComboBox::from_id_salt("periodo-relatorio")
                .selected_text(match periodo {
                    Some(30) => "30d",
                    Some(60) => "60d",
                    Some(90) => "90d",
                    _ => "Tudo",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut periodo, None, "Tudo");
                    ui.selectable_value(&mut periodo, Some(30), "30d");
                    ui.selectable_value(&mut periodo, Some(60), "60d");
                    ui.selectable_value(&mut periodo, Some(90), "90d");
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
                render_relatorio(ui, relatorio.as_ref(), self.state.metrica_plot);
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
                .selected_text(match periodo {
                    Some(30) => "30d",
                    Some(60) => "60d",
                    Some(90) => "90d",
                    _ => "Tudo",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut periodo, None, "Tudo");
                    ui.selectable_value(&mut periodo, Some(30), "30d");
                    ui.selectable_value(&mut periodo, Some(60), "60d");
                    ui.selectable_value(&mut periodo, Some(90), "90d");
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

fn render_relatorio(ui: &mut egui::Ui, relatorio: &RelatorioCanal, metrica: Option<Metrica>) {
    ui.label(RichText::new(formatar_sumario(relatorio)).size(16.0));
    ui.add_space(8.0);

    if let Some(tendencia) = metrica
        .and_then(|metrica| relatorio.tendencias.iter().find(|t| t.metrica == metrica))
        .or_else(|| relatorio.tendencias.first())
    {
        render_plot(ui, tendencia);
        ui.add_space(8.0);
    }

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
                    "{:.4} ({:.4}, {:.4})",
                    tendencia.slope_theil_sen, tendencia.slope_ic95.0, tendencia.slope_ic95.1
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
            ui.label(texto_comparacao(comparacao));
        }
    }

    if !relatorio.avisos.is_empty() {
        ui.add_space(8.0);
        ui.heading("Avisos");
        for aviso in &relatorio.avisos {
            ui.colored_label(COR_AVISO, format!("• {aviso}"));
        }
    }
}

fn render_plot(ui: &mut egui::Ui, tendencia: &TendenciaMetrica) {
    let n = tendencia.pontos.len();
    if n < 2 {
        return;
    }

    let pontos: Vec<[f64; 2]> = tendencia
        .pontos
        .iter()
        .enumerate()
        .map(|(indice, (_, valor))| [indice as f64, *valor])
        .collect();
    let datas = tendencia.pontos.clone();
    let x_final = (n - 1) as f64;
    let y_final = tendencia.intercept_theil_sen + tendencia.slope_theil_sen * x_final;

    let mut banda: Vec<[f64; 2]> = (0..n)
        .map(|indice| {
            let x = indice as f64;
            [
                x,
                tendencia.intercept_theil_sen + tendencia.slope_ic95.0 * x,
            ]
        })
        .collect();
    banda.extend((0..n).rev().map(|indice| {
        let x = indice as f64;
        [
            x,
            tendencia.intercept_theil_sen + tendencia.slope_ic95.1 * x,
        ]
    }));

    Plot::new("grafico-serie")
        .legend(Legend::default())
        .height(300.0)
        .x_axis_formatter(move |mark, _| {
            let indice = mark.value.round();
            if indice < 0.0 {
                return String::new();
            }
            datas
                .get(indice as usize)
                .map(|(data, _)| data.format("%Y-%m-%d").to_string())
                .unwrap_or_default()
        })
        .show(ui, |plot_ui| {
            plot_ui.polygon(
                Polygon::new(banda)
                    .name("IC95")
                    .fill_color(Color32::from_rgba_unmultiplied(64, 120, 220, 40))
                    .stroke(egui::Stroke::new(0.0, Color32::TRANSPARENT)),
            );
            plot_ui.line(
                Line::new(pontos)
                    .name("Série")
                    .color(Color32::from_rgb(90, 160, 255))
                    .width(2.0),
            );
            plot_ui.line(
                Line::new(vec![
                    [0.0, tendencia.intercept_theil_sen],
                    [x_final, y_final],
                ])
                .name("Theil-Sen")
                .color(COR_SUCESSO)
                .width(2.0),
            );
        });
}

/// Sumário curto do relatório para a GUI.
#[must_use]
pub fn formatar_sumario(relatorio: &RelatorioCanal) -> String {
    let total = relatorio.tendencias.len();
    if total == 0 {
        return format!(
            "No período de {} a {}, não foi possível analisar nenhuma métrica. Vê os avisos.",
            relatorio.periodo_inicio, relatorio.periodo_fim
        );
    }

    let significativas: Vec<&TendenciaMetrica> = relatorio
        .tendencias
        .iter()
        .filter(|tendencia| tendencia.p_ajustado_fdr < relatorio.nivel_significancia)
        .collect();

    if significativas.is_empty() {
        return format!(
            "No período de {} a {}, nenhuma das {} métricas analisadas apresenta tendência estatisticamente significativa.",
            relatorio.periodo_inicio, relatorio.periodo_fim, total
        );
    }

    let direcao = if significativas
        .iter()
        .all(|tendencia| tendencia.direcao == Direcao::Crescente)
    {
        "crescente"
    } else if significativas
        .iter()
        .all(|tendencia| tendencia.direcao == Direcao::Decrescente)
    {
        "decrescente"
    } else {
        "mista"
    };

    format!(
        "No período de {} a {}, {} de {} métricas analisadas apresentam tendência {} estatisticamente significativa (p<{} após FDR).",
        relatorio.periodo_inicio,
        relatorio.periodo_fim,
        significativas.len(),
        total,
        direcao,
        relatorio.nivel_significancia
    )
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

fn texto_comparacao(comparacao: &application::ComparacaoPares) -> String {
    let grupo = if comparacao.grupo == "todos" {
        "na base de dados".to_owned()
    } else if let Some(categoria) = comparacao.grupo.strip_prefix("categoria:") {
        format!("da categoria `{categoria}`")
    } else {
        format!("do grupo `{}`", comparacao.grupo)
    };
    format!(
        "O slope do canal ({:.4}) está no percentil {:.1} face a {} canais {}. Mediana dos pares: {:.4}.",
        comparacao.slope_canal,
        comparacao.percentil_canal_entre_pares,
        comparacao.n_pares,
        grupo,
        comparacao.slope_mediano_pares
    )
}

#[cfg(test)]
mod tests {
    use super::{cor_direcao, formatar_sumario};
    use analysis::Direcao;
    use application::{ComparacaoPares, MetadadosReproducao, RelatorioCanal, TendenciaMetrica};
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
            tendencias,
            comparacoes: Vec::<ComparacaoPares>::new(),
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
        assert!(!texto.contains("0 métricas"));
    }

    #[test]
    fn cores_de_direcao_respeitam_significancia() {
        assert_eq!(cor_direcao(Direcao::Crescente, false), Color32::GRAY);
        assert_eq!(cor_direcao(Direcao::Crescente, true), super::COR_SUCESSO);
        assert_eq!(cor_direcao(Direcao::Decrescente, true), super::COR_ERRO);
    }
}
