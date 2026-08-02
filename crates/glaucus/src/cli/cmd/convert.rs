// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus convert` — YAML to JSON and back.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::Source;
use crate::cli::runner;
use std::path::PathBuf;

/// Output language.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// JSON output.
    Json,
    /// YAML output.
    Yaml,
}

/// Arguments for `glaucus convert`.
#[derive(clap::Args, Debug)]
pub(crate) struct ConvertArgs {
    /// Files to convert. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// Language to convert to.
    #[arg(long, value_enum, default_value_t = Target::Json)]
    pub to: Target,
}

/// Runs the command, returning the exit code.
#[must_use]
pub(crate) fn run(args: &ConvertArgs, env: &mut Env<'_>, options: RenderOptions, json: bool) -> u8 {
    let sources = runner::resolve_sources(&args.files);
    let documents = runner::read_all(&sources, env.stdin);

    let mut worst = exit::OK;
    let mut findings = 0usize;
    for (source, text) in &documents {
        let code = match text {
            Err(error) => runner::report_io_error(error, env.stderr),
            Ok(text) => convert_one(text, args.to, source, env, options, json),
        };
        if code == exit::FINDINGS {
            findings += 1;
        }
        worst = worst.max(code);
    }
    runner::summary(env.stderr, documents.len(), findings, 0);
    worst
}

/// Converts one document into `target`, or reports why it could not be
/// converted.
///
/// Parsing `text` and re-rendering the resulting value in `target`'s
/// language are folded into one fallible step (`render`) so both potential
/// failure sources share a single diagnostic path. In practice only parsing
/// can fail here: `glaucus_serde`'s YAML emitter has no depth limit and,
/// serializing a `serde_json::Value`, never needs a non-string map key (the
/// one documented way `to_string` can fail) — every key JSON can produce is
/// already a `String`. So `render`'s `Target::Yaml` arm cannot itself
/// construct an `Err`, but sharing this path (rather than duplicating the
/// same error-reporting call for a branch nothing can reach) keeps the code
/// honest instead of carrying a diagnostic arm no test could ever exercise.
/// As in Task 6's `parse --emit json`, neither `glaucus_serde::Error` nor
/// `serde_json::Error` exposes a span worth depending on, so line/column are
/// always reported as `0`.
fn convert_one(
    text: &str,
    target: Target,
    source: &Source,
    env: &mut Env<'_>,
    options: RenderOptions,
    json: bool,
) -> u8 {
    match render(text, target) {
        Ok(rendered) => runner::write_document(env.stdout, &rendered, env.stderr),
        Err(error) => runner::report_parse_error(
            runner::ParseFailure {
                message: error.to_string(),
                line: 0,
                column: 0,
                source,
                text,
            },
            options,
            json,
            env.stderr,
        ),
    }
}

/// Parses `text` and renders it in `target`'s language.
fn render(text: &str, target: Target) -> glaucus_serde::Result<String> {
    let value: serde_json::Value = crate::from_str(text)?;
    match target {
        Target::Json => Ok(format!("{value}\n")),
        Target::Yaml => crate::to_string(&value),
    }
}

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

    // Fix round 1: the global `--format json` flag advertised itself in
    // `--help` on every subcommand but was silently ignored here — `run`
    // hardcoded `false` to `runner::report_parse_error`. Confirms it now
    // actually switches the diagnostic to JSON.
    #[test]
    fn format_json_emits_parseable_diagnostics_on_a_parse_failure() {
        let (code, _out, err) = drive(&["glaucus", "convert", "--format", "json"], "a: [1, 2\n");
        assert_eq!(code, exit::FINDINGS);
        let line = err
            .lines()
            .find(|l| l.starts_with('{'))
            .expect("no json diagnostic");
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["severity"], "error");
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

    // Not in the brief's given test list: without it, `run`'s per-document
    // `Err(error) => runner::report_io_error(...)` arm is never exercised
    // (every given test reads stdin), and the 100% coverage gate fails.
    #[test]
    fn missing_file_is_an_io_error() {
        let (code, _out, err) = drive(&["glaucus", "convert", "no-such-file.yaml"], "");
        assert_eq!(code, exit::IO);
        assert!(err.contains("no-such-file.yaml"));
    }

    // Blocker 2: `convert_one`'s stdout write was `let _ = write!(...)`,
    // discarding any failure. `glaucus convert big.yaml > /dev/full` used to
    // exit 0 having written nothing; a stdout that always fails now must
    // surface exit 3.
    #[test]
    fn stdout_write_failure_is_reported_as_an_io_error() {
        let (code, err) =
            crate::cli::tests::drive_with_failing_stdout(&["glaucus", "convert"], "a: 1\n");
        assert_eq!(code, exit::IO);
        assert!(err.contains("error:"), "no io error reported:\n{err}");
    }
}
