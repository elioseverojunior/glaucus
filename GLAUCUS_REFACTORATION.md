<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Refatoração do Glaucus — Análise e Escopo Completo

Documento de análise arquitetural do workspace `glaucus`, respondendo a três
perguntas de layout de crates, com as evidências que sustentam cada veredito e o
escopo completo da refatoração proposta.

Status: **análise concluída, execução pendente de aprovação.** Nenhuma das
mudanças descritas na seção 6 foi aplicada.

## Sumário executivo

| # | Pergunta | Veredito | Confiança |
|---|----------|----------|-----------|
| 1 | Manter `crates/glaucus-core`? | **Sim — é estruturalmente obrigatório**, não apenas idiomático | Certeza (verificado) |
| 2 | `crates/glaucus` pode ser biblioteca **e** CLI? | **Sim — verificado empiricamente** | Certeza (verificado) |
| 3 | Mover `glaucus-fmt` e `glaucus-validate` para `crates/glaucus`? | **Viável e defensável — mas só faz sentido movendo os quatro binários** | Recomendação (decisão de projeto) |

## 1. Contexto

Esta análise nasceu de três exigências:

1. Verificar se `crates/glaucus-fmt/` e `crates/glaucus-validate/` podem ser
   movidos para `crates/glaucus`.
2. Garantir que `crates/glaucus` consegue funcionar como biblioteca e também
   como CLI.
3. Reavaliar, à luz das boas práticas de Rust, se manter `crates/glaucus-core`
   é uma boa decisão — e, se não for, apresentar o raciocínio.

## 2. Estrutura atual do workspace

O workspace declara `members = ["crates/*"]` com `resolver = "3"`.

| Crate | Linhas | Arquivos | Binário | Dependências externas |
|-------|-------:|---------:|---------|-----------------------|
| `glaucus-core` | 6.920 | 10 | — | **nenhuma** (por política) |
| `glaucus-serde` | 4.936 | 8 | — | `serde` |
| `glaucus-ast` | 3.726 | 5 | — | nenhuma |
| `glaucus-cst` | 1.715 | 6 | — | nenhuma |
| `glaucus-schema` | 1.112 | 4 | — | nenhuma |
| `glaucus` | 937 | 1 | — | `serde` |
| `glaucus-mcp` | 589 | 2 | sim | `serde_json` |
| `glaucus-bench` | 541 | 2 | — | — |
| `glaucus-yaml-test-suite` | 490 | 1 | — | — |
| `glaucus-lsp` | 408 | 2 | sim | `serde_json` |
| `glaucus-validate` | 239 | 2 | sim | **nenhuma** |
| `glaucus-fmt` | 197 | 2 | sim | **nenhuma** |
| `glaucus-wasm` | 167 | 1 | — | `wasm-bindgen` |
| `glaucus-fuzz` | 0 | 0 | — | — |

O crate `glaucus` é uma fachada: reexporta os módulos dos crates satélites atrás
de features (`ast`, `cst`, `serde`, `schema`) e acrescenta funções de
conveniência (`from_str`, `to_string`, `from_str_node`, etc.).

## 3. Pergunta 1 — Manter `glaucus-core`?

### 3.1 Veredito

**Manter.** Não se trata apenas de boa prática: a alternativa não compila.

### 3.2 Evidência — grafo real de dependências

Saída de `cargo tree -i glaucus-core --workspace -e normal`, resumida:

```text
glaucus-core
├── glaucus            (direto)
├── glaucus-ast        → glaucus, glaucus-schema, glaucus-serde, glaucus-bench, glaucus-fuzz
├── glaucus-cst        → glaucus, glaucus-schema
├── glaucus-schema     → glaucus
├── glaucus-serde      → glaucus, glaucus-fuzz
├── glaucus-bench
└── glaucus-yaml-test-suite
```

São **oito crates** dependendo de `glaucus-core`.

### 3.3 Por que a fusão é impossível — ciclo de dependências

`glaucus` depende de `glaucus-ast`, e `glaucus-ast` depende de `glaucus-core`.
Se o conteúdo de `glaucus-core` fosse absorvido por `glaucus`, teríamos:

