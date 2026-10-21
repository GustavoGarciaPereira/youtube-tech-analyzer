use std::path::Path;
use std::sync::{Arc, Mutex};

use domain::ports::{AnalisadorDeSentimento, BoxFuture};
use domain::{ErroDominio, ScoreSentimento, Sentimento, VERSAO_MODELO_SENTIMENTO};
use ort::session::Session;
use ort::value::Tensor;
use tokenizers::{Tokenizer, TruncationParams};

use crate::erro::ErroNlp;

const MAX_TOKENS: usize = 128;
const ENTRADA_IDS: &str = "input_ids";
const ENTRADA_MASCARA: &str = "attention_mask";

/// Analisador de sentimento baseado no RoBERTa da CardiffNLP (3 classes).
///
/// A ordem das classes no modelo é [negativo, neutro, positivo]; o score
/// devolvido é P(positivo) - P(negativo), pelo que fica em [-1, 1].
pub struct RobertaAnalisador {
    motor: Arc<MotorInferencia>,
}

/// Estado partilhável entre tarefas de blocking.
///
/// O ONNX Runtime é síncrono e a sessão exige &mut self; a inferência corre
/// em spawn_blocking e a sessão é protegida por Mutex.
struct MotorInferencia {
    tokenizer: Tokenizer,
    sessao: Mutex<Session>,
    nome_saida: String,
}

impl RobertaAnalisador {
    /// Carrega o tokenizer, o runtime e o modelo ONNX de uma pasta.
    ///
    /// A pasta deve conter model.onnx, tokenizer.json e libonnxruntime.so,
    /// como os deixados por ModelManager::garantir_modelo_baixado.
    pub fn new(pasta_modelos: &Path) -> Result<Self, ErroNlp> {
        let caminho_modelo = pasta_modelos.join("model.onnx");
        let caminho_tokenizer = pasta_modelos.join("tokenizer.json");
        let caminho_runtime = pasta_modelos.join("libonnxruntime.so");
        if !caminho_modelo.is_file() || !caminho_tokenizer.is_file() || !caminho_runtime.is_file() {
            return Err(ErroNlp::FicheiroInvalido {
                caminho: pasta_modelos.display().to_string(),
                motivo: "model.onnx, tokenizer.json e libonnxruntime.so são obrigatórios"
                    .to_owned(),
            });
        }

        // Runtime carregado dinamicamente: evita ligar contra uma libonnxruntime
        // estática compilada para uma glibc mais recente do que a do sistema.
        ort::init_from(caminho_runtime.to_string_lossy().to_string())
            .commit()
            .map_err(|erro| ErroNlp::Ort(erro.to_string()))?;

        let mut tokenizer = Tokenizer::from_file(&caminho_tokenizer)
            .map_err(|erro| ErroNlp::Tokenizer(erro.to_string()))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(|erro| ErroNlp::Tokenizer(erro.to_string()))?;

        let sessao = Session::builder()
            .map_err(|erro| ErroNlp::Ort(erro.to_string()))?
            .commit_from_file(&caminho_modelo)
            .map_err(|erro| ErroNlp::Ort(erro.to_string()))?;

        verificar_contrato(&sessao)?;
        let nome_saida = sessao
            .outputs
            .first()
            .map(|saida| saida.name.clone())
            .ok_or_else(|| ErroNlp::ContratoModelo("modelo sem saídas".to_owned()))?;

        Ok(Self {
            motor: Arc::new(MotorInferencia {
                tokenizer,
                sessao: Mutex::new(sessao),
                nome_saida,
            }),
        })
    }
}

impl MotorInferencia {
    fn inferir(&self, texto: &str) -> Result<ScoreSentimento, ErroNlp> {
        let (ids, mascara) = preparar_entradas(&self.tokenizer, texto)?;
        let comprimento = ids.len();
        let shape = [1_i64, comprimento as i64];

        let tensor_ids =
            Tensor::from_array((shape, ids)).map_err(|erro| ErroNlp::Ort(erro.to_string()))?;
        let tensor_mascara =
            Tensor::from_array((shape, mascara)).map_err(|erro| ErroNlp::Ort(erro.to_string()))?;

        let mut sessao = self
            .sessao
            .lock()
            .map_err(|_| ErroNlp::Inferencia("sessão ONNX envenenada por um panic".to_owned()))?;

        let saidas = sessao
            .run(ort::inputs![
                ENTRADA_IDS => tensor_ids,
                ENTRADA_MASCARA => tensor_mascara,
            ])
            .map_err(|erro| ErroNlp::Inferencia(erro.to_string()))?;

        let valor = saidas
            .get(&self.nome_saida)
            .ok_or_else(|| ErroNlp::ContratoModelo(format!("saída {} ausente", self.nome_saida)))?;
        let (_, logits) = valor
            .try_extract_tensor::<f32>()
            .map_err(|erro| ErroNlp::Inferencia(erro.to_string()))?;
        if logits.len() < 3 {
            return Err(ErroNlp::ContratoModelo(format!(
                "esperadas 3 classes na saída, recebidas {}",
                logits.len()
            )));
        }

        let probabilidades = softmax(&logits[..3]);
        let (indice, _) = probabilidades
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .ok_or_else(|| ErroNlp::Inferencia("softmax vazio".to_owned()))?;

        // Ordem da CardiffNLP: 0 = negativo, 1 = neutro, 2 = positivo.
        let sentimento = match indice {
            0 => Sentimento::Negativo,
            1 => Sentimento::Neutro,
            _ => Sentimento::Positivo,
        };
        let score = (probabilidades[2] - probabilidades[0]).clamp(-1.0, 1.0);

        Ok(ScoreSentimento::novo(sentimento, score))
    }
}

