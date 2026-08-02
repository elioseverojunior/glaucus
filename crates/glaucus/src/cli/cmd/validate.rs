// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus validate` — JSON-Schema validation with optional autofix.

use crate::cli::diag::{RenderOptions, Report, Severity, render, render_json};
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::{Source, read_input, write_atomic};
use crate::cli::runner;
use std::path::PathBuf;

/// Arguments for `glaucus validate`.
#[derive(clap::Args, Debug)]
pub(crate) struct ValidateArgs {
    /// Documents to validate. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// JSON-Schema document to validate against.
    #[arg(short = 's', long = "schema")]
    pub schema: PathBuf,
    /// Apply comment-preserving autofix and write the result back.
    #[arg(long)]
    pub fix: bool,
}

/// Diagnostic-rendering policy for one validation pass.
///
/// Bundled instead of two positional parameters on `check_one`: that function
/// already carries five other arguments, and clippy's `too_many_arguments`
/// (threshold 6 in this workspace) fires at 7 — the same hazard
/// `runner::ParseFailure` exists to avoid. `options` and `json` are now both
/// fully resolved by `cmd::dispatch` from the global `--no-source`/`--format`
/// flags, so this struct is a plain bundle rather than a merge point.
#[derive(Debug, Clone, Copy)]
struct RenderPolicy {
    options: RenderOptions,
    json: bool,
}

