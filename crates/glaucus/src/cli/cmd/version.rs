// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus version` — the version, and optionally how the binary was built.

use crate::cli::env::Env;
use crate::cli::exit;
use crate::version::{BuildInfo, Format};

/// The `-o/--output` choices for `glaucus version`.
///
/// A CLI-side mirror of [`Format`] rather than a `ValueEnum` derive on the
/// library type: that would put an argument parser in a dependency of every
/// surface crate, including a future WASM one that has no command line at all.
///
/// Named `output`, not `format`: [`crate::cli::GlobalArgs`] already declares a
/// global `--format` flag (`human`/`json`, for diagnostic rendering) that
/// clap propagates into every subcommand. A second, differently-typed
/// `--format` here collides on that shared argument id -- clap does not
/// reject it at `Command::debug_assert()`, but panics at parse time
/// downcasting the value to the wrong enum. `-o/--output` is also what the
/// ported reference (`comply version`) uses for the identical concept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Output {
    /// Human-readable lines.
    Plain,
    /// A JSON object.
    Json,
    /// A TOML table.
    Toml,
}

impl Output {
    // Deliberately not a `const fn`: literal arguments to one get folded at
    // compile time, so the arms record no runtime coverage.
    #[allow(clippy::missing_const_for_fn)]
    fn as_format(self) -> Format {
        match self {
            Self::Plain => Format::Plain,
            Self::Json => Format::Json,
            Self::Toml => Format::Toml,
        }
    }
}

/// Arguments for `glaucus version`.
#[derive(clap::Args, Debug)]
pub(crate) struct VersionArgs {
    /// Include build provenance and the full gitversion stamp.
    #[arg(long)]
    pub full: bool,
    /// Output format. Also selectable via the global `--format` flag; when
    /// both are given and disagree, `-o/--output` wins.
    #[arg(short, long, value_enum)]
    pub output: Option<Output>,
}

/// Picks the effective output format.
///
/// `--output` is `version`'s own flag, so an explicit value always wins over
/// the global `--format json`/`--format human` -- including when it disagrees
/// with `format_json`. Only when `--output` was omitted does the global
/// `--format` flag get to decide, closing Blocker 3: `--format` is declared
/// `global = true` and clap advertises it on every subcommand, including this
/// one, so `glaucus version --format json` must not silently fall back to
/// plain text.
// Deliberately not a `const fn`: literal arguments to one get folded at
// compile time, so the arms record no runtime coverage (same rationale as
// `Output::as_format` above).
#[allow(clippy::missing_const_for_fn)]
#[must_use]
fn resolve_output(explicit: Option<Output>, format_json: bool) -> Output {
    match explicit {
        Some(output) => output,
        None if format_json => Output::Json,
        None => Output::Plain,
    }
}

/// Runs the command, returning the exit code.
///
/// # Panics
///
/// Panics if the compiled-in build provenance cannot be serialized. `render`
/// is fallible only via `serde_json`/`toml`, and both are total for
/// [`BuildInfo`]: every field is a `String` or a `Map<String, Value>` with
/// string keys and JSON-native values, which none of `toml`'s serializer
/// error variants (`UnsupportedType`, `UnsupportedNone`, `KeyNotString`,
/// `DateInvalid`) can reach, and a `serde_json::Value` cannot hold a NaN/Inf
/// float through its public constructors. A panic here means a future change
/// broke that invariant, not that the caller passed bad input.
#[must_use]
#[allow(clippy::expect_used)]
pub(crate) fn run(args: &VersionArgs, env: &mut Env<'_>, format_json: bool) -> u8 {
    let output = resolve_output(args.output, format_json);
    let rendered = BuildInfo::current()
        .render(output.as_format(), args.full)
        .expect("BuildInfo always serializes; see # Panics above");
    let _ = writeln!(env.stdout, "{rendered}");
    exit::OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn each_choice_maps_to_its_library_format() {
        assert_eq!(Output::Plain.as_format(), Format::Plain);
        assert_eq!(Output::Json.as_format(), Format::Json);
        assert_eq!(Output::Toml.as_format(), Format::Toml);
    }

    #[test]
    fn short_form_prints_the_version_line() {
        let (code, out, _err) = drive(&["glaucus", "version"], "");
        assert_eq!(code, exit::OK);
        assert!(out.trim_start().starts_with('v'), "{out}");
    }

    #[test]
    fn full_plain_names_every_provenance_field() {
        let (code, out, _err) = drive(&["glaucus", "version", "--full"], "");
        assert_eq!(code, exit::OK);
        for expected in ["commit:", "built:", "rustc:", "target:"] {
            assert!(out.contains(expected), "missing {expected} in: {out}");
        }
    }

    #[test]
    fn every_output_choice_runs_short_and_full() {
        for output in ["plain", "json", "toml"] {
            let (code, out, _err) = drive(&["glaucus", "version", "--output", output], "");
            assert_eq!(code, exit::OK, "{output} short");
            assert!(!out.is_empty(), "{output} short produced nothing");

            let (code, out, _err) =
                drive(&["glaucus", "version", "--full", "--output", output], "");
            assert_eq!(code, exit::OK, "{output} full");
            assert!(!out.is_empty(), "{output} full produced nothing");
        }
    }

    #[test]
    fn short_output_flag_works() {
        let (code, out, _err) = drive(&["glaucus", "version", "-o", "json"], "");
        assert_eq!(code, exit::OK);
        assert!(!out.is_empty());
    }

    #[test]
    fn json_output_is_parseable() {
        let (code, out, _err) = drive(&["glaucus", "version", "--full", "--output", "json"], "");
        assert_eq!(code, exit::OK);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(value["version"].is_string());
    }

    #[test]
    fn bad_output_value_is_a_usage_error() {
        let (code, _out, err) = drive(&["glaucus", "version", "--output", "yaml"], "");
        assert_eq!(code, exit::USAGE);
        assert!(err.contains("invalid value"));
    }

    #[test]
    fn resolve_output_prefers_the_explicit_choice_over_the_format_flag() {
        assert_eq!(resolve_output(Some(Output::Toml), true), Output::Toml);
        assert_eq!(resolve_output(Some(Output::Plain), false), Output::Plain);
    }

    #[test]
    fn resolve_output_falls_back_to_the_format_flag_when_output_is_unset() {
        assert_eq!(resolve_output(None, true), Output::Json);
        assert_eq!(resolve_output(None, false), Output::Plain);
    }

    // Blocker 3: `--format` is `global = true`, so clap advertises it on
    // every subcommand including `version` -- but `version::run` used to
    // ignore it entirely. Before the fix, `glaucus version --format json`
    // printed the plain `vX.Y.Z` line and exited 0: the user asked for JSON,
    // got plain text, and got no error.
    #[test]
    fn format_flag_selects_json_like_the_output_flag() {
        let (code, out, _err) = drive(&["glaucus", "version", "--format", "json"], "");
        assert_eq!(code, exit::OK);
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(value["version"].is_string(), "not json:\n{out}");
    }

    #[test]
    fn explicit_output_flag_wins_over_a_conflicting_format_flag() {
        let (code, out, _err) = drive(
            &["glaucus", "version", "--format", "json", "--output", "toml"],
            "",
        );
        assert_eq!(code, exit::OK);
        assert!(
            out.trim_start().starts_with("version ="),
            "not toml:\n{out}"
        );
    }
}
