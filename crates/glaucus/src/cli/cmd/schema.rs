// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus schema` — work with JSON-Schema documents.

use crate::cli::diag::RenderOptions;
use crate::cli::env::Env;
use crate::cli::exit;
use crate::cli::io::{Source, read_input};
use crate::cli::runner;
use std::path::PathBuf;

/// Arguments for `glaucus schema`.
#[derive(clap::Args, Debug)]
pub(crate) struct SchemaArgs {
    #[command(subcommand)]
    pub command: SchemaCommand,
}

/// Schema operations.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum SchemaCommand {
    /// Check that a schema document is well-formed.
    Check {
        /// The schema file.
        file: PathBuf,
    },
}

/// Runs the command, returning the exit code.
#[must_use]
pub(crate) fn run(args: &SchemaArgs, env: &mut Env<'_>, options: RenderOptions, json: bool) -> u8 {
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
                json,
                env.stderr,
            )
        }
    }
}

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
        let p = tmp(
            "ok.yaml",
            "type: object\nproperties:\n  a: {type: integer}\n",
        );
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

    // Fix round 1: the global `--format json` flag advertised itself in
    // `--help` on every subcommand but was silently ignored here — `run`
    // hardcoded `false` to `runner::report_parse_error`. Confirms it now
    // actually switches the diagnostic to JSON.
    #[test]
    fn format_json_emits_parseable_diagnostics_on_a_parse_failure() {
        let p = tmp("bad2.yaml", "type: [object\n");
        let (code, _out, err) = drive(
            &[
                "glaucus",
                "--format",
                "json",
                "schema",
                "check",
                p.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(code, exit::FINDINGS);
        let line = err
            .lines()
            .find(|l| l.starts_with('{'))
            .expect("no json diagnostic");
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["severity"], "error");
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
