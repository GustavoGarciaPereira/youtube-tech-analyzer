//! Teste manual da qualidade das classificações, sem SQLite.
//!
//! Uso:
//! `cargo run -p infra-ai --example classificar_manual -- --canal @handle --max-videos 3`

use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use domain::BoxFuture;
use domain::entities::nicho::ClassificacaoNicho;
use domain::errors::ErroDominio;
use domain::ids::VideoId;
use domain::ports::{CacheDeClassificacoes, ColetorDeCanal};
use infra_ai::cliente_llm::ClienteLLM;
use infra_ai::{ClassificadorComCache, ClienteOpenCode, Taxonomia};
use infra_youtube::{ApiV3Coletor, parse_canal_ref};

struct CacheMemoria {
    entradas: Mutex<HashMap<(VideoId, String, String), ClassificacaoNicho>>,
}

impl CacheMemoria {
    fn new() -> Self {
        Self {
            entradas: Mutex::new(HashMap::new()),
        }
    }
}

impl CacheDeClassificacoes for CacheMemoria {
    fn obter<'a>(
        &'a self,
        video_id: &'a VideoId,
        hash_conteudo: &'a str,
        versao_classificador: &'a str,
    ) -> BoxFuture<'a, Result<Option<ClassificacaoNicho>, ErroDominio>> {
        let chave = (
            video_id.clone(),
            hash_conteudo.to_owned(),
            versao_classificador.to_owned(),
        );
        Box::pin(async move {
            let entradas = self
                .entradas
                .lock()
                .map_err(|_| ErroDominio::Repositorio("cache lock envenenado".to_owned()))?;
            Ok(entradas.get(&chave).cloned())
        })
    }

    fn gravar<'a>(
        &'a self,
        video_id: &'a VideoId,
        hash_conteudo: &'a str,
        versao_classificador: &'a str,
        classificacao: &'a ClassificacaoNicho,
    ) -> BoxFuture<'a, Result<(), ErroDominio>> {
        let chave = (
            video_id.clone(),
            hash_conteudo.to_owned(),
            versao_classificador.to_owned(),
        );
        let classificacao = classificacao.clone();
        Box::pin(async move {
            let mut entradas = self
                .entradas
                .lock()
                .map_err(|_| ErroDominio::Repositorio("cache lock envenenado".to_owned()))?;
            entradas.insert(chave, classificacao);
            Ok(())
        })
    }

    fn contar_por_versao<'a>(
        &'a self,
        versao_classificador: &'a str,
    ) -> BoxFuture<'a, Result<usize, ErroDominio>> {
        let versao = versao_classificador.to_owned();
        Box::pin(async move {
            let entradas = self
                .entradas
                .lock()
                .map_err(|_| ErroDominio::Repositorio("cache lock envenenado".to_owned()))?;
            Ok(entradas
                .keys()
                .filter(|(_, _, versao_entrada)| versao_entrada == &versao)
                .count())
        })
    }
}

fn erro(mensagem: &str) -> Box<dyn Error> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        mensagem.to_owned(),
    ))
}

fn argumentos() -> Result<(String, usize), Box<dyn Error>> {
    let mut canal = None;
    let mut max_videos = 3;
    let mut args = std::env::args().skip(1);

    while let Some(argumento) = args.next() {
        match argumento.as_str() {
            "--canal" => canal = args.next(),
            "--max-videos" => {
                let valor = args
                    .next()
                    .ok_or_else(|| erro("--max-videos requer um valor"))?;
                max_videos = valor.parse()?;
            }
            "--help" | "-h" => {
                println!("Uso: classificar_manual --canal <@handle|URL|UC...> [--max-videos N]");
                std::process::exit(0);
            }
            outro => return Err(erro(&format!("argumento desconhecido: {outro}"))),
        }
    }

    let canal = canal.ok_or_else(|| erro("--canal é obrigatório"))?;
    Ok((canal, max_videos))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (canal_ref, max_videos) = argumentos()?;

    if std::env::var("YOUTUBE_API_KEY").is_err() {
        return Err(erro("YOUTUBE_API_KEY não definida"));
    }
    if std::env::var("OPENCODE_MODEL").is_err() {
        return Err(erro("OPENCODE_MODEL não definida"));
    }

    let canal_ref = parse_canal_ref(&canal_ref)?;
    let coletor = ApiV3Coletor::from_env_padrao()?;
    let canal = coletor.coletar_canal(&canal_ref).await?;
    let mut videos = coletor.coletar_videos(&canal.id).await?;
    videos.sort_by(|a, b| b.publicado_em.cmp(&a.publicado_em));
    videos.truncate(max_videos);

    println!("Canal: {} ({})", canal.nome, canal.id);
    if videos.is_empty() {
        println!("Nenhum vídeo encontrado.");
        return Ok(());
    }

    let cliente: Arc<dyn ClienteLLM> = Arc::new(ClienteOpenCode::new()?);
    let cache: Arc<dyn CacheDeClassificacoes> = Arc::new(CacheMemoria::new());
    let classificador =
        ClassificadorComCache::new(cliente, cache, Taxonomia::default(), "manual-v1");

    for video in videos {
        match classificador.classificar_detalhado(&video).await {
            Ok(detalhe) => {
                let justificativa = detalhe
                    .justificativa
                    .as_deref()
                    .unwrap_or("(cache sem justificativa)");
                println!(
                    "{} → {} ({:.2}) · {}",
                    video.titulo,
                    detalhe.classificacao.nicho.as_str(),
                    detalhe.classificacao.confianca,
                    justificativa
                );
            }
            Err(erro) => eprintln!("{} → erro: {erro}", video.titulo),
        }
    }

    Ok(())
}
