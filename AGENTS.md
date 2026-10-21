# AGENTS.md — YouTube Tech Analyzer

## 1. Visão Geral do Projeto
Pipeline de ETL em Rust para análise de canais de tecnologia do YouTube.
Objetivo: determinar se canais de tecnologia estão a migrar para nichos como
programação e educação, através de snapshots históricos de metadados,
comentários, classificação de nicho e análise de sentimento local (NLP).

## 2. Arquitetura (Clean/Hexagonal)
Workspace Cargo com separação rígida de responsabilidades. As dependências
apontam SEMPRE para dentro (em direção ao domínio).

- `crates/domain`: Entidades puras (`Canal`, `Video`, `Comentario`, `Snapshot`)
  e traits/portas (`Coletor`, `Repositorio`, `Classificador`). **Não depende de
  nenhum outro crate do workspace.** Sem IO, sem rede, sem base de dados.
- `crates/application`: Casos de uso (`ColetarCanal`, `GerarRelatorio`,
  `AnalisarSentimentos`) que orquestram as portas. Depende apenas de `domain`.
- `crates/infra-*` (`infra-youtube`, `infra-storage`, `infra-ai`, `infra-nlp`):
  Implementações concretas das portas. Dependem de `domain`.
- `crates/desktop`: lib de composição + GUI `egui`, com os binários
  `desktop` (GUI) e `yta` (CLI). É o *composition root*: injeta as
  implementações concretas nos casos de uso.

### Regra de Dependência (NÃO VIOLAR)
`domain` não pode ter dependências de `reqwest`, `rusqlite`, `egui` ou `tokio`.
`application` não pode depender de `infra-*` nem de `desktop`.

## 3. Comandos Essenciais
- **Build workspace**: `cargo build --workspace`
- **Testes**: `cargo test --workspace`
- **Verificação de lints (obrigatório antes de terminar)**: `cargo clippy --workspace --all-targets -- -D warnings`
- **Formatação**: `cargo fmt --all`
- **Executar a app desktop**: `cargo run -p desktop --bin desktop`
- **Executar a CLI**: `cargo run -p desktop --bin yta -- --help`
- **Preparar o modelo de sentimento**: `yta modelos baixar-sentimento` (~143 MB)
- **Analisar sentimentos de um canal**: `yta sentimentos <REF> [--concorrencia N]`

## 4. Convenções de Código
- **Edição**: 2024 (definida em `[workspace.package]`).
- **Erros**: `thiserror` para erros de domínio (`domain::errors`). `anyhow` para
  propagação com contexto na camada de `application` e `desktop`.
- **Async**: `tokio` com runtime multi-thread. O coletor deve usar `tokio::sync::mpsc`
  para *backpressure* entre produtor e consumidor.
- **Idempotência**: `canais`, `videos` e `comentarios` usam upsert por `id`
  (`ON CONFLICT(id) DO UPDATE`). Evitar `INSERT OR REPLACE`, que pode disparar
  `ON DELETE CASCADE` e apagar linhas filhas. `snapshots` usa
  `ON CONFLICT DO NOTHING` na chave composta
  (`entidade_tipo`, `entidade_id`, `metrica`, `data_coleta`); o histórico
  preserva uma linha por dia e por métrica. `data_coleta` usa `YYYY-MM-DD`
  em UTC. O marcador `Metrica::ComentariosColetados` (`entidade_tipo=video`)
  é a chave de `video_ja_coletado`; as métricas de visualizações/curtidas não
  indicam que os comentários já foram recolhidos. `comentarios_sentimento` usa
  PK `(comentario_id, versao_modelo)` e `ON CONFLICT DO NOTHING`: o mesmo
  comentário pode ter uma linha por versão do modelo, sem perder histórico.
- **Sem `unwrap()`/`expect()`** em código de produção. Use `?` e `Result`.

## 5. Estrutura de Ficheiros (resumo)
.
├── Cargo.toml # Virtual workspace
├── rust-toolchain.toml # Toolchain fixa
├── crates/
│ ├── domain/
│ ├── application/
│ ├── infra-youtube/
│ ├── infra-storage/
│ ├── infra-ai/
│ ├── infra-nlp/
│ └── desktop/
└── tests/ # Testes de integração