```text
glaucus-ast → glaucus      (para alcançar o antigo core)
glaucus     → glaucus-ast  (reexport da fachada, já existente)
```

Isso é um ciclo direto. O Cargo só tolera ciclos através de
`dev-dependencies`; em `dependencies` normais ele recusa a resolução. **Não
existe variante de "fundir o core na fachada" que compile.** O mesmo raciocínio
vale para `glaucus-cst`, `glaucus-schema` e `glaucus-serde`.

### 3.4 Critérios de um crate `-core` justificado

Mesmo ignorando o ciclo, `glaucus-core` satisfaz todos os critérios que
justificam a separação:

| Critério | Situação |
|----------|----------|
| Compartilhado por vários irmãos | 8 dependentes |
| Sem dependências externas | Sim — `[dependencies]` vazio, política registrada em `deny.toml` |
| Tamanho relevante | 6.920 linhas em 10 arquivos |
| Isolamento de MSRV/semver | A base estabiliza independentemente das camadas acima |
| Paralelismo de compilação | Unidade de codegen separada, reaproveitada por 8 crates |

### 3.5 Precedentes no ecossistema

O padrão `-core` é consolidado em Rust: `tracing` / `tracing-core`,
`sqlx` / `sqlx-core`, `regex` / `regex-syntax` / `regex-automata`. O exemplo
mais direto aparece na própria árvore de dependências deste projeto:
`serde v1.0.229` depende de `serde_core v1.0.229`.

## 4. Pergunta 2 — `crates/glaucus` como biblioteca e CLI

### 4.1 Veredito

**Sim, funciona.** Verificado empiricamente com um protótipo real, não por
suposição. O protótipo foi revertido após a medição.

### 4.2 Como foi verificado

Foi adicionado um alvo `[[bin]]` de verdade em `crates/glaucus`, com
`required-features`, e o resultado foi medido:

```text
lib      glaucus         required-features=[]
bin      glaucus-fmt     required-features=['ast', 'cst']
test     integration     required-features=[]
```

| Comando | Resultado observado |
|---------|---------------------|
| `cargo build -p glaucus --bins` | Compilou; binário executou e imprimiu `a: 1` |
| `cargo build -p glaucus --no-default-features` | **Exit 0** — binário ignorado, biblioteca compilada |
| `cargo tree -p glaucus --no-default-features` | Consumidor de biblioteca resolve apenas `glaucus-core` + `serde` |

### 4.3 Mecanismo — `required-features`

`required-features` em `[[bin]]` funciona como **omissão silenciosa**, não como
erro: quando a feature não está ativa, o Cargo simplesmente não constrói aquele
binário e continua normalmente. É exatamente esse comportamento que permite a um
único pacote atender bem tanto `cargo add glaucus` quanto `cargo install glaucus`.

## 5. Pergunta 3 — Mover `glaucus-fmt` e `glaucus-validate`

### 5.1 Veredito

Tecnicamente viável, com um argumento genuinamente bom a favor — mas mover
**apenas esses dois** é a pior das opções disponíveis.

### 5.2 O que torna a fusão barata neste caso

O Cargo **não possui o conceito de dependência exclusiva de binário**: toda
dependência de um pacote é resolvida por quem depende da sua biblioteca. Essa
limitação é a razão pela qual o ecossistema separa CLIs em crates próprios —
para que o consumidor da biblioteca não arraste `clap` junto.

Aqui esse motivo não existe:

- `glaucus-fmt` depende de: `glaucus`. Nada mais.
- `glaucus-validate` depende de: `glaucus`. Nada mais.

Ambos fazem parsing de argumentos manualmente, usando apenas `std::process::ExitCode`.
Movê-los para dentro de `glaucus` **não acrescenta nenhuma dependência** à
biblioteca. Além disso, nenhum outro crate do workspace depende deles.

### 5.3 O argumento mais forte a favor

Esses crates contêm **API de biblioteca real que nenhum usuário de biblioteca vai
encontrar**:

- `glaucus-fmt`: `format_str` (42 linhas de biblioteca, 155 de CLI)
- `glaucus-validate`: `validate_str`, `fix_str`, `Diagnostic` (165 linhas de
  biblioteca, 74 de CLI)