/// Runs the command, returning the exit code.
#[must_use]
pub(crate) fn run(
    args: &ValidateArgs,
    env: &mut Env<'_>,
    options: RenderOptions,
    json: bool,
) -> u8 {
    let policy = RenderPolicy { options, json };
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
            Ok(text) => check_one(source, text, &schema, env, policy, &mut findings),
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
    policy: RenderPolicy,
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
        let _ = if policy.json {
            render_json(&report, env.stderr)
        } else {
            render(&report, Some(text), policy.options, env.stderr)
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
        return runner::write_document(env.stdout, &fixed, env.stderr);
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
            &[
                "glaucus",
                "validate",
                "-s",
                s.to_str().unwrap(),
                d.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn type_error_renders_a_caret_diagnostic() {
        let s = tmp("s2.yaml", SCHEMA);
        let d = tmp("d2.yaml", "port: \"8080\"\n");
        let (code, _out, err) = drive(
            &[
                "glaucus",
                "validate",
                "-s",
                s.to_str().unwrap(),
                d.to_str().unwrap(),
            ],
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
            &[
                "glaucus",
                "validate",
                "-s",
                s.to_str().unwrap(),
                "--fix",
                d.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::OK);
        assert_eq!(std::fs::read_to_string(&d).unwrap(), "port: 80  # c\n");
    }

    #[test]
    fn no_source_suppresses_the_echoed_line() {
        let s = tmp(
            "s4.yaml",
            "type: object\nproperties:\n  pw: {type: integer}\n",
        );
        let d = tmp("d4.yaml", "pw: hunter2\n");
        let (code, _out, err) = drive(
            &[
                "glaucus",
                "validate",
                "--no-source",
                "-s",
                s.to_str().unwrap(),
                d.to_str().unwrap(),
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
                "glaucus",
                "validate",
                "--format",
                "json",
                "-s",
                s.to_str().unwrap(),
                d.to_str().unwrap(),
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
                &[
                    "glaucus",
                    alias,
                    "-s",
                    s.to_str().unwrap(),
                    d.to_str().unwrap(),
                ],
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
                "glaucus",
                "validate",
                "-s",
                s.to_str().unwrap(),
                bad.to_str().unwrap(),
                good.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("2 file(s)"), "did not process both:\n{err}");
    }

    // Not in the brief's given test list: without it, `run`'s schema-read
    // `Err(error) => return runner::report_io_error(...)` arm is never
    // exercised (every given test points `-s` at a schema `tmp()` creates),
    // and the 100% coverage gate fails.
    #[test]
    fn missing_schema_file_is_an_io_error() {
        let d = tmp("d8.yaml", "port: 1\n");
        let (code, _out, err) = drive(
            &[
                "glaucus",
                "validate",
                "-s",
                "no-such-schema.yaml",
                d.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::IO);
        assert!(err.contains("no-such-schema.yaml"));
    }

    // Not in the brief's given test list: without it, `run`'s per-document
    // `Err(error) => runner::report_io_error(...)` arm is never exercised
    // (`continue_on_error_processes_every_file` uses two files that both
    // exist; one just fails validation, which is a different code path).
    #[test]
    fn missing_data_file_is_an_io_error() {
        let s = tmp("s9.yaml", SCHEMA);
        let (code, _out, err) = drive(
            &[
                "glaucus",
                "validate",
                "-s",
                s.to_str().unwrap(),
                "no-such-data.yaml",
            ],
            "",
        );
        assert_eq!(code, exit::IO);
        assert!(err.contains("no-such-data.yaml"));
    }

    // Not in the brief's given test list: without it, `fix_one`'s
    // `let Some(path) = source.path() else { ... }` `None` arm (stdin) is
    // never exercised — every given `--fix` test targets a file.
    #[test]
    fn fix_via_stdin_writes_fixed_document_to_stdout() {
        let s = tmp("s10.yaml", SCHEMA);
        let (code, out, _err) = drive(
            &["glaucus", "validate", "-s", s.to_str().unwrap(), "--fix"],
            "port: \"80\"\n",
        );
        assert_eq!(code, exit::OK);
        assert_eq!(out, "port: 80\n");
    }

    // Not in the brief's given test list: without it, `fix_one`'s
    // `if fixed == text { return exit::OK }` no-op arm is never exercised —
    // `fix_coerces_and_writes` always produces a change.
    #[test]
    fn fix_on_an_already_canonical_file_is_a_no_op() {
        let s = tmp("s11.yaml", SCHEMA);
        let d = tmp("d11.yaml", "port: 80\n");
        let (code, _out, err) = drive(
            &[
                "glaucus",
                "validate",
                "-s",
                s.to_str().unwrap(),
                "--fix",
                d.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::OK);
        assert_eq!(std::fs::read_to_string(&d).unwrap(), "port: 80\n");
        assert!(err.contains("0 changed"), "expected a no-op fix:\n{err}");
    }

    // Not in the brief's given test list: without it, `fix_one`'s
    // `if let Err(error) = write_atomic(...)` branch is never exercised.
    // Revoking write permission on the directory (not the file) makes
    // `write_atomic`'s temp-file creation fail while the initial read still
    // succeeds, reaching the write step.
    #[cfg(unix)]
    #[test]
    fn fix_write_failure_is_reported_as_an_io_error() {
        use std::os::unix::fs::PermissionsExt;
        let s = tmp("s12.yaml", SCHEMA);
        let dir = std::env::temp_dir().join("glaucus-cmd-validate-unwritable");
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.join("locked.yaml");
        std::fs::write(&d, "port: \"80\"\n").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();

        let (code, _out, err) = drive(
            &[
                "glaucus",
                "validate",
                "-s",
                s.to_str().unwrap(),
                "--fix",
                d.to_str().unwrap(),
            ],
            "",
        );

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_file(&d).unwrap();
        std::fs::remove_dir(&dir).unwrap();

        assert_eq!(code, exit::IO);
        assert!(err.contains("error:"), "no io error reported:\n{err}");
    }

    // Blocker 2: `fix_one`'s stdin-to-stdout branch was `let _ =
    // write!(...)`, discarding any failure. A stdout that always fails now
    // must surface exit 3, not exit 0 having written nothing.
    #[test]
    fn fix_via_stdin_stdout_write_failure_is_reported_as_an_io_error() {
        let s = tmp("s13.yaml", SCHEMA);
        let (code, err) = crate::cli::tests::drive_with_failing_stdout(
            &["glaucus", "validate", "-s", s.to_str().unwrap(), "--fix"],
            "port: \"80\"\n",
        );
        assert_eq!(code, exit::IO);
        assert!(err.contains("error:"), "no io error reported:\n{err}");
    }
}