text

## 6. O que NÃO fazer
- Não adicionar `dependencies` de infraestrutura ao `domain`.
- Não colocar lógica de negócio no `desktop` (`ui/` deve apenas renderizar e
  chamar `AppState`).
- Não usar `panic!` para erros recuperáveis.
- Não executar `cargo run` sem antes garantir que `cargo clippy` passa.
- Não ligar o ONNX Runtime estaticamente (`ort` com `download-binaries`): os
  binários do `ort-sys` exigem glibc >= 2.32 (rc.10) ou 2.38 (rc.15). Usar
  `load-dynamic` e o runtime oficial descarregado em runtime.
- Não chamar `session.run()` diretamente numa tarefa async: usar
  `tokio::task::spawn_blocking`.
- Não reutilizar uma `versao_modelo` para um modelo diferente; é a chave de
  cache/re-análise em `comentarios_sentimento`.

## 7. Notas de Infraestrutura
- `opencode.json` usa o schema V2: `permissions` é uma **lista ordenada** de
  regras (`{ "action", "resource", "effect" }`), não um objeto chave-valor.
  Não regredir para a sintaxe V1.
- `comentarios.parent_id` não tem FK auto-referenciada: a ordem cronológica
  pode inserir uma resposta antes do comentário pai. Não "corrigir" isto sem
  mudar a estratégia de inserção.
- `ColetorDeCanal` e `ClassificadorDeNicho` devolvem futures boxed para serem
  object-safe e permitirem `Arc<dyn ...>` nos casos de uso.
- `CanalRef` é um value object de `domain`; o parsing de URLs/handles fica em
  `infra-youtube`, porque depende de `reqwest::Url`.
- `CacheDeClassificacoes` é uma porta separada em `domain`: `infra-ai` recebe
  `Arc<dyn CacheDeClassificacoes>` e não depende de `infra-storage`. O cache
  usa a tabela `classificacoes_nicho_cache`; a tabela canónica
  `classificacoes_nicho` mantém-se para relatórios e `nicho_do_canal`.
- Os binários `desktop` e `yta` carregam `.env` com `dotenvy` no arranque.
  Variáveis já exportadas na shell têm prioridade.
- `infra-nlp` carrega o ONNX Runtime dinamicamente: `ort =2.0.0-rc.10` com
  `default-features = false` e `features = ["load-dynamic"]`. O `ModelManager`
  descarrega `model.onnx`, `tokenizer.json` e `libonnxruntime.so` (ORT 1.22.0)
  para `XDG_DATA_HOME/yta/models/roberta-sentiment`. O `.so` oficial exige
  glibc 2.27; não trocar para os binários estáticos do `ort-sys`.
- A inferência é síncrona (C++): `AnalisadorDeSentimento::analisar` corre em
  `spawn_blocking`. A `Session` é protegida por `Mutex`, pelo que o `run()` é
  serializado; o semáforo do caso de uso paraleliza tokenização/IO, não ONNX.
- Modelo CardiffNLP `twitter-roberta-base-sentiment-latest` (Xenova, INT8),
  licença CC-BY-4.0: exige atribuição quando a funcionalidade for exposta.
- `versao_modelo` actual: `roberta-sentiment-int8-v1`. Mudar de modelo exige
  uma versão nova; as linhas antigas ficam para reprodutibilidade.

## 8. SQLite: DELETE/REPLACE e CASCADE
Nunca usar `INSERT OR REPLACE` em tabelas com `ON DELETE CASCADE`. O `REPLACE`
executa DELETE + INSERT internamente, disparando o CASCADE e apagando registos
dependentes. Usar sempre `INSERT ... ON CONFLICT(id) DO UPDATE`. A única
exceção deliberada são os snapshots, que usam `ON CONFLICT DO NOTHING` na
chave composta para preservar o histórico diário.

## 9. Comparações numéricas com zero
`f64::signum(0.0)` devolve `1.0` em Rust (não `0.0`). Para comparações tipo
"maior/menor/igual", usar sempre
`if x > 0.0 { 1 } else if x < 0.0 { -1 } else { 0 }`, nunca `x.signum()`.

