<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# glaucus-cli — Design Specification

- Date: 2026-08-01
- Status: approved, pending implementation plan
- Supersedes: section 5.5 of `GLAUCUS_REFACTORATION.md`

## 1. Goal

Ship one `glaucus` binary that is a complete YAML management tool, replacing the
four single-purpose binaries in the workspace. The binary is installable two
ways — through the `glaucus` package itself and through a thin `glaucus-cli`
wrapper — while `cargo add glaucus` continues to resolve zero additional
dependencies.

## 2. Decisions

Each row was an explicit choice during design, recorded so the reasoning is not
lost.

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Use `clap` (derive) + `tracing` | Subcommands, aliases, `--help` and completions come free; `tracing` gives real level filtering. Measured cost is 25 net-new crates for `clap` + `tracing` + `tracing-subscriber` + `anstream`; `anyhow` and `unicode-width` add 2 more, both zero-dependency, for roughly 27 in total. All confined to the binary. |
| D2 | Tier B command surface | Retires four crates instead of adding a fifth thing to maintain. |
| D3 | Diagnostics-first logging for v1 | Caret diagnostics are most of the value over `grep` for a YAML tool. Full observability deferred to F1. |
| D4 | Topology A: CLI lives in `glaucus`, `glaucus-cli` is a wrapper | Only shape satisfying both install paths without a dependency cycle. |
| D5 | CLI logic is library code; only `process.rs` is excluded from coverage | Makes the hard 100% gate achievable rather than aspirational. |
| D6 | `--no-source` to suppress source echo, default on under `CI=true` | The caret renderer echoes lines that may hold secrets. |

### 2.1 The cycle constraint

`glaucus` cannot depend on `glaucus-cli`, not even optionally. Verified:

```text
error: cyclic package dependency: package `mycli` depends on itself
```

A feature-gated `optional = true` dependency still closes a package cycle.
Therefore the CLI implementation must live inside the `glaucus` package, and
`glaucus-cli` may only depend one way, on `glaucus`.

### 2.2 Install behaviour

`cargo install` on a package whose only binary is gated does not fail silently.
Verified:

```text
warning: none of the package's binaries are available for install using the
selected features
  bin "glaucus" requires the features: `cli`
Consider enabling some of the needed features by passing, e.g., `--features="cli"`
```

Resulting install matrix:

| Command | Result |
|---------|--------|
| `cargo add glaucus` | Library only. Zero new dependencies. |
| `cargo install glaucus --features cli` | Installs the `glaucus` binary. |
| `cargo install glaucus` | Actionable warning naming `--features cli`. |
| `cargo install glaucus-cli` | Installs the `glaucus` binary, no flags needed. |

Installing both packages is redundant; both produce a binary named `glaucus`.
This is documented in the README rather than prevented.

## 3. Architecture

```text
crates/glaucus/
├── Cargo.toml            cli feature + [[bin]] glaucus (required-features)
└── src/
    ├── lib.rs            existing facade + `pub mod cli` (cfg feature = "cli")
    ├── main.rs           thin shim -> glaucus::cli::process::main()
    └── cli/
        ├── mod.rs        Cli/Command derive structs, run_with(Env) dispatch
        ├── process.rs    the ONLY untestable file: real argv/stdio/TTY
        ├── env.rs        Env: injected args + streams + colour choice
        ├── diag.rs       caret diagnostic renderer
        ├── logging.rs    tracing subscriber init, NO_COLOR/TTY policy
        ├── io.rs         file reads, atomic writes, stdin handling
        ├── exit.rs       exit-code taxonomy
        └── cmd/
            ├── fmt.rs  validate.rs  parse.rs  convert.rs
            └── schema.rs  lsp.rs  mcp.rs  completions.rs

crates/glaucus-cli/
├── Cargo.toml            depends on glaucus with features = ["cli"]
└── src/main.rs           thin shim -> glaucus::cli::process::main()
```

