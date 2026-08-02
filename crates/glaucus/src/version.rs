// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Version and build provenance, shared by every crate that reports a version.
//!
//! The values come from `build.rs`, which sits in this crate so that one
//! capture serves every surface rather than each binary carrying its own build
//! script. Both `glaucus` and the `glaucus-cli` wrapper call
//! `glaucus::cli::process::main()`, so both inherit it from here.
//!
//! [`crate::version::LONG_VERSION`] is unconditional: it costs nothing at
//! runtime (a `concat!` of compile-time literals) and needs no dependencies,
//! so it stays useful to library users regardless of which features are
//! active. Everything else here — `BuildInfo`, `Format` and the stamp parsing
//! — needs `serde_json` and `toml`, both gated behind the `cli` feature, so
//! those items are gated the same way. (Plain code font, not an intra-doc
//! link: both types only exist when `cli` is enabled, and this module doc
//! compiles regardless of that feature.)

#[cfg(feature = "cli")]
use serde::Serialize;
#[cfg(feature = "cli")]
use serde_json::{Map, Value};

/// Build provenance for a `--version` long form, assembled at compile time.
///
/// `concat!` so it costs nothing at runtime: every part is a literal that
/// `build.rs` handed to rustc. It lives here rather than in a binary because
/// `rustc-env` reaches only the package whose build script set it, so a const
/// in any other crate could not see these values.
///
/// This reports `CARGO_PKG_VERSION`, not the `GitVersion` `SemVer`: the stamp
/// needs parsing, which no const can do. `BuildInfo::short` (only present
/// under the `cli` feature) is the stamp-derived answer.
pub const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\ncommit:  ",
    env!("GIT_COMMIT"),
    "\nbuilt:   ",
    env!("BUILD_TIMESTAMP"),
    "\nrustc:   ",
    env!("RUSTC_VERSION"),
    "\ntarget:  ",
    env!("TARGET"),
);

/// How version information is rendered.
///
/// No `Default`: the caller's argument parser supplies the default, and a
/// derived one here would be an impl nothing calls.
#[cfg(feature = "cli")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable lines.
    Plain,
    /// A JSON object.
    Json,
    /// A TOML table.
    Toml,
}

/// The build provenance of a compiled binary.
#[cfg(feature = "cli")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildInfo {
    /// Release version, `SemVer` from the `GitVersion` stamp where there is one.
    pub version: String,
    /// Short commit hash, suffixed `-dirty` when the tree had local changes.
    pub commit: String,
    /// Build time, honouring `SOURCE_DATE_EPOCH`.
    pub built: String,
    /// The `rustc` that compiled it.
    pub rustc: String,
    /// The target triple it was compiled for.
    pub target: String,
    /// The `GitVersion` stamp, empty when gitversion was unavailable at build time.
    pub gitversion: Map<String, Value>,
}

#[cfg(feature = "cli")]
impl BuildInfo {
    /// The provenance of the running binary.
    #[must_use]
    pub fn current() -> Self {
        Self::from_parts(
            env!("CARGO_PKG_VERSION"),
            env!("GIT_COMMIT"),
            env!("BUILD_TIMESTAMP"),
            env!("RUSTC_VERSION"),
            env!("TARGET"),
            env!("GITVERSION_JSON"),
        )
    }

    /// Assemble from captured parts, deriving the version from `stamp`.
    #[must_use]
    pub fn from_parts(
        package_version: &str,
        commit: &str,
        built: &str,
        rustc: &str,
        target: &str,
        stamp: &str,
    ) -> Self {
        let gitversion = parse_stamp(stamp);
        let version = gitversion
            .get("SemVer")
            .and_then(Value::as_str)
            .unwrap_or(package_version)
            .to_owned();

        Self {
            version,
            commit: commit.to_owned(),
            built: built.to_owned(),
            rustc: rustc.to_owned(),
            target: target.to_owned(),
            gitversion,
        }
    }

    /// The bare version line, e.g. `v0.0.1-1`.
    #[must_use]
    pub fn short(&self) -> String {
        format!("v{}", self.version)
    }

    /// Render, either the short line or the full provenance.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be serialized to the requested
    /// format.
    pub fn render(&self, format: Format, full: bool) -> anyhow::Result<String> {
        if !full {
            return Ok(match format {
                Format::Plain => self.short(),
                Format::Json => format!("{{\n  \"version\": \"{}\"\n}}", self.short()),
                Format::Toml => format!("version = \"{}\"", self.short()),
            });
        }

        match format {
            Format::Plain => Ok(self.plain()),
            Format::Json => Ok(serde_json::to_string_pretty(self)?),
            // TOML has no null, so a stamp key with one cannot be represented.
            // Dropping those keys keeps `--format toml` usable; refusing to
            // render would make it useless on every real gitversion stamp.
            Format::Toml => {
                let mut sanitized = self.clone();
                sanitized.gitversion.retain(|_, value| !value.is_null());
                Ok(toml::to_string(&sanitized)?)
            }
        }
    }

