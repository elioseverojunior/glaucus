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

fn scanner_benchmark(c: &mut Criterion) {
    let large = generate_large(800);

    let mut group = c.benchmark_group("scanner");
    group.measurement_time(Duration::from_secs(15));

    for (name, input) in [
        ("small", SMALL_POD),
        ("medium", MEDIUM_HELM),
        ("large", large.as_str()),
    ] {
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_with_input(BenchmarkId::new("glaucus", name), &input, |b, input| {
            b.iter(|| {
                let mut scanner = glaucus_core::scanner::Scanner::new(black_box(input));
                while let Some(tok) = scanner.next_token() {
                    let _ = black_box(tok);
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, scanner_benchmark);
criterion_main!(benches);
