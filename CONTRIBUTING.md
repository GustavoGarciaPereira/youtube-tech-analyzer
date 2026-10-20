# Contribuir

Obrigado pelo interesse no YouTube Tech Analyzer.

## Antes de começares

- Para alterações grandes ou mudanças de arquitetura, abre primeiro uma issue
  para discutirmos a abordagem.
- Para bugs pequenos, correções de documentação ou testes, podes abrir um PR
  diretamente.
- Não incluas `.env`, bases de dados, chaves de API, dumps ou dados pessoais
  em commits, issues ou PRs.

## Preparar o ambiente

```bash
cp .env.example .env
# Edita .env e define YOUTUBE_API_KEY, se fores testar operações de rede.
cargo build --workspace
```

A GUI precisa de um ambiente Linux com X11 ou Wayland e OpenGL. A CLI não
precisa de ambiente gráfico.

## Verificações obrigatórias

Antes de abrires um PR, corre:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

O CI repete estas verificações num ambiente limpo.

## Regras do projeto

- Respeita a regra de dependências: `domain` não depende de infraestrutura;
  `application` não depende de `infra-*` nem de `desktop`.
- Não coloques lógica de negócio na GUI ou na CLI. Elas chamam os casos de uso.
- Não uses `unwrap()`/`expect()` em código de produção; usa `?`, `Result` e
  `anyhow`/`thiserror` conforme a camada.
- Em tabelas com `ON DELETE CASCADE`, usa `INSERT ... ON CONFLICT ... DO UPDATE`
  em vez de `INSERT OR REPLACE`.
- Comentários são recolhidos uma vez por vídeo; métricas são diárias. Mantém
  esta separação e adiciona testes quando alterares a idempotência.
- Novas funcionalidades devem incluir testes unitários ou de integração.
- Mantém a documentação (`README.md`, comentários de código e migrações)
  atualizada com o comportamento.

## Estilo de commits e PRs

- Commits pequenos e atómicos.
- Mensagens no imperativo, por exemplo:
  - `feat(cli): adiciona comando de categorização`
  - `fix(storage): preserva comentários no upsert`
  - `docs: atualiza instruções de desenvolvimento`
- No PR, descreve o problema, a solução e os comandos de verificação que
  executaste.

## Licença

Ao contribuíres, aceitas que a tua contribuição seja licenciada ao abrigo da
licença MIT deste repositório.