São 207 linhas de API testada e sem dependências, escondidas atrás de nomes de
crate que se leem como executáveis. Como `glaucus::fmt` e `glaucus::validate`,
ficariam ao lado de `glaucus::schema` e `glaucus::cst` — onde alguém procuraria
por elas.

### 5.4 Custos e armadilhas

1. **`schema` não é feature padrão.** Hoje `default = ["ast", "serde", "cst"]`.
   Um `cargo install glaucus` puro construiria `glaucus-fmt` e **silenciosamente
   ignoraria** `glaucus-validate`, que exige `schema`. Exige uma feature `cli`
   dedicada.
2. **O glob de cobertura quebra — e reproduziria exatamente a falha de CI recém
   corrigida.** `tarpaulin.toml` exclui `crates/*/src/main.rs`. Os binários
   passariam a ser `crates/glaucus/src/bin/*.rs`, deixando de casar com o glob, e
   cerca de 229 linhas de plumbing de CLI passariam a contar contra o portão de
   100%.
3. **Assimetria com `glaucus-lsp` e `glaucus-mcp`.** Têm exatamente a mesma
   forma (biblioteca + `[[bin]]` + dependência de `glaucus`), diferindo apenas
   por usarem `serde_json`. Mover dois de quatro deixa o layout menos coerente
   do que está hoje.
4. **Descoberta no crates.io.** `cargo install glaucus-fmt` é mais óbvio para
   quem procura um formatador do que `cargo install glaucus --features cli`.

O custo de registro é **zero hoje e não-zero para sempre depois**: `glaucus-fmt`,
`glaucus-validate` e `glaucus-core` ainda não foram publicados (retornam 404 no
crates.io).

### 5.5 Recomendação

Mover **os quatro** CLIs (`fmt`, `validate`, `lsp`, `mcp`) para `crates/glaucus`
atrás de uma feature `cli`, com `serde_json` como dependência opcional — **ou**
manter o layout como está. O meio-termo de mover apenas `fmt` e `validate` é o
que menos entrega e o que mais custa em coerência.

Como nada foi publicado ainda, este é o momento mais barato que existirá para
decidir.

## 6. Escopo completo da refatoração

Esta seção descreve a execução da recomendação 5.5 (mover os quatro binários).
**Nada aqui foi aplicado.**

### 6.1 Objetivo

Consolidar biblioteca e ferramentas de linha de comando em um único pacote
publicável `glaucus`, preservando: o portão de cobertura de 100%, o grafo de
dependências sem ciclos, e o custo zero de dependências para consumidores que
usam apenas a biblioteca.

### 6.2 Fase 0 — Pré-requisitos

- Working tree limpo e CI verde (o portão de cobertura já está em 100%).
- Confirmar que `glaucus-fmt`, `glaucus-validate`, `glaucus-lsp` e `glaucus-mcp`
  não são referenciados fora do workspace:

```bash
grep -rn "glaucus-fmt\|glaucus-validate\|glaucus-lsp\|glaucus-mcp" \
  --include=Cargo.toml --include=*.yml --include=*.md --include=*.rs .
```

### 6.3 Fase 1 — Mover o código de biblioteca

| Origem | Destino | Gate de feature |
|--------|---------|-----------------|
| `crates/glaucus-fmt/src/lib.rs` | `crates/glaucus/src/fmt.rs` | `#[cfg(all(feature = "ast", feature = "cst"))]` |
| `crates/glaucus-validate/src/lib.rs` | `crates/glaucus/src/validate.rs` | `#[cfg(feature = "schema")]` |
| `crates/glaucus-lsp/src/lib.rs` | `crates/glaucus/src/lsp.rs` | `#[cfg(feature = "cli")]` |
| `crates/glaucus-mcp/src/lib.rs` | `crates/glaucus/src/mcp.rs` | `#[cfg(feature = "cli")]` |

Ajuste obrigatório em cada arquivo movido: as chamadas passam de `glaucus::…`
para `crate::…`. Isso vale também para os módulos `#[cfg(test)]` internos, que
hoje usam o caminho externo. Exemplos:

