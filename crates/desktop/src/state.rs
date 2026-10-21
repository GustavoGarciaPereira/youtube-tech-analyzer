use std::path::PathBuf;
use std::time::{Duration, Instant};

use application::{EventoProgresso, ModoAnalise, RelatorioCanal, RelatorioColeta};
use domain::{CanalId, CategoriaManual, Metrica};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::prefs::{Preferencias, carregar};
use crate::wiring::Dependencias;

/// Ecrã ativo da GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecra {
    Canais,
    Relatorio,
    Definicoes,
}

impl Ecra {
    /// Representação estável para preferências.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::Canais => "canais",
            Self::Relatorio => "relatorio",
            Self::Definicoes => "definicoes",
        }
    }

    /// Interpreta a representação persistida.
    #[must_use]
    pub fn de_str(valor: &str) -> Option<Self> {
        match valor {
            "canais" => Some(Self::Canais),
            "relatorio" => Some(Self::Relatorio),
            "definicoes" => Some(Self::Definicoes),
            _ => None,
        }
    }
}

/// Canal numa lista leve da GUI.
#[derive(Debug, Clone)]
pub struct CanalResumo {
    /// Identificador.
    pub id: CanalId,
    /// Nome público.
    pub nome: String,
    /// Categoria manual.
    pub categoria: Option<CategoriaManual>,
    /// Inscritos no último snapshot.
    pub inscritos: Option<u64>,
    /// Total de vídeos no último snapshot.
    pub total_videos: Option<u64>,
}

/// Formato de exportação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatoExport {
    Md,
    Csv,
    Json,
}

impl FormatoExport {
    /// Todos os formatos.
    pub const TODOS: [Self; 3] = [Self::Md, Self::Csv, Self::Json];

