use std::sync::{Mutex, MutexGuard};

use domain::entities::{
    canal::Canal,
    comentario::Comentario,
    nicho::{ClassificacaoNicho, Nicho},
    snapshot::{EntidadeTipo, Metrica, Snapshot},
    video::Video,
};
use domain::errors::ErroDominio;
use domain::ids::{CanalId, VideoId};
use domain::ports::RepositorioDeCanais;
use rusqlite::{Connection, params};

use crate::error::Error;
use crate::mapeamento::{
    mapear_canal, mapear_classificacao, mapear_comentario, mapear_snapshot, mapear_video,
};

const SCHEMA: &str = include_str!("../migrations/schema.sql");

const UPSERT_CANAL: &str = r#"
INSERT INTO canais (
    id, nome, handle, url, descricao, inscritos, total_videos,
    topic_categories, criado_em, data_coleta
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
)
ON CONFLICT(id) DO UPDATE SET
    nome = excluded.nome,
    handle = excluded.handle,
    url = excluded.url,
    descricao = excluded.descricao,
    inscritos = excluded.inscritos,
    total_videos = excluded.total_videos,
    topic_categories = excluded.topic_categories,
    criado_em = excluded.criado_em,
    data_coleta = excluded.data_coleta
"#;

const UPSERT_VIDEO: &str = r#"
INSERT INTO videos (
    id, canal_id, titulo, descricao, url, tags, topic_categories,
    publicado_em, duracao_segundos, visualizacoes, curtidas,
    total_comentarios, data_coleta
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
)
ON CONFLICT(id) DO UPDATE SET
    canal_id = excluded.canal_id,
    titulo = excluded.titulo,
    descricao = excluded.descricao,
    url = excluded.url,
    tags = excluded.tags,
    topic_categories = excluded.topic_categories,
    publicado_em = excluded.publicado_em,
    duracao_segundos = excluded.duracao_segundos,
    visualizacoes = excluded.visualizacoes,
    curtidas = excluded.curtidas,
    total_comentarios = excluded.total_comentarios,
    data_coleta = excluded.data_coleta
"#;

const UPSERT_COMENTARIO: &str = r#"
INSERT INTO comentarios (
    id, video_id, autor, texto, publicado_em, curtidas, data_coleta
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
)
ON CONFLICT(id) DO UPDATE SET
    video_id = excluded.video_id,
    autor = excluded.autor,
    texto = excluded.texto,
    publicado_em = excluded.publicado_em,
    curtidas = excluded.curtidas,
    data_coleta = excluded.data_coleta
"#;

const INSERT_SNAPSHOT: &str = r#"
INSERT INTO snapshots (
    entidade_tipo, entidade_id, metrica, valor, data_coleta
) VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(entidade_tipo, entidade_id, metrica, data_coleta) DO NOTHING
"#;

const UPSERT_CLASSIFICACAO: &str = r#"
INSERT INTO classificacoes_nicho (
    video_id, nicho, confianca, metodo, data_classificacao
) VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(video_id) DO UPDATE SET
    nicho = excluded.nicho,
    confianca = excluded.confianca,
    metodo = excluded.metodo,
    data_classificacao = excluded.data_classificacao
"#;

const SELECT_CANAL: &str = r#"
SELECT id, nome, handle, url, descricao, inscritos, total_videos,
       topic_categories, criado_em
FROM canais
WHERE id = ?1
"#;

const SELECT_VIDEOS_CANAL: &str = r#"
SELECT id, canal_id, titulo, descricao, url, tags, topic_categories,
       publicado_em, duracao_segundos, visualizacoes, curtidas,
       total_comentarios
FROM videos
WHERE canal_id = ?1
ORDER BY publicado_em ASC, id ASC
"#;

const SELECT_COMENTARIOS_VIDEO: &str = r#"
SELECT id, video_id, autor, texto, publicado_em, curtidas
FROM comentarios
WHERE video_id = ?1
ORDER BY publicado_em ASC, id ASC
"#;

