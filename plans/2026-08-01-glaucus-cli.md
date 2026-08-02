<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# glaucus-cli Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship one `glaucus` binary that replaces the four single-purpose CLI crates, installable both as `cargo install glaucus --features cli` and as `cargo install glaucus-cli`.

**Architecture:** All CLI logic lives as ordinary library code in `crates/glaucus/src/cli/`, entered through `run_with(Env) -> u8` with argv and all three streams injected. Only `cli/process.rs` touches the real process, and it is the single file excluded from coverage. A thin `glaucus-cli` package depends one-way on `glaucus` to provide a flag-free install path.

**Tech Stack:** Rust 1.97, `clap` (derive), `tracing` + `tracing-subscriber`, `anstream`, `anyhow`, `unicode-width`, `serde_json`. All are optional dependencies behind the `cli` feature.

**Spec:** `specs/2026-08-01-glaucus-cli-design.md`

## Global Constraints

Every task's requirements implicitly include this section.

- **TDD is mandatory, in this order: RED → GREEN → REFACTOR.** Write the
  failing test first and run it; confirm it fails for the right reason (a
  missing item or a wrong value, not an unrelated compile error). Then write
  the minimal code to make it pass and run it again. Then refactor with the
  tests green. Implementation must never be written before its failing test
  exists. Every task's steps are already ordered this way — follow them as
  written rather than batching the implementation first.
  - *Exempt:* pure configuration with no observable behaviour — manifest and
    feature wiring, `tarpaulin.toml`, SPDX-only module stubs. Nothing with
    behaviour is exempt.
  - *Evidence required in every task report:* per test, the RED command and
    its verbatim failure output, then the GREEN command and its pass line.
    If RED was genuinely not captured, record that fact rather than
    reconstructing it — an accurate record beats a tidy one.
  - *Coverage-gap tests are a different activity, and the distinction matters.*
    When a test is added purely to exercise already-correct code that the 100%
    gate flagged, there is no genuine RED to capture — the test passes on its
    first run because the production code was never wrong, only unexercised.
    The RED in that case is the coverage measurement itself. Record it as
    "passed on first run (coverage-gap test, no behavioural RED)". Do NOT
    break the production code to manufacture a failure, and do NOT write a
    deliberately-wrong assertion so it can be 'fixed'. Both produce a tidy
    narrative and a worthless test.
  - *Infrastructure written before its consumers still needs tests NOW.* The
    gate is per-task, so a helper whose only real caller arrives three tasks
    later is uncovered the moment it is written. Every task so far has failed
    coverage on exactly this: `Severity::sgr()`'s Warning/Note arms (Task 2),
    `Source::label()` and the `write_atomic` no-file-name fallback (Task 3).
    Before declaring a task done, walk every function and every match arm you
    added and ask: does a test reach this line? If not, write one — a direct
    unit test on the helper is enough; it does not need a caller to exist.
  - *An unreachable error branch is a design defect, not a coverage problem.*
    A hard 100% gate plus the no-`unwrap`/`expect` rule together forbid them:
    you cannot test an impossible case, and `unreachable!()` is a panic. When a
    `Result` arm provably cannot be reached, restructure so it does not exist —
    funnel the fallible steps through one `?` and leave a single genuinely
    reachable `Err`. Task 7 did this for `convert`: the brief had separate error
    arms for parsing and for YAML serialisation, and the second was unreachable
    (verified empirically AND by reading `glaucus-serde`'s emitter, which has no
    depth or size limits). One `render() -> glaucus_serde::Result<String>`
    helper replaced both. Prove unreachability before restructuring — do not
    assume it.
- **DRY.** Each piece of logic has one representation. The per-command
  "resolve sources → read → report failure → accumulate exit code" loop lives
  in `cli::runner` (Task 3b) and is called by every command. No command
  re-implements it.
- **KISS / YAGNI.** Build only what a task's tests demand. No speculative
  hooks for deferred features F1–F5.
- **SRP + functions under 30 lines, files under 300, cyclomatic complexity
  under 10.** A command's `run` orchestrates only: it resolves arguments and
  delegates. Per-document work goes in a separate function.
- **DIP / dependency injection.** Nothing reaches for real stdio, argv, the
  clock, or the environment outside `cli::process`. Everything else takes
  what it needs as a parameter.
- **Builder pattern over wide constructors.** `Report` has seven fields and
  many construction sites, so it is built through chained self-returning
  methods ending in `.build()` (Task 2). No struct-literal `Report { .. }`
  outside its own module.
- **Structured logging.** `tracing` events carry fields, not interpolated
  strings: `tracing::info!(file = %path, reason = "unchanged", "skipped")`,
  never `tracing::info!("skipped {path} because unchanged")`.
- **Meaningful names, no abbreviations.** `options` not `options`, `error` not
  `e`, `path` not `p`, `report` not `r`. Exceptions: `env` (the established
  name of the injected environment type) and conventional loop indices.
- **Boy Scout Rule.** Leave touched code cleaner than found.
- **Layered architecture, one direction.** `cli::process` (real I/O boundary)
  → `cli` (orchestration) → the `glaucus` library (domain). A `cmd/*.rs` never
  touches real stdio, argv or environment variables; the library layer never
  learns that a CLI exists. A dependency pointing back up a layer is a defect.
- **ISP: pass the narrowest thing that works.** A function needing only an
  output sink takes `&mut dyn Write`, not `&mut Env`. `Env` is threaded only
  where a command genuinely needs several of its parts.
- **OCP.** A new command is added by adding a module and one enum variant.
  Shared behaviour is extended by parameterising `cli::runner`, never by
  editing another command's body.
- **Must Pattern.** Rust's form is `#[must_use]`. It is required on every
  builder method, on `run_with`, and on any function whose return value
  carries a decision (an exit code, a validity verdict, a resolved config).
- **Reliability — scope note.** Retry with backoff, circuit breakers and
  health checks are rules for networked services; this CLI makes no network
  calls, so they do not apply. The reliability rules that DO bind here are
  graceful degradation (continue-on-error across files, `--fail-fast` to opt
  out) and crash-safety (atomic writes).
- **Performance — measure, don't speculate.** Profile before optimising. Do
  not add caches or indices to the diagnostic path on suspicion; a run
  produces a handful of diagnostics over a file already held in memory.
  Avoid accidental super-linear work in loops over documents.
- **Validate all external input; never hardcode secrets.** Argv, file
  contents and environment variables are all untrusted.
- **Test artifacts never live in the repository.** Tests write under
  `std::env::temp_dir()` and delete what they create; no fixture directories
  or output folders are committed.
- **Coverage gate is hard 100%.** `mise run coverage` must report `100.00%` and exit 0 at the end of every task. The only exempt file is `crates/glaucus/src/cli/process.rs`.
  - **The CONTROLLER runs this gate, not the implementer.** It takes ~20 minutes, and a subagent cannot wait on a job that long — it stalls and never completes (this cost Task 1 four stalls and roughly 310k tokens). An implementer's gates are `cargo test -p glaucus --features cli`, `mise run cargo:fmt` and `mise run cargo:clippy`, all under two minutes. Where a task's steps say to run coverage, that step belongs to the controller.
  - **Reviewers: do not fail an implementer for not running coverage.**
- **Pass an explicit `timeout` on every Bash call that compiles Rust.** Use `timeout: 600000` (10 minutes) for `cargo test`, `cargo build`, `mise run cargo:clippy` and `mise run cargo:fmt`. The harness auto-backgrounds any call exceeding its 120-second default, and a subagent then stalls waiting for a notification it cannot receive. This has cost this run more time than any other single cause, in two distinct forms: an implementer backgrounding a long job deliberately, and the harness backgrounding a foreground call. Both end the same way.
- **Clippy is `-D warnings`.** `mise run cargo:clippy` must be clean.
- **Formatting.** Run `mise run cargo:fmt` before every commit. `rustfmt` uses `max_width = 100` and `fn_call_width = 60`; a macro call whose arguments exceed 60 columns gets exploded across lines.
- **No `unwrap`/`expect` on anything derived from file contents or argv.** A panic on external input is a denial of service. `unwrap` in `#[cfg(test)]` code is fine.
- **Commits must be GPG-signed:** use `git commit -S`. Never add a `Co-Authored-By` trailer.
- **`deny.toml` sets `multiple-versions = "deny"`.** Run `cargo deny check` in Task 1 and after any dependency change.
- **All CLI dependencies are `optional = true`.** `cargo add glaucus` must resolve exactly the dependency set it resolves today.
- **`Position.column` is 1-based in BYTES, not characters.** `Position.offset` is a byte offset from the start of input. Any caret arithmetic slices by byte index and then measures display width.
- **stdout carries data only. stderr carries diagnostics, logs and summaries.**
- Dependency version style in `[workspace.dependencies]` is a range, e.g. `">=4,<5"` — match it.

---

## File Structure

**Created:**

| File | Responsibility |
|------|----------------|
| `crates/glaucus/src/cli/mod.rs` | `Cli`/`Command` clap types, `run_with` dispatch |
| `crates/glaucus/src/cli/env.rs` | `Env` injection struct, `ColorChoice` |
| `crates/glaucus/src/cli/exit.rs` | Exit-code constants |
| `crates/glaucus/src/cli/process.rs` | Real argv/stdio/TTY/panic hook — **coverage-exempt** |
| `crates/glaucus/src/cli/diag.rs` | `Report`, `Severity`, caret renderer, JSON renderer |
| `crates/glaucus/src/cli/logging.rs` | `tracing` subscriber init, verbosity precedence |
| `crates/glaucus/src/cli/io.rs` | File reads, stdin, atomic writes |
| `crates/glaucus/src/cli/cmd/mod.rs` | Command module re-exports |
| `crates/glaucus/src/cli/cmd/{fmt,validate,parse,convert,schema,lsp,mcp,completions}.rs` | One command each |
| `crates/glaucus/src/fmt.rs` | Moved from `glaucus-fmt` |
| `crates/glaucus/src/validate.rs` | Moved from `glaucus-validate` |
| `crates/glaucus/src/lsp.rs` | Moved from `glaucus-lsp` |
| `crates/glaucus/src/mcp.rs` | Moved from `glaucus-mcp` |
| `crates/glaucus/src/main.rs` | 3-line shim |
| `crates/glaucus-cli/Cargo.toml`, `src/main.rs` | Wrapper package |

**Modified:** `Cargo.toml` (workspace deps), `crates/glaucus/Cargo.toml`, `crates/glaucus/src/lib.rs`, `tarpaulin.toml`, `.github/workflows/publish.yml`, `README.md`.

**Deleted (Task 13):** `crates/glaucus-fmt/`, `crates/glaucus-validate/`, `crates/glaucus-lsp/`, `crates/glaucus-mcp/`.

### Deliberate refinement of the spec

The spec sketches `run_with(env) -> ExitCode`. This plan uses `run_with(env) -> u8` and converts to `ExitCode` inside `process.rs`. `ExitCode` cannot be compared or inspected, so returning it would make every exit-code assertion impossible and put the conversion inside the tested surface. Behaviour is identical.

---

### Task 1: CLI scaffolding, `Env`, and the coverage exemption

**Files:**

- Modify: `Cargo.toml` (add `[workspace.dependencies]` entries)
- Modify: `crates/glaucus/Cargo.toml` (feature, optional deps, `[[bin]]`)
- Modify: `crates/glaucus/src/lib.rs` (add `pub mod cli`)
- Modify: `tarpaulin.toml` (add the exemption)
- Create: `crates/glaucus/src/main.rs`
- Create: `crates/glaucus/src/cli/{mod.rs,env.rs,exit.rs,process.rs}`

**Interfaces:**

- Produces: `glaucus::cli::env::{Env, ColorChoice}`; `glaucus::cli::exit::{OK, FINDINGS, USAGE, IO}`; `glaucus::cli::run_with(Env<'_>) -> u8`; `glaucus::cli::process::main() -> std::process::ExitCode`; test helper `glaucus::cli::tests::drive`.

- [ ] **Step 1: Add workspace dependencies**

In the root `Cargo.toml`, inside `[workspace.dependencies]`, add:

```toml
anstream = { version = ">=0.6,<0.7" }
anyhow = { version = ">=1,<2" }
clap = { version = ">=4,<5", features = ["derive"] }
tracing = { version = ">=0.1,<0.2" }
tracing-subscriber = { version = ">=0.3,<0.4", features = ["env-filter"] }
unicode-width = { version = ">=0.2,<0.3" }
```

- [ ] **Step 2: Wire the `cli` feature in `crates/glaucus/Cargo.toml`**

