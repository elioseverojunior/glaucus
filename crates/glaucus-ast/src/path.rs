// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing for dotted, indexed node paths.
//!
//! `glaucus_cst::Document::get` already resolves this syntax against the source.
//! `Node::get_path` and `Node::query` are a second and third consumer, and a
//! library whose layers answer different path dialects has a defect rather than a
//! feature — so the syntax is parsed in one place.
//!
//! # Syntax
//!
//! `.`-separated segments:
//!
//! - A segment parsing as a non-negative integer is a sequence index —
//!   `items.1.name`.
//! - Anything else is a mapping key.
//! - A negative number is a **key**, never an index: sequence indices are
//!   non-negative, so `-1` can only name a key.
//! - An empty segment — produced by `..` — is recursive descent, consumed by
//!   `query` and rejected by plain lookup.
//! - The empty path yields no segments and addresses the root.

/// One step of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Segment<'a> {
    /// A mapping key.
    Key(&'a str),
    /// A sequence index, carrying the text it was parsed from.
    ///
    /// The text is retained deliberately. `Document::get` decides index-vs-key by
    /// CONTEXT, not by the segment alone: `resolve_sequence` parses the segment as
    /// a `usize` while `resolve_mapping` compares it as text, so `1` addresses
    /// index 1 of a sequence *and* the key `"1"` of a mapping. Classifying it as
    /// an index and discarding the text would silently drop the second case and
    /// make the two layers disagree — the exact defect this module exists to
    /// prevent. Consumers reach for [`as_key`](Segment::as_key) when the node is a
    /// mapping and [`as_index`](Segment::as_index) when it is a sequence.
    Index(usize, &'a str),
    /// Recursive descent, written `..`.
    Descend,
}

impl<'a> Segment<'a> {
    /// The segment's text when addressing a mapping, or `None` for [`Descend`].
    ///
    /// [`Descend`]: Segment::Descend
    pub(crate) const fn as_key(&self) -> Option<&'a str> {
        match self {
            Segment::Key(k) | Segment::Index(_, k) => Some(k),
            Segment::Descend => None,
        }
    }

    /// The segment's value when addressing a sequence, or `None`.
    pub(crate) const fn as_index(&self) -> Option<usize> {
        match self {
            Segment::Index(i, _) => Some(*i),
            Segment::Key(_) | Segment::Descend => None,
        }
    }
}

/// Splits `path` into segments.
///
/// Never fails: any text is a valid mapping key, so there is no malformed path —
/// only one that does not resolve. Allocates the `Vec`; the segments borrow from
/// `path`.
pub(crate) fn parse_path(path: &str) -> Vec<Segment<'_>> {
    if path.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<Segment<'_>> = Vec::new();

    for raw in path.split('.') {
        let segment = if raw.is_empty() {
            Segment::Descend
        } else if let Ok(idx) = raw.parse::<usize>() {
            // `parse::<usize>` rejects a leading `-`, which is what makes a
            // negative number a key rather than an index.
            Segment::Index(idx, raw)
        } else {
            Segment::Key(raw)
        };

        // Consecutive descents collapse. Descent is idempotent -- widening to
        // "this node and everything below it" a second time reaches nothing new,
        // it only revisits, which would report the same match more than once.
        //
        // This is not a corner case: a LEADING `..` produces two empty segments,
        // because `"..a".split('.')` is `["", "", "a"]`. Without collapsing,
        // `..a` would descend twice and duplicate every nested match, while the
        // otherwise identical `a..b` -- one empty segment -- would not.
        if segment == Segment::Descend && out.last() == Some(&Segment::Descend) {
            continue;
        }
        out.push(segment);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{Segment, parse_path};

    #[test]
    fn the_empty_path_addresses_the_root() {
        let got = parse_path("");
        assert!(got.is_empty(), "expected empty, got {got:?}");
    }

    #[test]
    fn dotted_keys_split_into_keys() {
        assert_eq!(
            parse_path("a.b.c"),
            vec![Segment::Key("a"), Segment::Key("b"), Segment::Key("c")]
        );
    }

    #[test]
    fn non_negative_integers_are_indices() {
        assert_eq!(
            parse_path("items.1.name"),
            vec![
                Segment::Key("items"),
                Segment::Index(1, "1"),
                Segment::Key("name"),
            ]
        );
        assert_eq!(parse_path("0"), vec![Segment::Index(0, "0")]);
    }

    /// Sequence indices are non-negative, so a negative number can only name a
    /// key. `parse::<usize>` rejecting the leading `-` is what enforces this.
    #[test]
    fn negative_numbers_are_keys_not_indices() {
        assert_eq!(parse_path("-1"), vec![Segment::Key("-1")]);
        assert_eq!(
            parse_path("a.-1.b"),
            vec![Segment::Key("a"), Segment::Key("-1"), Segment::Key("b")]
        );
    }

    #[test]
    fn other_numeric_shapes_are_keys() {
        // `+1` IS an index: `parse::<usize>` accepts a leading `+`, and
        // `Document::get` classifies with the same call, so treating it as a key
        // here would be the divergence this module exists to prevent. Verified
        // against `Document::get` directly, not assumed.
        assert_eq!(parse_path("+1"), vec![Segment::Index(1, "+1")]);
        assert_eq!(
            parse_path("1.5"),
            vec![Segment::Index(1, "1"), Segment::Index(5, "5")]
        );
        assert_eq!(parse_path("0x1F"), vec![Segment::Key("0x1F")]);
    }

    #[test]
    fn a_double_dot_is_recursive_descent() {
        assert_eq!(
            parse_path("a..b"),
            vec![Segment::Key("a"), Segment::Descend, Segment::Key("b")]
        );
    }

    #[test]
    fn descent_at_the_start_is_a_leading_empty_segment() {
        assert_eq!(parse_path(".a"), vec![Segment::Descend, Segment::Key("a")]);
        // `"..a".split('.')` is `["", "", "a"]`, but the two descents collapse:
        // descent is idempotent, so `..a` and `.a` both mean "search from here
        // downward" and must not report nested matches twice.
        assert_eq!(parse_path("..a"), vec![Segment::Descend, Segment::Key("a")]);
        assert_eq!(
            parse_path("...a"),
            vec![Segment::Descend, Segment::Key("a")]
        );
        assert_eq!(
            parse_path("a..b"),
            vec![Segment::Key("a"), Segment::Descend, Segment::Key("b")]
        );
    }

    #[test]
    fn descent_at_the_end_is_a_trailing_empty_segment() {
        assert_eq!(parse_path("a."), vec![Segment::Key("a"), Segment::Descend]);
    }

    /// An index must remain usable as a mapping key, because `Document::get`
    /// decides by context: `1` is index 1 of a sequence AND key "1" of a mapping.
    #[test]
    fn an_index_is_still_addressable_as_a_key() {
        let seg = parse_path("1")[0];
        assert_eq!(seg.as_index(), Some(1));
        assert_eq!(seg.as_key(), Some("1"));
    }

    #[test]
    fn a_key_has_no_index_and_descend_has_neither() {
        let key = parse_path("name")[0];
        assert_eq!(key.as_key(), Some("name"));
        assert_eq!(key.as_index(), None);

        let descend = parse_path("a..b")[1];
        assert_eq!(descend.as_key(), None);
        assert_eq!(descend.as_index(), None);
    }
}
