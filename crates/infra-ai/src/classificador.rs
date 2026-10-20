//! Custo estimado por canal:
//! - 50 vídeos: 50 chamadas LLM na primeira execução (~$0.01–$0.05,
//!   dependendo do modelo).
//! - Execuções subsequentes: 0 chamadas se nenhum vídeo novo e o hash do
//!   conteúdo/template não mudou (cache hit).
//! - Se o template do prompt mudar (`VERSAO_PROMPT` bump), TODOS os vídeos
//!   são reclassificados — usar com consciência.
//! - Para grandes lotes, considerar classificar apenas vídeos recentes
//!   (últimos N por canal) numa primeira passagem.

use std::sync::Arc;

use chrono::{SecondsFormat, Utc};
use domain::BoxFuture;
use domain::entities::comentario::Comentario;
use domain::entities::nicho::{ClassificacaoNicho, Nicho};
use domain::entities::video::Video;
use domain::errors::ErroDominio;
use domain::ports::{CacheDeClassificacoes, ClassificadorDeNicho};
use serde::Deserialize;

use crate::cliente_llm::{ClienteLLM, PedidoClassificacao};
use crate::errors::ErroIA;
use crate::hash::hash_conteudo;
use crate::prompt::{Taxonomia, VERSAO_PROMPT, formatar_prompt, formatar_template};

/// Classificador de nicho com cache por conteúdo e versão.
pub struct ClassificadorComCache {
    cliente: Arc<dyn ClienteLLM>,
    cache: Arc<dyn CacheDeClassificacoes>,
    taxonomia: Taxonomia,
    versao_classificador: String,
}

impl ClassificadorComCache {
    /// Cria o classificador com as dependências injetadas.
    #[must_use]
    pub fn new(
        cliente: Arc<dyn ClienteLLM>,
        cache: Arc<dyn CacheDeClassificacoes>,
        taxonomia: Taxonomia,
        versao_classificador: impl Into<String>,
    ) -> Self {
        Self {
            cliente,
            cache,
            taxonomia,
            versao_classificador: versao_classificador.into(),
        }
    }

    async fn classificar_video(&self, video: &Video) -> Result<ClassificacaoNicho, ErroDominio> {
        self.taxonomia.validar().map_err(ErroDominio::from)?;

        let template = formatar_template(&self.taxonomia);
        let descricao = video.descricao.as_deref().unwrap_or("");
        let hash = hash_conteudo(&video.titulo, descricao, &video.tags, &template);

        if let Some(classificacao) = self
            .cache
            .obter(&video.id, &hash, &self.versao_classificador)
            .await?
        {
            return Ok(classificacao);
        }

        let pedido = PedidoClassificacao {
            texto: formatar_prompt(&self.taxonomia, video),
            taxonomia: self.taxonomia.nichos.clone(),
            versao_prompt: VERSAO_PROMPT.to_owned(),
        };
        let resposta = self
            .cliente
            .classificar(pedido)
            .await
            .map_err(ErroDominio::from)?;

        let bruta: RespostaLLM = serde_json::from_str(&resposta.conteudo).map_err(|erro| {
            ErroDominio::from(ErroIA::RespostaInvalida(format!("JSON inválido: {erro}")))
        })?;

        if !self
            .taxonomia
            .nichos
            .iter()
            .any(|nicho| nicho == &bruta.nicho)
        {
            return Err(ErroDominio::from(ErroIA::NichoInvalido(bruta.nicho)));
        }

        if !bruta.confianca.is_finite() || !(0.0..=1.0).contains(&bruta.confianca) {
            return Err(ErroDominio::from(ErroIA::RespostaInvalida(format!(
                "confiança fora de [0,1]: {}",
                bruta.confianca
            ))));
        }

        let nicho = Nicho::try_from(bruta.nicho.as_str())
            .map_err(|_| ErroIA::NichoInvalido(bruta.nicho.clone()))?;

        let classificacao = ClassificacaoNicho {
            video_id: video.id.clone(),
            nicho,
            confianca: bruta.confianca as f32,
            metodo: resposta.modelo,
            data_classificacao: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        };

        self.cache
            .gravar(&video.id, &hash, &self.versao_classificador, &classificacao)
            .await?;

        Ok(classificacao)
    }
}

impl ClassificadorDeNicho for ClassificadorComCache {
    fn classificar<'a>(
        &'a self,
        video: &'a Video,
        _comentarios: &'a [Comentario],
    ) -> BoxFuture<'a, Result<ClassificacaoNicho, ErroDominio>> {
        // v1 usa apenas metadados; comentários ficam fora do hash e do prompt.
        let video = video.clone();
        Box::pin(async move { self.classificar_video(&video).await })
    }
}

#[derive(Debug, Deserialize)]
struct RespostaLLM {
    nicho: String,
    confianca: f64,
}
