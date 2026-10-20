use domain::CanalRef;
use reqwest::Url;

use crate::errors::ColetaError;

/// Interpreta e normaliza uma referência de canal.
///
/// Formatos aceites:
/// - `https://www.youtube.com/@handle`
/// - `https://www.youtube.com/c/Nome`
/// - `https://www.youtube.com/user/Nome`
/// - `https://www.youtube.com/channel/UC...`
/// - `@handle`
/// - `UC...`
pub fn parse(entrada: &str) -> Result<CanalRef, ColetaError> {
    let entrada = entrada.trim();
    if entrada.is_empty() {
        return Err(ColetaError::UrlInvalida("entrada vazia".to_owned()));
    }

    if let Some(handle) = entrada.strip_prefix('@') {
        if handle.is_empty() {
            return Err(ColetaError::UrlInvalida(entrada.to_owned()));
        }
        return Ok(CanalRef::Handle(handle.to_owned()));
    }

    if eh_channel_id(entrada) {
        return Ok(CanalRef::ChannelId(entrada.to_owned()));
    }

    let normalizada = if entrada.contains("://") {
        entrada.to_owned()
    } else if entrada.starts_with("www.")
        || entrada.starts_with("youtube.com")
        || entrada.starts_with("m.youtube.com")
    {
        format!("https://{entrada}")
    } else {
        return Err(ColetaError::UrlInvalida(entrada.to_owned()));
    };

    let url = Url::parse(&normalizada).map_err(|_| ColetaError::UrlInvalida(entrada.to_owned()))?;
    let host = url
        .host_str()
        .ok_or_else(|| ColetaError::UrlInvalida(entrada.to_owned()))?;
    if host != "youtube.com" && !host.ends_with(".youtube.com") {
        return Err(ColetaError::UrlInvalida(entrada.to_owned()));
    }

    let segmentos: Vec<&str> = url
        .path_segments()
        .map(|segmentos| segmentos.filter(|segmento| !segmento.is_empty()).collect())
        .unwrap_or_default();

    match segmentos.as_slice() {
        [handle, ..] if handle.starts_with('@') && handle.len() > 1 => {
            Ok(CanalRef::Handle(handle.trim_start_matches('@').to_owned()))
        }
        ["c", nome, ..] if !nome.is_empty() => Ok(CanalRef::Custom((*nome).to_owned())),
        ["user", nome, ..] if !nome.is_empty() => Ok(CanalRef::Username((*nome).to_owned())),
        ["channel", id, ..] if eh_channel_id(id) => Ok(CanalRef::ChannelId((*id).to_owned())),
        _ => Err(ColetaError::UrlInvalida(entrada.to_owned())),
    }
}

fn eh_channel_id(valor: &str) -> bool {
    valor.len() == 24
        && valor.starts_with("UC")
        && valor.chars().all(|caractere| {
            caractere.is_ascii_alphanumeric() || caractere == '-' || caractere == '_'
        })
}

#[cfg(test)]
mod tests {
    use domain::CanalRef;

    use super::parse;

    const CANAL_ID: &str = "UC1234567890123456789012";

    #[test]
    fn aceita_url_com_handle() {
        assert_eq!(
            parse("https://www.youtube.com/@handle").expect("URL válida"),
            CanalRef::Handle("handle".to_owned())
        );
    }

    #[test]
    fn aceita_url_de_handle_com_subpagina() {
        assert_eq!(
            parse("https://www.youtube.com/@handle/videos").expect("URL válida"),
            CanalRef::Handle("handle".to_owned())
        );
    }

    #[test]
    fn aceita_url_custom() {
        assert_eq!(
            parse("https://www.youtube.com/c/Nome").expect("URL válida"),
            CanalRef::Custom("Nome".to_owned())
        );
    }

    #[test]
    fn aceita_url_de_username() {
        assert_eq!(
            parse("https://www.youtube.com/user/Nome").expect("URL válida"),
            CanalRef::Username("Nome".to_owned())
        );
    }

    #[test]
    fn aceita_url_de_channel_id() {
        assert_eq!(
            parse(&format!("https://www.youtube.com/channel/{CANAL_ID}")).expect("URL válida"),
            CanalRef::ChannelId(CANAL_ID.to_owned())
        );
    }

    #[test]
    fn aceita_handle_puro() {
        assert_eq!(
            parse("@handle").expect("handle válido"),
            CanalRef::Handle("handle".to_owned())
        );
    }

    #[test]
    fn aceita_channel_id_puro() {
        assert_eq!(
            parse(CANAL_ID).expect("channel id válido"),
            CanalRef::ChannelId(CANAL_ID.to_owned())
        );
    }

    #[test]
    fn rejeita_dominio_que_nao_e_youtube() {
        assert!(parse("https://example.com/@handle").is_err());
    }
}