### 3.1 Dependency injection boundary

```rust
pub struct Env<'a> {
    pub args:   Vec<OsString>,
    pub stdin:  &'a mut dyn BufRead,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    pub color:  ColorChoice,
}

pub fn run_with(env: Env<'_>) -> ExitCode;
```

`run_with` is the real entry point and is fully unit-testable: a test supplies a
`Vec<OsString>` and three in-memory buffers, then asserts on the exit code and
on the bytes written to each stream. No subprocess is ever spawned.

`process.rs` contains only what cannot be faked — `env::args_os()`, locking real
stdio, TTY detection, and the panic hook.

### 3.2 Feature wiring

```toml
[features]
default = ["ast", "serde", "cst"]
cli = ["ast", "cst", "schema", "serde",
       "dep:clap", "dep:tracing", "dep:tracing-subscriber",
       "dep:anstream", "dep:anyhow", "dep:unicode-width", "dep:serde_json"]

[[bin]]
name = "glaucus"
path = "src/main.rs"
required-features = ["cli"]
```

Every CLI dependency is declared `optional = true`, so the library's resolved
dependency graph is unchanged for consumers who do not enable `cli`.

### 3.3 Workspace effect

Retired: `glaucus-fmt`, `glaucus-validate`, `glaucus-lsp`, `glaucus-mcp`.
Added: `glaucus-cli`. Members go from 14 to 11.

Their library APIs move into `glaucus` and become discoverable:

| From | To |
|------|-----|
| `glaucus_fmt::format_str` | `glaucus::fmt::format_str` |
| `glaucus_validate::{validate_str, fix_str, Diagnostic}` | `glaucus::validate::*` |
| `glaucus_lsp::*` | `glaucus::lsp::*` |
| `glaucus_mcp::{Server, Outgoing, frame}` | `glaucus::mcp::*` |

Moved code changes `glaucus::` paths to `crate::`, including inside `#[cfg(test)]`
modules.

## 4. Command surface

| Command | Visible aliases | Hidden aliases | Purpose |
|---------|-----------------|----------------|---------|
| `fmt` | `format` | `f` | Trailing-whitespace and final-newline, comment-preserving |
| `validate` | `check` | `val`, `v` | JSON-Schema validation, `--fix` autofix |
| `parse` | `dump` | `p` | Emit events, AST, CST or JSON |
| `convert` | `to` | `conv` | YAML to JSON and back |
| `schema` | — | `sch` | `schema check <file>` validates a schema document |
| `lsp` | — | — | Language server over stdio |
| `mcp` | — | — | MCP server over stdio |
| `completions` | `comp` | — | Emit shell completions |

No alias is reused across commands, and none collides with a global flag.

### 4.1 Global flags

```text
-v, -vv          increase log verbosity (warn -> info -> debug)
-q, --quiet      errors only
    --color      auto | always | never   (auto respects NO_COLOR and TTY)
    --format     human | json            (diagnostic rendering)
    --no-source  suppress the echoed source line (default on when CI=true)
```

### 4.2 Resource limits

`ResourceLimits` already defends against YAML bombs. The CLI surfaces it, with
the library's own values as defaults:

```text
--max-depth <N>             default 128
--max-alias-expansions <N>  default 1024      (billion-laughs defense)
--max-document-size <BYTES> default 256 MiB
--max-node-count <N>        default 1000000
```

## 5. I/O contract

- stdout carries data only: formatted YAML, converted JSON, parse dumps.
- stderr carries everything else: diagnostics, logs, the summary line.
- No positional files means read stdin. A bare `-` means stdin explicitly.
- No directory walking in v1. Shell globs cover it, which avoids `walkdir` and
  `ignore` and avoids reimplementing `.gitignore` semantics. See F3.
- `--write` and `--check` remain mutually exclusive.

### 5.1 Exit codes

Unchanged from the current `glaucus-fmt`, so existing scripts keep working.

