#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use application::relatorio::export::{para_csv, para_json, para_markdown};
use application::{
    ColetaConfig, ColetarCanalUseCase, EventoProgresso, GerarRelatorioUseCase, MetadadosReproducao,
    ModoAnalise, RelatorioConfig,
};
use clap::{Parser, Subcommand, ValueEnum};
use desktop::wiring;
use domain::ports::RepositorioDeCanais;
use domain::{CanalId, CanalRef, CategoriaManual};
use infra_youtube::{MAX_DEPTH_PADRAO, parse_canal_ref};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
#[command(name = "yta", version, about = "YouTube Tech Analyzer CLI")]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Debug, Subcommand)]
enum Comando {
    /// Gere um canal.
    Canal {
        #[command(subcommand)]
        comando: CanalComando,
    },
    /// Operações sobre todos os canais.
    Canais {
        #[command(subcommand)]
        comando: CanaisComando,
    },
    /// Gera um relatório de um canal.
    Relatorio {
        /// Canal (ID, @handle ou URL).
        canal: String,
        /// Formato de saída.
        #[arg(long, value_enum, default_value_t = Formato::Md)]
        formato: Formato,
        /// Ficheiro de saída. Sem esta opção escreve em stdout.
        #[arg(long)]
        saida: Option<PathBuf>,
        /// Limita a análise aos últimos N dias (coleta ou publicação, conforme --modo).
        #[arg(long)]
        periodo_dias: Option<u32>,
        /// Eixo temporal: série de coletas ou cross-section por publicação.
        #[arg(long, value_enum, default_value_t = ModoArg::PorColeta)]
        modo: ModoArg,
        /// Idade mínima dos vídeos, em dias (só em --modo por-publicacao).
        #[arg(long, default_value_t = 7)]
        idade_minima_dias: u32,
        /// Não normaliza por idade no modo por-publicacao (usa valores brutos).
        #[arg(long)]
        sem_normalizacao: bool,
    },
    /// Prepara modelos locais de NLP.
    Modelos {
        #[command(subcommand)]
        comando: ModelosComando,
    },
    /// Informações sobre a base de dados.
    Db {
        #[command(subcommand)]
        comando: DbComando,
    },
}

#[derive(Debug, Subcommand)]
enum CanalComando {
    /// Resolve um canal e guarda os seus metadados.
    Adicionar {
        /// URL, @handle ou channel ID.
        referencia: String,
    },
    /// Lista os canais guardados.
    Listar,
    /// Define a categoria manual de um canal.
    Categorizar {
        /// ID, @handle ou URL.
        referencia: String,
        /// Categoria manual.
        #[arg(long, value_parser = parse_categoria)]
        categoria: CategoriaManual,
    },
    /// Coleta vídeos e comentários de um canal.
    Coletar {
        /// ID, @handle ou URL.
        referencia: String,
        /// Limita aos N vídeos mais recentes.
        #[arg(long)]
        max_videos: Option<usize>,
        /// Profundidade de respostas a comentários.
        #[arg(long)]
        max_depth: Option<i32>,
        /// Força a re-busca de comentários já coletados.
        #[arg(long)]
        recoletar_comentarios: bool,
    },
}

