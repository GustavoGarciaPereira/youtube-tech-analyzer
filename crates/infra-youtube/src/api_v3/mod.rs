mod canais;
mod client;
mod comentarios;
mod dto;
mod videos;

#[cfg(test)]
mod test_support;

use std::collections::HashMap;

use domain::entities::{canal::Canal, comentario::Comentario, video::Video};
use domain::errors::ErroDominio;
use domain::ids::{CanalId, VideoId};
use domain::ports::ColetorDeCanal;
use tokio::sync::Mutex;

use crate::errors::ColetaError;
use client::ApiV3Client;

/// Profundidade máxima de respostas usada por omissão.
///
/// `-1` (todas as respostas) não é o default por ser uma "bomba de quota".
pub const MAX_DEPTH_PADRAO: i32 = 2;

/// Coletor baseado na YouTube Data API v3.
///
/// A chave de API é lida do ambiente no construtor. O `max_depth` controla a
/// recolha de respostas a comentários e pode ser fixado por instância.
pub struct ApiV3Coletor {
    client: ApiV3Client,
    max_depth: i32,
    uploads: Mutex<HashMap<CanalId, String>>,
}

impl ApiV3Coletor {
    /// Cria um coletor com uma chave explícita.
    pub fn new(api_key: impl Into<String>, max_depth: i32) -> Result<Self, ColetaError> {
        validar_profundidade(max_depth)?;
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ColetaError::ChaveApiAusente);
        }
        Ok(Self {
            client: ApiV3Client::new(api_key)?,
            max_depth,
            uploads: Mutex::new(HashMap::new()),
        })
    }

    /// Lê `YOUTUBE_API_KEY` do ambiente e cria o coletor.
    pub fn from_env(max_depth: i32) -> Result<Self, ColetaError> {
        let chave = std::env::var("YOUTUBE_API_KEY").map_err(|_| ColetaError::ChaveApiAusente)?;
        if chave.trim().is_empty() {
            return Err(ColetaError::ChaveApiAusente);
        }
        Self::new(chave, max_depth)
    }

    /// Cria o coletor com a profundidade padrão (`2`).
    pub fn from_env_padrao() -> Result<Self, ColetaError> {
        Self::from_env(MAX_DEPTH_PADRAO)
    }

    /// Profundidade configurada para a recolha de comentários.
    #[must_use]
    pub fn max_depth(&self) -> i32 {
        self.max_depth
    }

    /// Recolhe comentários com uma profundidade explícita.
    ///
    /// Útil para testes e para operações pontuais que não devem usar o valor
    /// configurado no construtor.
    pub async fn coletar_comentarios_com_profundidade(
        &self,
        video_id: &VideoId,
        max_depth: i32,
    ) -> Result<Vec<Comentario>, ColetaError> {
        validar_profundidade(max_depth)?;
        comentarios::listar_comentarios(&self.client, video_id, max_depth).await
    }

    async fn guardar_uploads(&self, canal_id: &CanalId, uploads: String) {
        self.uploads.lock().await.insert(canal_id.clone(), uploads);
    }

    async fn obter_uploads(&self, canal_id: &CanalId) -> Result<String, ColetaError> {
        let existente = { self.uploads.lock().await.get(canal_id).cloned() };
        if let Some(uploads) = existente {
            return Ok(uploads);
        }

        let uploads = canais::obter_uploads_playlist(&self.client, canal_id).await?;
        self.guardar_uploads(canal_id, uploads.clone()).await;
        Ok(uploads)
    }

    #[cfg(test)]
    pub(crate) fn with_base_url(
        api_key: impl Into<String>,
        base_url: reqwest::Url,
        max_depth: i32,
    ) -> Result<Self, ColetaError> {
        validar_profundidade(max_depth)?;
        Ok(Self {
            client: ApiV3Client::with_base_url(api_key.into(), base_url)?,
            max_depth,
            uploads: Mutex::new(HashMap::new()),
        })
    }

    #[cfg(test)]
    pub(crate) fn client(&self) -> &ApiV3Client {
        &self.client
    }
}

impl ColetorDeCanal for ApiV3Coletor {
    async fn coletar_canal(&self, referencia: &str) -> Result<Canal, ErroDominio> {
        let (canal, uploads) = canais::resolver_canal(&self.client, referencia)
            .await
            .map_err(ErroDominio::from)?;
        self.guardar_uploads(&canal.id, uploads).await;
        Ok(canal)
    }

    async fn coletar_videos(&self, canal_id: &CanalId) -> Result<Vec<Video>, ErroDominio> {
        let uploads = self
            .obter_uploads(canal_id)
            .await
            .map_err(ErroDominio::from)?;
        videos::listar_videos(&self.client, canal_id, &uploads)
            .await
            .map_err(ErroDominio::from)
    }

    async fn coletar_comentarios(
        &self,
        video_id: &VideoId,
    ) -> Result<Vec<Comentario>, ErroDominio> {
        self.coletar_comentarios_com_profundidade(video_id, self.max_depth)
            .await
            .map_err(ErroDominio::from)
    }
}

fn validar_profundidade(max_depth: i32) -> Result<(), ColetaError> {
    if max_depth < -1 {
        return Err(ColetaError::RespostaInvalida(
            "max_depth deve ser -1 ou maior ou igual a 0".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) fn data_rfc3339(valor: &str) -> Result<String, ColetaError> {
    let data = chrono::DateTime::parse_from_rfc3339(valor)?;
    Ok(data
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

pub(crate) fn numero_u64(valor: Option<&str>, campo: &str) -> Result<u64, ColetaError> {
    match valor {
        Some(valor) if !valor.is_empty() => valor.parse::<u64>().map_err(|erro| {
            ColetaError::RespostaInvalida(format!("{campo} inválido ({valor}): {erro}"))
        }),
        _ => Ok(0),
    }
}

pub(crate) fn texto_opcional(valor: String) -> Option<String> {
    if valor.trim().is_empty() {
        None
    } else {
        Some(valor)
    }
}
