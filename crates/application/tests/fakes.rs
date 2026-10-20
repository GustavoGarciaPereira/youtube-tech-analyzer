// Módulo partilhado por vários testes; cada binário usa um subconjunto.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::Utc;
use domain::CanalRef;
use domain::entities::{
    canal::Canal,
    comentario::Comentario,
    nicho::{ClassificacaoNicho, Nicho},
    snapshot::{EntidadeTipo, Metrica, Snapshot},
    video::Video,
};
use domain::errors::ErroDominio;
use domain::ids::{CanalId, ComentarioId, VideoId};
use domain::ports::{ColetorDeCanal, RepositorioDeCanais};
use domain::value_objects::CategoriaManual;

/// Coletor configurável para testes, sem rede.
pub struct FakeColetor {
    canal: Canal,
    videos: Vec<Video>,
    comentarios: HashMap<VideoId, Vec<Comentario>>,
    falhas: HashSet<VideoId>,
    atraso: Duration,
    chamadas_comentarios: AtomicUsize,
    ativos: AtomicUsize,
    max_ativos: AtomicUsize,
}

impl FakeColetor {
    /// Cria um coletor com um canal, vídeos e comentários canned.
    #[must_use]
    pub fn novo(
        canal: Canal,
        videos: Vec<Video>,
        comentarios: HashMap<VideoId, Vec<Comentario>>,
    ) -> Self {
        Self {
            canal,
            videos,
            comentarios,
            falhas: HashSet::new(),
            atraso: Duration::ZERO,
            chamadas_comentarios: AtomicUsize::new(0),
            ativos: AtomicUsize::new(0),
            max_ativos: AtomicUsize::new(0),
        }
    }

    /// Configura vídeos que devem falhar na recolha de comentários.
    #[must_use]
    pub fn com_falhas(mut self, falhas: impl IntoIterator<Item = VideoId>) -> Self {
        self.falhas.extend(falhas);
        self
    }

    /// Adiciona um atraso à recolha de comentários.
    #[must_use]
    pub fn com_atraso(mut self, atraso: Duration) -> Self {
        self.atraso = atraso;
        self
    }

    /// Número de chamadas a `coletar_comentarios`.
    #[must_use]
    pub fn chamadas_comentarios(&self) -> usize {
        self.chamadas_comentarios.load(Ordering::SeqCst)
    }

    /// Pico de chamadas concorrentes a `coletar_comentarios`.
    #[must_use]
    pub fn max_ativos(&self) -> usize {
        self.max_ativos.load(Ordering::SeqCst)
    }
}

impl ColetorDeCanal for FakeColetor {
    fn coletar_canal<'a>(
        &'a self,
        _canal_ref: &CanalRef,
    ) -> Pin<Box<dyn Future<Output = Result<Canal, ErroDominio>> + Send + 'a>> {
        Box::pin(async move { Ok(self.canal.clone()) })
    }

    fn coletar_videos<'a>(
        &'a self,
        canal_id: &CanalId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Video>, ErroDominio>> + Send + 'a>> {
        let canal_id = canal_id.clone();
        Box::pin(async move {
            Ok(self
                .videos
                .iter()
                .filter(|video| video.canal_id == canal_id)
                .cloned()
                .collect())
        })
    }

    fn coletar_comentarios<'a>(
        &'a self,
        video_id: &VideoId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Comentario>, ErroDominio>> + Send + 'a>> {
        let video_id = video_id.clone();
        Box::pin(async move {
            self.chamadas_comentarios.fetch_add(1, Ordering::SeqCst);
            let ativos = self.ativos.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_ativos.fetch_max(ativos, Ordering::SeqCst);

            if !self.atraso.is_zero() {
                tokio::time::sleep(self.atraso).await;
            }

            self.ativos.fetch_sub(1, Ordering::SeqCst);

            if self.falhas.contains(&video_id) {
                return Err(ErroDominio::Coleta(format!(
                    "falha simulada para o vídeo {video_id}"
                )));
            }

            Ok(self.comentarios.get(&video_id).cloned().unwrap_or_default())
        })
    }
}

#[derive(Default)]
struct EstadoFake {
    canais: HashMap<CanalId, Canal>,
    videos: HashMap<VideoId, Video>,
    comentarios: HashMap<ComentarioId, Comentario>,
    snapshots: Vec<Snapshot>,
    classificacoes: HashMap<VideoId, ClassificacaoNicho>,
    nicho_por_canal: HashMap<CanalId, Nicho>,
    categoria_por_canal: HashMap<CanalId, CategoriaManual>,
}

/// Repositório em memória para testes.
pub struct FakeRepositorio {
    estado: Mutex<EstadoFake>,
}