const SELECT_SNAPSHOTS_CANAL: &str = r#"
SELECT entidade_tipo, entidade_id, metrica, valor, data_coleta
FROM snapshots
WHERE entidade_tipo = ?1 AND entidade_id = ?2
ORDER BY data_coleta DESC, metrica ASC
"#;

const SELECT_SERIE_TEMPORAL: &str = r#"
SELECT entidade_tipo, entidade_id, metrica, valor, data_coleta
FROM snapshots
WHERE entidade_tipo = ?1 AND entidade_id = ?2 AND metrica = ?3
ORDER BY data_coleta ASC
"#;

const SELECT_CLASSIFICACAO: &str = r#"
SELECT video_id, nicho, confianca, metodo, data_classificacao
FROM classificacoes_nicho
WHERE video_id = ?1
"#;

const SELECT_VIDEOS_POR_NICHO: &str = r#"
SELECT v.id, v.canal_id, v.titulo, v.descricao, v.url, v.tags,
       v.topic_categories, v.publicado_em, v.duracao_segundos,
       v.visualizacoes, v.curtidas, v.total_comentarios
FROM videos v
INNER JOIN classificacoes_nicho c ON c.video_id = v.id
WHERE c.nicho = ?1
ORDER BY v.publicado_em DESC, v.id ASC
"#;

/// Adaptador SQLite da porta [`RepositorioDeCanais`].
///
/// A conexão é aberta uma única vez e reutilizada. O schema é aplicado no
/// arranque com `CREATE TABLE IF NOT EXISTS`, pelo que abrir a mesma base de
/// dados várias vezes é seguro.
pub struct SqliteRepositorio {
    conexao: Mutex<Connection>,
}

impl SqliteRepositorio {
    /// Abre (ou cria) a base de dados no caminho indicado e aplica o schema.
    pub fn new(caminho: &str) -> Result<Self, Error> {
        tracing::debug!(caminho, "a abrir base de dados SQLite");
        let conexao = Connection::open(caminho)?;
        conexao.execute_batch(SCHEMA)?;
        Ok(Self {
            conexao: Mutex::new(conexao),
        })
    }

    /// Cria um repositório em memória, útil para testes.
    pub fn em_memoria() -> Result<Self, Error> {
        Self::new(":memory:")
    }

