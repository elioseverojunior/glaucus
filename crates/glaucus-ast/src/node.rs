// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The YAML representation tree (`Node`) and its constituent nodes.

use std::borrow::Cow;

use crate::path::{Segment, parse_path};
use glaucus_core::types::{CollectionStyle, ScalarStyle, Span, Tag, YamlVersion};

// ─── Representation Graph ───────────────────────────────────────────

/// A YAML node in the representation graph.
///
/// The lifetime `'a` ties borrowed scalar values to the input source,
/// enabling zero-copy parsing for plain scalars.
#[derive(Debug, Clone, PartialEq)]
pub enum Node<'a> {
    /// A scalar (leaf) value.
    Scalar(Scalar<'a>),
    /// An ordered sequence of nodes.
    Sequence(Sequence<'a>),
    /// An ordered mapping of key-value pairs.
    Mapping(Mapping<'a>),
}

impl<'a> Node<'a> {
    /// Returns the span of this node in the source input.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Node::Scalar(s) => s.span,
            Node::Sequence(s) => s.span,
            Node::Mapping(m) => m.span,
        }
    }

    /// Returns the tag of this node, if any.
    #[must_use]
    pub const fn tag(&self) -> Option<&Tag<'a>> {
        match self {
            Node::Scalar(s) => s.tag.as_ref(),
            Node::Sequence(s) => s.tag.as_ref(),
            Node::Mapping(m) => m.tag.as_ref(),
        }
    }

    /// Returns `true` if this node is a scalar.
    #[must_use]
    pub const fn is_scalar(&self) -> bool {
        matches!(self, Node::Scalar(_))
    }

    /// Returns `true` if this node is a sequence.
    #[must_use]
    pub const fn is_sequence(&self) -> bool {
        matches!(self, Node::Sequence(_))
    }

    /// Returns `true` if this node is a mapping.
    #[must_use]
    pub const fn is_mapping(&self) -> bool {
        matches!(self, Node::Mapping(_))
    }

    /// Returns the scalar value as a string slice, if this is a scalar node.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Node::Scalar(s) => Some(&s.value),
            _ => None,
        }
    }

    /// Follows one path segment, or `None`.
    ///
    /// Index-vs-key is decided by the NODE, not by the segment: a mapping asks
    /// for the segment's text and a sequence for its value, which is how
    /// `glaucus_cst::Document::get` behaves. So `1` addresses index 1 of a
    /// sequence and the key `"1"` of a mapping, and the two layers agree.
    ///
    /// A descent segment always yields `None` here — descent branches, and a
    /// single-result walk cannot express "any depth".
    fn step(&self, segment: Segment<'_>) -> Option<&Self> {
        match self {
            Node::Mapping(m) => {
                let key = segment.as_key()?;
                m.entries
                    .iter()
                    .find(|(k, _)| k.as_str() == Some(key))
                    .map(|(_, v)| v)
            }
            Node::Sequence(s) => s.items.get(segment.as_index()?),
            Node::Scalar(_) => None,
        }
    }

    /// Resolves a dotted, indexed path to the node it addresses.
    ///
    /// Segments are `.`-separated. A segment that parses as a non-negative
    /// integer indexes a sequence; anything else names a mapping key. A negative
    /// number is always a key, since sequence indices are non-negative. The empty
    /// path addresses the root.
    ///
    /// Returns a **borrow**. `get_path("spec")` on a manifest addresses a large
    /// subtree, and returning it by value would copy the whole thing on every
    /// lookup — turning a config-reading loop into a copying loop.
    ///
    /// Returns `None` for an absent key, an out-of-range index, a step into a
    /// scalar, or a `..` descent.
    ///
    /// # Examples
    ///
    /// ```
    /// let node = glaucus_ast::composer::compose_one(
    ///     "spec:\n  containers:\n    - name: app\n      port: 8080\n",
    /// )
    /// .unwrap();
    ///
    /// assert_eq!(
    ///     node.get_path("spec.containers.0.name").and_then(|n| n.as_str()),
    ///     Some("app")
    /// );
    /// assert_eq!(
    ///     node.get_path("spec.containers.0.port").and_then(|n| n.as_i64()),
    ///     Some(8080)
    /// );
    /// assert!(node.get_path("spec.containers.9").is_none());
    /// ```
    #[must_use]
    pub fn get_path(&self, path: &str) -> Option<&Self> {
        let mut current = self;
        for segment in parse_path(path) {
            current = current.step(segment)?;
        }
        Some(current)
    }

    /// Pushes every descendant of `self`, and `self` itself, onto `out`.
    ///
    /// Iterative, over an explicit worklist. A parsed tree cannot exceed
    /// `glaucus_core::limits::MAX_SAFE_DEPTH` (192), so recursion would in fact be
    /// safe here — but that ceiling exists precisely BECAUSE composition is
    /// recursive, and #37 wants that removed. A new recursive walk added now is
    /// one more site to convert later, and `Node::clone` (#36) shows how they
    /// accumulate. A worklist costs nothing here, so it is what this uses.
    fn collect_self_and_descendants<'n>(&'n self, out: &mut Vec<&'n Self>) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            out.push(node);
            match node {
                Node::Sequence(s) => stack.extend(s.items.iter().rev()),
                Node::Mapping(m) => stack.extend(m.entries.iter().map(|(_, v)| v).rev()),
                Node::Scalar(_) => {}
            }
        }
    }

    /// Resolves a path, returning every node it matches, in document order.
    ///
    /// Accepts the same syntax as [`get_path`](Self::get_path), plus `..` for
    /// recursive descent. Without a `..` the result holds at most one element and
    /// agrees with `get_path`.
    ///
    /// `..name` means "match `name` HERE, **or** anywhere below here" — matching
    /// at the current level too, so
    ///
    /// ```text
    /// a: 1
    /// b:
    ///   a: 2
    /// ```
    ///
    /// finds both `a`s, not only the nested one.
    ///
    /// No match is an empty `Vec`, not an error.
    ///
    /// # Examples
    ///
    /// ```
    /// let node = glaucus_ast::composer::compose_one(
    ///     "a: 1\nb:\n  a: 2\n  c:\n    a: 3\n",
    /// )
    /// .unwrap();
    ///
    /// let found: Vec<i64> = node.query("..a").iter().filter_map(|n| n.as_i64()).collect();
    /// assert_eq!(found, vec![1, 2, 3]);
    ///
    /// // Without `..` it agrees with get_path.
    /// assert_eq!(node.query("b.a").len(), 1);
    /// assert!(node.query("nope").is_empty());
    /// ```
    #[must_use]
    pub fn query(&self, path: &str) -> Vec<&Self> {
        let segments = parse_path(path);
        let mut frontier: Vec<&Self> = vec![self];

        for segment in segments {
            let mut next = Vec::new();
            if segment == Segment::Descend {
                // Descent widens the frontier to every node at or below each
                // current node. The following segment then matches against all of
                // them, which is what makes `..a` find `a` here AND below.
                for node in &frontier {
                    node.collect_self_and_descendants(&mut next);
                }
            } else {
                for node in &frontier {
                    if let Some(child) = node.step(segment) {
                        next.push(child);
                    }
                }
            }
            frontier = next;
            if frontier.is_empty() {
                return Vec::new();
            }
        }

        frontier
    }

    /// The scalar's text if this is a **plain** (unquoted) scalar.
    ///
    /// Quoting is the author stating that the value is text, so every typed
    /// accessor below declines a quoted scalar. This helper is what enforces
    /// that: `"true"` is a string, not a boolean, and `""` is the empty string,
    /// not null.
    fn plain_text(&self) -> Option<&str> {
        match self {
            Node::Scalar(s) if s.style == ScalarStyle::Plain => Some(&s.value),
            _ => None,
        }
    }

    /// Returns `true` if this is a plain scalar resolving to null.
    ///
    /// That is `null`, `Null`, `NULL`, `~`, or the empty plain scalar. A quoted
    /// `"null"` is the four-character string and returns `false`, as does any
    /// collection.
    #[must_use]
    pub fn is_null(&self) -> bool {
        self.plain_text().is_some_and(glaucus_core::schema::is_null)
    }

    /// Resolves this node as a boolean, or `None`.
    ///
    /// **YAML 1.2 resolution only.** A `Node` carries no document version, so the
    /// 1.1 spellings — `yes`, `on`, `y` and their negatives — are strings here.
    /// Those stay with the deserialiser, which learns the version from a `%YAML`
    /// directive; inventing an answer without that information would be the
    /// Norway problem by another route.
    ///
    /// Matching is case-sensitive: `true`, `True` and `TRUE` resolve, `tRuE` does
    /// not.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        self.plain_text()
            .and_then(|t| glaucus_core::schema::resolve_bool(t, YamlVersion::V1_2))
    }

    /// Resolves this node as a signed integer, or `None`.
    ///
    /// Accepts the Core Schema radix prefixes: `0x`/`0X` and `0o`/`0O`. A bare
    /// leading zero is decimal — `017` is seventeen.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        self.plain_text()
            .and_then(glaucus_core::schema::resolve_int)
    }

    /// Resolves this node as an unsigned integer, or `None`.
    ///
    /// A leading `-` is refused outright, so `-0` does not become zero.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        self.plain_text()
            .and_then(glaucus_core::schema::resolve_uint)
    }

    /// Resolves this node as a float, or `None`.
    ///
    /// Only the dotted infinity and NaN forms resolve. Bare `inf`, `infinity` and
    /// `nan` are strings under YAML 1.2, whatever Rust's own parser accepts.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        self.plain_text()
            .and_then(glaucus_core::schema::resolve_float)
    }

    /// Returns a reference to the sequence items, if this is a sequence node.
    #[must_use]
    pub fn as_sequence(&self) -> Option<&[Self]> {
        match self {
            Node::Sequence(s) => Some(&s.items),
            _ => None,
        }
    }

    /// Returns a reference to the mapping entries, if this is a mapping node.
    #[must_use]
    pub fn as_mapping(&self) -> Option<&[(Self, Self)]> {
        match self {
            Node::Mapping(m) => Some(&m.entries),
            _ => None,
        }
    }

    /// Converts this node into a `'static` lifetime by taking ownership of all borrowed data.
    #[must_use]
    pub fn into_owned(self) -> Node<'static> {
        match self {
            Node::Scalar(s) => Node::Scalar(s.into_owned()),
            Node::Sequence(s) => Node::Sequence(s.into_owned()),
            Node::Mapping(m) => Node::Mapping(m.into_owned()),
        }
    }
}