| Code | Meaning |
|------|---------|
| 0 | Success, no findings |
| 1 | Findings: unformatted under `--check`, schema violations, parse errors |
| 2 | Usage error (clap default) |
| 3 | I/O failure: unreadable file, permission denied |
| 101 | Panic. Distinct from every finding code. |

The 0/1 split lets CI distinguish "your YAML is wrong" from "the tool broke".

### 5.2 Atomic writes

`--write` renders to a temporary file in the same directory, then `rename()`s
over the target. A crash or a full disk mid-write can never leave a truncated
YAML file where a valid one used to be.

## 6. Errors, diagnostics and logging

### 6.1 Error strategy

`anyhow` inside `glaucus::cli` only, behind the `cli` feature. This matches the
project rule of `anyhow` for binaries and `thiserror`-style hand-rolled errors
for libraries. `anyhow` never appears in a public library signature; it stops at
the CLI boundary. The library's `glaucus_core::Error` is untouched.

### 6.2 Report type

Every failure source converts into one internal type, so exactly one place
decides what a problem looks like:

```rust
struct Report {
    severity: Severity,          // Error | Warning | Note
    message:  String,
    file:     Option<PathBuf>,
    span:     Option<Span>,      // existing line/column
    path:     Option<String>,    // JSON pointer, from validate
    help:     Option<String>,
}
```

Sources: `glaucus_core::Error` (parse), `validate::Diagnostic` (schema),
`io::Error`.

### 6.3 Rendering requirements

- Caret alignment uses display width, not byte or `char` count, so carets line
  up under CJK and emoji. Requires `unicode-width`.
- Tabs are expanded identically in the echoed line and the caret row.
- Long lines are windowed around the span with ellipses.
- `--format json` emits one JSON object per diagnostic, for CI and editors.

Target rendering:

```text
error: expected integer, found string
  --> deploy.yaml:12:9
   |
12 |   port: "8080"
   |         ^^^^^^ at .spec.port
   |
   = help: run with --fix to coerce
```

### 6.4 Secret-leak mitigation

The caret renderer echoes the offending source line, and YAML files routinely
hold secrets. A validation error on a password line would print it into CI logs,
which are often readable by more people than the file is. `--no-source`
suppresses the echo while keeping `file:line:col`, the message and the path. It
defaults on when `CI=true`.

### 6.5 Logging policy

`tracing` and `tracing-subscriber`, all output to stderr.

| Precedence | Rule |
|------------|------|
| 1. CLI flags | `-v` to INFO, `-vv` to DEBUG, `-q` to ERROR |
| 2. `RUST_LOG` | Honoured when no verbosity flag was passed |
| 3. Default | WARN |

CLI arguments beat environment variables beat defaults.

### 6.6 Visibility requirements

Four behaviours that keep the user informed:

1. Every skip is explained at INFO: which file, and why (unparseable,
   unchanged, not matched).
2. A summary line always closes the run: `3 files · 1 error · 2 fixed · 0.04s`.
3. `--check` names the files that differ, not just a count, so the failure is
   actionable without a second run.
4. An empty match is a warning, not a silent success. `glaucus fmt *.yml`
   matching nothing must say so rather than exit 0 in silence.

### 6.7 Panic policy

A panic on external input is a denial of service. The CLI installs a panic hook
that prints a short "this is a bug, please report" message with the version and
exits 101. No `unwrap` or `expect` on anything derived from file contents or
from argv.

## 7. Testing and coverage

### 7.1 Test families

- Argument parsing: table-driven over every alias, every flag, every rejection.
- Diagnostic rendering: golden outputs, including a non-ASCII caret case and a
  tab-expansion case.
- Per-command behaviour, driven through `run_with`.
- Exit codes: one test per code in section 5.1.
- `Cli::command().debug_assert()` in a unit test, clap's own validator, so a
  malformed command tree fails at test time rather than at first run.

### 7.2 Coverage bookkeeping

