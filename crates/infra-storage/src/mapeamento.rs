use domain::entities::{
    canal::Canal,
    comentario::Comentario,
    nicho::{ClassificacaoNicho, Nicho},
    snapshot::{EntidadeTipo, Metrica, Snapshot},
    video::Video,
};
use domain::ids::{CanalId, ComentarioId, VideoId};
use rusqlite::Row;

use crate::error::Error;

pub(crate) fn mapear_canal(row: &Row<'_>) -> Result<Canal, Error> {
    Ok(Canal {
        id: CanalId::new(row.get::<_, String>(0)?),
        nome: row.get(1)?,
        handle: row.get(2)?,
        url: row.get(3)?,
        descricao: row.get(4)?,
        inscritos: ler_u64(row, 5)?,
        total_videos: ler_u64(row, 6)?,
        topic_categories: ler_lista_json(row, 7)?,
        criado_em: row.get(8)?,
    })
}

pub(crate) fn mapear_video(row: &Row<'_>) -> Result<Video, Error> {
    Ok(Video {
        id: VideoId::new(row.get::<_, String>(0)?),
        canal_id: CanalId::new(row.get::<_, String>(1)?),
        titulo: row.get(2)?,
        descricao: row.get(3)?,
        url: row.get(4)?,
        tags: ler_lista_json(row, 5)?,
        topic_categories: ler_lista_json(row, 6)?,
        publicado_em: row.get(7)?,
        duracao_segundos: ler_u32(row, 8)?,
        visualizacoes: ler_u64(row, 9)?,
        curtidas: ler_u64(row, 10)?,
        total_comentarios: ler_u64(row, 11)?,
    })
}

pub(crate) fn mapear_comentario(row: &Row<'_>) -> Result<Comentario, Error> {
    Ok(Comentario {
        id: ComentarioId::new(row.get::<_, String>(0)?),
        video_id: VideoId::new(row.get::<_, String>(1)?),
        autor: row.get(2)?,
        texto: row.get(3)?,
        publicado_em: row.get(4)?,
        curtidas: ler_u64(row, 5)?,
    })
}

pub(crate) fn mapear_snapshot(row: &Row<'_>) -> Result<Snapshot, Error> {
    Ok(Snapshot {
        entidade_tipo: ler_entidade_tipo(row, 0)?,
        entidade_id: row.get(1)?,
        metrica: ler_metrica(row, 2)?,
        valor: row.get(3)?,
        data_coleta: row.get(4)?,
    })
}

pub(crate) fn mapear_classificacao(row: &Row<'_>) -> Result<ClassificacaoNicho, Error> {
    Ok(ClassificacaoNicho {
        video_id: VideoId::new(row.get::<_, String>(0)?),
        nicho: ler_nicho(row, 1)?,
        confianca: ler_confianca(row, 2)?,
        metodo: row.get(3)?,
        data_classificacao: row.get(4)?,
    })
}

fn ler_lista_json(row: &Row<'_>, indice: usize) -> Result<Vec<String>, Error> {
    let texto: Option<String> = row.get(indice)?;
    match texto.as_deref() {
        Some(texto) if !texto.is_empty() => Ok(serde_json::from_str(texto)?),
        _ => Ok(Vec::new()),
    }
}

fn ler_u64(row: &Row<'_>, indice: usize) -> Result<u64, Error> {
    let valor: i64 = row.get(indice)?;
    u64::try_from(valor).map_err(|_| {
        Error::InvalidData(format!(
            "coluna {indice} contém um valor negativo ({valor})"
        ))
    })
}

fn ler_u32(row: &Row<'_>, indice: usize) -> Result<u32, Error> {
    let valor = ler_u64(row, indice)?;
    u32::try_from(valor).map_err(|_| {
        Error::InvalidData(format!("coluna {indice} excede o limite de u32 ({valor})"))
    })
}

fn ler_confianca(row: &Row<'_>, indice: usize) -> Result<f32, Error> {
    let valor: f64 = row.get(indice)?;
    if valor.is_finite() && (0.0..=1.0).contains(&valor) {
        Ok(valor as f32)
    } else {
        Err(Error::InvalidData(format!(
            "coluna {indice} contém uma confiança fora do intervalo [0, 1]: {valor}"
        )))
    }
}

fn ler_nicho(row: &Row<'_>, indice: usize) -> Result<Nicho, Error> {
    let valor: String = row.get(indice)?;
    Ok(Nicho::try_from(valor.as_str())?)
}

fn ler_entidade_tipo(row: &Row<'_>, indice: usize) -> Result<EntidadeTipo, Error> {
    let valor: String = row.get(indice)?;
    Ok(EntidadeTipo::try_from(valor.as_str())?)
}

fn ler_metrica(row: &Row<'_>, indice: usize) -> Result<Metrica, Error> {
    let valor: String = row.get(indice)?;
    Ok(Metrica::try_from(valor.as_str())?)
}
