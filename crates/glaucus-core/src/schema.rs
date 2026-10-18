// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! YAML 1.2 Core Schema scalar resolution.
//!
//! Answers the question "what type is this plain scalar?" — is `0x1F` an
//! integer, is `yes` a boolean, is `inf` a float — for every consumer in the
//! workspace.
//!
//! # Why this lives in `glaucus-core`
//!
//! Resolution is a question about the **specification**, not about serde or about
//! schema validation, so every consumer must answer it identically. It was
//! previously written three times — in `glaucus-serde`'s deserialiser and twice
//! in `glaucus-schema` — and the copies diverged: one implemented the `0x`/`0o`
//! radix prefixes and the others did not, so `glaucus schema validate` rejected a
//! document that `glaucus from_str` read as an integer. Another lowercased before
//! matching booleans and accepted `tRuE`, a spelling the specification does not
//! list.
//!
//! These functions are the single answer. They belong beside
//! [`ScalarStyle`](crate::types::ScalarStyle), [`Tag`](crate::types::Tag) and
//! [`YamlVersion`], which describe the same domain.
//!
//! # Scope
//!
//! **Implicit resolution only.** An explicit tag (`!!str 123`) overrides these
//! answers, and presentation style does too — a quoted scalar is always a string.
//! Neither is visible here: these take a `&str` and nothing else, so the caller
//! that can see the tag and the style is the caller that must check them first.
//!
//! Every function is a pure `&str` → `Option<T>` and allocates nothing, which is
//! what keeps `glaucus-core`'s dependency table empty.

use crate::types::YamlVersion;

/// Returns `true` if `value` is the YAML 1.2 null production.
///
/// That is `null`, `Null`, `NULL`, `~`, or the empty scalar.
///
/// The empty case is why the caller must check presentation style first: an
/// empty *plain* scalar is null, but an empty *quoted* scalar (`""`) is the empty
/// string. This function cannot tell them apart.
#[must_use]
pub fn is_null(value: &str) -> bool {
    matches!(value, "null" | "Null" | "NULL" | "~" | "")
}

/// Resolves `value` as a boolean under the given YAML version.
///
/// Matching is **case-sensitive** against the enumerated spellings. The
/// specification lists `true`, `True` and `TRUE`; `tRuE` is not among them, and
/// lowercasing before comparison would accept spellings the grammar does not.
///
/// Under [`YamlVersion::V1_1`] the extended vocabulary — `y`, `yes`, `on` and
/// their negatives — resolves as well. Under [`YamlVersion::V1_2`] those are
/// strings: this is the "Norway problem", where unquoted `NO` becomes `false`.
/// The version is a parameter rather than a global because a `%YAML` directive is
/// document-scoped, so two documents in one stream can answer differently.
#[must_use]
pub fn resolve_bool(value: &str, version: YamlVersion) -> Option<bool> {
    let extended = version.is_1_1();

    if matches!(value, "true" | "True" | "TRUE")
        || (extended
            && matches!(
                value,
                "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON"
            ))
    {
        return Some(true);
    }

    if matches!(value, "false" | "False" | "FALSE")
        || (extended
            && matches!(
                value,
                "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF"
            ))
    {
        return Some(false);
    }

    None
}

/// Resolves `value` as a signed integer.
///
/// Accepts an optional `+`/`-` sign, then either a decimal run, a `0x`/`0X`
/// hexadecimal run, or a `0o`/`0O` octal run.
///
/// A bare leading zero is **not** octal in YAML 1.2 — `017` is seventeen, not
/// fifteen. Only the explicit `0o` prefix selects base 8. (YAML 1.1 did treat
/// leading-zero as octal; that narrowing is deliberate and is not covered by
/// [`YamlVersion`] here.)
#[must_use]
pub fn resolve_int(value: &str) -> Option<i64> {
    let (negative, digits) = value.strip_prefix('-').map_or_else(
        || {
            value
                .strip_prefix('+')
                .map_or((false, value), |rest| (false, rest))
        },
        |rest| (true, rest),
    );

    if digits.is_empty() {
        return None;
    }

    let abs = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()?
    } else if let Some(oct) = digits
        .strip_prefix("0o")
        .or_else(|| digits.strip_prefix("0O"))
    {
        i64::from_str_radix(oct, 8).ok()?
    } else {
        digits.parse::<i64>().ok()?
    };

    if negative { Some(-abs) } else { Some(abs) }
}

