// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subcommands.

pub(crate) mod completions;
pub(crate) mod convert;
pub(crate) mod fmt;
pub(crate) mod lsp;
pub(crate) mod mcp;
pub(crate) mod parse;
pub(crate) mod schema;
pub(crate) mod validate;
pub(crate) mod version;

use crate::cli::diag::RenderOptions;
use crate::cli::env::{ColorChoice, Env};
use crate::cli::{ColorArg, FormatArg, GlobalArgs};

/// The subcommand set.
///
/// `pub(crate)`, not `pub`: `cmd` is a private module of `cli`, and `Command`
/// is never named outside this crate (it is not part of the `Interfaces` this
/// task produces). A wider visibility would be unreachable and rejected by
/// this workspace's `unreachable_pub`/`unnameable_types` lints; it must match
/// the visibility of `Cli::command` below, or `private_interfaces` rejects
/// the mismatch the other way.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum Command {
    /// Emit a shell completion script.
    #[command(visible_alias = "comp")]
    Completions(completions::CompletionsArgs),
    /// Convert between YAML and JSON.
    #[command(visible_alias = "to", alias = "conv")]
    Convert(convert::ConvertArgs),
    /// Format YAML, comment-preserving.
    #[command(visible_alias = "format", alias = "f")]
    Fmt(fmt::FmtArgs),
    /// Run the language server over stdio.
    Lsp,
    /// Run the MCP server over stdio.
    Mcp,
    /// Inspect the parse pipeline.
    #[command(visible_alias = "dump", alias = "p")]
    Parse(parse::ParseArgs),
    /// Work with JSON-Schema documents.
    #[command(alias = "sch")]
    Schema(schema::SchemaArgs),
    /// Validate YAML against a JSON-Schema.
    #[command(visible_alias = "check", alias = "val")]
    Validate(validate::ValidateArgs),
    /// Show the version, and optionally how this binary was built.
    Version(version::VersionArgs),
}

/// Runs the selected command.
#[must_use]
pub(crate) fn dispatch(command: Command, global: &GlobalArgs, mut env: Env<'_>) -> u8 {
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
    match command {
        Command::Completions(args) => completions::run(&args, &mut env),
        Command::Convert(args) => convert::run(&args, &mut env, options, json),
        Command::Fmt(args) => fmt::run(&args, &mut env, options, json),
        Command::Lsp => lsp::run(&mut env),
        Command::Mcp => mcp::run(&mut env),
        Command::Parse(args) => parse::run(&args, &mut env, options, json),
        Command::Schema(args) => schema::run(&args, &mut env, options, json),
        Command::Validate(args) => validate::run(&args, &mut env, options, json),
        Command::Version(args) => version::run(&args, &mut env, json),
    }
}
