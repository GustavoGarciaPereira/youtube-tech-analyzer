use std::time::Duration;

use domain::BoxFuture;
use tokio::process::Command;

use crate::cliente_llm::{ClienteLLM, PedidoClassificacao, RespostaBruta};
use crate::errors::ErroIA;

const TIMEOUT: Duration = Duration::from_secs(60);

/// Cliente LLM baseado no CLI do OpenCode.
///
/// A CLI atual não expõe `--prompt <ficheiro>`: a mensagem é passada como
/// argumento posicional. Esta é a substituição autorizada ao fallback do
/// prompt original. O modelo é obrigatório e vem de `OPENCODE_MODEL`.
pub struct ClienteOpenCode {
    bin: String,
    model: String,
}

impl ClienteOpenCode {
    /// Lê `OPENCODE_BIN` (default `opencode`) e exige `OPENCODE_MODEL`.
    pub fn new() -> Result<Self, ErroIA> {
        let bin = std::env::var("OPENCODE_BIN").unwrap_or_else(|_| "opencode".to_owned());
        let model = std::env::var("OPENCODE_MODEL")
            .map_err(|_| ErroIA::Cliente("OPENCODE_MODEL não definido".to_owned()))?;

        if bin.trim().is_empty() {
            return Err(ErroIA::Cliente("OPENCODE_BIN vazio".to_owned()));
        }
        if model.trim().is_empty() {
            return Err(ErroIA::Cliente("OPENCODE_MODEL vazio".to_owned()));
        }

        Ok(Self { bin, model })
    }

    /// Cria um cliente explícito, útil para o *composition root*.
    #[must_use]
    pub fn with_config(bin: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            bin: bin.into(),
            model: model.into(),
        }
    }
}

impl ClienteLLM for ClienteOpenCode {
    fn classificar(
        &self,
        pedido: PedidoClassificacao,
    ) -> BoxFuture<'_, Result<RespostaBruta, ErroIA>> {
        Box::pin(async move {
            let mut command = Command::new(&self.bin);
            command
                .arg("run")
                .arg("--model")
                .arg(&self.model)
                .arg(&pedido.texto);
            command.kill_on_drop(true);

            let output = match tokio::time::timeout(TIMEOUT, command.output()).await {
                Ok(Ok(output)) => output,
                Ok(Err(erro)) => {
                    return Err(ErroIA::Cliente(format!(
                        "falha ao executar {}: {erro}",
                        self.bin
                    )));
                }
                Err(_) => return Err(ErroIA::Timeout),
            };

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ErroIA::Cliente(format!(
                    "{} terminou com {}: {}",
                    self.bin,
                    output.status,
                    stderr.trim()
                )));
            }

            let conteudo = String::from_utf8(output.stdout)
                .map_err(|erro| ErroIA::Cliente(format!("stdout inválido: {erro}")))?;
            if conteudo.trim().is_empty() {
                return Err(ErroIA::Cliente("stdout vazio".to_owned()));
            }

            Ok(RespostaBruta {
                conteudo,
                modelo: self.model.clone(),
            })
        })
    }
}