```toml
[features]
ast = ["dep:glaucus-ast"]
cli = [
  "ast",
  "cst",
  "schema",
  "serde",
  "dep:anstream",
  "dep:anyhow",
  "dep:clap",
  "dep:serde_json",
  "dep:tracing",
  "dep:tracing-subscriber",
  "dep:unicode-width",
]
cst = ["dep:glaucus-cst"]
default = ["ast", "serde", "cst"]
schema = ["dep:glaucus-schema", "ast", "cst"]
serde = ["dep:glaucus-serde", "ast"]

[dependencies]
anstream = { workspace = true, optional = true }
anyhow = { workspace = true, optional = true }
clap = { workspace = true, optional = true }
glaucus-ast = { workspace = true, optional = true }
glaucus-core.workspace = true
glaucus-cst = { workspace = true, optional = true }
glaucus-schema = { workspace = true, optional = true }
glaucus-serde = { workspace = true, optional = true }
serde.workspace = true
serde_json = { workspace = true, optional = true }
tracing = { workspace = true, optional = true }
tracing-subscriber = { workspace = true, optional = true }
unicode-width = { workspace = true, optional = true }

[[bin]]
name = "glaucus"
path = "src/main.rs"
required-features = ["cli"]
```

- [ ] **Step 3: Verify the library graph is unchanged, then check the supply chain**

Run:

```bash
cargo tree -p glaucus --no-default-features -e normal
cargo deny check
```

Expected: the first prints only `glaucus-core` and `serde` beneath `glaucus` — **no `clap`, no `tracing`**. The second passes, including `multiple-versions = "deny"`. If `cargo deny` reports a duplicate version, resolve it now with a `skip` entry in `deny.toml` and a comment naming the crate; do not defer it.

- [ ] **Step 4: Add the coverage exemption**

In `tarpaulin.toml`, replace the `exclude-files` line with:

```toml
exclude-files = [
  "crates/*/src/main.rs",
  # The only CLI file that touches the real process: argv, locked stdio, TTY
  # detection and the panic hook. Everything else in src/cli/ is ordinary
  # library code driven through `run_with(Env)` and is covered by unit tests.
  "crates/glaucus/src/cli/process.rs",
  "crates/glaucus-wasm/src/lib.rs",
]
```

- [ ] **Step 5: Create `crates/glaucus/src/cli/env.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The CLI's injected environment.

use std::ffi::OsString;
use std::io::{BufRead, Write};

/// When to emit ANSI colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Colour when stderr is a terminal and `NO_COLOR` is unset.
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}

/// Everything the CLI touches that is not pure computation.
///
/// Injecting these makes every code path reachable from a unit test without
/// spawning a process, which is what lets `src/cli/` meet the 100% gate.
pub struct Env<'a> {
    /// Full argv, including the program name at index 0.
    pub args: Vec<OsString>,
    /// Standard input.
    pub stdin: &'a mut dyn BufRead,
    /// Data output.
    pub stdout: &'a mut dyn Write,
    /// Diagnostics, logs and summaries.
    pub stderr: &'a mut dyn Write,
    /// Resolved colour policy.
    pub color: ColorChoice,
    /// True when running under CI; makes `--no-source` default on.
    pub is_ci: bool,
}
```

- [ ] **Step 6: Create `crates/glaucus/src/cli/exit.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Exit-code taxonomy. Kept identical to the retired `glaucus-fmt` so existing
//! scripts keep working.

/// Success, no findings.
pub const OK: u8 = 0;
/// Findings: unformatted under `--check`, schema violations, parse errors.
pub const FINDINGS: u8 = 1;
/// Usage error. Matches clap's own default.
pub const USAGE: u8 = 2;
/// I/O failure: unreadable file, permission denied.
pub const IO: u8 = 3;
```

- [ ] **Step 7: Write the failing test in `crates/glaucus/src/cli/mod.rs`**

Create the file with the module wiring, the clap root, and the tests:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Command-line interface. Enabled by the `cli` feature.

pub mod diag;
pub mod env;
pub mod exit;
pub mod io;
pub mod logging;
pub mod process;

mod cmd;

use clap::Parser;
use env::Env;

/// Safe YAML tooling.
#[derive(Parser, Debug)]
#[command(name = "glaucus", version, about, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: cmd::Command,
}

/// Parses `env.args` and runs the requested command, returning the exit code.
#[must_use]
pub fn run_with(env: Env<'_>) -> u8 {
    let parsed = match Cli::try_parse_from(&env.args) {
        Ok(cli) => cli,
        Err(e) => {
            // clap renders help and version to stdout, errors to stderr.
            let rendered = e.render().to_string();
            let sink: &mut dyn std::io::Write = if e.use_stderr() {
                env.stderr
            } else {
                env.stdout
            };
            let _ = write!(sink, "{rendered}");
            return if e.use_stderr() { exit::USAGE } else { exit::OK };
        }
    };
    cmd::dispatch(parsed.command, env)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use env::ColorChoice;
    use std::ffi::OsString;

    /// Drives the whole CLI with in-memory streams.
    ///
    /// Returns `(exit_code, stdout, stderr)`.
    pub(crate) fn drive(args: &[&str], stdin: &str) -> (u8, String, String) {
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let mut input = stdin.as_bytes();
        let code = run_with(Env {
            args: args.iter().map(OsString::from).collect(),
            stdin: &mut input,
            stdout: &mut out,
            stderr: &mut err,
            color: ColorChoice::Never,
            is_ci: false,
        });
        (
            code,
            String::from_utf8(out).expect("stdout utf8"),
            String::from_utf8(err).expect("stderr utf8"),
        )
    }

    #[test]
    fn clap_command_tree_is_valid() {
        // clap's own validator: catches duplicate aliases, bad arg ids, etc.
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_is_a_usage_error() {
        let (code, out, err) = drive(&["glaucus"], "");
        assert_eq!(code, exit::USAGE);
        assert!(out.is_empty());
        assert!(err.contains("Usage:"));
    }

    #[test]
    fn help_goes_to_stdout_and_exits_zero() {
        let (code, out, err) = drive(&["glaucus", "--help"], "");
        assert_eq!(code, exit::OK);
        assert!(out.contains("Usage:"));
        assert!(err.is_empty());
    }

    #[test]
    fn version_goes_to_stdout_and_exits_zero() {
        let (code, out, _err) = drive(&["glaucus", "--version"], "");
        assert_eq!(code, exit::OK);
        assert!(out.contains("glaucus"));
    }

    #[test]
    fn unknown_subcommand_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "nope"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("unrecognized") || err.contains("unexpected"));
    }
}
```

Add `use clap::CommandFactory;` inside the test module (needed by `Cli::command()`).

Create a placeholder `crates/glaucus/src/cli/cmd/mod.rs` so this compiles:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subcommands.

use crate::cli::env::Env;
use crate::cli::exit;

/// The subcommand set.
#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Print the CLI version and exit. Replaced in Task 4.
    Noop,
}

/// Runs the selected command.
pub fn dispatch(command: Command, _env: Env<'_>) -> u8 {
    match command {
        Command::Noop => exit::OK,
    }
}
```

Also add to `crates/glaucus/src/lib.rs`:

```rust
#[cfg(feature = "cli")]
pub mod cli;
```

- [ ] **Step 8: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cli::`

Expected: FAIL — `diag`, `io`, `logging`, `process` modules do not exist yet.

- [ ] **Step 9: Create the remaining module stubs**

Create `crates/glaucus/src/cli/diag.rs`, `io.rs` and `logging.rs` each containing only the SPDX header and a `//!` doc line; they are filled in by Tasks 2 and 3. Remove their `pub mod` lines from `mod.rs` for now and re-add them in those tasks, so this task compiles on its own.

Create `crates/glaucus/src/cli/process.rs`:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The only file that touches the real process. Excluded from coverage in
//! `tarpaulin.toml`: argv, locked stdio and TTY detection cannot be faked, and
//! everything worth testing lives behind `run_with(Env)`.

use crate::cli::env::{ColorChoice, Env};
use crate::cli::run_with;
use std::io::{BufWriter, IsTerminal};
use std::process::ExitCode;

/// Real-process entry point shared by both binaries.
#[must_use]
pub fn main() -> ExitCode {
    install_panic_hook();

    let color = if std::env::var_os("NO_COLOR").is_some() {
        ColorChoice::Never
    } else if std::io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    };
    let is_ci = std::env::var_os("CI").is_some();

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut input = stdin.lock();
    // Buffered: unbuffered per-line writes are the classic CLI throughput bug.
    let mut out = BufWriter::new(stdout.lock());
    let mut err = stderr.lock();

    let code = run_with(Env {
        args: std::env::args_os().collect(),
        stdin: &mut input,
        stdout: &mut out,
        stderr: &mut err,
        color,
        is_ci,
    });
    drop(out);
    ExitCode::from(code)
}

/// Turns a panic into exit 101 with a report-this message, so a genuine crash
/// can never be mistaken for a finding.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("glaucus {}: internal error: {info}", env!("CARGO_PKG_VERSION"));
        eprintln!("This is a bug. Please report it with the input that caused it.");
    }));
}
```

Create `crates/glaucus/src/main.rs`:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Binary entry point. All logic lives in `glaucus::cli`.

fn main() -> std::process::ExitCode {
    glaucus::cli::process::main()
}
```