/// A YAML scalar (leaf) value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scalar<'a> {
    /// The scalar value, borrowing from the input when no transformation is needed.
    pub value: Cow<'a, str>,
    /// Optional YAML tag.
    pub tag: Option<Tag<'a>>,
    /// The presentation style used in the source.
    pub style: ScalarStyle,
    /// Source span.
    pub span: Span,
}

impl Scalar<'_> {
    /// Converts this scalar into a `'static` lifetime by taking ownership of borrowed data.
    #[must_use]
    pub fn into_owned(self) -> Scalar<'static> {
        Scalar {
            value: Cow::Owned(self.value.into_owned()),
            tag: self.tag.map(Tag::into_owned),
            style: self.style,
            span: self.span,
        }
    }
}

/// A YAML sequence (ordered list).
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence<'a> {
    /// The items in the sequence.
    pub items: Vec<Node<'a>>,
    /// Optional YAML tag.
    pub tag: Option<Tag<'a>>,
    /// The presentation style used in the source.
    pub style: CollectionStyle,
    /// Source span.
    pub span: Span,
}

impl Sequence<'_> {
    /// Converts this sequence into a `'static` lifetime by taking ownership of borrowed data.
    #[must_use]
    pub fn into_owned(self) -> Sequence<'static> {
        Sequence {
            items: self.items.into_iter().map(Node::into_owned).collect(),
            tag: self.tag.map(Tag::into_owned),
            style: self.style,
            span: self.span,
        }
    }
}

