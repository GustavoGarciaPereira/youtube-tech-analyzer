use crate::errors::ErroAplicacao;

/// Configuração de uma execução de coleta de canal.
#[derive(Debug, Clone)]
pub struct ColetaConfig {
    /// Ignora vídeos cujos comentários já foram coletados hoje (UTC).
    pub apenas_nao_coletados: bool,
    /// Limita a coleta aos N vídeos mais recentes.
    pub max_videos: Option<usize>,
    /// Número máximo de vídeos processados em paralelo.
    pub concorrencia: usize,
    /// Continua para os restantes vídeos quando um falha.
    pub continuar_em_falha: bool,
}

impl Default for ColetaConfig {
    fn default() -> Self {
        Self {
            apenas_nao_coletados: true,
            max_videos: None,
            concorrencia: 3,
            continuar_em_falha: true,
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
