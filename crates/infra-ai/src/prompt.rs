use domain::Nicho;
use domain::entities::video::Video;

use crate::errors::ErroIA;

/// Versão do template de prompt. Mudar isto invalida o cache antigo.
pub const VERSAO_PROMPT: &str = "v1";

/// Taxonomia configurável de nichos.
#[derive(Debug, Clone)]
pub struct Taxonomia {
    /// Nichos válidos.
    pub nichos: Vec<String>,
    /// Instruções gerais para o modelo.
    pub instrucoes: String,
}

impl Default for Taxonomia {
    fn default() -> Self {
        Self {
            nichos: [
                Nicho::Programacao,
                Nicho::Educacao,
                Nicho::Tecnologia,
                Nicho::Outro,
            ]
            .into_iter()
            .map(|nicho| nicho.as_str().to_owned())
            .collect(),
            instrucoes: "Classifica o vídeo do YouTube abaixo num dos nichos fornecidos."
                .to_owned(),
        }
    }
}

impl Taxonomia {
    /// Valida que a taxonomia não está vazia e que todos os nichos são
    /// representáveis pelo enum [`Nicho`].
    pub fn validar(&self) -> Result<(), ErroIA> {
        if self.nichos.is_empty() {
            return Err(ErroIA::TaxonomiaVazia);
        }
        for nicho in &self.nichos {
            if Nicho::try_from(nicho.as_str()).is_err() {
                return Err(ErroIA::NichoInvalido(nicho.clone()));
            }
        }
        Ok(())
    }
}

/// Template estável, sem conteúdo do vídeo, usado no hash do cache.
#[must_use]
pub fn formatar_template(taxonomia: &Taxonomia) -> String {
    format!(
        "{}\nVersão do prompt: {}\nNichos válidos: {}",
        taxonomia.instrucoes,
        VERSAO_PROMPT,
        taxonomia.nichos.join(", ")
    )
}

/// Constrói o prompt completo para um vídeo.
#[must_use]
pub fn formatar_prompt(taxonomia: &Taxonomia, video: &Video) -> String {
    let descricao: String = video
        .descricao
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(2000)
        .collect();

    format!(
        "{}\n\nVersão do prompt: {}\nNichos válidos: {}\n\nTítulo: {}\nDescrição: {}\nTags: {}\n\nResponde apenas com JSON estrito: {{\"nicho\": \"...\", \"confianca\": 0.0, \"justificativa\": \"...\"}}",
        taxonomia.instrucoes,
        VERSAO_PROMPT,
        taxonomia.nichos.join(", "),
        video.titulo,
        descricao,
        video.tags.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use domain::entities::video::Video;
    use domain::ids::{CanalId, VideoId};

    use super::{Taxonomia, formatar_prompt};
    use crate::errors::ErroIA;

    fn video(descricao: String) -> Video {
        Video {
            id: VideoId::new("video-1"),
            canal_id: CanalId::new("canal-1"),
            titulo: "Rust para iniciantes".to_owned(),
            descricao: Some(descricao),
            url: "https://www.youtube.com/watch?v=video-1".to_owned(),
            tags: vec!["rust".to_owned(), "programação".to_owned()],
            topic_categories: Vec::new(),
            publicado_em: "2024-01-01T00:00:00Z".to_owned(),
            duracao_segundos: 600,
            visualizacoes: 100,
            curtidas: 10,
            total_comentarios: 2,
        }
    }

    #[test]
    fn prompt_inclui_metadados_nichos_e_trunca_descricao() {
        let taxonomia = Taxonomia::default();
        let prompt = formatar_prompt(&taxonomia, &video("a".repeat(3000)));

        assert!(prompt.contains("Rust para iniciantes"));
        assert!(prompt.contains("rust, programação"));
        assert!(prompt.contains("programacao"));
        assert!(prompt.contains("educacao"));
        assert!(prompt.contains("tecnologia"));
        assert!(prompt.contains("outro"));
        assert!(prompt.contains(&"a".repeat(2000)));
        assert!(!prompt.contains(&"a".repeat(2001)));
    }

    #[test]
    fn taxonomia_vazia_e_invalida() {
        let vazia = Taxonomia {
            nichos: Vec::new(),
            instrucoes: String::new(),
        };
        assert!(matches!(vazia.validar(), Err(ErroIA::TaxonomiaVazia)));

        let invalida = Taxonomia {
            nichos: vec!["hardware".to_owned()],
            instrucoes: String::new(),
        };
        assert!(matches!(invalida.validar(), Err(ErroIA::NichoInvalido(_))));
    }
}
