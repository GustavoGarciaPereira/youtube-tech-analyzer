use crate::errors::ErroDominio;
use crate::ports::BoxFuture;
use crate::value_objects::ScoreSentimento;

/// Versão do modelo de sentimento usada por omissão.
///
/// A versão faz parte da chave de persistência (`comentarios_sentimento`) e
/// da consulta do relatório. Mudar de modelo exige uma versão nova para não
/// reutilizar análises feitas com outro modelo.
pub const VERSAO_MODELO_SENTIMENTO: &str = "roberta-sentiment-int8-v1";

/// Porta de saída para classificar o sentimento de um texto.
///
/// A implementação concreta vive em infra-ai (ONNX Runtime + tokenizers) e é
/// injetada pelo composition root. Os futures são boxed para manter a trait
/// dyn-compatible, tal como nas restantes portas assíncronas.
///
/// A versão do modelo faz parte do contrato: permite re-analisar comentários
/// quando o modelo muda sem repetir trabalho já feito com a versão atual.
pub trait AnalisadorDeSentimento: Send + Sync {
    /// Classifica o sentimento de um texto.
    fn analisar<'a>(
        &'a self,
        texto: &'a str,
    ) -> BoxFuture<'a, Result<ScoreSentimento, ErroDominio>>;

    /// Identificador estável da versão do modelo em uso.
    fn versao_modelo(&self) -> &str;
}