- `glaucus::from_str_node(src)` → `crate::from_str_node(src)`
- `glaucus::cst::Document::parse(src)` → `crate::cst::Document::parse(src)`
- `glaucus::schema::validate(…)` → `crate::schema::validate(…)`

Declaração dos módulos em `crates/glaucus/src/lib.rs`:

```rust
#[cfg(all(feature = "ast", feature = "cst"))]
pub mod fmt;

#[cfg(feature = "schema")]
pub mod validate;
```

### 6.4 Fase 2 — Mover os binários

| Origem | Destino |
|--------|---------|
| `crates/glaucus-fmt/src/main.rs` | `crates/glaucus/src/bin/glaucus-fmt.rs` |
| `crates/glaucus-validate/src/main.rs` | `crates/glaucus/src/bin/glaucus-validate.rs` |
| `crates/glaucus-lsp/src/main.rs` | `crates/glaucus/src/bin/glaucus-lsp.rs` |
| `crates/glaucus-mcp/src/main.rs` | `crates/glaucus/src/bin/glaucus-mcp.rs` |

Cada `main.rs` passa a chamar `glaucus::fmt::format_str`, `glaucus::validate::…`
etc. em vez do crate irmão.

### 6.5 Fase 3 — Features e manifesto

Em `crates/glaucus/Cargo.toml`:

```toml
[features]
ast = ["dep:glaucus-ast"]
cst = ["dep:glaucus-cst"]
default = ["ast", "serde", "cst"]
serde = ["dep:glaucus-serde", "ast"]
schema = ["dep:glaucus-schema", "ast", "cst"]
# Reúne tudo que os binários precisam. `serde_json` entra aqui como dependência
# opcional para que consumidores apenas-biblioteca não a resolvam.
cli = ["ast", "cst", "schema", "dep:serde_json"]

[dependencies]
serde_json = { workspace = true, optional = true }

[[bin]]
name = "glaucus-fmt"
path = "src/bin/glaucus-fmt.rs"
required-features = ["cli"]

[[bin]]
name = "glaucus-validate"
path = "src/bin/glaucus-validate.rs"
required-features = ["cli"]

[[bin]]
name = "glaucus-lsp"
path = "src/bin/glaucus-lsp.rs"
required-features = ["cli"]

[[bin]]
name = "glaucus-mcp"
path = "src/bin/glaucus-mcp.rs"
required-features = ["cli"]
```

Uma única feature `cli` para os quatro binários (em vez de gates individuais)
evita o modo de falha descrito em 5.4.1, no qual `cargo install glaucus`
constrói apenas parte das ferramentas sem avisar.

### 6.6 Fase 4 — Ajustes de infraestrutura

Esta é a fase que reintroduz falhas de CI se for esquecida.

- **`tarpaulin.toml`** — estender o glob de exclusão, ou os binários passam a
  contar contra o portão de 100%:

```toml
exclude-files = [
  "crates/*/src/main.rs",
  "crates/*/src/bin/*.rs",
  "crates/glaucus-wasm/src/lib.rs",
]
```

- **`.github/workflows/publish.yml`** — remover os quatro crates da ordem de
  publicação; `glaucus` passa a ser o único pacote com binários.
- **`Cargo.toml` raiz** — remover as entradas correspondentes de
  `[workspace.dependencies]`; `serde_json` deixa de ser usado pelos crates
  removidos e passa a ser usado por `glaucus`.
- **`deny.toml`** — revisar se algum allow/skip cita os crates removidos.
- **`README.md` e `docs/`** — atualizar instruções de instalação de
  `cargo install glaucus-fmt` para `cargo install glaucus --features cli`.
- **`REUSE.toml`** — cobertura por `path = ["**"]`; nenhuma ação necessária.

### 6.7 Fase 5 — Remoção dos crates antigos

Remover os diretórios `crates/glaucus-fmt/`, `crates/glaucus-validate/`,
`crates/glaucus-lsp/` e `crates/glaucus-mcp/`. Como `members = ["crates/*"]` é
um glob, não há lista de membros a editar.