    /// Representação estável.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::Md => "md",
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }

    /// Interpreta a representação persistida.
    #[must_use]
    pub fn de_str(valor: &str) -> Option<Self> {
        match valor {
            "md" => Some(Self::Md),
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// Linha do histórico de progresso.
#[derive(Debug, Clone)]
pub struct LinhaProgresso {
    /// Texto a apresentar.
    pub texto: String,
    /// Linha de erro.
    pub erro: bool,
}

/// Estado acumulado de uma coleta.
#[derive(Debug, Clone, Default)]
pub struct ProgressoColeta {
    /// Nome do canal resolvido.
    pub canal: Option<String>,
    /// Índice do vídeo em processamento.
    pub video_atual: usize,
    /// Total de vídeos a processar.
    pub total: usize,
    /// Vídeos cujos comentários foram processados.
    pub videos_novos: usize,
    /// Vídeos cujos comentários já existiam.
    pub videos_ja_coletados: usize,
    /// Vídeos cuja coleta falhou.
    pub videos_falhados: usize,
    /// Últimas linhas do progresso.
    pub linhas: Vec<LinhaProgresso>,
}

impl ProgressoColeta {
    /// Aplica um evento de progresso, devolvendo o relatório no fim.
    pub fn aplicar(&mut self, evento: EventoProgresso) -> Option<RelatorioColeta> {
        match evento {
            EventoProgresso::CanalResolvido { nome, total_videos } => {
                self.canal = Some(nome.clone());
                self.push(format!("Canal: {nome} ({total_videos} vídeos)"), false);
            }
            EventoProgresso::VideosListados { total } => {
                self.total = total;
                self.push(format!("Vídeos a processar: {total}"), false);
            }
            EventoProgresso::VideoIniciado {
                indice,
                total,
                video_id,
                ..
            } => {
                self.video_atual = indice + 1;
                self.total = total;
                self.push(format!("→ {video_id}"), false);
            }
            EventoProgresso::VideoConcluido {
                video_id,
                comentarios,
            } => {
                self.videos_novos += 1;
                self.push(format!("✓ {video_id}: {comentarios} comentários"), false);
            }
            EventoProgresso::VideoFalhou { video_id, motivo } => {
                self.videos_falhados += 1;
                self.push(format!("✗ {video_id}: {motivo}"), true);
            }
            EventoProgresso::Finalizado { relatorio } => {
                self.videos_novos = relatorio.videos_novos;
                self.videos_ja_coletados = relatorio.videos_ja_coletados;
                self.videos_falhados = relatorio.videos_falhados;
                self.push(
                    format!(
                        "Concluído: {} novos, {} já coletados, {} falhas, {} comentários",
                        relatorio.videos_novos,
                        relatorio.videos_ja_coletados,
                        relatorio.videos_falhados,
                        relatorio.comentarios_total
                    ),
                    false,
                );
                return Some(relatorio);
            }
        }
        None
    }

    fn push(&mut self, texto: String, erro: bool) {
        self.linhas.push(LinhaProgresso { texto, erro });
        if self.linhas.len() > 20 {
            self.linhas.remove(0);
        }
    }
}

/// Estado da coleta em curso ou concluída.
#[derive(Default)]
pub enum EstadoColeta {
    #[default]
    Ocioso,
    EmCurso {
        cancelamento: CancellationToken,
        eventos_rx: mpsc::Receiver<EventoProgresso>,
        resultado_rx: mpsc::Receiver<Result<String, String>>,
        progresso: ProgressoColeta,
        iniciado_em: Instant,
    },
    Concluida(String),
}

/// Estado da geração de relatório.
#[derive(Default)]
pub enum EstadoRelatorio {
    #[default]
    Ocioso,
    EmGeracao {
        rx: mpsc::Receiver<Result<RelatorioCanal, String>>,
    },
    Pronto(Box<RelatorioCanal>),
    Erro(String),
}

/// Estado completo da aplicação.
pub struct AppState {
    /// Dependências de infraestrutura.
    pub deps: Dependencias,
    /// Handle do runtime tokio.
    pub rt: tokio::runtime::Handle,
    /// Caminho da base de dados.
    pub caminho_db: PathBuf,
    /// Ecrã ativo.
    pub ecra: Ecra,
    /// Canais carregados.
    pub canais: Vec<CanalResumo>,
    /// Canal selecionado.
    pub canal_selecionado: Option<CanalId>,
    /// Preferências.
    pub prefs: Preferencias,
    /// Coleta em curso.
    pub coleta: EstadoColeta,
    /// Relatório.
    pub relatorio: EstadoRelatorio,
    /// Input do novo canal.
    pub formulario_novo_canal: String,
    /// Toast e instante de criação.
    pub mensagem_toast: Option<(String, Instant)>,
    /// Receção do canal adicionado.
    pub add_rx: Option<mpsc::Receiver<Result<CanalId, String>>>,
    /// Formato de exportação por omissão.
    pub formato_export: FormatoExport,
    /// Período de análise.
    pub periodo_dias: Option<u32>,
    /// Eixo temporal usado no relatório.
    pub modo_analise: ModoAnalise,
    /// Escala logarítmica no gráfico de tendência.
    pub escala_log: bool,
    /// Métrica selecionada no gráfico.
    pub metrica_plot: Option<Metrica>,
    /// Instante da primeira alteração de preferências ainda não guardada.
    pub prefs_pendente_desde: Option<Instant>,
}

impl AppState {
    /// Cria o estado, carregando preferências e canais.
    #[must_use]
    pub fn novo(deps: Dependencias, rt: tokio::runtime::Handle, caminho_db: PathBuf) -> Self {
        let prefs = carregar();
        let ecra = prefs
            .ultimo_ecra
            .as_deref()
            .and_then(Ecra::de_str)
            .unwrap_or(Ecra::Canais);
        let formato_export = prefs
            .formato_export_default
            .as_deref()
            .and_then(FormatoExport::de_str)
            .unwrap_or(FormatoExport::Md);
        let periodo_dias = prefs.periodo_dias_default;
        let modo_analise = prefs
            .modo_analise_default
            .as_deref()
            .and_then(ModoAnalise::de_str)
            .unwrap_or(ModoAnalise::PorColeta);
        let prefs_log_scale = prefs.log_scale;

        let mut state = Self {
            deps,
            rt,
            caminho_db,
            ecra,
            canais: Vec::new(),
            canal_selecionado: None,
            prefs,
            coleta: EstadoColeta::Ocioso,
            relatorio: EstadoRelatorio::Ocioso,
            formulario_novo_canal: String::new(),
            mensagem_toast: None,
            add_rx: None,
            formato_export,
            periodo_dias,
            modo_analise,
            escala_log: prefs_log_scale,
            metrica_plot: Some(Metrica::Visualizacoes),
            prefs_pendente_desde: None,
        };

        if let Err(erro) = state.refresh_canais() {
            state.toast(format!("Erro ao carregar canais: {erro}"));
        }
        if let Some(id) = state.prefs.ultimo_canal.as_deref() {
            let id = CanalId::new(id);
            if state.canais.iter().any(|canal| canal.id == id) {
                state.canal_selecionado = Some(id);
            }
        }
        if state.ecra == Ecra::Relatorio && state.canal_selecionado.is_none() {
            state.ecra = Ecra::Canais;
        }

        state
    }

    /// Recarrega canais e categorias do repositório.
    pub fn refresh_canais(&mut self) -> Result<(), String> {
        let canais = self
            .deps
            .repositorio
            .listar_canais()
            .map_err(|erro| erro.to_string())?;
        let mut resumos = Vec::with_capacity(canais.len());
        for canal in canais {
            let categoria = self
                .deps
                .repositorio
                .categoria_manual_do_canal(&canal.id)
                .map_err(|erro| erro.to_string())?;
            resumos.push(CanalResumo {
                id: canal.id,
                nome: canal.nome,
                categoria,
                inscritos: Some(canal.inscritos),
                total_videos: Some(canal.total_videos),
            });
        }
        self.canais = resumos;

        if self
            .canal_selecionado
            .as_ref()
            .is_some_and(|id| !self.canais.iter().any(|canal| &canal.id == id))
        {
            self.canal_selecionado = None;
        }
        Ok(())
    }

    /// Seleciona um canal e agenda a gravação das preferências.
    pub fn selecionar_canal(&mut self, id: CanalId) {
        self.canal_selecionado = Some(id);
        self.marcar_prefs();
    }

    /// Muda de ecrã e agenda a gravação das preferências.
    pub fn mudar_ecra(&mut self, ecra: Ecra) {
        self.ecra = ecra;
        self.marcar_prefs();
    }

    /// Marca as preferências como alteradas para gravação com debounce.
    pub fn marcar_prefs(&mut self) {
        self.prefs_pendente_desde = Some(Instant::now());
    }

    /// Grava as preferências após 500 ms sem alterações.
    pub fn guardar_prefs_se_preciso(&mut self) {
        let Some(desde) = self.prefs_pendente_desde else {
            return;
        };
        if desde.elapsed() < Duration::from_millis(500) {
            return;
        }

        self.prefs.ultimo_canal = self
            .canal_selecionado
            .as_ref()
            .map(|id| id.as_str().to_owned());
        self.prefs.ultimo_ecra = Some(self.ecra.como_str().to_owned());
        self.prefs.formato_export_default = Some(self.formato_export.como_str().to_owned());
        self.prefs.periodo_dias_default = self.periodo_dias;
        self.prefs.modo_analise_default = Some(self.modo_analise.como_str().to_owned());
        self.prefs.log_scale = self.escala_log;
        if let Err(erro) = self.prefs.guardar() {
            self.toast(format!("Erro ao guardar preferências: {erro}"));
        }
        self.prefs_pendente_desde = None;
    }

    /// Mostra uma mensagem efémera.
    pub fn toast(&mut self, mensagem: impl Into<String>) {
        self.mensagem_toast = Some((mensagem.into(), Instant::now()));
    }

    /// Canal selecionado, se existir na lista.
    #[must_use]
    pub fn canal_atual(&self) -> Option<&CanalResumo> {
        let id = self.canal_selecionado.as_ref()?;
        self.canais.iter().find(|canal| &canal.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::{Ecra, FormatoExport, ProgressoColeta};
    use application::EventoProgresso;
    use domain::ids::VideoId;

    #[test]
    fn progresso_conta_eventos() {
        let mut progresso = ProgressoColeta::default();
        progresso.aplicar(EventoProgresso::CanalResolvido {
            nome: "Canal".to_owned(),
            total_videos: 2,
        });
        progresso.aplicar(EventoProgresso::VideosListados { total: 2 });
        progresso.aplicar(EventoProgresso::VideoIniciado {
            indice: 0,
            total: 2,
            video_id: VideoId::new("v1"),
            titulo: "Vídeo 1".to_owned(),
        });
        progresso.aplicar(EventoProgresso::VideoConcluido {
            video_id: VideoId::new("v1"),
            comentarios: 3,
        });
        progresso.aplicar(EventoProgresso::VideoFalhou {
            video_id: VideoId::new("v2"),
            motivo: "erro".to_owned(),
        });

        assert_eq!(progresso.canal.as_deref(), Some("Canal"));
        assert_eq!(progresso.total, 2);
        assert_eq!(progresso.video_atual, 1);
        assert_eq!(progresso.videos_novos, 1);
        assert_eq!(progresso.videos_falhados, 1);
        assert!(progresso.linhas.len() <= 20);
    }

    #[test]
    fn formatos_e_ecras_fazem_roundtrip() {
        for formato in FormatoExport::TODOS {
            assert_eq!(FormatoExport::de_str(formato.como_str()), Some(formato));
        }
        for ecra in [Ecra::Canais, Ecra::Relatorio, Ecra::Definicoes] {
            assert_eq!(Ecra::de_str(ecra.como_str()), Some(ecra));
        }
    }
}
