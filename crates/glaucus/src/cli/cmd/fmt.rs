// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus fmt` — format YAML, comment-preserving.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::{Source, write_atomic};
use crate::cli::runner;
use std::path::PathBuf;

/// Arguments for `glaucus fmt`.
#[derive(clap::Args, Debug)]
pub(crate) struct FmtArgs {
    /// Files to format. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// Exit 1 if any file is not already formatted.
    #[arg(long, conflicts_with = "write")]
    pub check: bool,
    /// Overwrite each file with its formatted output.
    #[arg(long)]
    pub write: bool,
}

/// Diagnostic-rendering policy for one format pass.
///
/// Bundled instead of two positional parameters on `format_one`: that function
/// already carries five other arguments, and clippy's `too_many_arguments`
/// (threshold 6 in this workspace) fires at 7 — the same hazard
/// `runner::ParseFailure` and `validate.rs`'s `RenderPolicy` exist to avoid.
#[derive(Debug, Clone, Copy)]
struct RenderPolicy {
    options: RenderOptions,
    json: bool,
}

/// Runs the command, returning the exit code.
#[must_use]
pub(crate) fn run(args: &FmtArgs, env: &mut Env<'_>, options: RenderOptions, json: bool) -> u8 {
    if no_write_target(args) {
        let _ = writeln!(env.stderr, "warning: no files matched; nothing to do");
        return exit::OK;
    }
    let policy = RenderPolicy { options, json };
    let sources = runner::resolve_sources(&args.files);
    let documents = runner::read_all(&sources, env.stdin);

    let mut worst = exit::OK;
    let mut changed = 0usize;
    let mut findings = 0usize;
    for (source, text) in &documents {
        let code = match text {
            Err(error) => runner::report_io_error(error, env.stderr),
            Ok(text) => format_one(args, source, text, env, policy, &mut changed),
        };
        if code == exit::FINDINGS {
            findings += 1;
        }
        worst = worst.max(code);
    }
    runner::summary(env.stderr, documents.len(), findings, changed);
    worst
}

/// Whether `--write` has no positional files to act on, so `run` should
/// short-circuit before touching stdin.
///
/// `--check` must NOT take this path: the spec's I/O contract says "no
/// positional files -> read stdin", so `--check` alone falls through to
/// `runner::resolve_sources`, which already returns `[Source::Stdin]` for an
/// empty list. Before this fix, `--check` shared this guard with `--write`
/// (Blocker 1), so `cat x.yaml | glaucus fmt --check` warned "nothing to do"
/// and exited 0 on unformatted input — a format gate that could never fail.
#[must_use]
const fn no_write_target(args: &FmtArgs) -> bool {
    args.files.is_empty() && args.write
}

/// Formats one document, or reports why it could not be parsed.
fn format_one(
    args: &FmtArgs,
    source: &Source,
    text: &str,
    env: &mut Env<'_>,
    policy: RenderPolicy,
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
                policy.options,
                policy.json,
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
    runner::write_document(env.stdout, formatted, env.stderr)
}

#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

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

    // Fix round 1: the global `--format json` flag advertised itself in
    // `--help` on every subcommand but was silently ignored here — `run`
    // hardcoded `false` to `runner::report_parse_error`. Confirms it now
    // actually switches the diagnostic to JSON.
    #[test]
    fn format_json_emits_parseable_diagnostics_on_a_parse_failure() {
        let (code, _out, err) = drive(&["glaucus", "fmt", "--format", "json"], "a: [1, 2\n");
        assert_eq!(code, exit::FINDINGS);
        let line = err
            .lines()
            .find(|l| l.starts_with('{'))
            .expect("no json diagnostic");
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["severity"], "error");
    }

    // Blocker 2: `emit`'s stdout write was `let _ = write!(...)`, discarding
    // any failure. `glaucus fmt big.yaml > /dev/full` used to exit 0 having
    // written nothing; a stdout that always fails now must surface exit 3.
    #[test]
    fn stdout_write_failure_is_reported_as_an_io_error() {
        let (code, err) =
            crate::cli::tests::drive_with_failing_stdout(&["glaucus", "fmt"], "a: 1\n");
        assert_eq!(code, exit::IO);
        assert!(err.contains("error:"), "no io error reported:\n{err}");
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let (code, _out, err) = drive(&["glaucus", "fmt", "no-such-file.yaml"], "");
        assert_eq!(code, exit::IO);
        assert!(err.contains("no-such-file.yaml"));
    }

    #[test]
    fn empty_match_warns_rather_than_silently_succeeding() {
        let (code, _out, err) = drive(&["glaucus", "fmt", "--write", "--"], "");
        assert_eq!(code, exit::OK);
        assert!(err.contains("no files"), "silent no-op:\n{err}");
    }

    // Blocker 1: `--check` with no positional files must fall through to
    // stdin, not take the "nothing to do" early return. Before the fix, the
    // guard was `args.files.is_empty() && (args.check || args.write)`, so
    // `printf 'a: 1   \n' | glaucus fmt --check` warned "nothing to do" and
    // exited 0 on unformatted input -- a format gate that can never fail.
    #[test]
    fn check_on_stdin_reports_findings_for_unformatted_input() {
        let (code, _out, err) = drive(&["glaucus", "fmt", "--check"], "a: 1   \n");
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("<stdin>"), "stdin not named:\n{err}");
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

    // Not in the brief's given test list: without it, `emit`'s
    // `source.path().filter(|_| !unchanged)` never takes its `None` arm (the
    // early `return exit::OK` before any write happens), and the 100%
    // coverage gate fails. The "0 changed" summary confirms the file was
    // genuinely skipped, not just written back with identical bytes.
    #[test]
    fn write_on_already_formatted_file_is_a_no_op() {
        let p = tmp("already-clean.yaml", "a: 1\n");
        let (code, _out, err) = drive(&["glaucus", "fmt", "--write", p.to_str().unwrap()], "");
        assert_eq!(code, exit::OK);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "a: 1\n");
        assert!(err.contains("0 changed"), "expected a no-op write:\n{err}");
    }

    // Not in the brief's given test list: without it, `emit`'s
    // `if let Err(error) = write_atomic(...)` branch is never exercised and
    // the 100% coverage gate fails. Revoking write permission on the
    // directory (not the file) makes `write_atomic`'s temp-file creation fail
    // while the initial read still succeeds, reaching the write step.
    #[cfg(unix)]
    #[test]
    fn write_failure_is_reported_as_an_io_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("glaucus-cmd-fmt-unwritable");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("locked.yaml");
        std::fs::write(&p, "a: 1   \n").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();

        let (code, _out, err) = drive(&["glaucus", "fmt", "--write", p.to_str().unwrap()], "");

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_file(&p).unwrap();
        std::fs::remove_dir(&dir).unwrap();

        assert_eq!(code, exit::IO);
        assert!(err.contains("error:"), "no io error reported:\n{err}");
    }
}