Nenhum stub de depreciação é necessário no crates.io: os quatro nomes nunca
foram publicados.

### 6.8 Verificação

Ordem de execução, do mais barato ao mais caro:

```bash
cargo build --workspace --all-features
mise run cargo:fmt:check
mise run cargo:clippy                 # -D warnings
cargo test --workspace --all-features
mise run coverage                     # portão de 100%, o mais sensível
cargo install --path crates/glaucus --features cli --dry-run
```

Critérios de aceite:

- `mise run coverage` termina com `100.00% coverage` e código de saída 0.
- Os quatro binários aparecem em `cargo metadata` com
  `required-features=['cli']`.
- `cargo tree -p glaucus --no-default-features` continua resolvendo apenas
  `glaucus-core` + `serde` — ou seja, `serde_json` **não** vaza para o
  consumidor de biblioteca.
- `cargo build -p glaucus --no-default-features` termina com exit 0.

### 6.9 Riscos e rollback

| Risco | Probabilidade | Impacto | Mitigação |
|-------|---------------|---------|-----------|
| Glob de cobertura esquecido | Alta | CI vermelho, idêntico à falha recém-corrigida | Fase 4, primeiro item; validado por `mise run coverage` |
| `serde_json` vazando para consumidores de biblioteca | Média | Peso indevido na biblioteca | `optional = true` + verificação com `cargo tree --no-default-features` |
| `cargo install glaucus` construindo binários parciais | Média | Ferramentas ausentes sem erro | Feature `cli` única para os quatro binários |
| Caminhos `glaucus::` remanescentes nos módulos movidos | Alta | Erro de compilação (falha ruidosa, não silenciosa) | Detectado imediatamente pelo build |

Rollback: a refatoração é inteiramente contida em movimentação de arquivos e
manifestos, sem alteração de lógica. Um `git revert` do commit restaura o estado
anterior integralmente.

### 6.10 Dimensionamento

- Arquivos movidos: 8 (4 de biblioteca, 4 de binário)
- Manifestos alterados: 3 (`crates/glaucus/Cargo.toml`, `Cargo.toml` raiz,
  `tarpaulin.toml`)
- Diretórios removidos: 4
- Linhas de lógica alteradas: nenhuma — apenas caminhos de import e gates de
  feature

## 7. Alternativa — não refatorar

O layout atual **não está errado**. Ele é internamente consistente: todo binário
é um crate, toda biblioteca é um crate, e a fachada reexporta o conjunto. Se o
objetivo for minimizar risco antes da primeira publicação, manter tudo como está
é uma escolha legítima.

O que **não** se recomenda é o meio-termo: mover apenas `glaucus-fmt` e
`glaucus-validate`, deixando `glaucus-lsp` e `glaucus-mcp` como crates
separados. Isso troca uma regra simples ("todo binário é um crate") por uma
exceção que precisará ser explicada em cada revisão futura.

## 8. Achados incidentais

- O nome `glaucus` **já está registrado no crates.io e pertence ao projeto** (o
  campo `repository` aponta para `elioseverojunior/glaucus`), mas ainda carrega
  a descrição de outro projeto: *"Generate .gitignore files from a declarative
  TOML specification."* Será substituída na primeira publicação real.
- `glaucus-core`, `glaucus-fmt` e `glaucus-validate` retornam 404 no crates.io —
  nunca foram publicados.
- `glaucus-fuzz` tem 0 linhas em 0 arquivos, mas consta como membro do workspace
  e depende de `glaucus-core`, `glaucus-ast` e `glaucus-serde`. Vale decidir se
  é um esqueleto intencional ou resíduo.

## 9. Estado e decisão pendente

| Item | Estado |
|------|--------|
| Pergunta 1 — `glaucus-core` | Respondida: manter (obrigatório) |
| Pergunta 2 — lib + CLI | Respondida: viável (verificado, protótipo revertido) |
| Pergunta 3 — mover binários | Respondida: recomendação registrada |
| Execução da seção 6 | **Não iniciada — aguardando decisão** |

A decisão pendente é binária: executar a seção 6 movendo os quatro binários, ou
manter o layout atual conforme a seção 7.