/// A YAML mapping (ordered key-value pairs).
///
/// Uses `Vec<(Node, Node)>` instead of `HashMap` to preserve insertion order
/// (required by the YAML spec) and to support non-string keys.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapping<'a> {
    /// The key-value entries in insertion order.
    pub entries: Vec<(Node<'a>, Node<'a>)>,
    /// Optional YAML tag.
    pub tag: Option<Tag<'a>>,
    /// The presentation style used in the source.
    pub style: CollectionStyle,
    /// Source span.
    pub span: Span,
}

impl Mapping<'_> {
    /// Converts this mapping into a `'static` lifetime by taking ownership of borrowed data.
    #[must_use]
    pub fn into_owned(self) -> Mapping<'static> {
        Mapping {
            entries: self
                .entries
                .into_iter()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
            tag: self.tag.map(Tag::into_owned),
            style: self.style,
            span: self.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use glaucus_core::types::{Position, Span};

    use super::*;

    #[test]
    fn node_span_collections() {
        let span = Span {
            start: Position {
                offset: 1,
                line: 1,
                column: 2,
            },
            end: Position {
                offset: 4,
                line: 1,
                column: 5,
            },
        };
        let seq = Node::Sequence(Sequence {
            items: vec![],
            tag: None,
            style: CollectionStyle::Block,
            span,
        });
        assert_eq!(seq.span(), span);

        let map = Node::Mapping(Mapping {
            entries: vec![],
            tag: None,
            style: CollectionStyle::Block,
            span,
        });
        assert_eq!(map.span(), span);
    }

    #[test]
    fn as_str_on_non_scalar() {
        let seq = Node::Sequence(Sequence {
            items: vec![],
            tag: None,
            style: CollectionStyle::Block,
            span: Span::point(Position::start()),
        });
        assert!(seq.as_str().is_none());
    }

    #[test]
    fn node_accessors() {
        let scalar = Node::Scalar(Scalar {
            value: Cow::Borrowed("hello"),
            tag: None,
            style: ScalarStyle::Plain,
            span: Span::point(Position::start()),
        });
        assert!(scalar.is_scalar());
        assert!(!scalar.is_sequence());
        assert!(!scalar.is_mapping());
        assert_eq!(scalar.as_str(), Some("hello"));
        assert!(scalar.as_sequence().is_none());
        assert!(scalar.as_mapping().is_none());
    }

    #[test]
    fn node_sequence_accessor() {
        let seq = Node::Sequence(Sequence {
            items: vec![Node::Scalar(Scalar {
                value: Cow::Borrowed("item"),
                tag: None,
                style: ScalarStyle::Plain,
                span: Span::point(Position::start()),
            })],
            tag: None,
            style: CollectionStyle::Block,
            span: Span::point(Position::start()),
        });
        assert!(seq.is_sequence());
        assert_eq!(seq.as_sequence().unwrap().len(), 1);
    }

    #[test]
    fn scalar_into_owned() {
        let scalar = Scalar {
            value: Cow::Borrowed("hello"),
            tag: Some(Tag {
                value: Cow::Borrowed("!!str"),
                span: Span::point(Position::start()),
            }),
            style: ScalarStyle::Plain,
            span: Span::point(Position::start()),
        };
        let owned: Scalar<'static> = scalar.into_owned();
        assert_eq!(&*owned.value, "hello");
        assert_eq!(&*owned.tag.unwrap().value, "!!str");
    }

    #[test]
    fn node_into_owned_scalar() {
        let node = Node::Scalar(Scalar {
            value: Cow::Borrowed("test"),
            tag: None,
            style: ScalarStyle::Plain,
            span: Span::point(Position::start()),
        });
        let owned: Node<'static> = node.into_owned();
        assert_eq!(owned.as_str(), Some("test"));
    }

    #[test]
    fn node_into_owned_sequence() {
        let node = Node::Sequence(Sequence {
            items: vec![Node::Scalar(Scalar {
                value: Cow::Borrowed("item"),
                tag: None,
                style: ScalarStyle::Plain,
                span: Span::point(Position::start()),
            })],
            tag: None,
            style: CollectionStyle::Block,
            span: Span::point(Position::start()),
        });
        let owned: Node<'static> = node.into_owned();
        assert_eq!(owned.as_sequence().unwrap().len(), 1);
        assert_eq!(owned.as_sequence().unwrap()[0].as_str(), Some("item"));
    }

    #[test]
    fn node_into_owned_mapping() {
        let node = Node::Mapping(Mapping {
            entries: vec![(
                Node::Scalar(Scalar {
                    value: Cow::Borrowed("key"),
                    tag: None,
                    style: ScalarStyle::Plain,
                    span: Span::point(Position::start()),
                }),
                Node::Scalar(Scalar {
                    value: Cow::Borrowed("val"),
                    tag: None,
                    style: ScalarStyle::Plain,
                    span: Span::point(Position::start()),
                }),
            )],
            tag: None,
            style: CollectionStyle::Block,
            span: Span::point(Position::start()),
        });
        let owned: Node<'static> = node.into_owned();
        let entries = owned.as_mapping().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0.as_str(), Some("key"));
        assert_eq!(entries[0].1.as_str(), Some("val"));
    }

    #[test]
    fn node_mapping_accessor() {
        let map = Node::Mapping(Mapping {
            entries: vec![(
                Node::Scalar(Scalar {
                    value: Cow::Borrowed("key"),
                    tag: None,
                    style: ScalarStyle::Plain,
                    span: Span::point(Position::start()),
                }),
                Node::Scalar(Scalar {
                    value: Cow::Borrowed("value"),
                    tag: None,
                    style: ScalarStyle::Plain,
                    span: Span::point(Position::start()),
                }),
            )],
            tag: None,
            style: CollectionStyle::Block,
            span: Span::point(Position::start()),
        });
        assert!(map.is_mapping());
        assert_eq!(map.as_mapping().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod typed_accessor_tests {
    use crate::node::Node;

    fn node(src: &str) -> Node<'static> {
        crate::composer::Composer::new(src)
            .next()
            .unwrap()
            .unwrap()
            .into_owned()
    }

    /// The scalar under key `v`, so quoting survives composition.
    fn v(src: &str) -> Node<'static> {
        node(&format!("v: {src}\n"))
            .as_mapping()
            .unwrap()
            .first()
            .unwrap()
            .1
            .clone()
    }

    #[test]
    fn plain_scalars_resolve() {
        assert!(v("null").is_null());
        assert!(v("~").is_null());
        assert_eq!(v("true").as_bool(), Some(true));
        assert_eq!(v("TRUE").as_bool(), Some(true));
        assert_eq!(v("false").as_bool(), Some(false));
        assert_eq!(v("42").as_i64(), Some(42));
        assert_eq!(v("42").as_u64(), Some(42));
        assert_eq!(v("1.5").as_f64(), Some(1.5));
    }

    #[test]
    fn radix_prefixes_resolve_through_both_integer_accessors() {
        assert_eq!(v("0x1F").as_i64(), Some(31));
        assert_eq!(v("0x1F").as_u64(), Some(31));
        assert_eq!(v("0o17").as_i64(), Some(15));
        assert_eq!(
            v("017").as_i64(),
            Some(17),
            "a bare leading zero is decimal"
        );
    }

    #[test]
    fn negative_resolves_as_signed_but_not_unsigned() {
        assert_eq!(v("-1").as_i64(), Some(-1));
        assert_eq!(v("-1").as_u64(), None);
        assert_eq!(v("-0").as_u64(), None, "`-0` must not become zero");
    }

    /// Quoting is the author stating the value is text. Every typed accessor must
    /// check the style, not just the characters.
    #[test]
    fn quoted_scalars_decline_every_typed_accessor() {
        for src in ["\"true\"", "'true'"] {
            let n = v(src);
            assert_eq!(n.as_bool(), None, "{src} is text");
            assert_eq!(n.as_str(), Some("true"), "{src} still reads as a string");
        }
        assert_eq!(v("\"42\"").as_i64(), None);
        assert_eq!(v("\"42\"").as_u64(), None);
        assert_eq!(v("\"1.5\"").as_f64(), None);
        assert!(!v("\"null\"").is_null(), "a quoted null is a string");
        assert!(
            !v("\"\"").is_null(),
            "an empty quoted scalar is the empty string"
        );
    }

    #[test]
    fn an_empty_plain_scalar_is_null() {
        assert!(v("").is_null());
    }

    #[test]
    fn collections_decline_every_typed_accessor() {
        for src in ["[1]", "{a: 1}"] {
            let n = v(src);
            assert!(!n.is_null(), "{src}");
            assert_eq!(n.as_bool(), None, "{src}");
            assert_eq!(n.as_i64(), None, "{src}");
            assert_eq!(n.as_u64(), None, "{src}");
            assert_eq!(n.as_f64(), None, "{src}");
        }
    }

    /// A `Node` carries no document version, so 1.1 boolean spellings are strings
    /// here. Resolving them would be the Norway problem by another route: the
    /// deserialiser answers differently because it can see the `%YAML` directive.
    #[test]
    fn yaml_1_1_boolean_spellings_are_strings_on_a_node() {
        for src in ["yes", "no", "on", "off", "y", "n", "Yes", "NO"] {
            assert_eq!(v(src).as_bool(), None, "{src} is a string on a Node");
            assert_eq!(v(src).as_str(), Some(src));
        }
    }

    #[test]
    fn boolean_matching_is_case_sensitive() {
        assert_eq!(v("tRuE").as_bool(), None);
        assert_eq!(v("fAlSe").as_bool(), None);
    }

    /// #26 travels with the resolvers: bare words are strings, dotted forms are
    /// floats.
    #[test]
    fn bare_inf_and_nan_are_strings_dotted_forms_are_floats() {
        for src in ["inf", "Infinity", "nan"] {
            assert_eq!(v(src).as_f64(), None, "{src} is a string");
        }
        assert!(v(".inf").as_f64().is_some_and(f64::is_infinite));
        assert!(
            v("-.inf")
                .as_f64()
                .is_some_and(|f| f.is_infinite() && f.is_sign_negative())
        );
        assert!(v(".nan").as_f64().is_some_and(f64::is_nan));
    }
}

