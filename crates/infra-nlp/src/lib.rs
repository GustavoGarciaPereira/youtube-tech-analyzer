//! Motor de NLP local: ONNX Runtime + tokenizers da HuggingFace.
//!
//! Não depende de Python nem de serviços externos em runtime: o modelo é
//! descarregado uma vez para a pasta de dados do utilizador e executado
//! localmente.

#![forbid(unsafe_code)]

pub mod analisador;
pub mod download;
pub mod erro;

pub use analisador::RobertaAnalisador;
pub use download::ModelManager;
pub use erro::ErroNlp;