impl FakeRepositorio {
    /// Cria um repositório vazio.
    #[must_use]
    pub fn new() -> Self {
        Self {
            estado: Mutex::new(EstadoFake::default()),
        }
    }

    fn estado(&self) -> MutexGuard<'_, EstadoFake> {
        self.estado.lock().expect("lock do repositório fake")
    }

    /// Número de comentários persistidos.
    #[must_use]
    pub fn contar_comentarios(&self) -> usize {
        self.estado().comentarios.len()
    }

    /// Número de vídeos persistidos.
    #[must_use]
    pub fn contar_videos(&self) -> usize {
        self.estado().videos.len()
    }

    /// Snapshots persistidos.
    #[must_use]
    pub fn snapshots(&self) -> Vec<Snapshot> {
        self.estado().snapshots.clone()
    }

    /// Define o nicho devolvido por `nicho_do_canal`.
    pub fn definir_nicho(&self, canal_id: CanalId, nicho: Nicho) {
        self.estado().nicho_por_canal.insert(canal_id, nicho);
    }
}

impl Default for FakeRepositorio {
    fn default() -> Self {
        Self::new()
    }
}

impl RepositorioDeCanais for FakeRepositorio {
    fn guardar_canal(&self, canal: &Canal) -> Result<(), ErroDominio> {
        self.estado().canais.insert(canal.id.clone(), canal.clone());
        Ok(())
    }

    fn obter_canal(&self, id: &CanalId) -> Result<Option<Canal>, ErroDominio> {
        Ok(self.estado().canais.get(id).cloned())
    }

    fn listar_canais(&self) -> Result<Vec<Canal>, ErroDominio> {
        let mut canais: Vec<Canal> = self.estado().canais.values().cloned().collect();
        canais.sort_by(|a, b| {
            a.nome
                .cmp(&b.nome)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        Ok(canais)
    }

    fn nicho_do_canal(&self, id: &CanalId) -> Result<Option<Nicho>, ErroDominio> {
        Ok(self.estado().nicho_por_canal.get(id).copied())
    }

    fn definir_categoria_manual(
        &self,
        canal_id: &CanalId,
        categoria: CategoriaManual,
    ) -> Result<(), ErroDominio> {
        let mut estado = self.estado();
        if !estado.canais.contains_key(canal_id) {
            return Err(ErroDominio::NaoEncontrado {
                recurso: "canal",
                id: canal_id.as_str().to_owned(),
            });
        }
        estado
            .categoria_por_canal
            .insert(canal_id.clone(), categoria);
        Ok(())
    }

    fn categoria_manual_do_canal(
        &self,
        canal_id: &CanalId,
    ) -> Result<Option<CategoriaManual>, ErroDominio> {
        Ok(self.estado().categoria_por_canal.get(canal_id).copied())
    }

    fn canais_por_categoria(&self, categoria: CategoriaManual) -> Result<Vec<Canal>, ErroDominio> {
        let estado = self.estado();
        let mut canais: Vec<Canal> = estado
            .canais
            .values()
            .filter(|canal| {
                estado
                    .categoria_por_canal
                    .get(&canal.id)
                    .is_some_and(|categoria_canal| *categoria_canal == categoria)
            })
            .cloned()
            .collect();
        canais.sort_by(|a, b| {
            a.nome
                .cmp(&b.nome)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        Ok(canais)
    }

    fn guardar_video(&self, video: &Video) -> Result<(), ErroDominio> {
        self.estado().videos.insert(video.id.clone(), video.clone());
        Ok(())
    }

    fn upsert_videos_batch(&self, videos: &[Video]) -> Result<(), ErroDominio> {
        let mut estado = self.estado();
        for video in videos {
            estado.videos.insert(video.id.clone(), video.clone());
        }
        Ok(())
    }

    fn listar_videos(&self, canal_id: &CanalId) -> Result<Vec<Video>, ErroDominio> {
        let mut videos: Vec<Video> = self
            .estado()
            .videos
            .values()
            .filter(|video| &video.canal_id == canal_id)
            .cloned()
            .collect();
        videos.sort_by(|a, b| a.publicado_em.cmp(&b.publicado_em));
        Ok(videos)
    }

    fn guardar_comentario(&self, comentario: &Comentario) -> Result<(), ErroDominio> {
        self.estado()
            .comentarios
            .insert(comentario.id.clone(), comentario.clone());
        Ok(())
    }

    fn upsert_comentarios_batch(&self, comentarios: &[Comentario]) -> Result<(), ErroDominio> {
        let mut estado = self.estado();
        for comentario in comentarios {
            estado
                .comentarios
                .insert(comentario.id.clone(), comentario.clone());
        }
        Ok(())
    }

    fn listar_comentarios(&self, video_id: &VideoId) -> Result<Vec<Comentario>, ErroDominio> {
        let mut comentarios: Vec<Comentario> = self
            .estado()
            .comentarios
            .values()
            .filter(|comentario| &comentario.video_id == video_id)
            .cloned()
            .collect();
        comentarios.sort_by(|a, b| a.publicado_em.cmp(&b.publicado_em));
        Ok(comentarios)
    }

    fn guardar_snapshot(&self, snapshot: &Snapshot) -> Result<(), ErroDominio> {
        upsert_snapshot(&mut self.estado(), snapshot.clone());
        Ok(())
    }

    fn upsert_snapshots_batch(&self, snapshots: &[Snapshot]) -> Result<(), ErroDominio> {
        let mut estado = self.estado();
        for snapshot in snapshots {
            upsert_snapshot(&mut estado, snapshot.clone());
        }
        Ok(())
    }

    fn comentarios_do_video_ja_coletados(&self, id: &VideoId) -> Result<bool, ErroDominio> {
        Ok(self.estado().snapshots.iter().any(|snapshot| {
            snapshot.entidade_tipo == EntidadeTipo::Video
                && snapshot.entidade_id == id.as_str()
                && snapshot.metrica == Metrica::ComentariosColetados
        }))
    }

    fn video_metricas_hoje_ja_coletadas(&self, id: &VideoId) -> Result<bool, ErroDominio> {
        let hoje = Utc::now().date_naive().to_string();
        Ok(self.estado().snapshots.iter().any(|snapshot| {
            snapshot.entidade_tipo == EntidadeTipo::Video
                && snapshot.entidade_id == id.as_str()
                && matches!(
                    snapshot.metrica,
                    Metrica::Visualizacoes | Metrica::Curtidas | Metrica::TotalComentarios
                )
                && snapshot.data_coleta == hoje
        }))
    }

    fn listar_snapshots(&self, canal_id: &CanalId) -> Result<Vec<Snapshot>, ErroDominio> {
        let mut snapshots: Vec<Snapshot> = self
            .estado()
            .snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.entidade_tipo == EntidadeTipo::Canal
                    && snapshot.entidade_id == canal_id.as_str()
            })
            .cloned()
            .collect();
        snapshots.sort_by(|a, b| b.data_coleta.cmp(&a.data_coleta));
        Ok(snapshots)
    }

    fn listar_serie_temporal(
        &self,
        entidade_tipo: EntidadeTipo,
        entidade_id: &str,
        metrica: Metrica,
    ) -> Result<Vec<Snapshot>, ErroDominio> {
        let mut snapshots: Vec<Snapshot> = self
            .estado()
            .snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.entidade_tipo == entidade_tipo
                    && snapshot.entidade_id == entidade_id
                    && snapshot.metrica == metrica
            })
            .cloned()
            .collect();
        snapshots.sort_by(|a, b| a.data_coleta.cmp(&b.data_coleta));
        Ok(snapshots)
    }