## 10. ClassificadorDeNicho e comentários
A trait recebe `comentarios` mas v1 ignora (hash usa só título+descrição+tags).
Quem chamar deve passar `&[]` para não pagar IO. Se v2 usar comentários,
o `hash_conteudo` tem de os incluir — senão há cache hits incorrectos.

## 11. Classificações: canónica vs cache
- `classificacoes_nicho` (canónica): PK `(video_id)`. Lida pelo relatório e
  por `nicho_do_canal`.
- `classificacoes_nicho_cache` (LLM): PK composta
  `(video_id, hash_conteudo, versao_classificador)`. Só evita re-chamar o
  LLM; nunca é lida pelo relatório.
- Todo caso de uso que classifica escreve nas duas. Nunca só numa.

## 14. Marcadores de idempotência: comentários vs métricas
- `ComentariosColetados` (snapshots): one-time por vídeo, SEM data.
  `comentarios_do_video_ja_coletados` verifica existência, não data.
- `Visualizacoes`/`Curtidas`/`TotalComentarios` (snapshots): diários,
  COM data UTC. A UNIQUE composta deduplica intra-dia; a série
  acumula entre dias.
- `video_metricas_hoje_ja_coletadas` (com data) só evita re-coletar
  no mesmo dia — NUNCA bloqueia o dia seguinte.
- A flag `--recoletar-comentarios` força refresh manual.

## 15. GUI (`crates/desktop/src/{app,state,prefs}.rs`)
- `eframe` 0.29 + `egui_plot` 0.29. O tema é manual (`egui::Visuals` escuro
  com accent azul); `modern-egui` não é usado.
- `eframe::App::update` NUNCA faz `block_on`/`.await`. Os casos de uso correm
  em `tokio::spawn` e comunicam por `mpsc`; a GUI faz `try_recv` e agenda
  `request_repaint_after(100ms)` enquanto houver tarefas ativas.
- `wiring::Dependencias.coletor` é `Option`: a GUI arranca sem
  `YOUTUBE_API_KEY`; adicionar/coletar mostra um toast. A CLI continua a usar
  `montar_coletor` diretamente.
- `TendenciaMetrica` transporta `pontos` e `intercept_theil_sen` para o
  gráfico. O Markdown/CSV ignoram-nos; o JSON inclui-os.
- Preferências em `XDG_CONFIG_HOME/yta/prefs.toml` (ou
  `~/.config/yta/prefs.toml`), com debounce de 500 ms; falhas ao ler
  preferências caem para os valores por omissão.
- Ctrl+R no ecrã Relatório regenera; exportações copiam para o clipboard.
- O crate `desktop` tem múltiplos binários; usar sempre `--bin desktop` ou
  `--bin yta`.

## 16. Análise de sentimento (NLP)
- Porta: `domain::ports::AnalisadorDeSentimento` (`analisar` + `versao_modelo`),
  object-safe com `BoxFuture`.
- Value objects: `Sentimento` (`Positivo`/`Neutro`/`Negativo`) e
  `ScoreSentimento` (score em [-1, 1] = P(pos) − P(neg)).
- Persistência: `comentarios_sentimento`, PK `(comentario_id, versao_modelo)`,
  `ON CONFLICT DO NOTHING`; `comentarios_sem_sentimento(canal, versao)` devolve
  comentários sem linha para essa versão — é o que permite re-análise.
- Caso de uso: `AnalisarSentimentosUseCase` com `AnalisarConfig`
  (`concorrencia = num_cpus::get() - 1`, mínimo 2). Falhas individuais contam
  como `falhados` e não abortam o lote; o comentário fica pendente.
- `total_comentarios` no relatório = pendentes para a versão actual, não o
  total histórico do canal.
- Ordem de classes do modelo: `[negativo, neutro, positivo]` (0/1/2). Validar
  com inferência real ao trocar de modelo.
- Teste de inferência real está `#[ignore]`; o CI nunca descarrega o modelo.
  Correr com `cargo test -p infra-nlp --test inferencia -- --ignored`.