```toml
exclude-files = [
  "crates/*/src/main.rs",                  # covers both new main.rs shims
  "crates/glaucus/src/cli/process.rs",     # NEW: real argv/stdio/TTY
  "crates/glaucus-wasm/src/lib.rs",
]
```

The existing `crates/*/src/main.rs` glob already matches both new shims, so no
glob rewrite is needed. Choosing `src/main.rs` over `src/bin/*.rs` is what
avoids that trap.

Everything under `src/cli/` other than `process.rs` is ordinary library code and
must reach 100%.

## 8. Performance

The CLI is feature-gated and cannot affect library benchmarks or the criterion
baselines. Three rules inside it:

- Read each file once into a `String` and parse borrowed. The parser is
  zero-copy with `Cow`; no clones on the hot path.
- Wrap stdout in a `BufWriter`. Unbuffered per-line writes are the classic CLI
  throughput bug.
- `--check` never opens a file for writing.

## 9. Reliability

Multi-file runs are continue-on-error: one unparseable file produces a
diagnostic and the batch proceeds, exiting 1 at the end. `--fail-fast` opts into
stopping at the first failure. With atomic writes, a batch can never leave the
tree half-converted.

## 10. Rollout

Phased so the coverage gate stays green at every step.

1. Add the `cli` feature, the `Env` and `run_with` skeleton, `process.rs`, and
   the `tarpaulin.toml` entry. No commands yet.
2. Move the library APIs in, rewriting `glaucus::` to `crate::`.
3. Add commands one at a time, each with its tests.
4. Add the `glaucus-cli` wrapper. Update `publish.yml`, README and docs.
5. Delete the four retired crates last, once parity is verified.

### 10.1 Risk to verify early

`deny.toml` sets `multiple-versions = "deny"`. Adding 25 crates makes a
duplicate-version collision materially more likely, and it fails the Supply
Chain job rather than the build. Step 1 must run `cargo deny check` as soon as
the dependencies land, not at the end.

All new dependencies are MIT or Apache-2.0, already in the `deny.toml`
allowlist, so no licence change is required.

## 11. Future work

Deferred deliberately, recorded so they remain backlog items.

| ID | Feature | Note |
|----|---------|------|
| F1 | Full observability: `indicatif` progress bars, per-phase timing spans (scan, parse, compose, validate), `--log-format pretty\|compact\|json` | Enhances diagnostics and logging. Requested during design. |
| F2 | `schema infer` from sample documents | Net-new engine. |
| F3 | Recursive directory walking with `.gitignore` semantics | Avoids `walkdir` and `ignore` in v1. |
| F4 | `query`, `diff`, `merge` | Each a substantial engine: a path expression language and a tree-diff algorithm. |
| F5 | Parallel multi-file processing | Needs `rayon`. Sequential is adequate at typical file sizes. |
| F6 | Resource-limit flags: `--max-depth`, `--max-alias-expansions`, `--max-document-size`, `--max-key-length`, `--max-node-count` (§4.2) | Deferred by the repo owner on 2026-08-02 in favour of finishing global flags and the crate retirement. Purely additive: `ResourceLimits`' defaults (depth 128, 1024 alias expansions, 256 MiB) still apply and still defend against YAML bombs; the flags only make them tunable per invocation. The full task specification is written and ready to execute in `plans/2026-08-01-glaucus-cli.md` as Task 14. |

## 12. Acceptance criteria

- `cargo add glaucus` resolves the same dependency set as before this change.
- `cargo install glaucus --features cli` and `cargo install glaucus-cli` both
  produce a working `glaucus` binary.
- `mise run coverage` reports 100.00% and exits 0.
- `mise run cargo:clippy` is clean at `-D warnings`.
- `cargo deny check` passes, including `multiple-versions = "deny"`.
- The four retired crates are gone and nothing references them.
- Every command in section 4 works, with every alias.
- Exit codes match section 5.1 exactly.