    /// The human-readable full form, laid out like `--version` already was.
    fn plain(&self) -> String {
        let mut out = format!(
            "{}\ncommit:  {}\nbuilt:   {}\nrustc:   {}\ntarget:  {}",
            self.short(),
            self.commit,
            self.built,
            self.rustc,
            self.target
        );

        if !self.gitversion.is_empty() {
            let stamp: Vec<String> = self
                .gitversion
                .iter()
                .map(|(key, value)| format!("  {key}: {}", scalar(value)))
                .collect();

            out.push_str("\ngitversion:\n");
            out.push_str(&stamp.join("\n"));
        }
        out
    }
}

/// Parse the baked stamp, adding the key `MajorMinor` synthesizes.
///
/// A stamp that will not parse is treated as absent: the binary still has to
/// report a truthful version, and the package version is the other source.
#[cfg(feature = "cli")]
fn parse_stamp(stamp: &str) -> Map<String, Value> {
    let mut stamp: Map<String, Value> = serde_json::from_str(stamp).unwrap_or_default();

    // Four-part .NET assembly versions (`0.0.1.0`). Nothing in a Rust project
    // consumes them, and they render as two more disagreeing versions beside
    // `SemVer` and `MajorMinorPatch`. Dropped here rather than in `build.rs`
    // because this is already the one place that normalises the stamp, and
    // `build.rs` handles it as raw text with no JSON parser.
    stamp.remove("AssemblySemVer");
    stamp.remove("AssemblySemFileVer");

    if let (Some(major), Some(minor)) = (stamp.get("Major"), stamp.get("Minor")) {
        let combined = Value::from(format!("{major}.{minor}"));
        stamp.insert("MajorMinor".to_owned(), combined);
    }
    stamp
}

