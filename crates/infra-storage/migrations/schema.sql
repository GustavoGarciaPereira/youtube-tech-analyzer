PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS canais (
    id TEXT PRIMARY KEY NOT NULL,
    nome TEXT NOT NULL,
    handle TEXT,
    url TEXT NOT NULL,
    descricao TEXT,
    inscritos INTEGER NOT NULL DEFAULT 0,
    total_videos INTEGER NOT NULL DEFAULT 0,
    topic_categories TEXT,
    criado_em TEXT,
    data_coleta TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS videos (
    id TEXT PRIMARY KEY NOT NULL,
    canal_id TEXT NOT NULL,
    titulo TEXT NOT NULL,
    descricao TEXT,
    url TEXT NOT NULL,
    tags TEXT,
    topic_categories TEXT,
    publicado_em TEXT NOT NULL,
    duracao_segundos INTEGER NOT NULL DEFAULT 0,
    visualizacoes INTEGER NOT NULL DEFAULT 0,
    curtidas INTEGER NOT NULL DEFAULT 0,
    total_comentarios INTEGER NOT NULL DEFAULT 0,
    data_coleta TEXT NOT NULL,
    FOREIGN KEY (canal_id) REFERENCES canais(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS comentarios (
    id TEXT PRIMARY KEY NOT NULL,
    video_id TEXT NOT NULL,
    autor TEXT NOT NULL,
    texto TEXT NOT NULL,
    curtidas INTEGER NOT NULL DEFAULT 0,
    publicado_em TEXT NOT NULL,
    eh_resposta INTEGER NOT NULL DEFAULT 0,
    nivel_resposta INTEGER NOT NULL DEFAULT 0,
    -- Sem FK auto-referenciada: a ordem cronológica dos comentários pode
    -- colocar uma resposta antes do pai durante a inserção.
    parent_id TEXT,
    data_coleta TEXT NOT NULL,
    FOREIGN KEY (video_id) REFERENCES videos(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entidade_tipo TEXT NOT NULL,
    entidade_id TEXT NOT NULL,
    metrica TEXT NOT NULL,
    valor INTEGER NOT NULL,
    data_coleta TEXT NOT NULL,
    UNIQUE (entidade_tipo, entidade_id, metrica, data_coleta)
);

CREATE TABLE IF NOT EXISTS classificacoes_nicho (
    video_id TEXT PRIMARY KEY NOT NULL,
    nicho TEXT NOT NULL,
    confianca REAL NOT NULL,
    metodo TEXT NOT NULL,
    data_classificacao TEXT NOT NULL,
    FOREIGN KEY (video_id) REFERENCES videos(id) ON DELETE CASCADE
);

-- Cache de classificações por conteúdo e versão do classificador.
-- Tabela separada da canónica para permitir múltiplas versões por vídeo
-- sem reconstruir a tabela `classificacoes_nicho`.
CREATE TABLE IF NOT EXISTS classificacoes_nicho_cache (
    video_id TEXT NOT NULL,
    hash_conteudo TEXT NOT NULL,
    versao_classificador TEXT NOT NULL,
    nicho TEXT NOT NULL,
    confianca REAL NOT NULL,
    metodo TEXT,
    data_classificacao TEXT NOT NULL,
    PRIMARY KEY (video_id, hash_conteudo, versao_classificador),
    FOREIGN KEY (video_id) REFERENCES videos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_videos_canal_id ON videos(canal_id);
CREATE INDEX IF NOT EXISTS idx_videos_data_coleta ON videos(data_coleta);
CREATE INDEX IF NOT EXISTS idx_comentarios_video_id ON comentarios(video_id);
CREATE INDEX IF NOT EXISTS idx_snapshots_entidade_id ON snapshots(entidade_id);
CREATE INDEX IF NOT EXISTS idx_snapshots_data_coleta ON snapshots(data_coleta);
CREATE INDEX IF NOT EXISTS idx_classificacoes_nicho_nicho ON classificacoes_nicho(nicho);
CREATE INDEX IF NOT EXISTS idx_classificacoes_cache_versao ON classificacoes_nicho_cache(versao_classificador);
