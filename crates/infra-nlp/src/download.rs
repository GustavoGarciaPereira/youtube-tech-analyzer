use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::erro::ErroNlp;

/// URL do modelo ONNX quantizado (INT8) da CardiffNLP via Xenova.
pub const URL_MODELO: &str = "https://huggingface.co/Xenova/twitter-roberta-base-sentiment-latest/resolve/main/onnx/model_quantized.onnx";

/// URL do tokenizer do mesmo modelo.
pub const URL_TOKENIZER: &str = "https://huggingface.co/Xenova/twitter-roberta-base-sentiment-latest/resolve/main/tokenizer.json";

/// Runtime ONNX Runtime oficial (manylinux, compatível com glibc >= 2.27).
pub const URL_RUNTIME: &str = "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-linux-x64-1.22.0.tgz";

/// Versão do runtime; tem de coincidir com a esperada pela crate ort.
pub const VERSAO_RUNTIME: &str = "1.22.0";

/// Tamanho mínimo aceitável para considerar um ficheiro descarregado.
const TAMANHO_MINIMO: u64 = 1024;

/// Tamanho mínimo do runtime descomprimido (~21 MB no original).
const TAMANHO_MINIMO_RUNTIME: u64 = 1_000_000;

/// Gere o download e a localização local dos artefactos do modelo.
#[derive(Debug, Clone)]
pub struct ModelManager {
    base: PathBuf,
    url_modelo: String,
    url_tokenizer: String,
}

impl ModelManager {
    /// Cria um gestor com o caminho por omissão e os URLs oficiais.
    ///
    /// O caminho é o mesmo que a aplicação usa para a base de dados:
    /// dirs::data_dir()/yta/models/roberta-sentiment.
    #[must_use]
    pub fn novo() -> Self {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("yta")
            .join("models")
            .join("roberta-sentiment");
        Self::com_fontes(base, URL_MODELO, URL_TOKENIZER)
    }

    /// Cria um gestor com caminho e fontes explícitos (útil em testes).
    #[must_use]
    pub fn com_fontes(
        base: PathBuf,
        url_modelo: impl Into<String>,
        url_tokenizer: impl Into<String>,
    ) -> Self {
        Self {
            base,
            url_modelo: url_modelo.into(),
            url_tokenizer: url_tokenizer.into(),
        }
    }

    /// Pasta onde os artefactos são guardados.
    #[must_use]
    pub fn base(&self) -> &Path {
        &self.base
    }

    /// Caminho do modelo ONNX.
    #[must_use]
    pub fn caminho_modelo(&self) -> PathBuf {
        self.base.join("model.onnx")
    }

    /// Caminho do tokenizer.
    #[must_use]
    pub fn caminho_tokenizer(&self) -> PathBuf {
        self.base.join("tokenizer.json")
    }

    /// Caminho da biblioteca do ONNX Runtime.
    #[must_use]
    pub fn caminho_runtime(&self) -> PathBuf {
        self.base.join("libonnxruntime.so")
    }

    /// Garante que o modelo e o tokenizer estão no disco, descarregando o que faltar.
    ///
    /// Devolve a pasta que contém os dois ficheiros.
    pub async fn garantir_modelo_baixado(&self) -> Result<PathBuf, ErroNlp> {
        tokio::fs::create_dir_all(&self.base).await?;

        if !ficheiro_utilizavel(&self.caminho_modelo()).await {
            tracing::info!(url = %self.url_modelo, "a descarregar modelo de sentimento");
            self.descarregar(&self.url_modelo, &self.caminho_modelo())
                .await?;
        }
        if !ficheiro_utilizavel(&self.caminho_tokenizer()).await {
            tracing::info!(url = %self.url_tokenizer, "a descarregar tokenizer");
            self.descarregar(&self.url_tokenizer, &self.caminho_tokenizer())
                .await?;
        }
        if !runtime_utilizavel(&self.caminho_runtime()).await {
            tracing::info!(url = %URL_RUNTIME, "a descarregar ONNX Runtime");
            let arquivo = self.base.join("onnxruntime.tgz");
            self.descarregar(URL_RUNTIME, &arquivo).await?;
            let origem = arquivo.clone();
            let destino = self.caminho_runtime();
            tokio::task::spawn_blocking(move || extrair_runtime(&origem, &destino))
                .await
                .map_err(|erro| {
                    ErroNlp::Inferencia(format!("tarefa de extração falhou: {erro}"))
                })??;
            let _ = tokio::fs::remove_file(&arquivo).await;
        }

        Ok(self.base.clone())
    }

