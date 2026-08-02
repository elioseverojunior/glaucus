// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Verbosity resolution. CLI flags beat `RUST_LOG` beats the default.

/// Resolves the effective `tracing` filter directive.
///
/// Precedence: an explicit `-v`/`-q` flag wins; otherwise `RUST_LOG`; otherwise
/// `warn`.
#[must_use]
pub fn level_for(verbose: u8, quiet: bool, rust_log: Option<&str>) -> &'static str {
    if quiet {
        return "error";
    }
    match verbose {
        0 => match rust_log {
            Some("trace") => "trace",
            Some("debug") => "debug",
            Some("info") => "info",
            Some("error") => "error",
            // `Some("warn")`, `None`, and any other unrecognised value all fall
            // back to the same default.
            _ => "warn",
        },
        1 => "info",
        _ => "debug",
    }
}

/// Installs the global `tracing` subscriber, writing to stderr.
///
/// Idempotent: a second call is a no-op, which matters because the unit tests
/// drive the CLI many times in one process. `try_init` returns an error on the
/// second call rather than panicking, which is why it is used instead of
/// `init`.
pub fn init(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_warn() {
        assert_eq!(level_for(0, false, None), "warn");
    }

    #[test]
    fn one_v_is_info_two_is_debug() {
        assert_eq!(level_for(1, false, None), "info");
        assert_eq!(level_for(2, false, None), "debug");
        assert_eq!(level_for(9, false, None), "debug");
    }

    #[test]
    fn quiet_is_error() {
        assert_eq!(level_for(0, true, None), "error");
    }

    #[test]
    fn rust_log_applies_only_when_no_flag_given() {
        assert_eq!(level_for(0, false, Some("trace")), "trace");
        // A CLI flag beats the environment.
        assert_eq!(level_for(1, false, Some("trace")), "info");
        assert_eq!(level_for(0, true, Some("trace")), "error");
    }

    #[test]
    fn rust_log_recognises_debug_info_and_error() {
        assert_eq!(level_for(0, false, Some("debug")), "debug");
        assert_eq!(level_for(0, false, Some("info")), "info");
        assert_eq!(level_for(0, false, Some("error")), "error");
    }

    #[test]
    fn init_is_idempotent() {
        // A second call must not panic: `try_init`'s `Err` is discarded
        // rather than propagated, which is the whole point of using it.
        init("warn");
        init("warn");
    }

    // Not driven by any CLI test: `level_for` only ever returns a recognised
    // directive, so this branch is unreachable through `run_with`. Exercised
    // directly here so the 100% coverage gate sees it, and so `init` never
    // panics on a directive it cannot parse.
    #[test]
    fn init_falls_back_to_warn_on_an_unparsable_directive() {
        init("cli=noisy");
    }
}