    fn guardar_classificacao(&self, classificacao: &ClassificacaoNicho) -> Result<(), ErroDominio> {
        self.estado()
            .classificacoes
            .insert(classificacao.video_id.clone(), classificacao.clone());
        Ok(())
    }

    fn obter_classificacao(
        &self,
        video_id: &VideoId,
    ) -> Result<Option<ClassificacaoNicho>, ErroDominio> {
        Ok(self.estado().classificacoes.get(video_id).cloned())
    }

    fn listar_videos_por_nicho(&self, nicho: Nicho) -> Result<Vec<Video>, ErroDominio> {
        let estado = self.estado();
        let mut videos: Vec<Video> = estado
            .videos
            .values()
            .filter(|video| {
                estado
                    .classificacoes
                    .get(&video.id)
                    .is_some_and(|classificacao| classificacao.nicho == nicho)
            })
            .cloned()
            .collect();
        videos.sort_by(|a, b| a.publicado_em.cmp(&b.publicado_em));
        Ok(videos)
    }
}

fn upsert_snapshot(estado: &mut EstadoFake, snapshot: Snapshot) {
    // Espelha o `ON CONFLICT DO NOTHING` do adaptador SQLite: a primeira
    // observação do dia é preservada.
    let existe = estado.snapshots.iter().any(|existente| {
        existente.entidade_tipo == snapshot.entidade_tipo
            && existente.entidade_id == snapshot.entidade_id
            && existente.metrica == snapshot.metrica
            && existente.data_coleta == snapshot.data_coleta
    });
    if !existe {
        estado.snapshots.push(snapshot);
    }
}
