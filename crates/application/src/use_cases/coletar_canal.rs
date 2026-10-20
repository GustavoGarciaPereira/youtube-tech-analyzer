use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use domain::CanalRef;
use domain::entities::{
    canal::Canal,
    comentario::Comentario,
    snapshot::{EntidadeTipo, Metrica, Snapshot},
    video::Video,
};
use domain::errors::ErroDominio;
use domain::ids::{CanalId, VideoId};
use domain::ports::{ColetorDeCanal, RepositorioDeCanais};
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::config::ColetaConfig;
use crate::errors::ErroAplicacao;
use crate::eventos::{EventoProgresso, FalhaVideo, RelatorioColeta};

/// Caso de uso de coleta completa de um canal.
pub struct ColetarCanalUseCase {
    coletor: Arc<dyn ColetorDeCanal>,
    repositorio: Arc<dyn RepositorioDeCanais>,
    config: ColetaConfig,
}

impl ColetarCanalUseCase {
    /// Cria o caso de uso com as portas e a configuração fornecidas.
    #[must_use]
    pub fn new(
        coletor: Arc<dyn ColetorDeCanal>,
        repositorio: Arc<dyn RepositorioDeCanais>,
        config: ColetaConfig,
    ) -> Self {
        Self {
            coletor,
            repositorio,
            config,
        }
    }

