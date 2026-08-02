// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

// `.clippy.toml`'s allow-unwrap-in-tests only covers `#[cfg(test)]` modules.
// This target is compiled as its own crate with no such cfg, so the workspace
// lints apply and are answered here: unwrapping fixture setup is the point —
// a broken fixture should abort loudly rather than be handled.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use glaucus_bench::fixtures::{MEDIUM_HELM, SMALL_POD, generate_large};
use std::hint::black_box;

// NOTE: `rust-yaml` (elioseverojunior/rust-yaml) is NOT compared here. It exposes
// only a high-level `load_str`/`dump_str` facade with no public streaming
// token/event API, so there is no equivalent to glaucus's `next_event()` or
// yaml-rust2's `next_token()` to benchmark at this stage. rust-yaml is
// compared at the tree level in `composer` (load), `emitter` (dump), and
// `end_to_end` (roundtrip).
fn parser_benchmark(c: &mut Criterion) {
    let large = generate_large(800);

    let mut group = c.benchmark_group("parser");
    group.measurement_time(Duration::from_secs(15));

    for (name, input) in [
        ("small", SMALL_POD),
        ("medium", MEDIUM_HELM),
        ("large", large.as_str()),
    ] {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("glaucus", name), &input, |b, input| {
            b.iter(|| {
                let mut parser = glaucus_core::parser::Parser::new(black_box(input));
                while let Some(event) = parser.next_event() {
                    let _ = black_box(event);
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("yaml_rust2", name), &input, |b, input| {
            b.iter(|| {
                let mut parser = yaml_rust2::parser::Parser::new_from_str(black_box(input));
                loop {
                    let (event, _marker) = parser.next_token().unwrap();
                    if black_box(event) == yaml_rust2::parser::Event::StreamEnd {
                        break;
                    }
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, parser_benchmark);
criterion_main!(benches);
