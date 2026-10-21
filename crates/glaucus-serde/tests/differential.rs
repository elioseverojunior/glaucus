// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Differential harness: the tree path and the streaming path must agree.
//!
//! Written **before** the streaming engine exists, deliberately.
//!
//! Two implementations of anchor resolution that must agree is the same shape
//! that produced three divergent scalar resolvers. In #40, `glaucus schema
//! validate` and `glaucus from_str` reached opposite verdicts on `0x1F` because
//! the logic had been written three times and drifted — and nobody noticed until
//! someone compared them by hand.
//!
//! A harness added at the end of that work would have arrived to a backlog of
//! accumulated differences, with no way to tell which were regressions and which
//! had always been there. This one covers the corpus while there is still only
//! one engine, so it runs green from the start and the first disagreement it ever
//! reports is a real one, introduced by the commit that broke it.
//!
//! Until #57 routes `from_str` by target type, [`via_streaming`] calls the same
//! function as [`via_tree`] and every case passes trivially. **That is the
//! point.**

use std::collections::BTreeMap;

use serde::Deserialize;

use glaucus_serde::from_str;

/// A float compared so that `NaN` equals `NaN`.
///
/// IEEE 754 says `NaN != NaN`, so a derived comparison reports `.nan` as a
/// disagreement on every run -- the harness would fail permanently on a document
/// both engines handled identically, and a harness that always fails detects
/// nothing. Equality here asks the question the harness actually means: did the
/// two engines produce the same value?
#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct F64(f64);

impl PartialEq for F64 {
    fn eq(&self, other: &Self) -> bool {
        // Bit equality would also split +0.0 from -0.0, which the engines are
        // not expected to distinguish, so `==` still decides the ordinary case.
        (self.0.is_nan() && other.0.is_nan()) || self.0 == other.0
    }
}

/// A span-free, style-free view of a document.
///
/// **Not `Value`.** `Value`'s `PartialEq` compares `span` and `style`, and
/// neither survives serde's data model: the streaming path builds values from
/// `Visitor` calls that carry no source position, and infers a default style. A
/// `Value` comparison is therefore green only while both sides call the SAME
/// function -- the moment #57 makes them differ it would report all of the
/// corpus as disagreeing, and a harness that always fails detects nothing.
///
/// What must agree across two engines is the structure and the scalar text,
/// which is what this captures. It is still shape-preserving: a typed struct
/// would silently discard any field the two paths disagreed about, which is
/// exactly the disagreement this exists to catch.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum Shape {
    // Order matters: serde tries these top to bottom. Collections first so a
    // sequence is never mistaken for a string, and Int before Float so a whole
    // number does not silently become 1.0 on one path only.
    Seq(Vec<Self>),
    Map(BTreeMap<String, Self>),
    Bool(bool),
    Int(i64),
    Float(F64),
    Str(String),
    Null,
}

/// One corpus entry. The name is carried so a failure says which case broke.
struct Case {
    name: &'static str,
    yaml: &'static str,
}