    /// Executa a coleta de um canal.
    pub async fn executar(
        &self,
        canal_ref: CanalRef,
        progresso: Option<mpsc::Sender<EventoProgresso>>,
        cancelamento: CancellationToken,
    ) -> Result<RelatorioColeta, ErroAplicacao> {
        if cancelamento.is_cancelled() {
            return Err(ErroAplicacao::Cancelada);
        }
        self.config.validar()?;

        let inicio = Instant::now();
        let hoje = data_hoje();

        let canal = self.coletor.coletar_canal(&canal_ref).await?;
        emitir(
            &progresso,
            EventoProgresso::CanalResolvido {
                nome: canal.nome.clone(),
                total_videos: para_usize(canal.total_videos, "total_videos")?,
            },
        );

        self.repositorio.guardar_canal(&canal)?;
        self.repositorio
            .upsert_snapshots_batch(&snapshots_canal(&canal, &hoje)?)?;
        verificar_cancelamento(&cancelamento)?;

        let mut videos = self.coletor.coletar_videos(&canal.id).await?;
        let agregados_canal = snapshots_agregados_canal(&canal.id, &videos, &hoje)?;
        videos.sort_by(|a, b| {
            b.publicado_em
                .cmp(&a.publicado_em)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        if let Some(max_videos) = self.config.max_videos {
            videos.truncate(max_videos);
        }
        if videos.is_empty() {
            return Err(ErroAplicacao::CanalSemVideos);
        }

        emitir(
            &progresso,
            EventoProgresso::VideosListados {
                total: videos.len(),
            },
        );
        self.repositorio.upsert_videos_batch(&videos)?;

        // Métricas por vídeo: todos os dias, exceto re-execuções no mesmo dia.
        let mut videos_metricas = Vec::new();
        for video in &videos {
            if !self
                .repositorio
                .video_metricas_hoje_ja_coletadas(&video.id)?
            {
                videos_metricas.push(video.clone());
            }
        }
        self.repositorio
            .upsert_snapshots_batch(&snapshots_videos(&videos_metricas, &hoje)?)?;
        self.repositorio.upsert_snapshots_batch(&agregados_canal)?;
        verificar_cancelamento(&cancelamento)?;

        // Comentários: uma vez por vídeo, independentemente do dia.
        let mut videos_restantes = Vec::new();
        if self.config.recoletar_comentarios {
            videos_restantes.extend(videos.iter().cloned());
        } else {
            for video in &videos {
                if !self
                    .repositorio
                    .comentarios_do_video_ja_coletados(&video.id)?
                {
                    videos_restantes.push(video.clone());
                }
            }
        }
        let videos_ja_coletados = videos.len() - videos_restantes.len();

        let total = videos_restantes.len();
        let contexto = ContextoVideo {
            coletor: Arc::clone(&self.coletor),
            repositorio: Arc::clone(&self.repositorio),
            progresso: progresso.clone(),
            cancelamento: cancelamento.clone(),
            semaforo: Arc::new(Semaphore::new(self.config.concorrencia)),
            hoje: hoje.clone(),
        };
        let mut tarefas = JoinSet::new();
        for (indice, video) in videos_restantes.into_iter().enumerate() {
            let contexto = contexto.clone();
            tarefas.spawn(async move { processar_video(contexto, indice, total, video).await });
        }

        let mut videos_novos = 0_usize;
        let mut videos_falhados = 0_usize;
        let mut comentarios_total = 0_usize;
        let mut falhas = Vec::new();
        let mut primeiro_erro: Option<ErroAplicacao> = None;

        while let Some(resultado) = tarefas.join_next().await {
            match resultado {
                Ok(ResultadoVideo::Concluido { comentarios }) => {
                    videos_novos += 1;
                    comentarios_total += comentarios;
                }
                Ok(ResultadoVideo::Falhou { falha }) => {
                    if !self.config.continuar_em_falha && primeiro_erro.is_none() {
                        primeiro_erro = Some(ErroAplicacao::Dominio(ErroDominio::Coleta(
                            falha.motivo.clone(),
                        )));
                    }
                    videos_falhados += 1;
                    falhas.push(falha);
                }
                Ok(ResultadoVideo::Ignorado) => {}
                Err(erro) => {
                    if primeiro_erro.is_none() {
                        primeiro_erro = Some(ErroAplicacao::Dominio(ErroDominio::Coleta(format!(
                            "tarefa de vídeo falhou: {erro}"
                        ))));
                    }
                }
            }
        }

        if cancelamento.is_cancelled() {
            return Err(ErroAplicacao::Cancelada);
        }

        let snapshot_final = Snapshot::novo(
            EntidadeTipo::Canal,
            canal.id.as_str(),
            Metrica::TotalComentariosColetados,
            i64::try_from(comentarios_total).map_err(|_| {
                ErroAplicacao::Dominio(ErroDominio::DadosInvalidos(
                    "total_comentarios_coletados excede i64".to_owned(),
                ))
            })?,
            hoje.as_str(),
        );
        self.repositorio.upsert_snapshots_batch(&[snapshot_final])?;

        if let Some(erro) = primeiro_erro {
            return Err(erro);
        }

        let relatorio = RelatorioColeta {
            canal,
            videos_novos,
            videos_ja_coletados,
            videos_falhados,
            comentarios_total,
            duracao: inicio.elapsed(),
            falhas,
        };
        emitir(
            &progresso,
            EventoProgresso::Finalizado {
                relatorio: relatorio.clone(),
            },
        );
        Ok(relatorio)
    }
}

#[derive(Clone)]
struct ContextoVideo {
    coletor: Arc<dyn ColetorDeCanal>,
    repositorio: Arc<dyn RepositorioDeCanais>,
    progresso: Option<mpsc::Sender<EventoProgresso>>,
    cancelamento: CancellationToken,
    semaforo: Arc<Semaphore>,
    hoje: String,
}

enum ResultadoVideo {
    Concluido { comentarios: usize },
    Falhou { falha: FalhaVideo },
    Ignorado,
}

async fn processar_video(
    contexto: ContextoVideo,
    indice: usize,
    total: usize,
    video: Video,
) -> ResultadoVideo {
    let ContextoVideo {
        coletor,
        repositorio,
        progresso,
        cancelamento,
        semaforo,
        hoje,
    } = contexto;

    if cancelamento.is_cancelled() {
        return ResultadoVideo::Ignorado;
    }

    let _permit = tokio::select! {
        _ = cancelamento.cancelled() => return ResultadoVideo::Ignorado,
        permit = semaforo.acquire_owned() => match permit {
            Ok(permit) => permit,
            Err(_) => {
                return ResultadoVideo::Falhou {
                    falha: FalhaVideo {
                        video_id: video.id,
                        motivo: "semáforo de concorrência fechado".to_owned(),
                    },
                };
            }
        },
    };

    if cancelamento.is_cancelled() {
        return ResultadoVideo::Ignorado;
    }

    let video_id = video.id.clone();
    emitir(
        &progresso,
        EventoProgresso::VideoIniciado {
            indice,
            total,
            video_id: video_id.clone(),
            titulo: video.titulo.clone(),
        },
    );

    match coletor.coletar_comentarios(&video.id).await {
        Ok(comentarios) => {
            match guardar_comentarios(&repositorio, &video_id, &comentarios, &hoje) {
                Ok(total_comentarios) => {
                    emitir(
                        &progresso,
                        EventoProgresso::VideoConcluido {
                            video_id,
                            comentarios: total_comentarios,
                        },
                    );
                    ResultadoVideo::Concluido {
                        comentarios: total_comentarios,
                    }
                }
                Err(erro) => {
                    let falha = FalhaVideo {
                        video_id,
                        motivo: erro.to_string(),
                    };
                    tracing::warn!(
                        video_id = falha.video_id.as_str(),
                        motivo = %falha.motivo,
                        "falha ao persistir comentários"
                    );
                    emitir(
                        &progresso,
                        EventoProgresso::VideoFalhou {
                            video_id: falha.video_id.clone(),
                            motivo: falha.motivo.clone(),
                        },
                    );
                    ResultadoVideo::Falhou { falha }
                }
            }
        }
        Err(erro) => {
            let falha = FalhaVideo {
                video_id,
                motivo: erro.to_string(),
            };
            tracing::warn!(
                video_id = falha.video_id.as_str(),
                motivo = %falha.motivo,
                "falha ao recolher comentários"
            );
            emitir(
                &progresso,
                EventoProgresso::VideoFalhou {
                    video_id: falha.video_id.clone(),
                    motivo: falha.motivo.clone(),
                },
            );
            ResultadoVideo::Falhou { falha }
        }
    }
}

fn guardar_comentarios(
    repositorio: &Arc<dyn RepositorioDeCanais>,
    video_id: &VideoId,
    comentarios: &[Comentario],
    hoje: &str,
) -> Result<usize, ErroDominio> {
    repositorio.upsert_comentarios_batch(comentarios)?;

    let total = comentarios.len();
    let marcador = Snapshot::novo(
        EntidadeTipo::Video,
        video_id.as_str(),
        Metrica::ComentariosColetados,
        i64::try_from(total).map_err(|_| {
            ErroDominio::DadosInvalidos("contagem de comentários excede i64".to_owned())
        })?,
        hoje,
    );
    repositorio.upsert_snapshots_batch(&[marcador])?;
    Ok(total)
}

fn snapshots_canal(canal: &Canal, hoje: &str) -> Result<Vec<Snapshot>, ErroAplicacao> {
    Ok(vec![
        Snapshot::novo(
            EntidadeTipo::Canal,
            canal.id.as_str(),
            Metrica::Inscritos,
            para_i64(canal.inscritos, "inscritos")?,
            hoje,
        ),
        Snapshot::novo(
            EntidadeTipo::Canal,
            canal.id.as_str(),
            Metrica::TotalVideos,
            para_i64(canal.total_videos, "total_videos")?,
            hoje,
        ),
    ])
}

fn snapshots_agregados_canal(
    canal_id: &CanalId,
    videos: &[Video],
    hoje: &str,
) -> Result<Vec<Snapshot>, ErroAplicacao> {
    let visualizacoes: u64 = videos.iter().map(|video| video.visualizacoes).sum();
    let curtidas: u64 = videos.iter().map(|video| video.curtidas).sum();
    let total_comentarios: u64 = videos.iter().map(|video| video.total_comentarios).sum();

    Ok(vec![
        Snapshot::novo(
            EntidadeTipo::Canal,
            canal_id.as_str(),
            Metrica::Visualizacoes,
            para_i64(visualizacoes, "visualizacoes")?,
            hoje,
        ),
        Snapshot::novo(
            EntidadeTipo::Canal,
            canal_id.as_str(),
            Metrica::Curtidas,
            para_i64(curtidas, "curtidas")?,
            hoje,
        ),
        Snapshot::novo(
            EntidadeTipo::Canal,
            canal_id.as_str(),
            Metrica::TotalComentarios,
            para_i64(total_comentarios, "total_comentarios")?,
            hoje,
        ),
    ])
}

fn snapshots_videos(videos: &[Video], hoje: &str) -> Result<Vec<Snapshot>, ErroAplicacao> {
    let mut snapshots = Vec::with_capacity(videos.len() * 3);
    for video in videos {
        snapshots.push(Snapshot::novo(
            EntidadeTipo::Video,
            video.id.as_str(),
            Metrica::Visualizacoes,
            para_i64(video.visualizacoes, "visualizacoes")?,
            hoje,
        ));
        snapshots.push(Snapshot::novo(
            EntidadeTipo::Video,
            video.id.as_str(),
            Metrica::Curtidas,
            para_i64(video.curtidas, "curtidas")?,
            hoje,
        ));
        snapshots.push(Snapshot::novo(
            EntidadeTipo::Video,
            video.id.as_str(),
            Metrica::TotalComentarios,
            para_i64(video.total_comentarios, "total_comentarios")?,
            hoje,
        ));
    }
    Ok(snapshots)
}

fn data_hoje() -> String {
    Utc::now().date_naive().to_string()
}

fn para_i64(valor: u64, campo: &str) -> Result<i64, ErroAplicacao> {
    i64::try_from(valor).map_err(|_| {
        ErroAplicacao::Dominio(ErroDominio::DadosInvalidos(format!(
            "{campo} excede i64: {valor}"
        )))
    })
}

fn para_usize(valor: u64, campo: &str) -> Result<usize, ErroAplicacao> {
    usize::try_from(valor).map_err(|_| {
        ErroAplicacao::Dominio(ErroDominio::DadosInvalidos(format!(
            "{campo} excede usize: {valor}"
        )))
    })
}

fn verificar_cancelamento(cancelamento: &CancellationToken) -> Result<(), ErroAplicacao> {
    if cancelamento.is_cancelled() {
        Err(ErroAplicacao::Cancelada)
    } else {
        Ok(())
    }
}

fn emitir(progresso: &Option<mpsc::Sender<EventoProgresso>>, evento: EventoProgresso) {
    if let Some(sender) = progresso {
        // A UI é um consumidor opcional: canal cheio ou fechado não bloqueia
        // nem falha a coleta.
        let _ = sender.try_send(evento);
    }
}

#[cfg(test)]
mod tests {
    use super::snapshots_agregados_canal;
    use domain::entities::{snapshot::Metrica, video::Video};
    use domain::ids::{CanalId, VideoId};

    fn video(id: &str, visualizacoes: u64, curtidas: u64, total_comentarios: u64) -> Video {
        Video {
            id: VideoId::new(id),
            canal_id: CanalId::new("canal-1"),
            titulo: format!("Vídeo {id}"),
            descricao: None,
            url: format!("https://www.youtube.com/watch?v={id}"),
            tags: Vec::new(),
            topic_categories: Vec::new(),
            publicado_em: "2024-01-01T00:00:00Z".to_owned(),
            duracao_segundos: 60,
            visualizacoes,
            curtidas,
            total_comentarios,
        }
    }

    #[test]
    fn agrega_metricas_de_engajamento_do_canal() {
        let videos = vec![video("v1", 100, 10, 2), video("v2", 200, 20, 3)];
        let snapshots = snapshots_agregados_canal(&CanalId::new("canal-1"), &videos, "2024-01-01")
            .expect("agregados válidos");

        let valor = |metrica| {
            snapshots
                .iter()
                .find(|snapshot| snapshot.metrica == metrica)
                .map(|snapshot| snapshot.valor)
        };

        assert_eq!(valor(Metrica::Visualizacoes), Some(300));
        assert_eq!(valor(Metrica::Curtidas), Some(30));
        assert_eq!(valor(Metrica::TotalComentarios), Some(5));
    }
}