    fn obter_conexao(&self) -> Result<MutexGuard<'_, Connection>, Error> {
        self.conexao.lock().map_err(|_| Error::Lock)
    }

    fn salvar_canal(&self, canal: &Canal) -> Result<(), Error> {
        let conexao = self.obter_conexao()?;
        let topic_categories = serde_json::to_string(&canal.topic_categories)?;
        conexao.execute(
            UPSERT_CANAL,
            params![
                canal.id.as_str(),
                canal.nome,
                canal.handle,
                canal.url,
                canal.descricao,
                inteiro(canal.inscritos, "inscritos")?,
                inteiro(canal.total_videos, "total_videos")?,
                topic_categories,
                canal.criado_em,
            ],
        )?;
        Ok(())
    }

    fn buscar_canal(&self, id: &CanalId) -> Result<Option<Canal>, Error> {
        let conexao = self.obter_conexao()?;
        let mut stmt = conexao.prepare(SELECT_CANAL)?;
        let mut rows = stmt.query(params![id.as_str()])?;
        match rows.next()? {
            Some(row) => Ok(Some(mapear_canal(row)?)),
            None => Ok(None),
        }
    }

    fn salvar_video(&self, video: &Video) -> Result<(), Error> {
        let conexao = self.obter_conexao()?;
        let tags = serde_json::to_string(&video.tags)?;
        let topic_categories = serde_json::to_string(&video.topic_categories)?;
        conexao.execute(
            UPSERT_VIDEO,
            params![
                video.id.as_str(),
                video.canal_id.as_str(),
                video.titulo,
                video.descricao,
                video.url,
                tags,
                topic_categories,
                video.publicado_em,
                i64::from(video.duracao_segundos),
                inteiro(video.visualizacoes, "visualizacoes")?,
                inteiro(video.curtidas, "curtidas")?,
                inteiro(video.total_comentarios, "total_comentarios")?,
            ],
        )?;
        Ok(())
    }

    fn buscar_videos(&self, canal_id: &CanalId) -> Result<Vec<Video>, Error> {
        let conexao = self.obter_conexao()?;
        let mut stmt = conexao.prepare(SELECT_VIDEOS_CANAL)?;
        let mut rows = stmt.query(params![canal_id.as_str()])?;
        let mut videos = Vec::new();
        while let Some(row) = rows.next()? {
            videos.push(mapear_video(row)?);
        }
        Ok(videos)
    }

    fn salvar_comentario(&self, comentario: &Comentario) -> Result<(), Error> {
        let conexao = self.obter_conexao()?;
        conexao.execute(
            UPSERT_COMENTARIO,
            params![
                comentario.id.as_str(),
                comentario.video_id.as_str(),
                comentario.autor,
                comentario.texto,
                comentario.publicado_em,
                inteiro(comentario.curtidas, "curtidas")?,
            ],
        )?;
        Ok(())
    }

    fn buscar_comentarios(&self, video_id: &VideoId) -> Result<Vec<Comentario>, Error> {
        let conexao = self.obter_conexao()?;
        let mut stmt = conexao.prepare(SELECT_COMENTARIOS_VIDEO)?;
        let mut rows = stmt.query(params![video_id.as_str()])?;
        let mut comentarios = Vec::new();
        while let Some(row) = rows.next()? {
            comentarios.push(mapear_comentario(row)?);
        }
        Ok(comentarios)
    }

    fn salvar_snapshot(&self, snapshot: &Snapshot) -> Result<(), Error> {
        let conexao = self.obter_conexao()?;
        let alteradas = conexao.execute(
            INSERT_SNAPSHOT,
            params![
                snapshot.entidade_tipo.as_str(),
                snapshot.entidade_id,
                snapshot.metrica.as_str(),
                snapshot.valor,
                snapshot.data_coleta,
            ],
        )?;
        if alteradas == 0 {
            tracing::debug!(
                entidade_tipo = snapshot.entidade_tipo.as_str(),
                entidade_id = snapshot.entidade_id.as_str(),
                metrica = snapshot.metrica.as_str(),
                data_coleta = snapshot.data_coleta.as_str(),
                "snapshot já existia para o dia"
            );
        }
        Ok(())
    }

    fn buscar_snapshots(&self, canal_id: &CanalId) -> Result<Vec<Snapshot>, Error> {
        let conexao = self.obter_conexao()?;
        let mut stmt = conexao.prepare(SELECT_SNAPSHOTS_CANAL)?;
        let mut rows = stmt.query(params![EntidadeTipo::Canal.as_str(), canal_id.as_str()])?;
        let mut snapshots = Vec::new();
        while let Some(row) = rows.next()? {
            snapshots.push(mapear_snapshot(row)?);
        }
        Ok(snapshots)
    }

    fn buscar_serie_temporal(
        &self,
        entidade_tipo: EntidadeTipo,
        entidade_id: &str,
        metrica: Metrica,
    ) -> Result<Vec<Snapshot>, Error> {
        let conexao = self.obter_conexao()?;
        let mut stmt = conexao.prepare(SELECT_SERIE_TEMPORAL)?;
        let mut rows = stmt.query(params![
            entidade_tipo.as_str(),
            entidade_id,
            metrica.as_str()
        ])?;
        let mut snapshots = Vec::new();
        while let Some(row) = rows.next()? {
            snapshots.push(mapear_snapshot(row)?);
        }
        Ok(snapshots)
    }

    fn salvar_classificacao(&self, classificacao: &ClassificacaoNicho) -> Result<(), Error> {
        let conexao = self.obter_conexao()?;
        conexao.execute(
            UPSERT_CLASSIFICACAO,
            params![
                classificacao.video_id.as_str(),
                classificacao.nicho.as_str(),
                f64::from(classificacao.confianca),
                classificacao.metodo,
                classificacao.data_classificacao,
            ],
        )?;
        Ok(())
    }

    fn buscar_classificacao(
        &self,
        video_id: &VideoId,
    ) -> Result<Option<ClassificacaoNicho>, Error> {
        let conexao = self.obter_conexao()?;
        let mut stmt = conexao.prepare(SELECT_CLASSIFICACAO)?;
        let mut rows = stmt.query(params![video_id.as_str()])?;
        match rows.next()? {
            Some(row) => Ok(Some(mapear_classificacao(row)?)),
            None => Ok(None),
        }
    }

    fn buscar_videos_por_nicho(&self, nicho: Nicho) -> Result<Vec<Video>, Error> {
        let conexao = self.obter_conexao()?;
        let mut stmt = conexao.prepare(SELECT_VIDEOS_POR_NICHO)?;
        let mut rows = stmt.query(params![nicho.as_str()])?;
        let mut videos = Vec::new();
        while let Some(row) = rows.next()? {
            videos.push(mapear_video(row)?);
        }
        Ok(videos)
    }
}

