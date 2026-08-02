// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Capture build provenance for [`glaucus::version`].
//!
//! This lived at the workspace root, where a virtual manifest has no package to
//! attach it to, so cargo never ran it and nothing consumed the variables it
//! set. It has to sit inside a package for both reasons: to be compiled at
//! all, and to be included in the published crate tarball.
//!
//! It sits in the library rather than a binary because a build script's
//! `rustc-env` reaches only its own package. Capturing here is what lets every
//! surface crate report the same provenance through one module instead of each
//! carrying a copy of this file — both `glaucus` and the `glaucus-cli` wrapper
//! call `glaucus::cli::process::main()`, so both inherit it from here.

use std::process::Command;

fn main() {
    println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_version());
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_timestamp());
    println!("cargo:rustc-env=GIT_COMMIT={}", git_commit());
    println!("cargo:rustc-env=GITVERSION_JSON={}", gitversion_json());

    // Cargo always sets TARGET for a build script, so there is no host-guessing
    // fallback here. A hardcoded `-unknown-linux-gnu` fallback would have
    // mislabelled every macOS and Windows build had it ever run.
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned())
    );

    println!("cargo:rerun-if-env-changed=RUSTC_VERSION");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}

fn rustc_version() -> String {
    if let Ok(version) = std::env::var("RUSTC_VERSION") {
        return version;
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map_or_else(
            || "unknown".to_owned(),
            |out| String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        )
}

/// Honour `SOURCE_DATE_EPOCH` so the binary stays reproducible.
///
/// Stamping wall-clock time makes two builds of identical source differ, which
/// undercuts the provenance the rest of this file exists to provide.
fn build_timestamp() -> String {
    let epoch = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok());

    epoch.map_or_else(
        || fmt(chrono::Utc::now()),
        |seconds| {
            chrono::DateTime::from_timestamp(seconds, 0).map_or_else(|| "unknown".to_owned(), fmt)
        },
    )
}

fn fmt(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// The short commit hash, suffixed `-dirty` when the tree has local changes.
///
/// Absent git, or in a source tarball with no repository, this yields `unknown`
/// rather than failing the build.
fn git_commit() -> String {
    let Some(hash) = git(&["rev-parse", "--short=12", "HEAD"]) else {
        return "unknown".to_owned();
    };
    match git(&["status", "--porcelain"]) {
        Some(status) if !status.is_empty() => format!("{hash}-dirty"),
        _ => hash,
    }
}

/// The `GitVersion` stamp, or `{}` when gitversion cannot supply one.
///
/// A machine without gitversion still builds, and the version falls back to
/// `CARGO_PKG_VERSION`.
///
/// Newlines are stripped because `cargo:rustc-env` is line-oriented -- a
/// pretty-printed stamp would be truncated at its first line. Only the line
/// breaks go; interior spaces are left alone, since a branch name may contain
/// them and removing all whitespace would corrupt the value.
fn gitversion_json() -> String {
    Command::new("gitversion")
        .args(["/output", "json"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map_or_else(
            || "{}".to_owned(),
            |out| {
                String::from_utf8_lossy(&out.stdout)
                    .replace(['\n', '\r'], "")
                    .trim()
                    .to_owned()
            },
        )
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
