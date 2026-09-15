// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The measurement #58 turns on: streaming versus the tree path, same targets.
//!
//! The existing `serde` bench cannot answer this. It deserialises into
//! `glaucus::Value`, which takes the tree fast path by design — `Value` IS a
//! tree, so streaming into one would compose the same graph anyway. Benching it
//! would compare the tree path with itself.
//!
//! What #57 actually changed is every OTHER target, so that is what is measured
//! here, on two shapes:
//!
//! - `Generic`, an owned recursive value. The whole document is materialised, so
//!   this isolates the cost of the intermediate `Node` graph and nothing else.
//! - `Partial`, a struct naming three fields. This is where skipping is supposed
//!   to pay: the tree path builds every node before the target discards most of
//!   them, while streaming walks the events and drops what is not asked for.

// `.clippy.toml`'s allow-unwrap-in-tests only covers `#[cfg(test)]` modules.
// This target is compiled as its own crate with no such cfg, so the workspace
// lints apply and are answered here: unwrapping fixture setup is the point —
// a broken fixture should abort loudly rather than be handled.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use glaucus_bench::fixtures::{MEDIUM_HELM, SMALL_POD, generate_large};
use serde::Deserialize;

/// An owned, span-free view of a whole document.
///
/// Nothing reads the payload back: the work being timed is building it, and
/// inspecting it afterwards would time the inspection too.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Generic {
    Seq(Vec<Self>),
    Map(BTreeMap<String, Self>),
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Null,
}

/// A few fields out of a document; everything else is ignored.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Partial {
    #[serde(default, rename = "apiVersion")]
    api_version: Option<String>,
    #[serde(default)]
    kind: Option<String>,
}

/// Deserialises through the `Node` tree, which `from_str` no longer does.
///
/// Spelled out because `from_str` streams since #57; calling it for both sides
/// would compare the streaming path with itself and report a dead heat.
fn via_tree<T: serde::de::DeserializeOwned + 'static>(input: &str) -> T {
    let (node, version) = glaucus_ast::composer::compose_one_versioned(
        input,
        glaucus_core::error::ParserConfig::default(),
    )
    .unwrap();
    let mut de =
        glaucus::serde_integration::de::Deserializer::from_node_with(&node, version.is_1_1());
    T::deserialize(&mut de).unwrap()
}

fn streaming_vs_tree(c: &mut Criterion) {
    let large = generate_large(800);

    let mut group = c.benchmark_group("streaming_vs_tree");
    // The decision rests on confidence intervals, so the sample has to be big
    // enough for them to mean something.
    group.measurement_time(Duration::from_secs(20));

    for (name, yaml) in [
        ("small", SMALL_POD),
        ("medium", MEDIUM_HELM),
        ("large", large.as_str()),
    ] {
        group.throughput(Throughput::Bytes(yaml.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("generic/streaming", name),
            &yaml,
            |b, i| {
                b.iter(|| glaucus::serde_integration::from_str::<Generic>(black_box(i)).unwrap());
            },
        );
        group.bench_with_input(BenchmarkId::new("generic/tree", name), &yaml, |b, i| {
            b.iter(|| via_tree::<Generic>(black_box(i)));
        });

        group.bench_with_input(
            BenchmarkId::new("partial/streaming", name),
            &yaml,
            |b, i| {
                b.iter(|| glaucus::serde_integration::from_str::<Partial>(black_box(i)).unwrap());
            },
        );
        group.bench_with_input(BenchmarkId::new("partial/tree", name), &yaml, |b, i| {
            b.iter(|| via_tree::<Partial>(black_box(i)));
        });
    }

    group.finish();
}

criterion_group!(benches, streaming_vs_tree);
criterion_main!(benches);