/// A stamp value as a bare scalar -- a string without its JSON quotes.
#[cfg(feature = "cli")]
fn scalar(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), ToOwned::to_owned)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_long_version_reports_build_provenance() {
        assert!(
            LONG_VERSION.starts_with(env!("CARGO_PKG_VERSION")),
            "the release version has to lead: {LONG_VERSION}"
        );
        for label in ["commit:", "built:", "rustc:", "target:"] {
            assert!(
                LONG_VERSION.contains(label),
                "no {label} line: {LONG_VERSION}"
            );
        }
    }

    /// A trimmed `GitVersion` stamp, including the `null` that TOML cannot hold.
    //
    // Every field has to agree: this is a captured `gitversion /output json`, and
    // it cannot report 0.0.1 in `SemVer` and 0.1.0 in `MajorMinorPatch`. The
    // comma after `SemVer` is load-bearing -- without it the JSON does not parse,
    // `parse_stamp` returns an empty map, and `version` silently falls back to
    // the package version rather than failing.
    #[cfg(feature = "cli")]
    const STAMP: &str = r#"{"Major":0,"Minor":0,"Patch":1,"SemVer":"0.0.1-1",
        "MajorMinorPatch":"0.0.1","BranchName":"main","BuildMetaData":null}"#;

    #[cfg(feature = "cli")]
    fn info(stamp: &str) -> BuildInfo {
        BuildInfo::from_parts(
            "9.9.9",
            "abc123-dirty",
            "2026-07-29 00:00:00 UTC",
            "rustc 1.97.1",
            "x86_64-apple-darwin",
            stamp,
        )
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_running_binarys_provenance_comes_from_the_build_script() {
        // The only path that reads build.rs's output, so nothing else would
        // catch a `rustc-env` key being renamed or dropped.
        let built = BuildInfo::current();

        assert!(!built.commit.is_empty(), "no commit captured");
        assert!(!built.target.is_empty(), "no target captured");
        assert!(built.short().starts_with('v'), "got {}", built.short());
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_short_form_prefixes_the_semver_from_the_stamp() {
        assert_eq!(info(STAMP).short(), "v0.0.1-1");
    }

    #[cfg(feature = "cli")]
    #[test]
    fn an_absent_stamp_falls_back_to_the_package_version() {
        // A build on a machine without gitversion still has to report something
        // truthful, and the package version is the only other source.
        assert_eq!(info("{}").short(), "v9.9.9");
    }

    #[cfg(feature = "cli")]
    #[test]
    fn a_malformed_stamp_is_treated_as_absent_rather_than_failing() {
        assert_eq!(info("not json at all").short(), "v9.9.9");
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_dotnet_assembly_versions_are_dropped() {
        // Four-part .NET assembly versions that nothing in a Rust project reads.
        // Left in, `version --full` shows `0.0.1.0` twice beside `SemVer` and
        // `MajorMinorPatch`, reading as four disagreeing versions.
        let built = info(
            r#"{"Major":0,"Minor":0,"Patch":1,"SemVer":"0.0.1-1",
               "AssemblySemVer":"0.0.1.0","AssemblySemFileVer":"0.0.1.0"}"#,
        );

        for dropped in ["AssemblySemVer", "AssemblySemFileVer"] {
            assert!(
                !built.gitversion.contains_key(dropped),
                "{dropped} survived into the stamp"
            );
        }
        assert_eq!(built.short(), "v0.0.1-1", "the version still comes through");
    }

    #[cfg(feature = "cli")]
    #[test]
    fn major_minor_is_synthesized_from_the_stamp() {
        // Raw gitversion has no `MajorMinor` key, so anything reading the stamp
        // has to supply it or lose the field.
        let built = info(STAMP);
        assert_eq!(
            built.gitversion.get("MajorMinor").and_then(Value::as_str),
            Some("0.0")
        );
    }

    #[cfg(feature = "cli")]
    #[test]
    fn without_full_every_format_yields_only_the_short_line() {
        for format in [Format::Plain, Format::Json, Format::Toml] {
            let out = info(STAMP).render(format, false).unwrap();
            assert!(out.contains("v0.0.1-1"), "{format:?}: {out}");
            assert!(
                !out.contains("rustc"),
                "{format:?} leaked provenance: {out}"
            );
        }
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_full_plain_form_names_every_provenance_field() {
        let out = info(STAMP).render(Format::Plain, true).unwrap();

        for expected in [
            "v0.0.1-1",
            "abc123-dirty",
            "2026-07-29 00:00:00 UTC",
            "rustc 1.97.1",
            "x86_64-apple-darwin",
            "BranchName",
        ] {
            assert!(out.contains(expected), "missing {expected} in: {out}");
        }
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_full_plain_form_omits_the_stamp_section_when_there_is_no_stamp() {
        // A bare `gitversion:` heading with nothing under it reads as a stamp
        // that failed to render, rather than one that was never captured.
        let out = info("{}").render(Format::Plain, true).unwrap();

        assert!(out.contains("v9.9.9"), "{out}");
        assert!(!out.contains("gitversion:"), "empty heading shown: {out}");
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_same_parts_always_describe_the_same_build() {
        // The struct is a value: two reads of one binary's provenance must not
        // differ, or `--full --format json` would be unstable between
        // invocations.
        assert_eq!(info(STAMP), info(STAMP));
        assert!(format!("{:?}", info(STAMP)).contains("BuildInfo"));
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_full_json_form_keeps_the_stamp_types() {
        let out = info(STAMP).render(Format::Json, true).unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(parsed["version"], Value::from("0.0.1-1"));
        // A number stays a number: stringifying the whole stamp would make the
        // JSON output lossy against the raw gitversion stamp.
        assert_eq!(parsed["gitversion"]["Major"], Value::from(0));
    }

    #[cfg(feature = "cli")]
    #[test]
    fn the_full_toml_form_drops_entries_toml_cannot_represent() {
        // TOML has no null. Dropping those keys is the only representable
        // choice; failing the render over them would make `--format toml`
        // useless.
        let out = info(STAMP).render(Format::Toml, true).unwrap();

        assert!(out.contains("BranchName"), "{out}");
        assert!(!out.contains("BuildMetaData"), "null survived: {out}");
    }

    // Real GitVersion output is always a flat map of strings/numbers/bools, but
    // the type does not forbid a nested object. This is the shape that would
    // trip the classic "TOML values must precede tables" pitfall if `toml`
    // required callers to pre-sort keys: `AAA_Nested`'s object value sorts
    // before `ZZZ_Scalar`'s string value in the `BTreeMap` iteration order.
    // `toml::to_string` still succeeds -- it reorders internally -- which is
    // the invariant the CLI's `version::run` relies on to treat `render` as
    // infallible for any `BuildInfo`.
    #[cfg(feature = "cli")]
    #[test]
    fn an_unusual_gitversion_shape_still_renders_successfully() {
        let out = info(r#"{"AAA_Nested":{"x":1},"ZZZ_Scalar":"value"}"#)
            .render(Format::Toml, true)
            .unwrap();
        assert!(out.contains("ZZZ_Scalar"), "{out}");
    }
}
// REUSE-IgnoreEnd
