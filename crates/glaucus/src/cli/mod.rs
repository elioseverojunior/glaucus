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
pub mod runner;

mod cmd;

use clap::Parser;
use env::Env;

use crate::version::LONG_VERSION;

/// Safe YAML tooling.
#[derive(Parser, Debug)]
#[command(
    name = "glaucus",
    version,
    long_version = LONG_VERSION,
    about,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: cmd::Command,
    #[command(flatten)]
    pub global: GlobalArgs,
}

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
            return if e.use_stderr() {
                exit::USAGE
            } else {
                exit::OK
            };
        }
    };
    let level = logging::level_for(
        parsed.global.verbose,
        parsed.global.quiet,
        std::env::var("RUST_LOG").ok().as_deref(),
    );
    logging::init(level);
    cmd::dispatch(parsed.command, &parsed.global, env)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use clap::CommandFactory;
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

    /// A writer whose every `write` call fails, for exercising the
    /// `exit::IO` propagation path when a command's stdout write fails (e.g.
    /// `glaucus convert big.yaml > /dev/full`).
    pub(crate) struct FailingWriter;

    impl std::io::Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Drives the whole CLI with a stdout that always fails to write.
    ///
    /// Returns `(exit_code, stderr)` — there is nothing to inspect on
    /// stdout, since `FailingWriter` never accepts a byte.
    pub(crate) fn drive_with_failing_stdout(args: &[&str], stdin: &str) -> (u8, String) {
        let mut err: Vec<u8> = Vec::new();
        let mut input = stdin.as_bytes();
        let mut out = FailingWriter;
        let code = run_with(Env {
            args: args.iter().map(OsString::from).collect(),
            stdin: &mut input,
            stdout: &mut out,
            stderr: &mut err,
            color: ColorChoice::Never,
            is_ci: false,
        });
        (code, String::from_utf8(err).expect("stderr utf8"))
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

    // Not in the brief's given test list: without it, `cmd::dispatch`'s
    // `ColorArg::Always` match arm is never exercised — every given test uses
    // either the default (`Auto`) or `never`, and the 100% coverage gate
    // fails. `drive`'s harness always sets `env.color` to `Never`, so this is
    // the only way to observe `--color always` actually overriding it.
    #[test]
    fn color_always_forces_ansi_even_though_stderr_is_not_a_terminal() {
        let (_c, _o, err) = drive(&["glaucus", "--color", "always", "parse"], "a: [1\n");
        assert!(err.contains('\u{1b}'), "expected ansi:\n{err}");
    }
}
