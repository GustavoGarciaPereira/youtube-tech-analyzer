use std::time::Duration;

use domain::{entities::canal::Canal, ids::VideoId};

/// Falha isolada de um vídeo durante a coleta.
#[derive(Debug, Clone)]
pub struct FalhaVideo {
    /// Vídeo que falhou.
    pub video_id: VideoId,
    /// Motivo legível da falha.
    pub motivo: String,
}

/// Resultado final de uma execução de `ColetarCanalUseCase`.
#[derive(Debug, Clone)]
pub struct RelatorioColeta {
    /// Canal resolvido.
    pub canal: Canal,
    /// Vídeos efetivamente processados com sucesso.
    pub videos_novos: usize,
    /// Vídeos ignorados por já terem sido coletados hoje.
    pub videos_ja_coletados: usize,
    /// Vídeos que falharam durante o processamento.
    pub videos_falhados: usize,
    /// Total de comentários persistidos com sucesso.
    pub comentarios_total: usize,
    /// Duração total da execução.
    pub duracao: Duration,
    /// Detalhe das falhas por vídeo.
    pub falhas: Vec<FalhaVideo>,
}

/// Eventos de progresso opcionais enviados para a UI.
#[derive(Debug, Clone)]
pub enum EventoProgresso {
    /// O canal foi resolvido.
    CanalResolvido {
        /// Nome público do canal.
        nome: String,
        /// Total de vídeos reportado pelo canal.
        total_videos: usize,
    },
    /// A lista de vídeos a processar foi determinada.
    VideosListados {
        /// Número de vídeos após truncamento e antes do filtro de idempotência.
        total: usize,
    },
    /// O processamento de um vídeo começou.
    VideoIniciado {
        /// Índice zero-based na lista de vídeos a processar.
        indice: usize,
        /// Total de vídeos a processar.
        total: usize,
        /// Vídeo em processamento.
        video_id: VideoId,
        /// Título do vídeo.
        titulo: String,
    },
    /// O processamento de um vídeo terminou com sucesso.
    VideoConcluido {
        /// Vídeo concluído.
        video_id: VideoId,
        /// Número de comentários persistidos.
        comentarios: usize,
    },
    /// O processamento de um vídeo falhou.
    VideoFalhou {
        /// Vídeo que falhou.
        video_id: VideoId,
        /// Motivo legível da falha.
        motivo: String,
    },
    /// A execução terminou com sucesso.
    Finalizado {
        /// Relatório final da coleta.
        relatorio: RelatorioColeta,
    },
}