/// Deserialises through the `Node` tree: compose, then walk the tree.
///
/// Spelled out rather than calling `from_str`, which since #57 streams. This is
/// exactly what `from_str` used to do, version directive included -- reading the
/// directive matters, or a `%YAML 1.1` document would report a difference that
/// is an artefact of the harness rather than of the engines.
fn via_tree(yaml: &str) -> Result<Shape, String> {
    let (node, version) = glaucus_ast::composer::compose_one_versioned(
        yaml,
        glaucus_core::error::ParserConfig::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut de = glaucus_serde::de::Deserializer::from_node_with(&node, version.is_1_1());
    Shape::deserialize(&mut de).map_err(|e| e.to_string())
}

/// Deserialises through the parser event stream, without composing a tree.
///
/// Since #57 this is what `from_str` does for every target that is not `Value`,
/// so the two helpers now exercise genuinely different engines and the corpus
/// finally means something.
fn via_streaming(yaml: &str) -> Result<Shape, String> {
    from_str::<Shape>(yaml).map_err(|e| e.to_string())
}

/// Asserts both paths agree on whether `yaml` parses, and on what it produces.
///
/// Error *messages* are deliberately not compared. Two engines can reject the
/// same document for the same reason while wording it differently, and treating
/// that as a difference would bury the real signal. What must match is the
/// verdict — accept or reject — and, when accepted, the value.
///
/// The target is [`Shape`]; see its documentation for why not [`Value`].
fn assert_paths_agree(name: &str, yaml: &str) -> Option<String> {
    match (via_tree(yaml), via_streaming(yaml)) {
        (Ok(tree), Ok(streaming)) => (tree != streaming).then(|| {
            format!("{name}: both paths accepted but produced different values\n    tree:      {tree:?}\n    streaming: {streaming:?}")
        }),
        (Err(_), Err(_)) => None,
        (Ok(tree), Err(e)) => Some(format!(
            "{name}: tree accepted, streaming REJECTED\n    tree:      {tree:?}\n    streaming: {e}"
        )),
        (Err(e), Ok(streaming)) => Some(format!(
            "{name}: tree REJECTED, streaming accepted\n    tree:      {e}\n    streaming: {streaming:?}"
        )),
    }
}

/// Twenty cases, chosen for where two engines are most likely to drift apart.
const CORPUS: &[Case] = &[
    // ─── scalars and resolution ─────────────────────────────────────
    Case {
        name: "plain-scalars",
        yaml: "a: 1\nb: text\nc: true\nd: null\ne: 1.5\n",
    },
    Case {
        name: "radix-integers",
        yaml: "hex: 0x1F\nHEX: 0X1f\noct: 0o17\nneg: -0x10\nzero: 017\n",
    },
    Case {
        name: "bare-inf-nan-are-strings",
        yaml: "a: inf\nb: nan\nc: infinity\nd: -inf\n",
    },
    Case {
        name: "dotted-inf-nan-are-floats",
        yaml: "a: .inf\nb: -.inf\nc: .nan\nd: .INF\n",
    },
    Case {
        name: "quoted-values-stay-text",
        yaml: "a: \"1\"\nb: 'true'\nc: \"null\"\nd: \"\"\n",
    },
    // ─── structure ──────────────────────────────────────────────────
    Case {
        name: "nested-collections",
        yaml: "a:\n  b:\n    - 1\n    - c: 2\n      d: [3, 4]\n",
    },
    Case {
        name: "empty-document",
        yaml: "",
    },
    Case {
        name: "comment-only-document",
        yaml: "# nothing but a comment\n",
    },
    Case {
        name: "non-string-mapping-keys",
        yaml: "1: one\ntrue: yes-key\n0x1F: hex-key\n",
    },
    Case {
        name: "empty-collections",
        yaml: "a: []\nb: {}\nc: [[], {}]\n",
    },
    Case {
        // The two styles denote the same value, so any engine that treats a
        // flow collection as a different shape than its block spelling shows up
        // here rather than in whichever downstream test happened to use one.
        name: "flow-and-block-agree",
        yaml: "flow: {a: [1, 2], b: {c: 3}}\nblock:\n  a:\n    - 1\n    - 2\n  b:\n    c: 3\n",
    },
    Case {
        name: "sequence-of-mappings",
        yaml: "- a: 1\n  b: 2\n- a: 3\n  b: 4\n",
    },
    Case {
        name: "single-element-collections",
        yaml: "a: [1]\nb: {c: 1}\nd:\n  - 1\n",
    },
    // ─── anchors and aliases ────────────────────────────────────────
    Case {
        name: "anchor-on-scalar",
        yaml: "a: &x 1\nb: *x\n",
    },
    Case {
        name: "anchor-on-mapping",
        yaml: "a: &x {p: 1, q: 2}\nb: *x\n",
    },
    Case {
        name: "anchor-on-sequence",
        yaml: "a: &x [1, 2, 3]\nb: *x\n",
    },
    Case {
        name: "nested-anchors",
        yaml: "a: &outer\n  inner: &in [1, 2]\n  other: *in\nb: *outer\n",
    },
    Case {
        name: "anchor-aliased-repeatedly",
        yaml: "a: &x [1, 2]\nb: *x\nc: *x\nd: *x\ne: *x\n",
    },
    // ─── merge keys ─────────────────────────────────────────────────
    Case {
        name: "merge-simple",
        yaml: "d: &d {a: 1, b: 2}\nm:\n  <<: *d\n",
    },
    Case {
        name: "merge-overriding",
        yaml: "d: &d {a: 1, b: 2}\nm:\n  <<: *d\n  a: 9\n",
    },
    Case {
        name: "merge-sequence-of-sources",
        yaml: "x: &x {a: 1}\ny: &y {b: 2}\nm:\n  <<: [*x, *y]\n",
    },
    // ─── tags and directives ────────────────────────────────────────
    Case {
        name: "explicit-tags",
        yaml: "a: !!str 123\nb: !!int \"456\"\nc: !!bool \"true\"\nd: !!null \"null\"\n",
    },
    Case {
        name: "yaml-1-1-directive",
        yaml: "%YAML 1.1\n---\na: yes\nb: no\nc: on\nd: off\n",
    },
    Case {
        name: "binary-tag",
        yaml: "a: !!binary SGVsbG8gV29ybGQh\n",
    },
    Case {
        // Verbatim tags arrive with an empty handle, a shape no other case in
        // this corpus produces.
        name: "verbatim-tags",
        yaml: "a: !<tag:yaml.org,2002:str> 123\nb: !<!local> x\n",
    },
];

/// Malformed input. Agreeing about FAILURE matters as much as agreeing about
/// success: a streaming path that accepts what the tree path rejects has widened
/// what glaucus considers valid YAML without anyone deciding to.
const MALFORMED: &[Case] = &[
    Case {
        name: "unclosed-flow-sequence",
        yaml: "a: [1, 2\n",
    },
    Case {
        name: "unclosed-flow-mapping",
        yaml: "a: {b: 1\n",
    },
    Case {
        name: "undefined-alias",
        yaml: "a: *nope\n",
    },
    Case {
        name: "duplicate-key",
        yaml: "a: 1\na: 2\n",
    },
    Case {
        name: "tab-indentation",
        yaml: "a:\n\tb: 1\n",
    },
    Case {
        name: "bad-anchor-name",
        yaml: "a: &\nb: 1\n",
    },
];

#[test]
fn tree_and_streaming_paths_agree_on_the_corpus() {
    let disagreements: Vec<String> = CORPUS
        .iter()
        .filter_map(|c| assert_paths_agree(c.name, c.yaml))
        .collect();

    assert!(
        disagreements.is_empty(),
        "{} of {} corpus cases disagree:\n\n{}",
        disagreements.len(),
        CORPUS.len(),
        disagreements.join("\n\n")
    );
}

#[test]
fn tree_and_streaming_paths_agree_on_malformed_input() {
    let disagreements: Vec<String> = MALFORMED
        .iter()
        .filter_map(|c| assert_paths_agree(c.name, c.yaml))
        .collect();

    assert!(
        disagreements.is_empty(),
        "{} of {} malformed cases disagree:\n\n{}",
        disagreements.len(),
        MALFORMED.len(),
        disagreements.join("\n\n")
    );
}

/// Guards the harness itself.
///
/// Every case is collected and reported together rather than failing on the
/// first, so one broken case does not hide the other nineteen. This checks the
/// corpus is actually populated — a harness that silently iterates an empty list
/// passes forever and protects nothing, which is the failure mode #23 found in
/// the conformance suite.
#[test]
fn the_corpus_is_not_empty() {
    // A FLOOR, not an equality. The guard exists to stop the corpus being
    // gutted into a vacuous pass -- every case deleted and the suite still
    // green. An exact count would also reject every legitimate addition, so it
    // degrades into mechanical number-bumping and stops being read. Ratchet
    // this upward as cases are added; never lower it to make a deletion pass.
    assert!(
        CORPUS.len() >= 25,
        "corpus shrank to {} cases; it should only grow",
        CORPUS.len()
    );
    assert!(!MALFORMED.is_empty(), "malformed cases must be covered too");

    let mut names: Vec<&str> = CORPUS.iter().chain(MALFORMED).map(|c| c.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(
        before,
        names.len(),
        "case names must be unique to identify failures"
    );
}