impl AnalisadorDeSentimento for RobertaAnalisador {
    fn analisar<'a>(
        &'a self,
        texto: &'a str,
    ) -> BoxFuture<'a, Result<ScoreSentimento, ErroDominio>> {
        let motor = Arc::clone(&self.motor);
        let texto = texto.to_owned();

        Box::pin(async move {
            // A inferência é síncrona (C++); sem spawn_blocking prendia uma
            // worker thread do Tokio durante todo o lote.
            let resultado = tokio::task::spawn_blocking(move || motor.inferir(&texto))
                .await
                .map_err(|erro| {
                    ErroDominio::Sentimento(format!("tarefa de inferência falhou: {erro}"))
                })?;
            resultado.map_err(ErroDominio::from)
        })
    }

    fn versao_modelo(&self) -> &str {
        VERSAO_MODELO_SENTIMENTO
    }
}

fn verificar_contrato(sessao: &Session) -> Result<(), ErroNlp> {
    let nomes: Vec<&str> = sessao
        .inputs
        .iter()
        .map(|entrada| entrada.name.as_str())
        .collect();
    for esperado in [ENTRADA_IDS, ENTRADA_MASCARA] {
        if !nomes.contains(&esperado) {
            return Err(ErroNlp::ContratoModelo(format!(
                "entrada {} ausente; o modelo expõe: {}",
                esperado,
                nomes.join(", ")
            )));
        }
    }
    Ok(())
}

/// Converte texto em ids e máscara de atenção com o limite de tokens aplicado.
fn preparar_entradas(tokenizer: &Tokenizer, texto: &str) -> Result<(Vec<i64>, Vec<i64>), ErroNlp> {
    let encoding = tokenizer
        .encode(texto, true)
        .map_err(|erro| ErroNlp::Tokenizer(erro.to_string()))?;
    let ids: Vec<i64> = encoding.get_ids().iter().map(|id| i64::from(*id)).collect();
    let mascara: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|valor| i64::from(*valor))
        .collect();
    if ids.is_empty() || ids.len() != mascara.len() {
        return Err(ErroNlp::Inferencia(format!(
            "tokenização inválida: {} ids, {} valores de máscara",
            ids.len(),
            mascara.len()
        )));
    }
    Ok((ids, mascara))
}

fn softmax(logits: &[f32]) -> [f64; 3] {
    let maximo = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let exponenciais: Vec<f64> = logits
        .iter()
        .map(|logit| (f64::from(*logit) - maximo).exp())
        .collect();
    let soma: f64 = exponenciais.iter().sum();
    [
        exponenciais[0] / soma,
        exponenciais[1] / soma,
        exponenciais[2] / soma,
    ]
}

#[cfg(test)]
mod tests {
    use super::{preparar_entradas, softmax};
    use std::path::PathBuf;
    use tokenizers::{Tokenizer, TruncationParams};

    fn tokenizer_de_teste() -> Tokenizer {
        let caminho = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tokenizer.json");
        let mut tokenizer = Tokenizer::from_file(caminho).expect("tokenizer de teste");
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: 128,
                ..Default::default()
            }))
            .expect("truncation");
        tokenizer
    }

    #[test]
    fn entradas_tem_o_mesmo_comprimento_e_respeitam_o_limite() {
        let tokenizer = tokenizer_de_teste();
        let texto = "hello world rust ".repeat(100);
        let (ids, mascara) = preparar_entradas(&tokenizer, &texto).expect("entradas");

        assert_eq!(ids.len(), mascara.len());
        assert_eq!(ids.len(), 128);
        assert!(mascara.iter().all(|valor| *valor == 1));
    }

    #[test]
    fn softmax_soma_um_e_preserva_a_ordem() {
        let probabilidades = softmax(&[1.0, 2.0, 3.0]);
        let soma: f64 = probabilidades.iter().sum();
        assert!((soma - 1.0).abs() < 1e-12);
        assert!(probabilidades[2] > probabilidades[1]);
        assert!(probabilidades[1] > probabilidades[0]);
    }
}