- [ ] **Step 10: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cli::`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 11: Verify the gates**

Run:

```bash
mise run cargo:fmt
mise run cargo:clippy
mise run coverage
```

Expected: clippy clean; coverage `100.00%`, exit 0.

- [ ] **Step 12: Commit**

```bash
git add Cargo.toml tarpaulin.toml crates/glaucus/Cargo.toml crates/glaucus/src/
git commit -S -m "feat(cli): add cli feature, Env injection and process entry point"
```

---

### Task 2: Diagnostic renderer

**Files:**

- Modify: `crates/glaucus/src/cli/diag.rs`
- Modify: `crates/glaucus/src/cli/mod.rs` (re-add `pub mod diag;`)

**Interfaces:**

Consumes: `crate::cli::env::ColorChoice`.

Produces: `diag::{Severity, Report, RenderOptions}`; `diag::render(&Report, Option<&str>, RenderOptions, &mut dyn Write) -> std::io::Result<()>`; `diag::render_json(&Report, &mut dyn Write) -> std::io::Result<()>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/glaucus/src/cli/diag.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report::builder(Severity::Error, "expected integer, found string")
            .file(Some("deploy.yaml".into()))
            .location(1, 9)
            .path(".spec.port")
            .help("run with --fix to coerce")
            .build()
    }

    #[test]
    fn builder_leaves_unset_fields_empty() {
        let report = Report::builder(Severity::Warning, "m").build();
        assert_eq!(report.severity, Severity::Warning);
        assert_eq!(report.message, "m");
        assert!(report.file.is_none());
        assert_eq!(report.line, 0);
        assert_eq!(report.column, 0);
        assert!(report.path.is_none());
        assert!(report.help.is_none());
    }

    #[test]
    fn builder_chains_every_field() {
        let report = Report::builder(Severity::Error, "boom")
            .file(Some("a.yaml".into()))
            .location(3, 7)
            .path(".spec")
            .help("try --fix")
            .build();
        assert_eq!(report.file, Some("a.yaml".into()));
        assert_eq!(report.line, 3);
        assert_eq!(report.column, 7);
        assert_eq!(report.path.as_deref(), Some(".spec"));
        assert_eq!(report.help.as_deref(), Some("try --fix"));
    }

    fn options(show_source: bool) -> RenderOptions {
        RenderOptions { color: false, show_source }
    }

    #[test]
    fn renders_caret_under_the_byte_column() {
        let src = "  port: \"8080\"\n";
        let mut out = Vec::new();
        render(&report(), Some(src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("error: expected integer, found string"));
        assert!(s.contains("--> deploy.yaml:1:9"));
        // column 9 is 1-based bytes -> 8 columns of padding before the caret.
        assert!(s.contains("\n   |         ^"), "caret misaligned:\n{s}");
        assert!(s.contains("at .spec.port"));
        assert!(s.contains("= help: run with --fix to coerce"));
    }

    #[test]
    fn caret_aligns_under_wide_characters() {
        // Each CJK char is 3 bytes but 2 display columns. Byte column 7 is the
        // 3rd char, so display padding must be 4, not 6.
        let src = "名前: x\n";
        let mut r = report();
        r.column = 7;
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, Some(src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\n   |     ^"), "wide-char caret misaligned:\n{s}");
    }

    #[test]
    fn tabs_expand_consistently_in_line_and_caret() {
        let src = "\tport: 1\n";
        let mut r = report();
        r.column = 2; // just after the tab
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, Some(src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("    port: 1"), "tab not expanded:\n{s}");
        assert!(s.contains("\n   |     ^"), "caret ignores tab width:\n{s}");
    }

    #[test]
    fn no_source_suppresses_the_echo_but_keeps_location() {
        let src = "  password: hunter2\n";
        let mut out = Vec::new();
        render(&report(), Some(src), options(false), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("--> deploy.yaml:1:9"));
        assert!(!s.contains("hunter2"), "secret leaked:\n{s}");
    }

    #[test]
    fn long_lines_are_windowed() {
        let src = format!("{}port: 1\n", "x".repeat(400));
        let mut r = report();
        r.column = 401;
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, Some(&src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("..."), "long line not windowed:\n{s}");
        assert!(s.lines().all(|l| l.chars().count() <= 160), "line too long:\n{s}");
    }

    #[test]
    fn missing_source_renders_header_only() {
        let mut out = Vec::new();
        render(&report(), None, options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("--> deploy.yaml:1:9"));
        assert!(!s.contains(" | "));
    }

    #[test]
    fn zero_line_means_unknown_location() {
        let mut r = report();
        r.line = 0;
        r.column = 0;
        r.file = None;
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, None, options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "error: expected integer, found string\n");
    }

    #[test]
    fn severity_words_are_distinct() {
        assert_eq!(Severity::Error.label(), "error");
        assert_eq!(Severity::Warning.label(), "warning");
        assert_eq!(Severity::Note.label(), "note");
    }

    #[test]
    fn colour_wraps_the_severity_label() {
        let mut out = Vec::new();
        render(&report(), None, RenderOptions { color: true, show_source: false }, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\u{1b}["), "expected ANSI escape:\n{s}");
    }

    #[test]
    fn json_render_emits_one_object_per_line() {
        let mut out = Vec::new();
        render_json(&report(), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["severity"], "error");
        assert_eq!(v["line"], 1);
        assert_eq!(v["column"], 9);
        assert_eq!(v["file"], "deploy.yaml");
        assert_eq!(v["path"], ".spec.port");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cli::diag`

Expected: FAIL — `Report`, `Severity`, `RenderOptions`, `render`, `render_json` are undefined.

- [ ] **Step 3: Implement the renderer**

Replace the body of `crates/glaucus/src/cli/diag.rs` (above the test module) with:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One diagnostic type and one renderer, so exactly one place decides what a
//! problem looks like on screen.

use std::io::Write;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

/// A tab renders as this many spaces, in both the echoed line and the caret row.
const TAB_WIDTH: usize = 4;
/// Maximum display width of an echoed source line before it is windowed.
const MAX_LINE_WIDTH: usize = 120;

/// How serious a report is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A failure.
    Error,
    /// Something suspicious that did not stop the run.
    Warning,
    /// Additional context.
    Note,
}

impl Severity {
    /// The word shown at the head of the report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }

    /// The ANSI SGR parameters used when colour is on.
    const fn sgr(self) -> &'static str {
        match self {
            Self::Error => "1;31",
            Self::Warning => "1;33",
            Self::Note => "1;36",
        }
    }
}

/// A single problem, normalised from any source.
#[derive(Debug, Clone)]
pub struct Report {
    /// How serious it is.
    pub severity: Severity,
    /// One-line description.
    pub message: String,
    /// Source file, when the input was not stdin.
    pub file: Option<PathBuf>,
    /// 1-based line. `0` means unknown.
    pub line: u32,
    /// 1-based column **in bytes**. `0` means unknown.
    pub column: u32,
    /// JSON-pointer-ish path to the offending node.
    pub path: Option<String>,
    /// Suggested next step.
    pub help: Option<String>,
}

impl Report {
    /// Starts a report. Chain the optional parts, then [`ReportBuilder::build`].
    #[must_use]
    pub fn builder(severity: Severity, message: impl Into<String>) -> ReportBuilder {
        ReportBuilder {
            report: Self {
                severity,
                message: message.into(),
                file: None,
                line: 0,
                column: 0,
                path: None,
                help: None,
            },
        }
    }
}

/// Fluent constructor for [`Report`].
///
/// Seven fields, most of them optional, built at many call sites — exactly the
/// shape a wide constructor or a bare struct literal handles badly. Struct
/// literals for `Report` are confined to this module.
#[derive(Debug, Clone)]
pub struct ReportBuilder {
    report: Report,
}

impl ReportBuilder {
    /// Sets the source file. `None` means stdin.
    #[must_use]
    pub fn file(mut self, file: Option<PathBuf>) -> Self {
        self.report.file = file;
        self
    }

    /// Sets the 1-based line and 1-based BYTE column. Zero means unknown.
    #[must_use]
    pub fn location(mut self, line: u32, column: u32) -> Self {
        self.report.line = line;
        self.report.column = column;
        self
    }

    /// Sets the JSON-pointer path to the offending node.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.report.path = Some(path.into());
        self
    }

    /// Sets the suggested next step.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.report.help = Some(help.into());
        self
    }

    /// Finishes the report.
    #[must_use]
    pub fn build(self) -> Report {
        self.report
    }
}

/// Rendering policy.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Emit ANSI colour.
    pub color: bool,
    /// Echo the offending source line. Off suppresses possible secrets.
    pub show_source: bool,
}

/// Renders `report` in human form.
///
/// # Errors
///
/// Propagates write failures from `out`.
pub fn render(
    report: &Report,
    source: Option<&str>,
    options: RenderOptions,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let label = report.severity.label();
    if options.color {
        writeln!(out, "\u{1b}[{}m{label}\u{1b}[0m: {}", report.severity.sgr(), report.message)?;
    } else {
        writeln!(out, "{label}: {}", report.message)?;
    }

    if report.line == 0 {
        return Ok(());
    }

    let name = report
        .file
        .as_ref()
        .map_or_else(|| "<stdin>".to_string(), |p| p.display().to_string());
    writeln!(out, "  --> {name}:{}:{}", report.line, report.column)?;

    let Some(text) = source.filter(|_| options.show_source) else {
        return Ok(());
    };
    let Some(raw) = text.lines().nth(report.line as usize - 1) else {
        return Ok(());
    };

    let gutter = report.line.to_string();
    let pad = " ".repeat(gutter.len().max(2));
    let (shown, caret_col) = window(raw, report.column);

    writeln!(out, "{pad} |")?;
    writeln!(out, "{gutter} | {shown}")?;
    let mut caret = format!("{pad} | {}^", " ".repeat(caret_col));
    if let Some(p) = &report.path {
        caret.push_str(&format!(" at {p}"));
    }
    writeln!(out, "{caret}")?;

    if let Some(h) = &report.help {
        writeln!(out, "{pad} |")?;
        writeln!(out, "{pad} = help: {h}")?;
    }
    Ok(())
}

/// Expands tabs, windows over-long lines, and returns the display column the
/// caret belongs at.
///
/// `column` is 1-based **bytes**, matching `Position::column`, so the prefix is
/// sliced by byte index and then measured for display width.
fn window(raw: &str, column: u32) -> (String, usize) {
    let byte_col = (column as usize).saturating_sub(1).min(raw.len());
    let prefix = raw.get(..byte_col).unwrap_or(raw);

    let expanded: String = raw.replace('\t', &" ".repeat(TAB_WIDTH));
    let caret_col = prefix.replace('\t', &" ".repeat(TAB_WIDTH)).width();

    if expanded.width() <= MAX_LINE_WIDTH {
        return (expanded, caret_col);
    }
    // Window around the caret so the interesting part stays visible.
    let start = caret_col.saturating_sub(MAX_LINE_WIDTH / 2);
    let shown: String = expanded.chars().skip(start).take(MAX_LINE_WIDTH).collect();
    (format!("...{shown}..."), caret_col - start + 3)
}

/// Renders `report` as one JSON object followed by a newline.
///
/// # Errors
///
/// Propagates write failures from `out`.
pub fn render_json(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    let value = serde_json::json!({
        "severity": report.severity.label(),
        "message": report.message,
        "file": report.file.as_ref().map(|p| p.display().to_string()),
        "line": report.line,
        "column": report.column,
        "path": report.path,
        "help": report.help,
    });
    writeln!(out, "{value}")
}
```

Re-add `pub mod diag;` to `crates/glaucus/src/cli/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cli::diag`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S -m "feat(cli): add diagnostic renderer with width-correct carets"
```

---

### Task 3: Logging and I/O

**Files:**

- Modify: `crates/glaucus/src/cli/logging.rs`, `crates/glaucus/src/cli/io.rs`
- Modify: `crates/glaucus/src/cli/mod.rs` (re-add `pub mod io; pub mod logging;`)

**Interfaces:**

- Produces: `logging::level_for(verbose: u8, quiet: bool, rust_log: Option<&str>) -> &'static str`; `io::{read_input, write_atomic}`; `io::Source`.

- [ ] **Step 1: Write the failing tests**

In `crates/glaucus/src/cli/logging.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_warn() {
        assert_eq!(level_for(0, false, None), "warn");
    }

    #[test]
    fn one_v_is_info_two_is_debug() {
        assert_eq!(level_for(1, false, None), "info");
        assert_eq!(level_for(2, false, None), "debug");
        assert_eq!(level_for(9, false, None), "debug");
    }

    #[test]
    fn quiet_is_error() {
        assert_eq!(level_for(0, true, None), "error");
    }

    #[test]
    fn rust_log_applies_only_when_no_flag_given() {
        assert_eq!(level_for(0, false, Some("trace")), "trace");
        // A CLI flag beats the environment.
        assert_eq!(level_for(1, false, Some("trace")), "info");
        assert_eq!(level_for(0, true, Some("trace")), "error");
    }
}
```

In `crates/glaucus/src/cli/io.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_stdin_when_source_is_stdin() {
        let mut input = Cursor::new(b"a: 1\n".to_vec());
        let got = read_input(&Source::Stdin, &mut input).unwrap();
        assert_eq!(got, "a: 1\n");
    }

    #[test]
    fn reads_a_file_from_disk() {
        let dir = std::env::temp_dir().join("glaucus-io-read");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.yaml");
        std::fs::write(&p, "b: 2\n").unwrap();
        let mut empty = Cursor::new(Vec::new());
        let got = read_input(&Source::File(p.clone()), &mut empty).unwrap();
        assert_eq!(got, "b: 2\n");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn missing_file_is_an_error() {
        let mut empty = Cursor::new(Vec::new());
        let err = read_input(&Source::File("nope.yaml".into()), &mut empty).unwrap_err();
        assert!(err.to_string().contains("nope.yaml"), "no context: {err}");
    }

    #[test]
    fn non_utf8_input_is_an_error() {
        let dir = std::env::temp_dir().join("glaucus-io-utf8");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("bad.yaml");
        std::fs::write(&p, [0xff, 0xfe]).unwrap();
        let mut empty = Cursor::new(Vec::new());
        let err = read_input(&Source::File(p.clone()), &mut empty).unwrap_err();
        assert!(err.to_string().contains("UTF-8"), "wrong error: {err}");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = std::env::temp_dir().join("glaucus-io-write");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("y.yaml");
        std::fs::write(&p, "old\n").unwrap();
        write_atomic(&p, "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new\n");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn atomic_write_to_unwritable_dir_is_an_error() {
        let path = std::path::Path::new("/nonexistent-dir-glaucus/z.yaml");
        assert!(write_atomic(path, "x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("glaucus-io-perm");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.yaml");
        std::fs::write(&path, "password: old\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&path, "password: new\n").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "atomic write widened permissions to {mode:o}");
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_to_a_new_path_uses_the_umask() {
        // No source file to inherit from: the helper must not error.
        let dir = std::env::temp_dir().join("glaucus-io-new");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fresh.yaml");
        let _ = std::fs::remove_file(&path);
        write_atomic(&path, "a: 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 1\n");
        std::fs::remove_file(&path).unwrap();
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cli::`

Expected: FAIL — `level_for`, `read_input`, `write_atomic`, `Source` undefined.

- [ ] **Step 3: Implement `logging.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Verbosity resolution. CLI flags beat `RUST_LOG` beats the default.

/// Resolves the effective `tracing` filter directive.
///
/// Precedence: an explicit `-v`/`-q` flag wins; otherwise `RUST_LOG`; otherwise
/// `warn`.
#[must_use]
pub fn level_for(verbose: u8, quiet: bool, rust_log: Option<&str>) -> &'static str {
    if quiet {
        return "error";
    }
    match verbose {
        0 => match rust_log {
            Some("trace") => "trace",
            Some("debug") => "debug",
            Some("info") => "info",
            Some("error") => "error",
            Some("warn") | None => "warn",
            Some(_) => "warn",
        },
        1 => "info",
        _ => "debug",
    }
}
```

- [ ] **Step 4: Implement `io.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Input reading and crash-safe output.

use anyhow::{Context, Result};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

/// Where a document comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Standard input.
    Stdin,
    /// A path on disk.
    File(PathBuf),
}

impl Source {
    /// The name to show in diagnostics.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Stdin => "<stdin>".to_string(),
            Self::File(p) => p.display().to_string(),
        }
    }
}

/// Reads a source to a `String`.
///
/// # Errors
///
/// Returns an error when the file cannot be read or is not valid UTF-8.
pub fn read_input(source: &Source, stdin: &mut dyn BufRead) -> Result<String> {
    match source {
        Source::Stdin => {
            let mut buf = Vec::new();
            stdin.read_to_end(&mut buf).context("reading stdin")?;
            String::from_utf8(buf).context("stdin is not valid UTF-8")
        }
        Source::File(p) => {
            let bytes = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
            String::from_utf8(bytes)
                .with_context(|| format!("{} is not valid UTF-8", p.display()))
        }
    }
}

/// Writes `contents` to `path` atomically: a temporary file in the same
/// directory, then a rename.
///
/// A crash or a full disk mid-write can never leave a truncated YAML file where
/// a valid one used to be.
///
/// # Errors
///
/// Returns an error when the temporary file cannot be created, written, or
/// renamed over the target.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map_or_else(|| "out".to_string(), |n| n.to_string_lossy().into_owned());
    let temporary = directory.join(format!(".{name}.glaucus-tmp"));

    {
        let mut file = std::fs::File::create(&temporary)
            .with_context(|| format!("creating {}", temporary.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("writing {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing {}", temporary.display()))?;
    }

    inherit_permissions(path, &temporary)?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Copies `source`'s permission bits onto `target`.
///
/// Without this, the rename above would replace a `0600` file holding secrets
/// with a fresh `0644` one — silently widening access to every YAML file this
/// tool rewrites. A missing `source` means the target is new, so the process
/// umask is the right answer and there is nothing to copy.
#[cfg(unix)]
fn inherit_permissions(source: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::metadata(source) else {
        return Ok(());
    };
    let mode = metadata.permissions().mode();
    std::fs::set_permissions(target, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting mode on {}", target.display()))
}

/// Non-Unix platforms have no mode bits to copy.
#[cfg(not(unix))]
fn inherit_permissions(_source: &Path, _target: &Path) -> Result<()> {
    Ok(())
}
```

Re-add `pub mod io;` and `pub mod logging;` to `mod.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cli::`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 6: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S -m "feat(cli): add verbosity resolution and atomic file I/O"
```

---

### Task 3b: Shared command runner

Exists so no command re-implements source resolution, document reading, or
failure reporting. Without it the same loop appears five times (DRY), and each
command's `run` blows past the 30-line limit (SRP).

**Files:**

- Create: `crates/glaucus/src/cli/runner.rs`
- Modify: `crates/glaucus/src/cli/mod.rs` (add `pub mod runner;`)

**Interfaces:**

Consumes: `io::{Source, read_input}`, `diag::{Report, RenderOptions, render, render_json}`, `exit::*`.

Produces: `runner::{resolve_sources, read_all, report_io_error, report_parse_error, summary}`.

- [ ] **Step 1: Write the failing tests**

Create `crates/glaucus/src/cli/runner.rs` with only the SPDX header, a `//!`
line, and this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn no_files_resolves_to_stdin() {
        assert_eq!(resolve_sources(&[]), vec![Source::Stdin]);
    }

    #[test]
    fn files_resolve_in_order() {
        let files = vec![PathBuf::from("a.yaml"), PathBuf::from("b.yaml")];
        let got = resolve_sources(&files);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], Source::File("a.yaml".into()));
        assert_eq!(got[1], Source::File("b.yaml".into()));
    }

    #[test]
    fn read_all_pairs_each_source_with_its_text() {
        let mut stdin = Cursor::new(b"a: 1\n".to_vec());
        let documents = read_all(&[Source::Stdin], &mut stdin);
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].1.as_ref().unwrap(), "a: 1\n");
    }

    #[test]
    fn read_all_keeps_going_after_a_failure() {
        let directory = std::env::temp_dir().join("glaucus-runner-read");
        std::fs::create_dir_all(&directory).unwrap();
        let good = directory.join("good.yaml");
        std::fs::write(&good, "b: 2\n").unwrap();
        let mut stdin = Cursor::new(Vec::new());
        let sources = vec![Source::File("missing.yaml".into()), Source::File(good.clone())];

        let documents = read_all(&sources, &mut stdin);

        assert_eq!(documents.len(), 2, "a failure must not truncate the batch");
        assert!(documents[0].1.is_err());
        assert_eq!(documents[1].1.as_ref().unwrap(), "b: 2\n");
        std::fs::remove_file(good).unwrap();
    }

    #[test]
    fn io_error_is_reported_and_yields_the_io_code() {
        let error = anyhow::anyhow!("reading nope.yaml: not found");
        let mut stderr = Vec::new();
        let code = report_io_error(&error, &mut stderr);
        assert_eq!(code, exit::IO);
        let text = String::from_utf8(stderr).unwrap();
        assert!(text.starts_with("error: "), "unexpected: {text}");
        assert!(text.contains("nope.yaml"));
    }

    #[test]
    fn summary_names_counts_on_stderr() {
        let mut stderr = Vec::new();
        summary(&mut stderr, 3, 1, 2);
        let text = String::from_utf8(stderr).unwrap();
        assert!(text.contains('3') && text.contains('1') && text.contains('2'), "{text}");
    }

    #[test]
    fn summary_warns_when_nothing_matched() {
        let mut stderr = Vec::new();
        summary(&mut stderr, 0, 0, 0);
        let text = String::from_utf8(stderr).unwrap();
        assert!(text.contains("no files"), "empty match must not be silent: {text}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cli::runner`

Expected: FAIL — `resolve_sources`, `read_all`, `report_io_error`, `summary`
are undefined. Capture the failure output for the report.

- [ ] **Step 3: Write the minimal implementation**

Above the test module in `crates/glaucus/src/cli/runner.rs`:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The one place that turns command arguments into documents and failures
//! into rendered reports. Commands decide what to DO with a document; they
//! never re-implement getting one.

use crate::cli::diag::{RenderOptions, Report, Severity, render, render_json};
use crate::cli::exit;
use crate::cli::io::{Source, read_input};
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// Turns positional file arguments into sources, defaulting to stdin.
#[must_use]
pub fn resolve_sources(files: &[PathBuf]) -> Vec<Source> {
    if files.is_empty() {
        vec![Source::Stdin]
    } else {
        files.iter().cloned().map(Source::File).collect()
    }
}

/// Reads every source, pairing each with its text or its read error.
///
/// Reading the whole batch up front — rather than inside each command's loop —
/// is what lets a command borrow `stderr` and `stdout` freely afterwards, and
/// it makes continue-on-error the default: one unreadable file cannot truncate
/// the batch.
#[must_use]
pub fn read_all(
    sources: &[Source],
    stdin: &mut dyn BufRead,
) -> Vec<(Source, anyhow::Result<String>)> {
    sources
        .iter()
        .map(|source| (source.clone(), read_input(source, stdin)))
        .collect()
}

/// Reports a read failure. Returns the exit code the caller should fold in.
#[must_use]
pub fn report_io_error(error: &anyhow::Error, stderr: &mut dyn Write) -> u8 {
    let _ = writeln!(stderr, "error: {error:#}");
    exit::IO
}

/// A parse failure and the document it came from.
///
/// Grouped into a struct rather than passed as eight positional arguments.
/// Clippy's `too_many_arguments` rejects the flat form outright at
/// `-D warnings`, and the flat form was genuinely hazardous: `line` and
/// `column` are both `u32` and adjacent, so transposing them compiled cleanly
/// and silently misplaced every caret. Named fields make that impossible.
pub struct ParseFailure<'a> {
    /// One-line description of what went wrong.
    pub message: String,
    /// 1-based line. `0` means unknown.
    pub line: u32,
    /// 1-based column **in bytes**. `0` means unknown.
    pub column: u32,
    /// Where the document came from.
    pub source: &'a Source,
    /// The document text, for the echoed source line.
    pub text: &'a str,
}

/// Reports a parse failure as a caret diagnostic. Returns the exit code.
#[must_use]
pub fn report_parse_error(
    failure: ParseFailure<'_>,
    options: RenderOptions,
    json: bool,
    stderr: &mut dyn Write,
) -> u8 {
    let report = Report::builder(Severity::Error, failure.message)
        .file(failure.source.path())
        .location(failure.line, failure.column)
        .build();
    let _ = if json {
        render_json(&report, stderr)
    } else {
        render(&report, Some(failure.text), options, stderr)
    };
    exit::FINDINGS
}

/// Writes the closing summary. An empty match warns rather than passing in
/// silence, which would read as "all good".
pub fn summary(stderr: &mut dyn Write, files: usize, findings: usize, changed: usize) {
    if files == 0 {
        let _ = writeln!(stderr, "warning: no files matched; nothing to do");
        return;
    }
    let _ = writeln!(
        stderr,
        "{files} file(s) · {findings} finding(s) · {changed} changed"
    );
}
```

Add `pub mod runner;` to `crates/glaucus/src/cli/mod.rs`.

Add to `crates/glaucus/src/cli/io.rs`, so `runner` can build a `Report`
without matching on `Source` at every call site:

```rust
impl Source {
    /// The path, when this source is a file.
    #[must_use]
    pub fn path(&self) -> Option<std::path::PathBuf> {
        match self {
            Self::Stdin => None,
            Self::File(path) => Some(path.clone()),
        }
    }
}
```

with its test in `io.rs`:

```rust
    #[test]
    fn path_is_some_only_for_files() {
        assert_eq!(Source::Stdin.path(), None);
        assert_eq!(Source::File("x.yaml".into()).path(), Some("x.yaml".into()));
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cli::`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 5: Refactor**

Confirm every function here is under 30 lines and the file is under 300. If
`report_parse_error`'s parameter list feels wide, that is acceptable: it is a
single call-site shape used identically by four commands, and collapsing it
into a struct would add a type nobody else needs (YAGNI).

- [ ] **Step 6: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S --amend --no-edit
```

---

### Task 4: Move `fmt` into the library and add the `fmt` command

**Files:**

- Create: `crates/glaucus/src/fmt.rs` (from `crates/glaucus-fmt/src/lib.rs`)
- Create: `crates/glaucus/src/cli/cmd/fmt.rs`
- Modify: `crates/glaucus/src/lib.rs`, `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

- Consumes: `io::{Source, read_input, write_atomic}`, `diag::{Report, Severity, RenderOptions, render}`, `exit::*`.
- Produces: `glaucus::fmt::format_str(&str) -> crate::error::Result<String>`; `cmd::fmt::{FmtArgs, run}`.

**Note on the moved signature.** `glaucus_fmt::format_str` returned `Result<String, String>`, which throws away the span. It becomes `crate::error::Result<String>` so the CLI can render a caret. The crate was never published, so this breaks nothing.

- [ ] **Step 1: Write the failing tests**

Create `crates/glaucus/src/fmt.rs` with the moved implementation's tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_trailing_whitespace() {
        assert_eq!(format_str("a: 1   \nb: 2\n").unwrap(), "a: 1\nb: 2\n");
    }

    #[test]
    fn rejects_invalid_yaml() {
        assert!(format_str("a: [1, 2").is_err());
    }

    #[test]
    fn idempotent() {
        let once = format_str("x: y  \n").unwrap();
        assert_eq!(format_str(&once).unwrap(), once);
    }

    #[test]
    fn preserves_comments() {
        assert_eq!(format_str("a: 1  # c\n").unwrap(), "a: 1  # c\n");
    }
}
```

Add to `crates/glaucus/src/cli/cmd/fmt.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::cli::tests::drive;
    use crate::cli::exit;

    fn tmp(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("glaucus-cmd-fmt");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn stdin_is_formatted_to_stdout() {
        let (code, out, _err) = drive(&["glaucus", "fmt"], "a: 1   \n");
        assert_eq!(code, exit::OK);
        assert_eq!(out, "a: 1\n");
    }

    #[test]
    fn check_reports_findings_and_names_the_file() {
        let p = tmp("dirty.yaml", "a: 1   \n");
        let (code, out, err) = drive(&["glaucus", "fmt", "--check", p.to_str().unwrap()], "");
        assert_eq!(code, exit::FINDINGS);
        assert!(out.is_empty());
        assert!(err.contains("dirty.yaml"), "file not named:\n{err}");
    }

    #[test]
    fn check_on_clean_file_succeeds() {
        let p = tmp("clean.yaml", "a: 1\n");
        let (code, _out, _err) = drive(&["glaucus", "fmt", "--check", p.to_str().unwrap()], "");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn write_rewrites_the_file() {
        let p = tmp("write.yaml", "a: 1   \n");
        let (code, _out, _err) = drive(&["glaucus", "fmt", "--write", p.to_str().unwrap()], "");
        assert_eq!(code, exit::OK);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "a: 1\n");
    }

    #[test]
    fn check_and_write_together_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "fmt", "--check", "--write"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("cannot be used with"));
    }

    #[test]
    fn invalid_yaml_produces_a_diagnostic_and_findings() {
        let p = tmp("bad.yaml", "a: [1, 2\n");
        let (code, _out, err) = drive(&["glaucus", "fmt", "--check", p.to_str().unwrap()], "");
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("error:"), "no diagnostic:\n{err}");
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let (code, _out, err) = drive(&["glaucus", "fmt", "no-such-file.yaml"], "");
        assert_eq!(code, exit::IO);
        assert!(err.contains("no-such-file.yaml"));
    }

    #[test]
    fn empty_match_warns_rather_than_silently_succeeding() {
        let (code, _out, err) = drive(&["glaucus", "fmt", "--check", "--", ], "");
        assert_eq!(code, exit::OK);
        assert!(err.contains("no files"), "silent no-op:\n{err}");
    }

    #[test]
    fn format_alias_works() {
        let (code, out, _err) = drive(&["glaucus", "format"], "a: 1   \n");
        assert_eq!(code, exit::OK);
        assert_eq!(out, "a: 1\n");
    }

    #[test]
    fn f_alias_works() {
        let (code, out, _err) = drive(&["glaucus", "f"], "a: 1   \n");
        assert_eq!(code, exit::OK);
        assert_eq!(out, "a: 1\n");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli fmt`

Expected: FAIL — `crate::fmt` and the `fmt` subcommand do not exist.

- [ ] **Step 3: Implement `crates/glaucus/src/fmt.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe YAML formatting: trailing-whitespace trim plus a single final newline.

/// Formats `src`, preserving comments, indentation and scalar content.
///
/// # Errors
///
/// Returns the parse error if `src` is not valid YAML — a formatter must not
/// touch unparseable input. The error carries a span, so callers can point at
/// the offending line and column.
pub fn format_str(src: &str) -> crate::error::Result<String> {
    crate::from_str_node(src)?;
    Ok(crate::cst::Document::parse(src).reformatted())
}
```

Add to `crates/glaucus/src/lib.rs`:

```rust
#[cfg(all(feature = "ast", feature = "cst"))]
pub mod fmt;
```

- [ ] **Step 4: Implement `crates/glaucus/src/cli/cmd/fmt.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus fmt` — format YAML, comment-preserving.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::{Source, write_atomic};
use crate::cli::runner;
use std::io::Write;
use std::path::PathBuf;

/// Arguments for `glaucus fmt`.
#[derive(clap::Args, Debug)]
pub struct FmtArgs {
    /// Files to format. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// Exit 1 if any file is not already formatted.
    #[arg(long, conflicts_with = "write")]
    pub check: bool,
    /// Overwrite each file with its formatted output.
    #[arg(long)]
    pub write: bool,
}

/// Runs the command, returning the exit code.
#[must_use]
pub fn run(args: &FmtArgs, env: &mut Env<'_>, options: RenderOptions) -> u8 {
    if args.files.is_empty() && (args.check || args.write) {
        let _ = writeln!(env.stderr, "warning: no files matched; nothing to do");
        return exit::OK;
    }
    let sources = runner::resolve_sources(&args.files);
    let documents = runner::read_all(&sources, env.stdin);

    let mut worst = exit::OK;
    let mut changed = 0usize;
    let mut findings = 0usize;
    for (source, text) in &documents {
        let code = match text {
            Err(error) => runner::report_io_error(error, env.stderr),
            Ok(text) => format_one(args, source, text, env, options, &mut changed),
        };
        if code == exit::FINDINGS {
            findings += 1;
        }
        worst = worst.max(code);
    }
    runner::summary(env.stderr, documents.len(), findings, changed);
    worst
}

/// Formats one document, or reports why it could not be parsed.
fn format_one(
    args: &FmtArgs,
    source: &Source,
    text: &str,
    env: &mut Env<'_>,
    options: RenderOptions,
    changed: &mut usize,
) -> u8 {
    match crate::fmt::format_str(text) {
        Ok(formatted) => emit(args, source, text, &formatted, env, changed),
        Err(error) => {
            let span = error.span();
            runner::report_parse_error(
                runner::ParseFailure {
                    message: error.to_string(),
                    line: span.map_or(0, |span| span.start.line),
                    column: span.map_or(0, |span| span.start.column),
                    source,
                    text,
                },
                options,
                false,
                env.stderr,
            )
        }
    }
}

/// Delivers the formatted text per `--check` / `--write` / the stdout default.
fn emit(
    args: &FmtArgs,
    source: &Source,
    original: &str,
    formatted: &str,
    env: &mut Env<'_>,
    changed: &mut usize,
) -> u8 {
    let unchanged = formatted == original;
    if args.check {
        if unchanged {
            return exit::OK;
        }
        let _ = writeln!(env.stderr, "would reformat {}", source.label());
        return exit::FINDINGS;
    }
    if args.write {
        let Some(path) = source.path().filter(|_| !unchanged) else {
            return exit::OK;
        };
        if let Err(error) = write_atomic(&path, formatted) {
            return runner::report_io_error(&error, env.stderr);
        }
        *changed += 1;
        return exit::OK;
    }
    let _ = write!(env.stdout, "{formatted}");
    exit::OK
}
```

Replace `crates/glaucus/src/cli/cmd/mod.rs`:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subcommands.

pub mod fmt;

use crate::cli::diag::RenderOptions;
use crate::cli::env::{ColorChoice, Env};

/// The subcommand set.
#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Format YAML, comment-preserving.
    #[command(visible_alias = "format", alias = "f")]
    Fmt(fmt::FmtArgs),
}

/// Runs the selected command.
pub fn dispatch(command: Command, mut env: Env<'_>) -> u8 {
    let options = RenderOptions {
        color: env.color == ColorChoice::Always
            || (env.color == ColorChoice::Auto),
        show_source: !env.is_ci,
    };
    match command {
        Command::Fmt(args) => fmt::run(&args, &mut env, options),
    }
}
```

If `crate::error::Error` has no `span()` accessor, add one in the same commit:

```rust
impl Error {
    /// The source span, when the error carries one.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
}
```

Match the existing field name; if the field is not `Option<Span>`, return it directly and drop the `map_or` in the caller.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli`

Expected: PASS, 14 new tests.

- [ ] **Step 6: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/
git commit -S -m "feat(cli): add fmt subcommand and move format_str into glaucus"
```

---

### Task 5: Move `validate` into the library and add the `validate` command

**Files:**

- Create: `crates/glaucus/src/validate.rs` (from `crates/glaucus-validate/src/lib.rs`)
- Create: `crates/glaucus/src/cli/cmd/validate.rs`
- Modify: `crates/glaucus/src/lib.rs`, `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

- Produces: `glaucus::validate::{Diagnostic, validate_str, fix_str}`; `cmd::validate::{ValidateArgs, run}`.

- [ ] **Step 1: Write the failing tests**

Move the eight existing tests from `crates/glaucus-validate/src/lib.rs` verbatim into `crates/glaucus/src/validate.rs`, changing `glaucus::` to `crate::` throughout. Then add to `crates/glaucus/src/cli/cmd/validate.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    fn tmp(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("glaucus-cmd-validate");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    const SCHEMA: &str = "type: object\nproperties:\n  port: {type: integer}\n";

    #[test]
    fn valid_document_succeeds() {
        let s = tmp("s1.yaml", SCHEMA);
        let d = tmp("d1.yaml", "port: 8080\n");
        let (code, _out, _err) = drive(
            &["glaucus", "validate", "-s", s.to_str().unwrap(), d.to_str().unwrap()],
            "",
        );
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn type_error_renders_a_caret_diagnostic() {
        let s = tmp("s2.yaml", SCHEMA);
        let d = tmp("d2.yaml", "port: \"8080\"\n");
        let (code, _out, err) = drive(
            &["glaucus", "validate", "-s", s.to_str().unwrap(), d.to_str().unwrap()],
            "",
        );
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("error:"), "no diagnostic:\n{err}");
        assert!(err.contains("d2.yaml:1:"), "no location:\n{err}");
    }

    #[test]
    fn fix_coerces_and_writes() {
        let s = tmp("s3.yaml", SCHEMA);
        let d = tmp("d3.yaml", "port: \"80\"  # c\n");
        let (code, _out, _err) = drive(
            &["glaucus", "validate", "-s", s.to_str().unwrap(), "--fix", d.to_str().unwrap()],
            "",
        );
        assert_eq!(code, exit::OK);
        assert_eq!(std::fs::read_to_string(&d).unwrap(), "port: 80  # c\n");
    }

    #[test]
    fn no_source_suppresses_the_echoed_line() {
        let s = tmp("s4.yaml", "type: object\nproperties:\n  pw: {type: integer}\n");
        let d = tmp("d4.yaml", "pw: hunter2\n");
        let (code, _out, err) = drive(
            &[
                "glaucus", "validate", "--no-source", "-s",
                s.to_str().unwrap(), d.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::FINDINGS);
        assert!(!err.contains("hunter2"), "secret leaked:\n{err}");
    }

    #[test]
    fn json_format_emits_parseable_objects() {
        let s = tmp("s5.yaml", SCHEMA);
        let d = tmp("d5.yaml", "port: \"x\"\n");
        let (code, _out, err) = drive(
            &[
                "glaucus", "validate", "--format", "json", "-s",
                s.to_str().unwrap(), d.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::FINDINGS);
        let line = err.lines().find(|l| l.starts_with('{')).expect("no json");
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["severity"], "error");
    }

    #[test]
    fn missing_schema_flag_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "validate", "x.yaml"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("--schema") || err.contains("required"));
    }

    #[test]
    fn check_and_val_aliases_work() {
        let s = tmp("s6.yaml", SCHEMA);
        let d = tmp("d6.yaml", "port: 1\n");
        for alias in ["check", "val"] {
            let (code, _o, _e) = drive(
                &["glaucus", alias, "-s", s.to_str().unwrap(), d.to_str().unwrap()],
                "",
            );
            assert_eq!(code, exit::OK, "alias {alias} failed");
        }
    }

    #[test]
    fn continue_on_error_processes_every_file() {
        let s = tmp("s7.yaml", SCHEMA);
        let bad = tmp("bad7.yaml", "port: \"x\"\n");
        let good = tmp("good7.yaml", "port: 1\n");
        let (code, _out, err) = drive(
            &[
                "glaucus", "validate", "-s", s.to_str().unwrap(),
                bad.to_str().unwrap(), good.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("2 file(s)"), "did not process both:\n{err}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli validate`

Expected: FAIL — `crate::validate` and the subcommand do not exist.

- [ ] **Step 3: Create `crates/glaucus/src/validate.rs`**

Copy `crates/glaucus-validate/src/lib.rs` verbatim, then replace every `glaucus::` with `crate::` (there are 7 occurrences in the non-test body: `from_str_node` ×3, `schema::Schema` ×2, `schema::validate`, `cst::Document`, `schema::coerce_to_schema`, `schema::apply_defaults`). Add to `crates/glaucus/src/lib.rs`:

```rust
#[cfg(feature = "schema")]
pub mod validate;
```

- [ ] **Step 4: Implement `crates/glaucus/src/cli/cmd/validate.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus validate` — JSON-Schema validation with optional autofix.

use crate::cli::diag::{RenderOptions, Report, Severity, render, render_json};
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::{Source, read_input, write_atomic};
use crate::cli::runner;
use std::io::Write;
use std::path::PathBuf;

/// Arguments for `glaucus validate`.
#[derive(clap::Args, Debug)]
pub struct ValidateArgs {
    /// Documents to validate. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// JSON-Schema document to validate against.
    #[arg(short = 's', long = "schema")]
    pub schema: PathBuf,
    /// Apply comment-preserving autofix and write the result back.
    #[arg(long)]
    pub fix: bool,
}

/// Runs the command, returning the exit code.
#[must_use]
pub fn run(args: &ValidateArgs, env: &mut Env<'_>, options: RenderOptions, json: bool) -> u8 {
    let schema = match read_input(&Source::File(args.schema.clone()), env.stdin) {
        Ok(schema) => schema,
        Err(error) => return runner::report_io_error(&error, env.stderr),
    };
    let sources = runner::resolve_sources(&args.files);
    let documents = runner::read_all(&sources, env.stdin);

    let mut worst = exit::OK;
    let mut findings = 0usize;
    let mut changed = 0usize;
    for (source, text) in &documents {
        worst = worst.max(match text {
            Err(error) => runner::report_io_error(error, env.stderr),
            Ok(text) if args.fix => fix_one(source, text, &schema, env, &mut changed),
            Ok(text) => check_one(source, text, &schema, env, options, json, &mut findings),
        });
    }
    runner::summary(env.stderr, documents.len(), findings, changed);
    worst
}

/// Validates one document, rendering every diagnostic it produces.
fn check_one(
    source: &Source,
    text: &str,
    schema: &str,
    env: &mut Env<'_>,
    options: RenderOptions,
    json: bool,
    findings: &mut usize,
) -> u8 {
    let diagnostics = crate::validate::validate_str(text, schema);
    if diagnostics.is_empty() {
        return exit::OK;
    }
    *findings += diagnostics.len();
    for diagnostic in diagnostics {
        let mut builder = Report::builder(Severity::Error, diagnostic.message)
            .file(source.path())
            .location(diagnostic.line, diagnostic.column)
            .help("run with --fix to coerce");
        if !diagnostic.path.is_empty() {
            builder = builder.path(diagnostic.path);
        }
        let report = builder.build();
        let _ = if json {
            render_json(&report, env.stderr)
        } else {
            render(&report, Some(text), options, env.stderr)
        };
    }
    exit::FINDINGS
}

/// Applies comment-preserving autofix to one document.
fn fix_one(
    source: &Source,
    text: &str,
    schema: &str,
    env: &mut Env<'_>,
    changed: &mut usize,
) -> u8 {
    let (fixed, summary) = crate::validate::fix_str(text, schema);
    let _ = writeln!(env.stderr, "{}: {summary}", source.label());
    let Some(path) = source.path() else {
        let _ = write!(env.stdout, "{fixed}");
        return exit::OK;
    };
    if fixed == text {
        return exit::OK;
    }
    if let Err(error) = write_atomic(&path, &fixed) {
        return runner::report_io_error(&error, env.stderr);
    }
    *changed += 1;
    exit::OK
}
```

Extend `Command` in `cmd/mod.rs`:

```rust
    /// Validate YAML against a JSON-Schema.
    #[command(visible_alias = "check", alias = "val")]
    Validate(validate::ValidateArgs),
```

and the `dispatch` match arm:

```rust
        Command::Validate(args) => validate::run(&args, &mut env, options, json),
```

`dispatch` now needs the `json` flag; see Task 12 for the global-args plumbing. Until then, thread a `json: bool` parameter through `dispatch` from `run_with`, defaulting to `false`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli`

Expected: PASS.

- [ ] **Step 6: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/
git commit -S -m "feat(cli): add validate subcommand and move validate API into glaucus"
```

---

### Task 6: `parse` command

**Files:**

- Create: `crates/glaucus/src/cli/cmd/parse.rs`
- Modify: `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

- Produces: `cmd::parse::{ParseArgs, Emit, run}`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn emits_events_by_default() {
        let (code, out, _err) = drive(&["glaucus", "parse"], "a: 1\n");
        assert_eq!(code, exit::OK);
        assert!(out.contains("MappingStart"), "no events:\n{out}");
    }

    #[test]
    fn emits_json() {
        let (code, out, _err) = drive(&["glaucus", "parse", "--emit", "json"], "a: 1\n");
        assert_eq!(code, exit::OK);
        let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn emits_cst() {
        let (code, out, _err) = drive(&["glaucus", "parse", "--emit", "cst"], "a: 1 # c\n");
        assert_eq!(code, exit::OK);
        assert!(!out.is_empty());
    }

    #[test]
    fn emits_ast() {
        let (code, out, _err) = drive(&["glaucus", "parse", "--emit", "ast"], "a: 1\n");
        assert_eq!(code, exit::OK);
        assert!(!out.is_empty());
    }

    #[test]
    fn invalid_yaml_is_a_finding_with_a_diagnostic() {
        let (code, _out, err) = drive(&["glaucus", "parse"], "a: [1, 2\n");
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("error:"));
    }

    #[test]
    fn bad_emit_value_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "parse", "--emit", "nope"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("invalid value"));
    }

    #[test]
    fn dump_and_p_aliases_work() {
        for alias in ["dump", "p"] {
            let (code, _o, _e) = drive(&["glaucus", alias], "a: 1\n");
            assert_eq!(code, exit::OK, "alias {alias} failed");
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cmd::parse`

Expected: FAIL — subcommand undefined.

- [ ] **Step 3: Implement `crates/glaucus/src/cli/cmd/parse.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus parse` — inspect the parse pipeline.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::Source;
use crate::cli::runner;
use std::io::Write;
use std::path::PathBuf;

/// What representation to print.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emit {
    /// The SAX-style event stream.
    Events,
    /// The abstract syntax tree.
    Ast,
    /// The lossless concrete syntax tree.
    Cst,
    /// JSON equivalent of the document.
    Json,
}

/// Arguments for `glaucus parse`.
#[derive(clap::Args, Debug)]
pub struct ParseArgs {
    /// Files to parse. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// Representation to print.
    #[arg(long, value_enum, default_value_t = Emit::Events)]
    pub emit: Emit,
}

/// Runs the command, returning the exit code.
#[must_use]
pub fn run(args: &ParseArgs, env: &mut Env<'_>, options: RenderOptions) -> u8 {
    let sources = runner::resolve_sources(&args.files);
    let documents = runner::read_all(&sources, env.stdin);

    let mut worst = exit::OK;
    let mut findings = 0usize;
    for (source, text) in &documents {
        let code = match text {
            Err(error) => runner::report_io_error(error, env.stderr),
            Ok(text) => parse_one(text, args.emit, source, env, options),
        };
        if code == exit::FINDINGS {
            findings += 1;
        }
        worst = worst.max(code);
    }
    runner::summary(env.stderr, documents.len(), findings, 0);
    worst
}

/// Emits one document in the requested representation, or reports why it could
/// not be parsed.
fn parse_one(
    text: &str,
    emit: Emit,
    source: &Source,
    env: &mut Env<'_>,
    options: RenderOptions,
) -> u8 {
    match emit_one(text, emit, env) {
        Ok(()) => exit::OK,
        Err(error) => {
            let span = error.span();
            runner::report_parse_error(
                runner::ParseFailure {
                    message: error.to_string(),
                    line: span.map_or(0, |span| span.start.line),
                    column: span.map_or(0, |span| span.start.column),
                    source,
                    text,
                },
                options,
                false,
                env.stderr,
            )
        }
    }
}

fn emit_one(text: &str, emit: Emit, env: &mut Env<'_>) -> crate::error::Result<()> {
    match emit {
        Emit::Events => {
            let mut parser = crate::parser::Parser::new(text);
            while let Some(event) = parser.next_event() {
                let event = event?;
                let _ = writeln!(env.stdout, "{:?}", event.kind);
            }
        }
        Emit::Ast => {
            let node = crate::from_str_node(text)?;
            let _ = writeln!(env.stdout, "{node:#?}");
        }
        Emit::Cst => {
            let doc = crate::cst::Document::parse(text);
            let _ = writeln!(env.stdout, "{doc:#?}");
        }
        Emit::Json => {
            let value: serde_json::Value = crate::from_str(text)
                .map_err(|_| crate::error::Error::spanless(
                    crate::error::ErrorKind::UnexpectedEof,
                ))?;
            let _ = writeln!(env.stdout, "{value}");
        }
    }
    Ok(())
}
```

If `Emit::Json`'s error mapping does not compile because `from_str` returns a different error type, convert with `.map_err(|e| ...)` into the same `Report` shape used above rather than into `crate::error::Error`; keep the caret only when a span is available.

Extend `Command` and `dispatch` in `cmd/mod.rs`:

```rust
    /// Inspect the parse pipeline.
    #[command(visible_alias = "dump", alias = "p")]
    Parse(parse::ParseArgs),
```

```rust
        Command::Parse(args) => parse::run(&args, &mut env, options),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cmd::parse`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S -m "feat(cli): add parse subcommand"
```

---

### Task 7: `convert` command

**Files:**

- Create: `crates/glaucus/src/cli/cmd/convert.rs`
- Modify: `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

- Produces: `cmd::convert::{ConvertArgs, Target, run}`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn yaml_to_json_is_the_default() {
        let (code, out, _err) = drive(&["glaucus", "convert"], "a: 1\n");
        assert_eq!(code, exit::OK);
        let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn json_to_yaml() {
        let (code, out, _err) = drive(&["glaucus", "convert", "--to", "yaml"], "{\"a\":1}");
        assert_eq!(code, exit::OK);
        assert!(out.contains("a: 1"), "not yaml:\n{out}");
    }

    #[test]
    fn nothing_is_written_to_stderr_on_success() {
        let (_code, _out, err) = drive(&["glaucus", "convert"], "a: 1\n");
        assert!(!err.contains("error:"));
    }

    #[test]
    fn invalid_input_is_a_finding() {
        let (code, _out, err) = drive(&["glaucus", "convert"], "a: [1, 2\n");
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("error:"));
    }

    #[test]
    fn to_alias_works() {
        let (code, _o, _e) = drive(&["glaucus", "to"], "a: 1\n");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn conv_alias_works() {
        let (code, _o, _e) = drive(&["glaucus", "conv"], "a: 1\n");
        assert_eq!(code, exit::OK);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cmd::convert`

Expected: FAIL — subcommand undefined.

- [ ] **Step 3: Implement `crates/glaucus/src/cli/cmd/convert.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus convert` — YAML to JSON and back.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::Source;
use crate::cli::runner;
use std::io::Write;
use std::path::PathBuf;

/// Output language.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// JSON output.
    Json,
    /// YAML output.
    Yaml,
}

/// Arguments for `glaucus convert`.
#[derive(clap::Args, Debug)]
pub struct ConvertArgs {
    /// Files to convert. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// Language to convert to.
    #[arg(long, value_enum, default_value_t = Target::Json)]
    pub to: Target,
}

/// Runs the command, returning the exit code.
#[must_use]
pub fn run(args: &ConvertArgs, env: &mut Env<'_>, options: RenderOptions) -> u8 {
    let sources = runner::resolve_sources(&args.files);
    let documents = runner::read_all(&sources, env.stdin);

    let mut worst = exit::OK;
    let mut findings = 0usize;
    for (source, text) in &documents {
        let code = match text {
            Err(error) => runner::report_io_error(error, env.stderr),
            Ok(text) => convert_one(text, args.to, source, env, options),
        };
        if code == exit::FINDINGS {
            findings += 1;
        }
        worst = worst.max(code);
    }
    runner::summary(env.stderr, documents.len(), findings, 0);
    worst
}

/// Converts one document into `target`, or reports why it could not be read.
fn convert_one(
    text: &str,
    target: Target,
    source: &Source,
    env: &mut Env<'_>,
    options: RenderOptions,
) -> u8 {
    let value: serde_json::Value = match crate::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return runner::report_parse_error(
                runner::ParseFailure {
                    message: error.to_string(),
                    line: 0,
                    column: 0,
                    source,
                    text,
                },
                options,
                false,
                env.stderr,
            );
        }
    };
    let rendered = match target {
        Target::Json => format!("{value}\n"),
        Target::Yaml => match crate::to_string(&value) {
            Ok(yaml) => yaml,
            Err(error) => {
                return runner::report_parse_error(
                    runner::ParseFailure {
                        message: error.to_string(),
                        line: 0,
                        column: 0,
                        source,
                        text,
                    },
                    options,
                    false,
                    env.stderr,
                );
            }
        },
    };
    let _ = write!(env.stdout, "{rendered}");
    exit::OK
}
```

Extend `Command` and `dispatch`:

```rust
    /// Convert between YAML and JSON.
    #[command(visible_alias = "to", alias = "conv")]
    Convert(convert::ConvertArgs),
```

```rust
        Command::Convert(args) => convert::run(&args, &mut env, options),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cmd::convert`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S -m "feat(cli): add convert subcommand"
```

---

### Task 8: `schema check` command

**Files:**

- Create: `crates/glaucus/src/cli/cmd/schema.rs`
- Modify: `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

- Produces: `cmd::schema::{SchemaArgs, SchemaCommand, run}`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    fn tmp(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("glaucus-cmd-schema");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn well_formed_schema_passes() {
        let p = tmp("ok.yaml", "type: object\nproperties:\n  a: {type: integer}\n");
        let (code, _out, _err) = drive(&["glaucus", "schema", "check", p.to_str().unwrap()], "");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn unparseable_schema_is_a_finding() {
        let p = tmp("bad.yaml", "type: [object\n");
        let (code, _out, err) = drive(&["glaucus", "schema", "check", p.to_str().unwrap()], "");
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("error:"));
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let (code, _out, err) = drive(&["glaucus", "schema", "check", "nope.yaml"], "");
        assert_eq!(code, exit::IO);
        assert!(err.contains("nope.yaml"));
    }

    #[test]
    fn sch_alias_works() {
        let p = tmp("ok2.yaml", "type: object\n");
        let (code, _o, _e) = drive(&["glaucus", "sch", "check", p.to_str().unwrap()], "");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn missing_sub_subcommand_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "schema"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("Usage:"));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cmd::schema`

Expected: FAIL — subcommand undefined.

- [ ] **Step 3: Implement `crates/glaucus/src/cli/cmd/schema.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus schema` — work with JSON-Schema documents.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::{Source, read_input};
use crate::cli::runner;
use std::io::Write;
use std::path::PathBuf;

/// Arguments for `glaucus schema`.
#[derive(clap::Args, Debug)]
pub struct SchemaArgs {
    #[command(subcommand)]
    pub command: SchemaCommand,
}

/// Schema operations.
#[derive(clap::Subcommand, Debug)]
pub enum SchemaCommand {
    /// Check that a schema document is well-formed.
    Check {
        /// The schema file.
        file: PathBuf,
    },
}

/// Runs the command, returning the exit code.
#[must_use]
pub fn run(args: &SchemaArgs, env: &mut Env<'_>, options: RenderOptions) -> u8 {
    let SchemaCommand::Check { file } = &args.command;
    let source = Source::File(file.clone());
    let text = match read_input(&source, env.stdin) {
        Ok(text) => text,
        Err(error) => return runner::report_io_error(&error, env.stderr),
    };
    match crate::from_str_node(&text) {
        Ok(node) => {
            let _ = crate::schema::Schema::from_node(&node);
            let _ = writeln!(env.stderr, "{}: ok", source.label());
            exit::OK
        }
        Err(error) => {
            let span = error.span();
            runner::report_parse_error(
                runner::ParseFailure {
                    message: error.to_string(),
                    line: span.map_or(0, |span| span.start.line),
                    column: span.map_or(0, |span| span.start.column),
                    source: &source,
                    text: &text,
                },
                options,
                false,
                env.stderr,
            )
        }
    }
}
```

Extend `Command` and `dispatch`:

```rust
    /// Work with JSON-Schema documents.
    #[command(alias = "sch")]
    Schema(schema::SchemaArgs),
```

```rust
        Command::Schema(args) => schema::run(&args, &mut env, options),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cmd::schema`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S -m "feat(cli): add schema check subcommand"
```

---

### Task 9: Move the LSP server and add the `lsp` command

**Files:**

- Create: `crates/glaucus/src/lsp.rs` (from `crates/glaucus-lsp/src/lib.rs`)
- Create: `crates/glaucus/src/cli/cmd/lsp.rs`
- Modify: `crates/glaucus/src/lib.rs`, `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

- Produces: `glaucus::lsp::*` (same public items the old crate exported); `cmd::lsp::run`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn empty_stdin_exits_cleanly() {
        let (code, _out, _err) = drive(&["glaucus", "lsp"], "");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn a_single_request_is_answered_on_stdout() {
        // Content-Length framing, initialize request.
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let msg = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        let (code, out, _err) = drive(&["glaucus", "lsp"], &msg);
        assert_eq!(code, exit::OK);
        assert!(out.contains("Content-Length:"), "no framed reply:\n{out}");
    }
}
```

Adjust the framing to match whatever `glaucus-lsp`'s existing `main.rs` reads; copy that logic rather than inventing it.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p glaucus --features cli cmd::lsp`

Expected: FAIL — subcommand undefined.

- [ ] **Step 3: Move the library and implement the command**

Copy `crates/glaucus-lsp/src/lib.rs` to `crates/glaucus/src/lsp.rs`, replacing `glaucus::` with `crate::`. Add to `lib.rs`:

```rust
#[cfg(feature = "cli")]
pub mod lsp;
```

Create `crates/glaucus/src/cli/cmd/lsp.rs`, moving the loop from `crates/glaucus-lsp/src/main.rs` and replacing its direct use of `std::io::stdin()`/`stdout()` with `env.stdin` and `env.stdout`:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus lsp` — language server over stdio.

use crate::cli::env::Env;
use crate::cli::exit;

/// Runs the server until stdin closes.
pub fn run(env: &mut Env<'_>) -> u8 {
    crate::lsp::serve(env.stdin, env.stdout);
    exit::OK
}
```

If `glaucus-lsp` has no `serve` entry point, add one in `lsp.rs` with the signature `pub fn serve(input: &mut dyn BufRead, output: &mut dyn Write)` containing the body of the old `main`, so the loop becomes testable.

Extend `Command` and `dispatch`:

```rust
    /// Run the language server over stdio.
    Lsp,
```

```rust
        Command::Lsp => lsp::run(&mut env),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli`

Expected: PASS. All tests moved from `glaucus-lsp` must also pass in their new location.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/
git commit -S -m "feat(cli): add lsp subcommand and move the server into glaucus"
```

---

### Task 10: Move the MCP server and add the `mcp` command

**Files:**

- Create: `crates/glaucus/src/mcp.rs` (from `crates/glaucus-mcp/src/lib.rs`)
- Create: `crates/glaucus/src/cli/cmd/mcp.rs`
- Modify: `crates/glaucus/src/lib.rs`, `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

- Produces: `glaucus::mcp::{Server, Outgoing, frame}`; `cmd::mcp::run`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn empty_stdin_exits_cleanly() {
        let (code, _out, _err) = drive(&["glaucus", "mcp"], "");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn blank_lines_are_skipped() {
        let (code, out, _err) = drive(&["glaucus", "mcp"], "\n\n");
        assert_eq!(code, exit::OK);
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_json_produces_an_error_object() {
        let (code, out, _err) = drive(&["glaucus", "mcp"], "not json\n");
        assert_eq!(code, exit::OK);
        assert!(out.contains("error"), "no error object:\n{out}");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p glaucus --features cli cmd::mcp`

Expected: FAIL — subcommand undefined.

- [ ] **Step 3: Move the library and implement the command**

Copy `crates/glaucus-mcp/src/lib.rs` to `crates/glaucus/src/mcp.rs`, replacing `glaucus::` with `crate::`. Add to `lib.rs`:

```rust
#[cfg(feature = "cli")]
pub mod mcp;
```

Create `crates/glaucus/src/cli/cmd/mcp.rs`, moving the loop from `crates/glaucus-mcp/src/main.rs` and reading from `env.stdin` / writing to `env.stdout` instead of the real process streams:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus mcp` — MCP server over stdio.

use crate::cli::env::Env;
use crate::cli::exit;
use serde_json::Value;
use std::io::{BufRead, Write};

/// Runs the server until stdin closes or an `exit` request arrives.
pub fn run(env: &mut Env<'_>) -> u8 {
    let mut server = crate::mcp::Server::new();
    let mut line = String::new();
    loop {
        line.clear();
        match env.stdin.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if handle_line(&mut server, trimmed, env.stdout) == Flow::Exit {
            break;
        }
    }
    exit::OK
}
```

**Port `crates/glaucus-mcp/src/main.rs`'s body — do not reinvent it.** These APIs
were verified against the source:

- `Server::new()` exists and is a `const fn`.
- `Server::handle(&mut self, method: &str, id: Option<Value>, params: &Value) -> Vec<Outgoing>` — four arguments, returning a **Vec**, not an `Option`.
- `Outgoing` has two variants, `Response { id, result }` and `Error { id, code, message }`, each mapping to a `serde_json::json!` object carrying `"jsonrpc": "2.0"`.
- Output is written with `frame(&json.to_string())`, **not** `writeln!`. Reads are line-based (`read_line`); writes are `Content-Length`-framed. That asymmetry is deliberate — preserve it.
- `method == "exit"` breaks the loop after its responses are flushed.
- A malformed line emits a `-32700` parse error with `"id": null` and continues.

The original `main` is 64 lines, so split it: `run` holds the loop, `handle_line` does parse-dispatch-write and returns whether to exit, and `write_outgoing` maps one `Outgoing` to framed JSON. That keeps every function under the 30-line limit. Define `Flow { Continue, Exit }` locally.

Extend `Command` and `dispatch`:

```rust
    /// Run the MCP server over stdio.
    Mcp,
```

```rust
        Command::Mcp => mcp::run(&mut env),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli`

Expected: PASS. All tests moved from `glaucus-mcp` must also pass.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/
git commit -S -m "feat(cli): add mcp subcommand and move the server into glaucus"
```

---

### Task 11: `completions` command

**Files:**

- Create: `crates/glaucus/src/cli/cmd/completions.rs`
- Modify: `crates/glaucus/src/cli/cmd/mod.rs`, `crates/glaucus/Cargo.toml`

**Interfaces:**

- Produces: `cmd::completions::{CompletionsArgs, run}`.

- [ ] **Step 1: Add `clap_complete` to the workspace and the `cli` feature**

Root `Cargo.toml`:

```toml
clap_complete = { version = ">=4,<5" }
```

`crates/glaucus/Cargo.toml`: add `clap_complete = { workspace = true, optional = true }` and `"dep:clap_complete"` to the `cli` feature. Re-run `cargo deny check`.

- [ ] **Step 2: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn bash_completions_go_to_stdout() {
        let (code, out, err) = drive(&["glaucus", "completions", "bash"], "");
        assert_eq!(code, exit::OK);
        assert!(out.contains("glaucus"), "no script:\n{out}");
        assert!(err.is_empty());
    }

    #[test]
    fn zsh_and_fish_are_supported() {
        for shell in ["zsh", "fish"] {
            let (code, out, _e) = drive(&["glaucus", "completions", shell], "");
            assert_eq!(code, exit::OK, "shell {shell} failed");
            assert!(!out.is_empty());
        }
    }

    #[test]
    fn unknown_shell_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "completions", "cmd.exe"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("invalid value"));
    }

    #[test]
    fn comp_alias_works() {
        let (code, _o, _e) = drive(&["glaucus", "comp", "bash"], "");
        assert_eq!(code, exit::OK);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cmd::completions`

Expected: FAIL — subcommand undefined.

- [ ] **Step 4: Implement `crates/glaucus/src/cli/cmd/completions.rs`**

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus completions` — emit a shell completion script.

use crate::cli::Cli;
use crate::cli::env::Env;
use crate::cli::exit;
use clap::CommandFactory;

/// Arguments for `glaucus completions`.
#[derive(clap::Args, Debug)]
pub struct CompletionsArgs {
    /// Shell to generate for.
    pub shell: clap_complete::Shell,
}

/// Runs the command, returning the exit code.
pub fn run(args: &CompletionsArgs, env: &mut Env<'_>) -> u8 {
    let mut cmd = Cli::command();
    clap_complete::generate(args.shell, &mut cmd, "glaucus", env.stdout);
    exit::OK
}
```

Extend `Command` and `dispatch`:

```rust
    /// Emit a shell completion script.
    #[command(visible_alias = "comp")]
    Completions(completions::CompletionsArgs),
```

```rust
        Command::Completions(args) => completions::run(&args, &mut env),
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli cmd::completions`

Expected: PASS — every test added in this task's Step 1, none failing.

- [ ] **Step 6: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add Cargo.toml crates/glaucus/
git commit -S -m "feat(cli): add completions subcommand"
```

---

### Task 12: Global flags and logging wiring

**Files:**

- Modify: `crates/glaucus/src/cli/mod.rs`, `crates/glaucus/src/cli/cmd/mod.rs`, `crates/glaucus/src/cli/logging.rs`

**Interfaces:**

- Produces: `cli::GlobalArgs`; `dispatch(command, global, env)`.

- [ ] **Step 1: Write the failing tests**

Add to the test module in `crates/glaucus/src/cli/mod.rs`:

```rust
    #[test]
    fn verbose_flag_is_accepted_before_and_after_the_subcommand() {
        let (code, _o, _e) = drive(&["glaucus", "-v", "fmt"], "a: 1\n");
        assert_eq!(code, exit::OK);
        let (code, _o, _e) = drive(&["glaucus", "fmt", "-v"], "a: 1\n");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn quiet_and_verbose_conflict() {
        let (code, _o, err) = drive(&["glaucus", "fmt", "-v", "-q"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("cannot be used with"));
    }

    #[test]
    fn color_never_suppresses_ansi() {
        let (_c, _o, err) = drive(&["glaucus", "--color", "never", "parse"], "a: [1\n");
        assert!(!err.contains('\u{1b}'), "ansi leaked:\n{err}");
    }

    #[test]
    fn bad_color_value_is_a_usage_error() {
        let (code, _o, err) = drive(&["glaucus", "--color", "mauve", "fmt"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("invalid value"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cli::tests`

Expected: FAIL — global flags are not defined.

- [ ] **Step 3: Add `GlobalArgs` and thread it through**

In `crates/glaucus/src/cli/mod.rs`:

```rust
/// Flags accepted by every subcommand.
#[derive(clap::Args, Debug, Clone)]
pub struct GlobalArgs {
    /// Increase log verbosity. Repeat for more.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Errors only.
    #[arg(short = 'q', long = "quiet", global = true, conflicts_with = "verbose")]
    pub quiet: bool,
    /// When to emit colour.
    #[arg(long, global = true, value_enum, default_value_t = ColorArg::Auto)]
    pub color: ColorArg,
    /// Diagnostic rendering format.
    #[arg(long, global = true, value_enum, default_value_t = FormatArg::Human)]
    pub format: FormatArg,
    /// Suppress the echoed source line. Defaults on under CI.
    #[arg(long, global = true)]
    pub no_source: bool,
}

/// `--color` values.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorArg {
    /// Colour when stderr is a terminal.
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}

/// `--format` values.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatArg {
    /// Caret diagnostics for humans.
    Human,
    /// One JSON object per diagnostic.
    Json,
}
```

Add `#[command(flatten)] pub global: GlobalArgs,` to `Cli`, and change `run_with`'s tail to:

```rust
    let level = logging::level_for(
        parsed.global.verbose,
        parsed.global.quiet,
        std::env::var("RUST_LOG").ok().as_deref(),
    );
    logging::init(level);
    cmd::dispatch(parsed.command, &parsed.global, env)
```

Update `dispatch` to take `global: &GlobalArgs` and compute:

```rust
    let color = match global.color {
        ColorArg::Always => true,
        ColorArg::Never => false,
        ColorArg::Auto => env.color == ColorChoice::Auto,
    };
    let options = RenderOptions {
        color,
        show_source: !(global.no_source || env.is_ci),
    };
    let json = global.format == FormatArg::Json;
```

Add to `logging.rs`:

```rust
/// Installs the global subscriber. Idempotent: a second call is a no-op, which
/// matters because the unit tests drive the CLI many times in one process.
pub fn init(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
```

`try_init` returns an error on the second call rather than panicking, which is why it is used instead of `init`. Exclude nothing: the `let _ =` path is exercised by the first and second test that runs.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli`

Expected: PASS.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S -m "feat(cli): add global flags, colour policy and tracing wiring"
```

---

### Task 13: `glaucus-cli` wrapper, retire the four crates, update CI and docs

**Files:**

- Create: `crates/glaucus-cli/Cargo.toml`, `crates/glaucus-cli/src/main.rs`, `crates/glaucus-cli/README.md`
- Delete: `crates/glaucus-fmt/`, `crates/glaucus-validate/`, `crates/glaucus-lsp/`, `crates/glaucus-mcp/`
- Modify: `Cargo.toml`, `.github/workflows/publish.yml`, `README.md`

**Interfaces:**

- Consumes: `glaucus::cli::process::main`.

- [ ] **Step 1: Create the wrapper package**

`crates/glaucus-cli/Cargo.toml`:

```toml
# SPDX-FileCopyrightText: Glaucus contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

[package]
authors.workspace = true
categories.workspace = true
description = "CLI: the glaucus YAML toolchain. Installs the `glaucus` binary."
edition.workspace = true
exclude.workspace = true
keywords.workspace = true
license.workspace = true
name = "glaucus-cli"
readme = "README.md"
repository.workspace = true
rust-version.workspace = true
version.workspace = true

[dependencies]
glaucus = { workspace = true, features = ["cli"] }

[[bin]]
name = "glaucus"
path = "src/main.rs"

[lints]
workspace = true
```

`crates/glaucus-cli/src/main.rs`:

```rust
// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Flag-free install path. All logic lives in `glaucus::cli`.

fn main() -> std::process::ExitCode {
    glaucus::cli::process::main()
}
```

`crates/glaucus-cli/README.md`: one paragraph stating that this package exists so `cargo install glaucus-cli` works without `--features cli`, that it installs the same `glaucus` binary as `cargo install glaucus --features cli`, and that installing both is redundant.

- [ ] **Step 2: Verify no cycle and that both install paths work**

Run:

```bash
cargo metadata --format-version 1 > /dev/null
cargo build -p glaucus-cli
cargo tree -p glaucus --no-default-features -e normal
```

Expected: metadata resolves (no `cyclic package dependency`); the wrapper builds; the library tree still shows only `glaucus-core` and `serde`.

- [ ] **Step 3: Delete the four retired crates**

```bash
git rm -r crates/glaucus-fmt crates/glaucus-validate crates/glaucus-lsp crates/glaucus-mcp
```

Remove their `[workspace.dependencies]` entries from the root `Cargo.toml`. `members = ["crates/*"]` is a glob, so no member list needs editing.

- [ ] **Step 4: Verify nothing still references them**

Run:

```bash
grep -rn "glaucus-fmt\|glaucus-validate\|glaucus-lsp\|glaucus-mcp\|glaucus_fmt\|glaucus_validate\|glaucus_lsp\|glaucus_mcp" \
  --include=*.rs --include=*.toml --include=*.yml --include=*.md . \
  | grep -v "^./plans/" | grep -v "^./specs/" | grep -v "^./GLAUCUS_REFACTORATION.md"
```

Expected: no output. Fix any hit — likely `publish.yml`'s crate list and `README.md`'s install instructions.

- [ ] **Step 5: Update `publish.yml` and `README.md`**

In `.github/workflows/publish.yml`, remove the four retired crates from the publish order and add `glaucus-cli` after `glaucus`. In `README.md`, replace any `cargo install glaucus-fmt` style instructions with:

```bash
cargo install glaucus-cli            # flag-free
cargo install glaucus --features cli # equivalent
```

- [ ] **Step 6: Run the full gate set**

```bash
cargo build --workspace --all-features
mise run cargo:fmt && mise run cargo:clippy
cargo test --workspace --all-features
mise run coverage
cargo deny check
```

Expected: all pass; coverage `100.00%`.

- [ ] **Step 7: Smoke-test the installed binary**

```bash
cargo install --path crates/glaucus-cli --root /tmp/glaucus-smoke --force
/tmp/glaucus-smoke/bin/glaucus --help
printf 'a: 1   \n' | /tmp/glaucus-smoke/bin/glaucus fmt
printf 'a: 1\n' | /tmp/glaucus-smoke/bin/glaucus convert --to json
rm -rf /tmp/glaucus-smoke
```

Expected: help lists all eight commands; `fmt` prints `a: 1`; `convert` prints `{"a":1}`.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -S -m "feat(cli): add glaucus-cli wrapper and retire the four CLI crates"
```

---

## Self-Review

**Spec coverage.** Every spec section maps to a task: §3 architecture → Tasks 1, 13; §3.2 feature wiring → Task 1; §4 command surface → Tasks 4–11; §4.1 global flags → Task 12; §4.2 resource limits → **gap, see below**; §5 I/O contract → Tasks 3, 4; §5.1 exit codes → Task 1 plus per-command tests; §5.2 atomic writes → Task 3; §6.1 errors → Task 3; §6.2–6.3 rendering → Task 2; §6.4 `--no-source` → Tasks 2, 12; §6.5 logging → Tasks 3, 12; §6.6 visibility → Tasks 4, 5; §6.7 panic hook → Task 1; §7 testing → all; §8 performance → Task 1 (`BufWriter`); §9 reliability → Tasks 4, 5; §10 rollout → task order; §11 future work → not implemented, by design.

**Gap found: §4.2 resource-limit flags are not implemented by any task.** Added as Task 14 below.

**Placeholder scan.** No `TBD`/`TODO`. Three steps say "match the existing name" for items I could not verify without reading the moved file (`Error::span`, `Server::handle`, the LSP framing); each names the exact file to read and what to do if the shape differs. These are verification instructions, not placeholders.

**Type consistency.** `run_with(Env) -> u8` is used consistently. `RenderOptions { color, show_source }` matches between Task 2 and its consumers. `Report` field names are identical in Tasks 2, 4, 5, 6, 7, 8. `Source`/`read_input`/`write_atomic` signatures match between Task 3 and consumers. `dispatch` gains a `global: &GlobalArgs` parameter in Task 12; Tasks 4–11 pass `options`/`json` positionally and must be updated when Task 12 lands — called out in Task 5 Step 4.

---

### Task 14: Resource-limit flags

**Files:**

- Modify: `crates/glaucus/src/cli/mod.rs` (extend `GlobalArgs`), `crates/glaucus/src/cli/cmd/mod.rs`

**Interfaces:**

Consumes: `crate::limits::ResourceLimits`, `crate::error::ParserConfig`.

Produces: `GlobalArgs::limits(&self) -> crate::limits::ResourceLimits`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn max_depth_flag_rejects_deep_documents() {
        let deep = "a:\n".repeat(50) + "  b: 1\n";
        let (code, _o, err) = drive(&["glaucus", "--max-depth", "3", "parse"], &deep);
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("depth"), "no depth error:\n{err}");
    }

    #[test]
    fn default_limits_accept_ordinary_documents() {
        let (code, _o, _e) = drive(&["glaucus", "parse"], "a:\n  b:\n    c: 1\n");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn limits_helper_reflects_the_flags() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["glaucus", "--max-node-count", "7", "parse"]).unwrap();
        assert_eq!(cli.global.limits().max_node_count, 7);
    }

    #[test]
    fn non_numeric_limit_is_a_usage_error() {
        let (code, _o, err) = drive(&["glaucus", "--max-depth", "lots", "parse"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("invalid value"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p glaucus --features cli cli::tests`

Expected: FAIL — the flags do not exist.

- [ ] **Step 3: Extend `GlobalArgs`**

```rust
    /// Maximum collection nesting depth.
    #[arg(long, global = true, default_value_t = 128)]
    pub max_depth: usize,
    /// Maximum alias expansions. Defends against billion-laughs inputs.
    #[arg(long, global = true, default_value_t = 1024)]
    pub max_alias_expansions: usize,
    /// Maximum document size in bytes.
    #[arg(long, global = true, default_value_t = 268_435_456)]
    pub max_document_size: usize,
    /// Maximum mapping-key length in bytes.
    #[arg(long, global = true, default_value_t = 1024)]
    pub max_key_length: usize,
    /// Maximum node count in the representation graph.
    #[arg(long, global = true, default_value_t = 1_000_000)]
    pub max_node_count: usize,
```

and the helper:

```rust
impl GlobalArgs {
    /// The resource limits selected on the command line.
    #[must_use]
    pub fn limits(&self) -> crate::limits::ResourceLimits {
        crate::limits::ResourceLimits {
            max_depth: self.max_depth,
            max_alias_expansions: self.max_alias_expansions,
            max_document_size: self.max_document_size,
            max_key_length: self.max_key_length,
            max_node_count: self.max_node_count,
        }
    }
}
```

Thread the limits into every command that parses, by replacing bare `crate::from_str_node(text)` and `crate::parser::Parser::new(text)` calls with the config-taking constructors:

```rust
let config = crate::error::ParserConfig { limits: global.limits(), ..Default::default() };
let mut parser = crate::parser::Parser::with_config(text, config);
```

Confirm the exact `ParserConfig` field set before writing; if the struct is not `Default`-able, build it with the constructor the library already exposes.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glaucus --features cli`

Expected: PASS.

- [ ] **Step 5: Verify the gates and commit**

```bash
mise run cargo:fmt && mise run cargo:clippy && mise run coverage
git add crates/glaucus/src/cli/
git commit -S -m "feat(cli): expose resource limits as global flags"
```
