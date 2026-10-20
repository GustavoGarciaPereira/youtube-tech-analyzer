use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use domain::BoxFuture;
use domain::entities::nicho::ClassificacaoNicho;
use domain::errors::ErroDominio;
use domain::ids::VideoId;
use domain::ports::CacheDeClassificacoes;
use infra_ai::ErroIA;
use infra_ai::cliente_llm::{ClienteLLM, PedidoClassificacao, RespostaBruta};

/// Cache em memória para testes.
#[derive(Default)]
pub struct FakeCache {
    entradas: Mutex<HashMap<(VideoId, String, String), ClassificacaoNicho>>,
}

impl FakeCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inserir(
        &self,
        video_id: VideoId,
        hash_conteudo: String,
        versao_classificador: String,
        classificacao: ClassificacaoNicho,
    ) {
        self.entradas.lock().expect("lock do cache fake").insert(
            (video_id, hash_conteudo, versao_classificador),
            classificacao,
        );
    }

    #[must_use]
    pub fn esta_vazio(&self) -> bool {
        self.entradas.lock().expect("lock do cache fake").is_empty()
    }

    #[must_use]
    pub fn total_por_versao(&self, versao: &str) -> usize {
        self.entradas
            .lock()
            .expect("lock do cache fake")
            .keys()
            .filter(|(_, _, versao_entrada)| versao_entrada == versao)
            .count()
    }
}

impl CacheDeClassificacoes for FakeCache {
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
            Ok(self
                .entradas
                .lock()
                .expect("lock do cache fake")
                .get(&chave)
                .cloned())
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
            self.entradas
                .lock()
                .expect("lock do cache fake")
                .insert(chave, classificacao);
            Ok(())
        })
    }

    fn contar_por_versao<'a>(
        &'a self,
        versao_classificador: &'a str,
    ) -> BoxFuture<'a, Result<usize, ErroDominio>> {
        let versao = versao_classificador.to_owned();
        Box::pin(async move { Ok(self.total_por_versao(&versao)) })
    }
}

/// Cliente LLM canned para testes.
pub struct FakeClienteLLM {
    respostas: Mutex<VecDeque<Result<RespostaBruta, ErroIA>>>,
    chamadas: AtomicUsize,
}

impl FakeClienteLLM {
    #[must_use]
    pub fn novo(respostas: Vec<Result<RespostaBruta, ErroIA>>) -> Self {
        Self {
            respostas: Mutex::new(respostas.into()),
            chamadas: AtomicUsize::new(0),
        }
    }

    #[must_use]
    pub fn chamadas(&self) -> usize {
        self.chamadas.load(Ordering::SeqCst)
    }

    pub fn resposta(conteudo: &str) -> Result<RespostaBruta, ErroIA> {
        Ok(RespostaBruta {
            conteudo: conteudo.to_owned(),
            modelo: "fake-model".to_owned(),
        })
    }
}

impl ClienteLLM for FakeClienteLLM {
    fn classificar(
        &self,
        _pedido: PedidoClassificacao,
    ) -> BoxFuture<'_, Result<RespostaBruta, ErroIA>> {
        Box::pin(async move {
            self.chamadas.fetch_add(1, Ordering::SeqCst);
            self.respostas
                .lock()
                .expect("lock do cliente fake")
                .pop_front()
                .unwrap_or_else(|| Err(ErroIA::Cliente("sem respostas canned".to_owned())))
        })
    }
}
