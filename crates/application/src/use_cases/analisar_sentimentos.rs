use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::CanalId;
use domain::entities::canal::Canal;
use domain::ports::{AnalisadorDeSentimento, RepositorioDeCanais};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::errors::ErroAplicacao;

/// Configuração da análise de sentimentos.
#[derive(Debug, Clone, Copy)]
pub struct AnalisarConfig {
    /// Número máximo de inferências em paralelo. Deve ser >= 1.
    pub concorrencia: usize,
}

impl Default for AnalisarConfig {
    fn default() -> Self {
        Self {
            // Deixa um núcleo livre para a UI/CLI; nunca menos de 2.
            concorrencia: num_cpus::get().saturating_sub(1).max(2),
        }
    }
}

/// Resultado de uma execução da análise de sentimentos.
#[derive(Debug, Clone)]
pub struct RelatorioSentimentos {
    /// Canal analisado.
    pub canal: Canal,
    /// Comentários sem análise para a versão actual do modelo.
    pub total_comentarios: usize,
    /// Comentários analisados e persistidos com sucesso.
    pub analisados: usize,
    /// Comentários que falharam (inferência ou persistência).
    pub falhados: usize,
    /// Tempo total da execução.
    pub duracao: Duration,
}

/// Caso de uso que analisa os comentários de um canal ainda sem sentimento.
pub struct AnalisarSentimentosUseCase {
    analisador: Arc<dyn AnalisadorDeSentimento>,
    repositorio: Arc<dyn RepositorioDeCanais>,
    config: AnalisarConfig,
}

impl AnalisarSentimentosUseCase {
    /// Cria o caso de uso.
    #[must_use]
    pub fn new(
        analisador: Arc<dyn AnalisadorDeSentimento>,
        repositorio: Arc<dyn RepositorioDeCanais>,
        config: AnalisarConfig,
    ) -> Self {
        Self {
            analisador,
            repositorio,
            config,
        }
    }

    /// Analisa todos os comentários do canal sem resultado para esta versão.
    ///
    /// Falhas individuais são registadas e contabilizadas, mas não abortam o
    /// lote: um comentário problemático não deve travar os restantes.
    pub async fn executar(
        &self,
        canal_id: &CanalId,
    ) -> Result<RelatorioSentimentos, ErroAplicacao> {
        if self.config.concorrencia == 0 {
            return Err(ErroAplicacao::ConfigInvalida(
                "concorrência deve ser >= 1".to_owned(),
            ));
        }

        let inicio = Instant::now();
        let canal = self
            .repositorio
            .obter_canal(canal_id)?
            .ok_or_else(|| ErroAplicacao::SemDados(format!("canal {canal_id} não encontrado")))?;

        let versao = self.analisador.versao_modelo().to_owned();
        let pendentes = self
            .repositorio
            .comentarios_sem_sentimento(&canal.id, &versao)?;
        let total_comentarios = pendentes.len();

        if pendentes.is_empty() {
            return Ok(RelatorioSentimentos {
                canal,
                total_comentarios,
                analisados: 0,
                falhados: 0,
                duracao: inicio.elapsed(),
            });
        }

        let semaforo = Arc::new(Semaphore::new(self.config.concorrencia));
        let mut tarefas = JoinSet::new();

        for comentario in pendentes {
            let analisador = Arc::clone(&self.analisador);
            let repositorio = Arc::clone(&self.repositorio);
            let semaforo = Arc::clone(&semaforo);
            let versao = versao.clone();

            tarefas.spawn(async move {
                let Ok(_permit) = semaforo.acquire_owned().await else {
                    return false;
                };
                let Ok(score) = analisador.analisar(&comentario.texto).await else {
                    tracing::warn!(
                        comentario = %comentario.id,
                        "falha ao analisar sentimento"
                    );
                    return false;
                };
                match repositorio.guardar_sentimento(&comentario.id, &score, &versao) {
                    Ok(()) => true,
                    Err(erro) => {
                        tracing::warn!(
                            comentario = %comentario.id,
                            erro = %erro,
                            "falha ao guardar sentimento"
                        );
                        false
                    }
                }
            });
        }

        let mut analisados = 0;
        let mut falhados = 0;
        while let Some(resultado) = tarefas.join_next().await {
            match resultado {
                Ok(true) => analisados += 1,
                Ok(false) => falhados += 1,
                Err(erro) => {
                    falhados += 1;
                    tracing::warn!(erro = %erro, "tarefa de sentimento falhou");
                }
            }
        }

        Ok(RelatorioSentimentos {
            canal,
            total_comentarios,
            analisados,
            falhados,
            duracao: inicio.elapsed(),
        })
    }
}
