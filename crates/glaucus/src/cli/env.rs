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
