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
/// and silently misplaced every caret. Named fields make that mistake
/// impossible to write.
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

/// Writes `data` to `stdout` as-is, reporting a write failure as an I/O error.
///
/// Shared by every command that emits a whole document to stdout on success
/// (`fmt`'s default/`--check` pass-through, `convert`, `parse`, `validate
/// --fix` on stdin). Before this existed, each of those sites discarded
/// `write!`'s `Result` with `let _ =`, so `glaucus convert big.yaml >
/// /dev/full` exited 0 having written nothing — the spec reserves exit 3 for
/// exactly this, and on the output side it was unreachable (Blocker 2).
#[must_use]
pub fn write_document(stdout: &mut dyn Write, data: &str, stderr: &mut dyn Write) -> u8 {
    match write!(stdout, "{data}") {
        Ok(()) => exit::OK,
        Err(error) => {
            let error = anyhow::Error::new(error).context("writing to stdout");
            report_io_error(&error, stderr)
        }
    }
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
        let sources = vec![
            Source::File("missing.yaml".into()),
            Source::File(good.clone()),
        ];

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
        assert!(
            text.contains('3') && text.contains('1') && text.contains('2'),
            "{text}"
        );
    }

    #[test]
    fn summary_warns_when_nothing_matched() {
        let mut stderr = Vec::new();
        summary(&mut stderr, 0, 0, 0);
        let text = String::from_utf8(stderr).unwrap();
        assert!(
            text.contains("no files"),
            "empty match must not be silent: {text}"
        );
    }

    // Not in the brief's given test list: without these, `report_parse_error`
    // and its `json` branch are never exercised by anything in this task, and
    // the 100% coverage gate on `src/cli/` fails. Real callers arrive in
    // Tasks 4-8, so this task must reach the function directly.
    #[test]
    fn report_parse_error_renders_human_diagnostic_and_yields_findings() {
        let source = Source::File(PathBuf::from("bad.yaml"));
        let text = "a: [1, 2\n";
        let mut stderr = Vec::new();

        let code = report_parse_error(
            ParseFailure {
                message: "unterminated flow sequence".to_string(),
                line: 1,
                column: 4,
                source: &source,
                text,
            },
            RenderOptions {
                color: false,
                show_source: true,
            },
            false,
            &mut stderr,
        );

        assert_eq!(code, exit::FINDINGS);
        let rendered = String::from_utf8(stderr).unwrap();
        assert!(rendered.contains("unterminated flow sequence"));
        assert!(rendered.contains("bad.yaml"));
    }

    #[test]
    fn report_parse_error_renders_json_when_requested() {
        let source = Source::Stdin;
        let text = "a: [1, 2\n";
        let mut stderr = Vec::new();

        let code = report_parse_error(
            ParseFailure {
                message: "unterminated flow sequence".to_string(),
                line: 1,
                column: 4,
                source: &source,
                text,
            },
            RenderOptions {
                color: false,
                show_source: true,
            },
            true,
            &mut stderr,
        );

        assert_eq!(code, exit::FINDINGS);
        let rendered = String::from_utf8(stderr).unwrap();
        let value: serde_json::Value = serde_json::from_str(rendered.trim()).unwrap();
        assert_eq!(value["message"], "unterminated flow sequence");
        assert_eq!(value["severity"], "error");
    }
}
