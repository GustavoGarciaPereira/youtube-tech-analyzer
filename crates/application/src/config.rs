use crate::errors::ErroAplicacao;

/// Configuração de uma execução de coleta de canal.
#[derive(Debug, Clone)]
pub struct ColetaConfig {
    /// Limita a coleta aos N vídeos mais recentes.
    pub max_videos: Option<usize>,
    /// Número máximo de vídeos processados em paralelo.
    pub concorrencia: usize,
    /// Continua para os restantes vídeos quando um falha.
    pub continuar_em_falha: bool,
    /// Força a re-busca de comentários mesmo que já tenham sido coletados.
    pub recoletar_comentarios: bool,
}

impl Default for ColetaConfig {
    fn default() -> Self {
        Self {
            max_videos: None,
            concorrencia: 3,
            continuar_em_falha: true,
            recoletar_comentarios: false,
        }
    }
}

impl ColetaConfig {
    pub(crate) fn validar(&self) -> Result<(), ErroAplicacao> {
        if self.concorrencia == 0 {
            return Err(ErroAplicacao::ConfigInvalida(
                "concorrencia deve ser maior que zero".to_owned(),
            ));
        }
        Ok(())
    }
}
