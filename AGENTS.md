# AGENTS.md — YouTube Tech Analyzer

## 1. Visão Geral do Projeto
Pipeline de ETL em Rust para análise de canais de tecnologia do YouTube.
Objetivo: determinar se canais de tecnologia estão a migrar para nichos como
programação e educação, através de snapshots históricos de metadados,
comentários e classificação de nicho.

## 2. Arquitetura (Clean/Hexagonal)
Workspace Cargo com separação rígida de responsabilidades. As dependências
apontam SEMPRE para dentro (em direção ao domínio).

- `crates/domain`: Entidades puras (`Canal`, `Video`, `Comentario`, `Snapshot`)
  e traits/portas (`Coletor`, `Repositorio`, `Classificador`). **Não depende de
  nenhum outro crate do workspace.** Sem IO, sem rede, sem base de dados.
- `crates/application`: Casos de uso (`ColetarCanal`, `GerarRelatorio`) que
  orquestram as portas. Depende apenas de `domain`.
- `crates/infra-*` (`infra-youtube`, `infra-storage`, `infra-ai`): Implementações
  concretas das portas. Dependem de `domain`.
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
  indicam que os comentários já foram recolhidos.
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
│ └── desktop/
└── tests/ # Testes de integração

text

## 6. O que NÃO fazer
- Não adicionar `dependencies` de infraestrutura ao `domain`.
- Não colocar lógica de negócio no `desktop` (`ui/` deve apenas renderizar e
  chamar `AppState`).
- Não usar `panic!` para erros recuperáveis.
- Não executar `cargo run` sem antes garantir que `cargo clippy` passa.

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