impl RepositorioDeCanais for SqliteRepositorio {
    fn guardar_canal(&self, canal: &Canal) -> Result<(), ErroDominio> {
        self.salvar_canal(canal).map_err(ErroDominio::from)
    }

    fn obter_canal(&self, id: &CanalId) -> Result<Option<Canal>, ErroDominio> {
        self.buscar_canal(id).map_err(ErroDominio::from)
    }

    fn guardar_video(&self, video: &Video) -> Result<(), ErroDominio> {
        self.salvar_video(video).map_err(ErroDominio::from)
    }

    fn listar_videos(&self, canal_id: &CanalId) -> Result<Vec<Video>, ErroDominio> {
        self.buscar_videos(canal_id).map_err(ErroDominio::from)
    }

    fn guardar_comentario(&self, comentario: &Comentario) -> Result<(), ErroDominio> {
        self.salvar_comentario(comentario)
            .map_err(ErroDominio::from)
    }

    fn listar_comentarios(&self, video_id: &VideoId) -> Result<Vec<Comentario>, ErroDominio> {
        self.buscar_comentarios(video_id).map_err(ErroDominio::from)
    }

    fn guardar_snapshot(&self, snapshot: &Snapshot) -> Result<(), ErroDominio> {
        self.salvar_snapshot(snapshot).map_err(ErroDominio::from)
    }

    fn listar_snapshots(&self, canal_id: &CanalId) -> Result<Vec<Snapshot>, ErroDominio> {
        self.buscar_snapshots(canal_id).map_err(ErroDominio::from)
    }

    fn listar_serie_temporal(
        &self,
        entidade_tipo: EntidadeTipo,
        entidade_id: &str,
        metrica: Metrica,
    ) -> Result<Vec<Snapshot>, ErroDominio> {
        self.buscar_serie_temporal(entidade_tipo, entidade_id, metrica)
            .map_err(ErroDominio::from)
    }

    fn guardar_classificacao(&self, classificacao: &ClassificacaoNicho) -> Result<(), ErroDominio> {
        self.salvar_classificacao(classificacao)
            .map_err(ErroDominio::from)
    }

    fn obter_classificacao(
        &self,
        video_id: &VideoId,
    ) -> Result<Option<ClassificacaoNicho>, ErroDominio> {
        self.buscar_classificacao(video_id)
            .map_err(ErroDominio::from)
    }

    fn listar_videos_por_nicho(&self, nicho: Nicho) -> Result<Vec<Video>, ErroDominio> {
        self.buscar_videos_por_nicho(nicho)
            .map_err(ErroDominio::from)
    }
}

fn inteiro(valor: u64, campo: &str) -> Result<i64, Error> {
    i64::try_from(valor)
        .map_err(|_| Error::InvalidData(format!("{campo} excede o limite de i64: {valor}")))
}
