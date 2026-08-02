// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus parse` — inspect the parse pipeline.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::Source;
use crate::cli::runner;
use crate::cst::SyntaxElement;
use std::path::PathBuf;

/// What representation to print.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Emit {
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
pub(crate) struct ParseArgs {
    /// Files to parse. With none, read stdin.
    pub files: Vec<PathBuf>,
    /// Representation to print.
    #[arg(long, value_enum, default_value_t = Emit::Events)]
    pub emit: Emit,
}

/// A parse failure with just enough information to render a caret diagnostic.
///
/// `Events` and `Ast` fail through `crate::error::Error`, which carries a real
/// source span (recovered by the `From` impl below). `Json` goes through
/// `glaucus_serde`, whose error type does not expose a span worth depending
/// on here, so it always reports line/column `0` — the diagnostic still names
/// the problem, it just cannot point at it. `Cst` never constructs one:
/// `Document::parse` is infallible.
struct Failure {
    /// One-line description of what went wrong.
    message: String,
    /// 1-based line. `0` means unknown.
    line: u32,
    /// 1-based column in bytes. `0` means unknown.
    column: u32,
}

impl From<crate::error::Error> for Failure {
    fn from(error: crate::error::Error) -> Self {
        let span = error.span();
        Self {
            message: error.to_string(),
            line: span.map_or(0, |span| span.start.line),
            column: span.map_or(0, |span| span.start.column),
        }
    }
}

/// Runs the command, returning the exit code.
#[must_use]
pub(crate) fn run(args: &ParseArgs, env: &mut Env<'_>, options: RenderOptions, json: bool) -> u8 {
    let sources = runner::resolve_sources(&args.files);
    let documents = runner::read_all(&sources, env.stdin);

    let mut worst = exit::OK;
    let mut findings = 0usize;
    for (source, text) in &documents {
        let code = match text {
            Err(error) => runner::report_io_error(error, env.stderr),
            Ok(text) => parse_one(text, args.emit, source, env, options, json),
        };
        if code == exit::FINDINGS {
            findings += 1;
        }
        worst = worst.max(code);
    }
    runner::summary(env.stderr, documents.len(), findings, 0);
    worst
}

/// Emits one document in the requested representation, or reports why it
/// could not be parsed.
fn parse_one(
    text: &str,
    emit: Emit,
    source: &Source,
    env: &mut Env<'_>,
    options: RenderOptions,
    json: bool,
) -> u8 {
    match render_one(text, emit) {
        Ok(rendered) => runner::write_document(env.stdout, &rendered, env.stderr),
        Err(failure) => runner::report_parse_error(
            runner::ParseFailure {
                message: failure.message,
                line: failure.line,
                column: failure.column,
                source,
                text,
            },
            options,
            json,
            env.stderr,
        ),
    }
}

/// Renders `text` in the representation `emit` selects.
///
/// Builds the whole result in memory rather than writing each piece to
/// `env.stdout` as it goes (as `Events` and `Ast` once did): a single write
/// afterwards, shared with every other command via
/// `runner::write_document`, is the one place that needs to handle a stdout
/// write failure (Blocker 2), instead of one failure-handling branch per
/// `Emit` variant.
fn render_one(text: &str, emit: Emit) -> Result<String, Failure> {
    Ok(match emit {
        Emit::Events => render_events(text)?,
        Emit::Ast => format!("{:#?}\n", crate::from_str_node(text)?),
        Emit::Cst => {
            let document = crate::cst::Document::parse(text);
            let mut dump = String::new();
            dump_cst(&SyntaxElement::Node(document.root()), 0, &mut dump);
            dump
        }
        Emit::Json => {
            let value: serde_json::Value = crate::from_str(text).map_err(|error| Failure {
                message: error.to_string(),
                line: 0,
                column: 0,
            })?;
            format!("{value}\n")
        }
    })
}

/// Renders the SAX-style event stream for `text`.
fn render_events(text: &str) -> Result<String, Failure> {
    use std::fmt::Write as _;
    let mut parser = crate::parser::Parser::new(text);
    let mut rendered = String::new();
    while let Some(event) = parser.next_event() {
        let event = event?;
        let _ = writeln!(rendered, "{:?}", event.kind);
    }
    Ok(rendered)
}

/// Recursively renders one CST element's kind and text, indented by depth.
///
/// `Document` has no `Debug` impl of its own — it round-trips losslessly via
/// `Display` instead — so `--emit cst` gets this small, purpose-built dump.
fn dump_cst(element: &SyntaxElement, depth: usize, out: &mut String) {
    use std::fmt::Write as _;
    let indent = "  ".repeat(depth);
    let _ = writeln!(out, "{indent}{:?} {:?}", element.kind(), element.text());
    if let SyntaxElement::Node(node) = element {
        for child in node.children_with_tokens() {
            dump_cst(&child, depth + 1, out);
        }
    }
}

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

    // Fix round 1: the global `--format json` flag advertised itself in
    // `--help` on every subcommand but was silently ignored here — `run`
    // hardcoded `false` to `runner::report_parse_error`. Confirms it now
    // actually switches the diagnostic to JSON.
    #[test]
    fn format_json_emits_parseable_diagnostics_on_a_parse_failure() {
        let (code, _out, err) = drive(&["glaucus", "parse", "--format", "json"], "a: [1, 2\n");
        assert_eq!(code, exit::FINDINGS);
        let line = err
            .lines()
            .find(|l| l.starts_with('{'))
            .expect("no json diagnostic");
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["severity"], "error");
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

    // Not in the brief's given test list: without it, `run`'s per-document
    // `Err(error) => runner::report_io_error(...)` arm is never exercised
    // (every given test reads stdin), and the 100% coverage gate fails.
    #[test]
    fn missing_file_is_an_io_error() {
        let (code, _out, err) = drive(&["glaucus", "parse", "no-such-file.yaml"], "");
        assert_eq!(code, exit::IO);
        assert!(err.contains("no-such-file.yaml"));
    }

    // Not in the brief's given test list: without it, `emit_one`'s `Ast` arm
    // never takes its error path (`crate::from_str_node(text)?`), and the
    // 100% coverage gate fails.
    #[test]
    fn ast_emit_reports_invalid_yaml_as_a_finding() {
        let (code, _out, err) = drive(&["glaucus", "parse", "--emit", "ast"], "a: [1, 2\n");
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("error:"), "no diagnostic:\n{err}");
    }

    // Not in the brief's given test list: without it, `render_one`'s `Json`
    // arm never takes its error path (the `map_err` closure and the `?`
    // after it), and the 100% coverage gate fails.
    #[test]
    fn json_emit_reports_invalid_yaml_as_a_finding() {
        let (code, _out, err) = drive(&["glaucus", "parse", "--emit", "json"], "a: [1, 2\n");
        assert_eq!(code, exit::FINDINGS);
        assert!(err.contains("error:"), "no diagnostic:\n{err}");
    }

    // Blocker 2: `parse_one`'s stdout write was `let _ = write!(...)` (spread
    // across every `Emit` arm before this fix consolidated them through
    // `render_one` + `runner::write_document`), discarding any failure. A
    // stdout that always fails now must surface exit 3, not exit 0 having
    // written nothing.
    #[test]
    fn stdout_write_failure_is_reported_as_an_io_error() {
        let (code, err) =
            crate::cli::tests::drive_with_failing_stdout(&["glaucus", "parse"], "a: 1\n");
        assert_eq!(code, exit::IO);
        assert!(err.contains("error:"), "no io error reported:\n{err}");
    }
}