#[derive(Debug, Subcommand)]
enum CanaisComando {
    /// Coleta todos os canais guardados.
    ColetarTodos {
        /// Limita aos N vídeos mais recentes de cada canal.
        #[arg(long)]
        max_videos: Option<usize>,
        /// Força a re-busca de comentários já coletados.
        #[arg(long)]
        recoletar_comentarios: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ModelosComando {
    /// Descarrega o modelo de sentimento (~128 MB) para a pasta de dados.
    BaixarSentimento,
}

#[derive(Debug, Subcommand)]
enum DbComando {
    /// Imprime o caminho da base de dados em uso.
    Caminho,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ModoArg {
    /// Série temporal por dia de coleta (default).
    PorColeta,
    /// Cross-section por data de publicação de cada vídeo.
    PorPublicacao,
}

impl ModoArg {
    const fn interno(self) -> ModoAnalise {
        match self {
            Self::PorColeta => ModoAnalise::PorColeta,
            Self::PorPublicacao => ModoAnalise::PorPublicacao,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Formato {
    Md,
    Csv,
    Json,
}

fn parse_categoria(valor: &str) -> Result<CategoriaManual, String> {
    CategoriaManual::de_str(valor).ok_or_else(|| {
        let validas = CategoriaManual::todas()
            .iter()
            .map(|categoria| categoria.como_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("categoria inválida: {valor}; usa uma de: {validas}")
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Carrega .env do diretório atual ou de um diretório pai. Variáveis já
    // exportadas na shell têm prioridade.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let caminho = wiring::caminho_db()?;

    match cli.comando {
        Comando::Db {
            comando: DbComando::Caminho,
        } => println!("{}", caminho.display()),
        Comando::Modelos {
            comando: ModelosComando::BaixarSentimento,
        } => {
            let gestor = infra_nlp::ModelManager::novo();
            eprintln!(
                "A garantir modelo de sentimento (~150 MB) em {}",
                gestor.base().display()
            );
            let pasta = gestor.garantir_modelo_baixado().await?;
            println!("{}", pasta.display());
        }
        Comando::Canal { comando } => executar_canal(comando, &caminho).await?,
        Comando::Canais { comando } => executar_canais(comando, &caminho).await?,
        Comando::Relatorio {
            canal,
            formato,
            saida,
            periodo_dias,
            modo,
            idade_minima_dias,
            sem_normalizacao,
        } => {
            let opcoes = OpcoesRelatorio {
                formato,
                saida,
                periodo_dias,
                modo: modo.interno(),
                idade_minima_dias,
                views_por_dia: !sem_normalizacao,
            };
            executar_relatorio(&canal, &opcoes, &caminho).await?;
        }
    }

    Ok(())
}

async fn executar_canal(comando: CanalComando, caminho: &Path) -> anyhow::Result<()> {
    match comando {
        CanalComando::Adicionar { referencia } => {
            let repositorio = wiring::abrir_repositorio(caminho)?;
            let coletor = wiring::montar_coletor(None, MAX_DEPTH_PADRAO)?;
            let canal_ref = parse_canal_ref(&referencia)?;
            let canal = coletor.coletar_canal(&canal_ref).await?;
            repositorio.guardar_canal(&canal)?;
            println!("{}", canal.id);
        }
        CanalComando::Listar => {
            let repositorio = wiring::abrir_repositorio(caminho)?;
            listar_canais(repositorio.as_ref())?;
        }
        CanalComando::Categorizar {
            referencia,
            categoria,
        } => {
            let repositorio = wiring::abrir_repositorio(caminho)?;
            let canal_id = resolver_canal_local(repositorio.as_ref(), &referencia)?;
            repositorio.definir_categoria_manual(&canal_id, categoria)?;
            println!("{} → {}", canal_id, categoria.como_str());
        }
        CanalComando::Coletar {
            referencia,
            max_videos,
            max_depth,
            recoletar_comentarios,
        } => {
            let repositorio = wiring::abrir_repositorio(caminho)?;
            let coletor = wiring::montar_coletor(None, max_depth.unwrap_or(MAX_DEPTH_PADRAO))?;
            let config = ColetaConfig {
                max_videos,
                recoletar_comentarios,
                ..ColetaConfig::default()
            };
            let caso = ColetarCanalUseCase::new(coletor, repositorio, config);
            let canal_ref = parse_canal_ref(&referencia)?;
            coletar_um(&caso, canal_ref).await?;
        }
    }

    Ok(())
}

async fn executar_canais(comando: CanaisComando, caminho: &Path) -> anyhow::Result<()> {
    match comando {
        CanaisComando::ColetarTodos {
            max_videos,
            recoletar_comentarios,
        } => {
            let repositorio = wiring::abrir_repositorio(caminho)?;
            let coletor = wiring::montar_coletor(None, MAX_DEPTH_PADRAO)?;
            let canais = repositorio.listar_canais()?;

            if canais.is_empty() {
                eprintln!("Nenhum canal guardado. Usa `yta canal adicionar` primeiro.");
                return Ok(());
            }

            let config = ColetaConfig {
                max_videos,
                recoletar_comentarios,
                ..ColetaConfig::default()
            };
            let caso = ColetarCanalUseCase::new(coletor, repositorio, config);
            let (tx, rx) = mpsc::channel(256);
            let consumidor = tokio::spawn(consumir_progresso(rx));
            let mut falhas = 0_usize;

            for canal in canais {
                eprintln!("→ {}", canal.nome);
                let resultado = caso
                    .executar(
                        CanalRef::ChannelId(canal.id.as_str().to_owned()),
                        Some(tx.clone()),
                        CancellationToken::new(),
                    )
                    .await;
                if let Err(erro) = resultado {
                    falhas += 1;
                    eprintln!("✗ {}: {erro}", canal.nome);
                }
            }

            drop(tx);
            let _ = consumidor.await;

            if falhas > 0 {
                return Err(anyhow!("{falhas} canal(is) falharam na coleta"));
            }
        }
    }

    Ok(())
}

async fn coletar_um(caso: &ColetarCanalUseCase, canal_ref: CanalRef) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel(128);
    let consumidor = tokio::spawn(consumir_progresso(rx));
    let resultado = caso
        .executar(canal_ref, Some(tx), CancellationToken::new())
        .await;
    let _ = consumidor.await;
    resultado?;
    Ok(())
}

async fn consumir_progresso(mut rx: mpsc::Receiver<EventoProgresso>) {
    while let Some(evento) = rx.recv().await {
        match evento {
            EventoProgresso::CanalResolvido { nome, total_videos } => {
                eprintln!("Canal: {nome} ({total_videos} vídeos)")
            }
            EventoProgresso::VideosListados { total } => {
                eprintln!("Vídeos a processar: {total}");
            }
            EventoProgresso::VideoIniciado { .. } => {}
            EventoProgresso::VideoConcluido {
                video_id,
                comentarios,
            } => eprintln!("✓ {video_id}: {comentarios} comentários"),
            EventoProgresso::VideoFalhou { video_id, motivo } => {
                eprintln!("✗ {video_id}: {motivo}");
            }
            EventoProgresso::Finalizado { relatorio } => {
                eprintln!(
                    "Concluído: {} novos, {} já coletados, {} falhas, {} comentários",
                    relatorio.videos_novos,
                    relatorio.videos_ja_coletados,
                    relatorio.videos_falhados,
                    relatorio.comentarios_total
                );
            }
        }
    }
}

struct OpcoesRelatorio {
    formato: Formato,
    saida: Option<PathBuf>,
    periodo_dias: Option<u32>,
    modo: ModoAnalise,
    idade_minima_dias: u32,
    views_por_dia: bool,
}

async fn executar_relatorio(
    referencia: &str,
    opcoes: &OpcoesRelatorio,
    caminho: &Path,
) -> anyhow::Result<()> {
    let repositorio = wiring::abrir_repositorio(caminho)?;
    let canal_id = resolver_canal_local(repositorio.as_ref(), referencia)?;
    let config = RelatorioConfig {
        periodo_dias: opcoes.periodo_dias,
        modo: opcoes.modo,
        idade_minima_dias: opcoes.idade_minima_dias,
        views_por_dia: opcoes.views_por_dia,
        ..RelatorioConfig::default()
    };
    let caso = GerarRelatorioUseCase::new(repositorio, config);
    let comando = std::env::args().collect::<Vec<_>>().join(" ");
    let metadados = MetadadosReproducao::new(infra_storage::VERSAO_SCHEMA, Some(comando));
    let relatorio = caso.executar(&canal_id, metadados).await?;

    let conteudo = match opcoes.formato {
        Formato::Md => para_markdown(&relatorio),
        Formato::Csv => para_csv(&relatorio),
        Formato::Json => para_json(&relatorio)?,
    };

    if let Some(saida) = opcoes.saida.as_deref() {
        std::fs::write(saida, conteudo).with_context(|| format!("escrever {}", saida.display()))?;
        eprintln!("Relatório escrito em {}", saida.display());
    } else {
        print!("{conteudo}");
    }

    Ok(())
}

fn listar_canais(repositorio: &dyn RepositorioDeCanais) -> anyhow::Result<()> {
    let canais = repositorio.listar_canais()?;
    println!(
        "{:<28} {:<32} {:<22} {:>12} {:>8}",
        "ID", "NOME", "CATEGORIA", "INSCRITOS", "VÍDEOS"
    );
    for canal in canais {
        let categoria = repositorio
            .categoria_manual_do_canal(&canal.id)?
            .map(|categoria| categoria.como_str().to_owned())
            .unwrap_or_else(|| "—".to_owned());
        println!(
            "{:<28} {:<32} {:<22} {:>12} {:>8}",
            canal.id, canal.nome, categoria, canal.inscritos, canal.total_videos
        );
    }
    Ok(())
}

fn resolver_canal_local(
    repositorio: &dyn RepositorioDeCanais,
    referencia: &str,
) -> anyhow::Result<CanalId> {
    if let Ok(Some(canal)) = repositorio.obter_canal(&CanalId::new(referencia)) {
        return Ok(canal.id);
    }

    let canal_ref = parse_canal_ref(referencia)?;
    for canal in repositorio.listar_canais()? {
        let combina = match &canal_ref {
            CanalRef::ChannelId(id) => canal.id.as_str() == id,
            CanalRef::Handle(handle) => {
                let esperado = format!("@{handle}");
                canal.handle.as_deref() == Some(esperado.as_str())
                    || canal
                        .url
                        .trim_end_matches('/')
                        .ends_with(&format!("/@{handle}"))
            }
            CanalRef::Custom(nome) => canal.url.contains(&format!("/c/{nome}")),
            CanalRef::Username(nome) => canal.url.contains(&format!("/user/{nome}")),
        };
        if combina {
            return Ok(canal.id);
        }
    }

    Err(anyhow!(
        "canal não encontrado na base de dados; usa `yta canal adicionar {referencia}` primeiro"
    ))
}
