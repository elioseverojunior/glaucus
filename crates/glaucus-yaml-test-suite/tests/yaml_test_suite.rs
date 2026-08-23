// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests using the official YAML test suite.
//!
//! Runs all test cases from <https://github.com/yaml/yaml-test-suite>
//! using the `data/` directory format and reports pass/fail statistics.

use glaucus_yaml_test_suite::{TestResult, load_all_tests, run_test};
use std::path::Path;

#[test]
fn yaml_test_suite() {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");

    // A missing corpus must FAIL, not skip. The pass-rate floor at the bottom of
    // this function is worthless if the whole test can opt out of running because
    // a directory happens to be absent -- that is the same "the declared guarantee
    // is not the enforced guarantee" defect the floor itself had. Every workflow
    // checks out with `submodules: "recursive"`, so in CI this branch can only mean
    // the checkout is broken, and the run must go red rather than green-and-empty.
    //
    // The escape hatch is deliberately opt-IN and explicit: a contributor running
    // some unrelated test locally without the submodule sets the variable and
    // thereby states that they are knowingly not testing conformance. CI never
    // sets it, so CI can never take this branch silently.
    if !data_dir.exists() {
        assert!(
            std::env::var_os("GLAUCUS_ALLOW_MISSING_TEST_SUITE").is_some(),
            "YAML test suite data not found at {}.\n\
             Run: git submodule update --init --recursive\n\
             To skip deliberately (never in CI): GLAUCUS_ALLOW_MISSING_TEST_SUITE=1",
            data_dir.display()
        );
        eprintln!(
            "YAML test suite data not found at {}; skipping by explicit opt-out.",
            data_dir.display()
        );
        return;
    }

    let all_tests = load_all_tests(&data_dir);

    let mut total = 0usize;
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for tc in &all_tests {
        total += 1;

        match run_test(tc) {
            TestResult::Pass => passed += 1,
            TestResult::Fail(reason) => {
                failed += 1;
                failures.push(format!("{} ({}):\n{reason}", tc.id, tc.name));
            }
        }
    }

    // Counts of test cases, in the low thousands — nowhere near the 2^53 where
    // an integer stops being exactly representable as f64. This is a progress
    // percentage for the report, not a measured value.
    #[allow(clippy::cast_precision_loss)]
    let pass_rate = if total > 0 {
        (passed as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    eprintln!();
    eprintln!("╔══════════════════════════════════════╗");
    eprintln!("║     YAML Test Suite Results          ║");
    eprintln!("╠══════════════════════════════════════╣");
    eprintln!("║  Total:   {total:>5}                      ║");
    eprintln!("║  Passed:  {passed:>5}                      ║");
    eprintln!("║  Failed:  {failed:>5}                      ║");
    eprintln!("║  Skipped:     0                      ║");
    eprintln!("║  Rate:   {pass_rate:>5.1}%                      ║");
    eprintln!("╚══════════════════════════════════════╝");

    // Categorize failures
    let mut cat_wrong_accept = 0usize; // accepted invalid YAML
    let mut cat_wrong_reject = 0usize; // rejected valid YAML
    let mut cat_wrong_events = 0usize; // wrong event stream

    for f in &failures {
        if f.contains("expected error but parser succeeded") {
            cat_wrong_accept += 1;
        } else if f.contains("unexpected error") {
            cat_wrong_reject += 1;
        } else {
            cat_wrong_events += 1;
        }
    }

    eprintln!();
    eprintln!("  Failure breakdown:");
    eprintln!("    Wrong accept (should reject): {cat_wrong_accept}");
    eprintln!("    Wrong reject (should accept): {cat_wrong_reject}");
    eprintln!("    Wrong events (diff):          {cat_wrong_events}");

    if !failures.is_empty() {
        eprintln!();
        eprintln!("=== All failures ===");
        for f in &failures {
            eprintln!();
            eprintln!("{f}");
        }
    }

    // Conformance is 735/735 and the invariant is 100%. This assertion is what
    // makes that an invariant rather than a claim.
    //
    // It read `>= 10.0` until this change -- a floor two orders of magnitude below
    // the documented guarantee, under which a regression to 50% still reported
    // green in `cargo test`. Do NOT lower it. Every issue in the parity series
    // edits the scanner, composer or deserialiser, which is precisely the code
    // this corpus exists to protect; a real floor is what turns a regression there
    // into a red build instead of a silent loss of conformance.
    //
    // `total == 0` (an empty or partial submodule checkout) yields a 0.0 rate and
    // therefore also fails here, which is the honest outcome.
    assert!(
        pass_rate >= 100.0,
        "Pass rate {pass_rate:.1}% is below the required 100% \
         ({passed}/{total} passed, {failed} failed)"
    );
}
