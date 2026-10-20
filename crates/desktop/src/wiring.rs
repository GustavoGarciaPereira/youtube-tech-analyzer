use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, anyhow};
use domain::ports::{ColetorDeCanal, RepositorioDeCanais};
use infra_storage::SqliteRepositorio;
use infra_youtube::{ApiV3Coletor, MAX_DEPTH_PADRAO};

/// Dependências de infraestrutura injetadas nos casos de uso.
pub struct Dependencias {
    /// Repositório SQLite.
    pub repositorio: Arc<dyn RepositorioDeCanais>,
    /// Coletor YouTube.
    pub coletor: Arc<dyn ColetorDeCanal>,
}

/// Caminho da base de dados.
///
/// `YTA_DB_PATH` tem prioridade; caso contrário usa
/// `XDG_DATA_HOME/yta/yta.db` ou `~/.local/share/yta/yta.db`.
pub fn caminho_db() -> anyhow::Result<PathBuf> {
    if let Some(caminho) = std::env::var_os("YTA_DB_PATH") {
        return Ok(PathBuf::from(caminho));
    }

    let base = if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local").join("share")
    } else {
        return Err(anyhow!(
            "não foi possível determinar o diretório de dados; define YTA_DB_PATH"
        ));
    };

    Ok(base.join("yta").join("yta.db"))
}

/// Abre o repositório, criando o diretório pai se necessário.
pub fn abrir_repositorio(caminho: &Path) -> anyhow::Result<Arc<dyn RepositorioDeCanais>> {
    if let Some(parent) = caminho.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("criar diretório {}", parent.display()))?;
        }
    }

    let caminho = caminho
        .to_str()
        .ok_or_else(|| anyhow!("o caminho da base de dados não é UTF-8"))?;
    let repositorio = SqliteRepositorio::new(caminho)
        .with_context(|| format!("abrir base de dados {caminho}"))?;
    Ok(Arc::new(repositorio))
}

/// Constrói o coletor do YouTube com a profundidade de comentários pedida.
pub fn montar_coletor(
    api_key: Option<String>,
    max_depth: i32,
) -> anyhow::Result<Arc<dyn ColetorDeCanal>> {
    let coletor = match api_key {
        Some(chave) => ApiV3Coletor::new(chave, max_depth),
        None => ApiV3Coletor::from_env(max_depth),
    }
    .context("criar coletor do YouTube")?;
    Ok(Arc::new(coletor))
}

/// Monta repositório e coletor com a profundidade padrão.
pub fn montar(caminho: &Path, api_key: Option<String>) -> anyhow::Result<Dependencias> {
    let repositorio = abrir_repositorio(caminho)?;
    let coletor = montar_coletor(api_key, MAX_DEPTH_PADRAO)?;
    Ok(Dependencias {
        repositorio,
        coletor,
    })
}