#[cfg(test)]
mod get_path_tests {
    use crate::node::Node;

    fn doc() -> Node<'static> {
        crate::composer::compose_one(
            "spec:\n  containers:\n    - name: app\n      port: 8080\n    - name: sidecar\n\
             meta:\n  '1': numeric-key\n  '-1': negative-key\n",
        )
        .unwrap()
        .into_owned()
    }

    #[test]
    fn walks_mappings_and_sequences_in_one_path() {
        let d = doc();
        assert_eq!(
            d.get_path("spec.containers.0.name").and_then(Node::as_str),
            Some("app")
        );
        assert_eq!(
            d.get_path("spec.containers.1.name").and_then(Node::as_str),
            Some("sidecar")
        );
        assert_eq!(
            d.get_path("spec.containers.0.port").and_then(Node::as_i64),
            Some(8080)
        );
    }

    #[test]
    fn the_empty_path_returns_the_root() {
        let d = doc();
        assert!(std::ptr::eq(
            d.get_path("").unwrap(),
            std::ptr::from_ref(&d)
        ));
    }

    #[test]
    fn returns_a_borrow_of_the_subtree_not_a_copy() {
        let d = doc();
        let sub = d.get_path("spec.containers").unwrap();
        assert!(std::ptr::eq(sub, d.get_path("spec.containers").unwrap()));
        assert_eq!(sub.as_sequence().map(<[Node<'_>]>::len), Some(2));
    }

    #[test]
    fn absent_key_returns_none() {
        assert!(doc().get_path("spec.missing").is_none());
    }

    #[test]
    fn index_out_of_range_returns_none() {
        assert!(doc().get_path("spec.containers.9").is_none());
    }

    #[test]
    fn stepping_into_a_scalar_returns_none() {
        assert!(doc().get_path("spec.containers.0.name.deeper").is_none());
    }

    #[test]
    fn indexing_a_mapping_that_has_no_such_key_returns_none() {
        // `spec` is a mapping with no key "0", so an index finds nothing.
        assert!(doc().get_path("spec.0").is_none());
    }

    /// A numeric segment must still address a numeric mapping KEY, because
    /// `Document::get` resolves it that way. Losing this was the reason
    /// `Segment::Index` retains the text it was parsed from (#42).
    #[test]
    fn a_numeric_segment_addresses_a_numeric_mapping_key() {
        assert_eq!(
            doc().get_path("meta.1").and_then(Node::as_str),
            Some("numeric-key")
        );
    }

    /// Sequence indices are non-negative, so `-1` can only be a key.
    #[test]
    fn a_negative_segment_addresses_a_key_never_an_index() {
        assert_eq!(
            doc().get_path("meta.-1").and_then(Node::as_str),
            Some("negative-key")
        );
        assert!(doc().get_path("spec.containers.-1").is_none());
    }

    /// Descent branches; a single-result walk cannot express it. `query` does.
    #[test]
    fn a_descent_segment_returns_none() {
        assert!(doc().get_path("spec..name").is_none());
        assert!(doc().get_path(".spec").is_none());
    }
}

#[cfg(test)]
mod query_tests {
    use crate::node::Node;

    fn n(src: &str) -> Node<'static> {
        crate::composer::compose_one(src).unwrap().into_owned()
    }

    fn ints(results: &[&Node<'_>]) -> Vec<i64> {
        results.iter().filter_map(|x| x.as_i64()).collect()
    }

    /// Descent must match at the CURRENT level as well as below. Matching only
    /// the nested one is the obvious wrong implementation.
    #[test]
    fn descent_matches_here_and_below() {
        let d = n("a: 1\nb:\n  a: 2\n");
        assert_eq!(ints(&d.query("..a")), vec![1, 2]);
    }

    #[test]
    fn descent_finds_matches_at_several_depths_in_document_order() {
        let d = n("a: 1\nb:\n  a: 2\n  c:\n    a: 3\n");
        assert_eq!(ints(&d.query("..a")), vec![1, 2, 3]);
    }

    #[test]
    fn descent_reaches_through_sequences() {
        let d = n("items:\n  - a: 1\n  - a: 2\n  - b:\n      a: 3\n");
        assert_eq!(ints(&d.query("..a")), vec![1, 2, 3]);
    }

    /// A path without `..` must agree with `get_path` on the same input.
    #[test]
    fn without_descent_it_agrees_with_get_path() {
        let d = n("spec:\n  containers:\n    - name: app\n");
        for path in [
            "",
            "spec",
            "spec.containers",
            "spec.containers.0.name",
            "nope",
            "spec.9",
        ] {
            let q = d.query(path);
            match d.get_path(path) {
                Some(one) => {
                    assert_eq!(q.len(), 1, "{path}");
                    assert!(std::ptr::eq(q[0], one), "{path}");
                }
                None => assert!(q.is_empty(), "{path} should be empty"),
            }
        }
    }

    #[test]
    fn no_match_is_an_empty_vec_not_an_error() {
        let d = n("a: 1\n");
        let got = d.query("..zzz");
        assert!(got.is_empty(), "expected empty, got {got:?}");
        let got = d.query("zzz");
        assert!(got.is_empty(), "expected empty, got {got:?}");
    }

    #[test]
    fn descent_can_be_followed_by_more_segments() {
        let d = n("x:\n  cfg:\n    port: 1\ny:\n  deep:\n    cfg:\n      port: 2\n");
        assert_eq!(ints(&d.query("..cfg.port")), vec![1, 2]);
    }

    #[test]
    fn a_leading_descent_searches_the_whole_document() {
        let d = n("a:\n  name: one\nb:\n  name: two\n");
        let got: Vec<&str> = d
            .query("..name")
            .iter()
            .filter_map(|x| x.as_str())
            .collect();
        assert_eq!(got, vec!["one", "two"]);
    }

    /// Mapping KEYS are labels, not nodes the path descends through.
    #[test]
    fn descent_does_not_match_mapping_keys() {
        let d = n("name: outer\nnested:\n  name: inner\n");
        let got: Vec<&str> = d
            .query("..name")
            .iter()
            .filter_map(|x| x.as_str())
            .collect();
        assert_eq!(got, vec!["outer", "inner"], "values only, never keys");
    }

    #[test]
    fn descent_on_a_scalar_root_finds_nothing_below_it() {
        let d = n("just-a-scalar\n");
        let got = d.query("..a");
        assert!(got.is_empty(), "expected empty, got {got:?}");
    }
}