/// Resolves `value` as an unsigned integer.
///
/// Refuses a leading `-` outright rather than parsing and then rejecting, so
/// `-0` cannot slip through as zero.
#[must_use]
pub fn resolve_uint(value: &str) -> Option<u64> {
    if value.starts_with('-') {
        return None;
    }
    let digits = value.strip_prefix('+').unwrap_or(value);

    if digits.is_empty() {
        return None;
    }

    if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16).ok();
    }
    digits
        .strip_prefix("0o")
        .or_else(|| digits.strip_prefix("0O"))
        .map_or_else(
            || digits.parse::<u64>().ok(),
            |oct| u64::from_str_radix(oct, 8).ok(),
        )
}

/// Resolves `value` as a float.
///
/// Handles the dotted infinity and NaN forms explicitly, then falls back to
/// Rust's parser **behind a digit guard**. See the comment on the guard: removing
/// it silently reintroduces a defect.
#[must_use]
pub fn resolve_float(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        ".inf" | "+.inf" => Some(f64::INFINITY),
        "-.inf" => Some(f64::NEG_INFINITY),
        ".nan" => Some(f64::NAN),

        // The digit guard exists because Rust's `f64::from_str` also accepts
        // `inf`, `infinity` and `nan`, which YAML 1.2 §10.2 does NOT recognise:
        // the float production admits exactly `[-+]?\.(inf|Inf|INF)` and
        // `\.(nan|NaN|NAN)`, matched above, and the leading dot is mandatory.
        // Bare `inf` / `nan` / `infinity` are plain strings.
        //
        // Requiring at least one ASCII digit is a complete separator rather than a
        // heuristic: every other form in the YAML float grammar
        // (`[-+]?( \.[0-9]+ | [0-9]+(\.[0-9]*)? )([eE][-+]?[0-9]+)?`) contains a
        // digit by construction, while the three bare words contain none.
        //
        // This is the Norway problem's shape -- implicit resolution firing on a
        // word that should stay text -- and worse in consequence. A float infinity
        // has no JSON representation, so an `inf` that resolved as a float becomes
        // `null` on the way out. The value does not change type; it vanishes.
        _ if value.bytes().any(|b| b.is_ascii_digit()) => value.parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_null, resolve_bool, resolve_float, resolve_int, resolve_uint};
    use crate::types::YamlVersion;

    // ─── null ───────────────────────────────────────────────────────

    #[test]
    fn null_accepts_the_enumerated_spellings_and_the_empty_scalar() {
        for v in ["null", "Null", "NULL", "~", ""] {
            assert!(is_null(v), "{v:?} is the null production");
        }
    }

    #[test]
    fn null_is_case_sensitive_and_rejects_everything_else() {
        // `nULL` is not among the listed spellings.
        for v in ["nULL", "NuLl", "nil", "none", "None", "~~", " ", "n"] {
            assert!(!is_null(v), "{v:?} must not resolve as null");
        }
    }

    // ─── bool ───────────────────────────────────────────────────────

    #[test]
    fn bool_resolves_the_core_schema_spellings_in_both_versions() {
        for version in [YamlVersion::V1_2, YamlVersion::V1_1] {
            for v in ["true", "True", "TRUE"] {
                assert_eq!(resolve_bool(v, version), Some(true), "{v:?} @ {version:?}");
            }
            for v in ["false", "False", "FALSE"] {
                assert_eq!(resolve_bool(v, version), Some(false), "{v:?} @ {version:?}");
            }
        }
    }

    #[test]
    fn bool_matching_is_case_sensitive() {
        // A previous copy of this logic lowercased before matching and so accepted
        // `tRuE`. The specification enumerates three spellings; that is not one.
        for v in ["tRuE", "TrUe", "fAlSe", "FALSe"] {
            assert_eq!(
                resolve_bool(v, YamlVersion::V1_2),
                None,
                "{v:?} is not an enumerated spelling"
            );
            assert_eq!(resolve_bool(v, YamlVersion::V1_1), None, "{v:?} under 1.1");
        }
    }

    #[test]
    fn bool_extended_vocabulary_is_1_1_only() {
        // The Norway problem: unquoted `NO` is boolean false under 1.1 and the
        // string "NO" under 1.2.
        let truthy = ["y", "Y", "yes", "Yes", "YES", "on", "On", "ON"];
        let falsy = ["n", "N", "no", "No", "NO", "off", "Off", "OFF"];

        for v in truthy {
            assert_eq!(
                resolve_bool(v, YamlVersion::V1_1),
                Some(true),
                "{v:?} @ 1.1"
            );
            assert_eq!(resolve_bool(v, YamlVersion::V1_2), None, "{v:?} @ 1.2");
        }
        for v in falsy {
            assert_eq!(
                resolve_bool(v, YamlVersion::V1_1),
                Some(false),
                "{v:?} @ 1.1"
            );
            assert_eq!(resolve_bool(v, YamlVersion::V1_2), None, "{v:?} @ 1.2");
        }
    }

    #[test]
    fn bool_rejects_non_booleans_in_both_versions() {
        for v in ["", "t", "f", "1", "0", "yep", "ONN", "of"] {
            for version in [YamlVersion::V1_2, YamlVersion::V1_1] {
                assert_eq!(resolve_bool(v, version), None, "{v:?} @ {version:?}");
            }
        }
    }

    // ─── int ────────────────────────────────────────────────────────

    #[test]
    fn int_resolves_decimal_with_optional_sign() {
        assert_eq!(resolve_int("0"), Some(0));
        assert_eq!(resolve_int("42"), Some(42));
        assert_eq!(resolve_int("+42"), Some(42));
        assert_eq!(resolve_int("-42"), Some(-42));
        assert_eq!(resolve_int("-0"), Some(0));
    }

    #[test]
    fn int_resolves_both_radix_prefixes_in_both_cases() {
        assert_eq!(resolve_int("0x1F"), Some(31));
        assert_eq!(resolve_int("0X1f"), Some(31));
        assert_eq!(resolve_int("-0x1F"), Some(-31));
        assert_eq!(resolve_int("+0x10"), Some(16));

        assert_eq!(resolve_int("0o52"), Some(42));
        assert_eq!(resolve_int("0O52"), Some(42));
        assert_eq!(resolve_int("-0o52"), Some(-42));
    }

    #[test]
    fn int_treats_a_bare_leading_zero_as_decimal_not_octal() {
        // YAML 1.2 dropped leading-zero octal. `017` is seventeen; only the
        // explicit `0o` prefix selects base 8.
        assert_eq!(resolve_int("017"), Some(17));
        assert_eq!(resolve_int("0755"), Some(755));
        assert_eq!(resolve_int("0o17"), Some(15));
    }

    #[test]
    fn int_rejects_malformed_input() {
        for v in [
            "", "+", "-", "0x", "0X", "0o", "0xZZ", "0o99", "1_000", "abc", " 1", "1 ",
        ] {
            assert_eq!(resolve_int(v), None, "{v:?} must not resolve as an integer");
        }
    }

    #[test]
    fn int_rejects_overflow() {
        assert_eq!(resolve_int("9223372036854775808"), None);
        assert_eq!(resolve_int("0xFFFFFFFFFFFFFFFF"), None);
    }

    // ─── uint ───────────────────────────────────────────────────────

    #[test]
    fn uint_resolves_decimal_and_both_radix_prefixes() {
        assert_eq!(resolve_uint("0"), Some(0));
        assert_eq!(resolve_uint("42"), Some(42));
        assert_eq!(resolve_uint("+42"), Some(42));
        assert_eq!(resolve_uint("0x1F"), Some(31));
        assert_eq!(resolve_uint("0X1f"), Some(31));
        assert_eq!(resolve_uint("0o52"), Some(42));
        assert_eq!(resolve_uint("0O52"), Some(42));
        assert_eq!(resolve_uint("18446744073709551615"), Some(u64::MAX));
    }

    #[test]
    fn uint_refuses_a_leading_minus_outright() {
        // Refused before parsing, so `-0` cannot slip through as zero.
        assert_eq!(resolve_uint("-0"), None);
        assert_eq!(resolve_uint("-1"), None);
        assert_eq!(resolve_uint("-0x1F"), None);
    }

    #[test]
    fn uint_rejects_malformed_input() {
        for v in ["", "+", "-", "0x", "0o", "0xZZ", "1_000", "abc"] {
            assert_eq!(resolve_uint(v), None, "{v:?} must not resolve as unsigned");
        }
    }

    // ─── float ──────────────────────────────────────────────────────

    #[test]
    fn float_resolves_the_ordinary_productions() {
        assert!((resolve_float("1.5").unwrap() - 1.5).abs() < f64::EPSILON);
        assert!((resolve_float("-1.5").unwrap() + 1.5).abs() < f64::EPSILON);
        assert!((resolve_float("+3.25").unwrap() - 3.25).abs() < f64::EPSILON);
        assert!((resolve_float(".5").unwrap() - 0.5).abs() < f64::EPSILON);
        assert!((resolve_float("1e10").unwrap() - 1e10).abs() < f64::EPSILON);
        assert!((resolve_float("-1.5e-3").unwrap() + 0.0015).abs() < 1e-12);
        assert!(resolve_float("0.0").unwrap().abs() < f64::EPSILON);
    }

    /// Regression test for #26. **Do not "simplify" this to a bare
    /// `value.parse::<f64>()`** — Rust's parser accepts `inf`, `infinity` and
    /// `nan`, which YAML 1.2 §10.2 does not. The leading dot is mandatory, and
    /// removing the digit guard in [`resolve_float`] silently reintroduces the
    /// defect: a bare `inf` resolved as a float has no JSON representation, so it
    /// becomes `null` on the way out. The value does not change type; it vanishes.
    #[test]
    fn float_rejects_bare_inf_and_nan_regression_issue_26() {
        for v in [
            "inf",
            "Inf",
            "INF",
            "infinity",
            "Infinity",
            "INFINITY",
            "nan",
            "NaN",
            "NAN",
            "-inf",
            "+inf",
            "-infinity",
        ] {
            assert_eq!(
                resolve_float(v),
                None,
                "{v:?} is a string under YAML 1.2, not a float"
            );
        }
    }

    /// The other half of #26: the dotted forms MUST keep resolving. A guard that
    /// rejected these too would "fix" the bug by breaking the feature.
    #[test]
    fn float_accepts_the_dotted_inf_and_nan_forms_regression_issue_26() {
        for v in [".inf", "+.inf", ".Inf", ".INF"] {
            assert!(
                resolve_float(v).is_some_and(|f| f.is_infinite() && f.is_sign_positive()),
                "{v:?} must be positive infinity"
            );
        }
        for v in ["-.inf", "-.Inf", "-.INF"] {
            assert!(
                resolve_float(v).is_some_and(|f| f.is_infinite() && f.is_sign_negative()),
                "{v:?} must be negative infinity"
            );
        }
        for v in [".nan", ".NaN", ".NAN"] {
            assert!(
                resolve_float(v).is_some_and(f64::is_nan),
                "{v:?} must be NaN"
            );
        }
    }

    #[test]
    fn float_rejects_malformed_input() {
        for v in ["", "+", "-", ".", "abc", "1_000.5", "1.2.3"] {
            assert_eq!(resolve_float(v), None, "{v:?} must not resolve as a float");
        }
    }
}
