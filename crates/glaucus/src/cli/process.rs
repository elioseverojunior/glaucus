// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The only file that touches the real process.
//!
//! Excluded from coverage in `tarpaulin.toml`: argv, locked stdio and TTY
//! detection cannot be faked, and everything worth testing lives behind
//! `run_with(Env)`.

use crate::cli::env::{ColorChoice, Env};
use crate::cli::{exit, run_with};
use std::io::{BufWriter, IsTerminal, Write as _};
use std::process::ExitCode;

/// Real-process entry point shared by both binaries.
#[must_use]
pub fn main() -> ExitCode {
    install_panic_hook();
    let color = resolve_color();
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
    drop(input);
    // `write!` only fills `BufWriter`'s internal buffer; the real `write(2)`
    // (where `/dev/full`/a broken pipe surfaces) waits for this flush. A bare
    // `drop(out)` discarded exactly that failure (Blocker 2).
    let flushed = out.flush();
    drop(out);
    ExitCode::from(finalize_exit_code(code, flushed))
}

/// When to emit ANSI colour: never under `NO_COLOR`, otherwise following
/// whether stderr is a real terminal.
#[must_use]
fn resolve_color() -> ColorChoice {
    if std::env::var_os("NO_COLOR").is_some() {
        ColorChoice::Never
    } else if std::io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    }
}

/// Folds a stdout flush failure into `code`, but only when the command
/// itself already succeeded -- a command that already failed keeps
/// reporting its own exit code rather than being masked by an unrelated
/// flush error.
#[must_use]
fn finalize_exit_code(code: u8, flushed: std::io::Result<()>) -> u8 {
    if code == exit::OK {
        flushed.map_or(exit::IO, |()| code)
    } else {
        code
    }
}

/// Turns a panic into exit 101 with a report-this message, so a genuine crash
/// can never be mistaken for a finding.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!(
            "glaucus {}: internal error: {info}",
            env!("CARGO_PKG_VERSION")
        );
        eprintln!("This is a bug. Please report it with the input that caused it.");
    }));
}