    async fn descarregar(&self, url: &str, destino: &Path) -> Result<(), ErroNlp> {
        let cliente = reqwest::Client::builder()
            .user_agent("yta/0.1 (+https://github.com/GustavoGarciaPereira/youtube-tech-analyzer)")
            .build()
            .map_err(|erro| ErroNlp::Rede {
                url: url.to_owned(),
                fonte: erro.to_string(),
            })?;

        let resposta = cliente
            .get(url)
            .send()
            .await
            .map_err(|erro| ErroNlp::Rede {
                url: url.to_owned(),
                fonte: erro.to_string(),
            })?;

        let status = resposta.status();
        if !status.is_success() {
            return Err(ErroNlp::Http {
                url: url.to_owned(),
                status: status.as_u16(),
            });
        }

        let esperado = resposta.content_length();
        let parcial = PathBuf::from(format!("{}.part", destino.display()));
        let mut ficheiro = tokio::fs::File::create(&parcial).await?;
        let mut total = 0_u64;
        let mut stream = resposta.bytes_stream();

        while let Some(pedaco) = stream.next().await {
            let pedaco = pedaco.map_err(|erro| ErroNlp::Rede {
                url: url.to_owned(),
                fonte: erro.to_string(),
            })?;
            ficheiro.write_all(&pedaco).await?;
            total += pedaco.len() as u64;
        }
        ficheiro.flush().await?;
        drop(ficheiro);

        if total < TAMANHO_MINIMO {
            let _ = tokio::fs::remove_file(&parcial).await;
            return Err(ErroNlp::FicheiroInvalido {
                caminho: destino.display().to_string(),
                motivo: format!("apenas {total} bytes descarregados"),
            });
        }
        if let Some(esperado) = esperado
            && esperado != total
        {
            let _ = tokio::fs::remove_file(&parcial).await;
            return Err(ErroNlp::FicheiroInvalido {
                caminho: destino.display().to_string(),
                motivo: format!("esperados {esperado} bytes, recebidos {total}"),
            });
        }

        tokio::fs::rename(&parcial, destino).await?;
        Ok(())
    }
}

async fn runtime_utilizavel(caminho: &Path) -> bool {
    matches!(
        tokio::fs::metadata(caminho).await,
        Ok(metadados) if metadados.is_file() && metadados.len() >= TAMANHO_MINIMO_RUNTIME
    )
}

/// Extrai libonnxruntime.so.NNNN do tarball oficial para destino.
fn extrair_runtime(origem: &Path, destino: &Path) -> Result<(), ErroNlp> {
    let ficheiro = std::fs::File::open(origem)?;
    let descompactado = flate2::read::GzDecoder::new(ficheiro);
    let mut arquivo = tar::Archive::new(descompactado);
    let sufixo = format!("lib/libonnxruntime.so.{VERSAO_RUNTIME}");

    for entrada in arquivo.entries()? {
        let mut entrada = entrada?;
        let caminho = entrada.path()?.to_path_buf();
        if !caminho.to_string_lossy().ends_with(&sufixo) {
            continue;
        }

        let parcial = PathBuf::from(format!("{}.part", destino.display()));
        let mut saida = std::fs::File::create(&parcial)?;
        std::io::copy(&mut entrada, &mut saida)?;
        drop(saida);

        let tamanho = std::fs::metadata(&parcial)?.len();
        if tamanho < TAMANHO_MINIMO_RUNTIME {
            let _ = std::fs::remove_file(&parcial);
            return Err(ErroNlp::FicheiroInvalido {
                caminho: origem.display().to_string(),
                motivo: format!("runtime com apenas {tamanho} bytes"),
            });
        }
        std::fs::rename(&parcial, destino)?;
        return Ok(());
    }

    Err(ErroNlp::FicheiroInvalido {
        caminho: origem.display().to_string(),
        motivo: format!("{sufixo} não encontrado no arquivo"),
    })
}

async fn ficheiro_utilizavel(caminho: &Path) -> bool {
    matches!(
        tokio::fs::metadata(caminho).await,
        Ok(metadados) if metadados.is_file() && metadados.len() >= TAMANHO_MINIMO
    )
}

#[cfg(test)]
mod tests {
    use super::extrair_runtime;

    #[test]
    fn extrai_o_runtime_do_tarball_oficial() {
        let pasta = tempfile::tempdir().expect("pasta temporária");
        let origem = pasta.path().join("onnxruntime.tgz");
        let dados = vec![0_u8; 1_000_001];

        {
            let ficheiro = std::fs::File::create(&origem).expect("tgz");
            let gz = flate2::write::GzEncoder::new(ficheiro, flate2::Compression::fast());
            let mut construtor = tar::Builder::new(gz);
            let mut cabecalho = tar::Header::new_gnu();
            cabecalho.set_size(dados.len() as u64);
            cabecalho.set_mode(0o644);
            cabecalho.set_cksum();
            construtor
                .append_data(
                    &mut cabecalho,
                    "onnxruntime-linux-x64-1.22.0/lib/libonnxruntime.so.1.22.0",
                    dados.as_slice(),
                )
                .expect("entrada tar");
            let gz = construtor.into_inner().expect("fechar tar");
            gz.finish().expect("fechar gzip");
        }

        let destino = pasta.path().join("libonnxruntime.so");
        extrair_runtime(&origem, &destino).expect("extração");
        assert_eq!(
            std::fs::metadata(&destino).expect("destino").len(),
            dados.len() as u64
        );
    }
}
