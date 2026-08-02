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
use rust_yaml::Yaml;
use std::hint::black_box;

fn emitter_benchmark(c: &mut Criterion) {
    let large = generate_large(800);
    let ry = Yaml::new();

    // Pre-parse all native trees outside the hot loop — each library emits
    // from its own tree, so we measure emission, not cross-library conversion.
    let glaucus_small = glaucus_ast::composer::compose_all(SMALL_POD)
        .unwrap()
        .remove(0);
    let glaucus_medium = glaucus_ast::composer::compose_all(MEDIUM_HELM)
        .unwrap()
        .remove(0);
    let glaucus_large = glaucus_ast::composer::compose_all(&large)
        .unwrap()
        .remove(0);

    let yr2_small = yaml_rust2::YamlLoader::load_from_str(SMALL_POD).unwrap();
    let yr2_medium = yaml_rust2::YamlLoader::load_from_str(MEDIUM_HELM).unwrap();
    let yr2_large = yaml_rust2::YamlLoader::load_from_str(&large).unwrap();

    let ry_small = ry.load_str(SMALL_POD).unwrap();
    let ry_medium = ry.load_str(MEDIUM_HELM).unwrap();
    let ry_large = ry.load_str(&large).unwrap();

    let config = glaucus_ast::emitter::EmitterConfig::default();

    let mut group = c.benchmark_group("emitter");
    group.measurement_time(Duration::from_secs(30));

    for (name, input_len, glaucus_node, yr2_doc, ry_value) in [
        (
            "small",
            SMALL_POD.len(),
            &glaucus_small,
            &yr2_small[0],
            &ry_small,
        ),
        (
            "medium",
            MEDIUM_HELM.len(),
            &glaucus_medium,
            &yr2_medium[0],
            &ry_medium,
        ),
        (
            "large",
            large.len(),
            &glaucus_large,
            &yr2_large[0],
            &ry_large,
        ),
    ] {
        group.throughput(Throughput::Bytes(input_len as u64));

        group.bench_with_input(
            BenchmarkId::new("glaucus", name),
            glaucus_node,
            |b, node| {
                b.iter(|| glaucus_ast::emitter::emit_to_string(black_box(node), &config));
            },
        );

        group.bench_with_input(BenchmarkId::new("yaml_rust2", name), yr2_doc, |b, doc| {
            b.iter(|| {
                let mut out = String::new();
                yaml_rust2::YamlEmitter::new(&mut out)
                    .dump(black_box(doc))
                    .unwrap();
                out
            });
        });

        group.bench_with_input(BenchmarkId::new("rust_yaml", name), ry_value, |b, value| {
            b.iter(|| ry.dump_str(black_box(value)).unwrap());
        });
    }

    group.finish();
}

criterion_group!(benches, emitter_benchmark);
criterion_main!(benches);
