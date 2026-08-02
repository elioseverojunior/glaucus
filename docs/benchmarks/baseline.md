<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Benchmark Baseline

> **Status — read this before quoting any number below.**
>
> The **2026-06-03** figures remain the authoritative baseline.
>
> A full re-run was executed on **2026-08-01**. It completed successfully — all six
> bench targets, 77 measurements, every one statistically tight — but it is
> **rejected as a baseline** because the host was running roughly 8x slower than
> normal for reasons unrelated to this project. Its numbers are recorded in the
> appendix for provenance and **must not be compared against the 2026-06-03
> table, nor cited as glaucus performance**. See
> [The 2026-08-01 re-run](#the-2026-08-01-re-run-rejected).

---

## Fixtures

| Name | Source | Size |
|------|--------|------|
| `small` | `SMALL_POD` — minimal Kubernetes Pod spec | 218 bytes |
| `medium` | `MEDIUM_HELM` — realistic Helm values.yaml | 3,364 bytes (~3.3 KB) |
| `large` | `generate_large(800)` — 800-entry programmatic YAML | 124,579 bytes (~121.7 KB) |

Sizes computed directly from `crates/glaucus-bench/src/fixtures.rs`. An earlier
revision of this document recorded `medium` as "~5 KB", overstating it by about
50%; the source comments carried the same error and were corrected alongside
this document.

---

## Excluded competitor: `serde_yml`

`serde_yml 0.0.13` is **excluded** from all benchmarks.

- **RUSTSEC-2025-0068** — unsound: a Serializer use-after-free can trigger a
  segfault in safe Rust.
- The upstream repository is archived; no fix is expected.
- Glaucus's `deny.toml` enforces `vulnerability = "deny"` with zero ignored
  advisories.

A commented-out entry in `glaucus-bench/Cargo.toml` documents this exclusion
in-source.

---

## Authoritative baseline — 2026-06-03

**Branch:** `worktree-feat+beat-noyalib`
**Purpose:** establishes the measurement baseline for Phase 4b optimization work.

### Environment

| Field | Value |
|-------|-------|
| Toolchain | `rustc 1.93.1 (01f6ddf75 2026-02-11)` |
| Platform | `Darwin 24.6.0 x86_64` (Intel Mac, native x86\_64) |
| CPU | Intel Xeon W-2150B @ 3.00 GHz (Skylake-W, 10 cores) |
| System load at run time | 4.05 / 7.45 / 8.27 (1/5/15 min) — moderately loaded |
| Settings | `--warm-up-time 2 --measurement-time 8` |

Times are medians (middle value of Criterion's `[low median high]` bracket).

### `serde_deserialize`

Deserializes each fixture into the library's generic value type. `serde_json`
operates on the JSON-equivalent fixture and is a speed-of-light reference, not a
YAML comparison.

| Library | small | medium | large |
|---------|-------|--------|-------|
| **glaucus** | **30.66 µs** | **318.1 µs** | **17.72 ms** |
| noyalib | 49.43 µs | 232.3 µs | 10.79 ms |
| serde\_yaml\_ng | 33.43 µs | 343.4 µs | 18.82 ms |
| serde\_saphyr | 57.85 µs | 571.7 µs | 33.51 ms |
| serde\_json (JSON) | 3.94 µs | 49.51 µs | 3.09 ms |

noyalib leads deserialize by ~1.4–1.6x at every size; glaucus holds 2nd, ahead of
serde\_yaml\_ng by 6–8%. The gap is consistent across sizes, pointing to
algorithmic overhead rather than a fixture-size effect.

### `serde_serialize`

| Library | small | medium | large |
|---------|-------|--------|-------|
| **glaucus** | **8.50 µs** | **81.93 µs** | **3.70 ms** |
| noyalib | 10.35 µs | 62.80 µs | 5.37 ms |
| serde\_yaml\_ng | 13.81 µs | 185.1 µs | 10.44 ms |
| serde\_saphyr | 14.99 µs | 136.0 µs | 6.62 ms |
| serde\_json (JSON) | 1.03 µs | 7.43 µs | 344 µs |

Glaucus leads serialize at every size (1.2–1.45x over noyalib). The emitter's
design — trusting the caller's `ScalarStyle`, no re-quoting overhead — pays off
here.

### `roundtrip_compare` (tree level)

| Library | small | medium | large |
|---------|-------|--------|-------|
| **glaucus** | **28.65 µs** | **424.4 µs** | **13.05 ms** |
| yaml\_rust2 | 33.49 µs | 414.1 µs | 15.08 ms |
| rust\_yaml | 77.07 µs | 1.50 ms | 705.5 ms |

Glaucus leads on small (~14%) and large (~13%); medium is a statistical tie with
yaml\_rust2. rust\_yaml is ~54x slower on large inputs.

---

## The 2026-08-01 re-run (rejected)

### What was run

| Field | Value |
|-------|-------|
| Date | 2026-08-01 06:56 UTC |
| Branch / commit | `main` @ `daf377c` |
| Toolchain | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| Platform | `Darwin 24.6.0 x86_64`, Intel Xeon W-2150B @ 3.00 GHz, 10 physical / 20 logical |
| Settings | `--warm-up-time 2 --measurement-time 8` |
| Scope | All six bench targets — 77 measurements |
| Wall time | ~85 min (`scanner` 289s, `parser` 371s, `composer` 1108s, `emitter` 605s, `end_to_end` 1639s, `serde` 1078s) |
| Load average during run | **34.2 – 39.4** throughout |

`target/criterion` was cleared beforehand, so no `change%` figure in this run
refers to a stale cached baseline — a caveat the previous revision had to carry.

### Why it is rejected

Every one of the 30 measurements comparable to 2026-06-03 came out slower, by
**4.57x to 20.89x (median 7.93x)**. The decisive evidence is `serde_json` — a
third-party JSON library that glaucus does not touch, and which no change in this
repository can affect:

| Benchmark | 2026-06-03 | 2026-08-01 | Ratio |
|-----------|-----------:|-----------:|------:|
| `serde_deserialize/serde_json/small` | 3.94 µs | 29.71 µs | 7.54x |
| `serde_deserialize/serde_json/large` | 3.09 ms | 14.11 ms | 4.57x |
| `serde_serialize/serde_json/large` | 344 µs | 7.19 ms | 20.89x |
| `serde_deserialize/glaucus/small` | 30.66 µs | 234.79 µs | 7.66x |
| `serde_deserialize/noyalib/large` | 10.79 ms | 115.35 ms | 10.69x |

Confirmed independently of Criterion: a **pure single-core integer loop** with no
I/O and no allocation took **5.19 s** against roughly 0.4 s expected on a healthy
3.0 GHz W-2150B. The cores themselves were executing about an order of magnitude
slow, which accounts for the whole effect.

Two consequences follow, and the second is the one that matters:

1. Absolute times are inflated and cannot be compared with 2026-06-03.
2. The degradation is **not uniform** (4.57x–20.89x), so it distorts the ratios
   *between* libraries as well. Even same-run rankings are unsafe to quote.

### What is still trustworthy

The run is sound as a harness check, and that part is worth keeping:

- All six bench targets built and completed with `rc=0`.
- Criterion confidence intervals were tight throughout — spread between the low
  and high bounds had a median of **1.96%** and a maximum of **4.2%**. The
  measurements are *precise*; they are precisely measuring a slow machine.
- Outlier rates of 3–12% per case are normal for Criterion.

Note that the load average was a poor signal here and nearly caused the wrong
call in both directions: it read 34–39 while actual CPU utilisation was only
~13%, so it overstated contention — yet the machine really was degraded, for a
reason load average never showed. The single-core calibration loop settled it;
the load number alone would not have.

### Before re-running

1. Establish why the host is executing ~10x slow — thermal or power throttling on
   the W-2150B is the leading hypothesis and was not confirmed. `Drift.appex`,
   `VTDecoderXPCService` and `WindowServer` were all active during the window.
2. Gate the run on the single-core calibration loop rather than on load average:
   require it near ~0.4 s before measuring.
3. Note that the toolchain has also moved (1.93.1 → 1.97.1). That is a genuine
   confound for a future comparison, though far too small to explain 8x.

---

## Appendix — full 2026-08-01 measurements

**Do not cite these figures.** They are recorded for provenance only, taken on a
host running ~8x slow. Medians, with the width of Criterion's confidence interval
as a percentage of the median.

### `scanner`

| Library | small | medium | large | max spread |
|---|---|---|---|---|
| `glaucus` | 70.59 µs | 706.95 µs | 35.09 ms | ±2.6% |

### `parser`

| Library | small | medium | large | max spread |
|---|---|---|---|---|
| `glaucus` | 161.60 µs | 1.47 ms | 74.32 ms | ±2.9% |
| `yaml_rust2` | 178.95 µs | 1.87 ms | 92.45 ms | ±3.7% |

### `composer`

| Library | small | medium | large | max spread |
|---|---|---|---|---|
| `glaucus` | 218.21 µs | 2.02 ms | 101.92 ms | ±1.8% |
| `rust_yaml` | 481.88 µs | 8.37 ms | 5429.20 ms | ±2.3% |
| `yaml_rust2` | 292.98 µs | 2.96 ms | 149.68 ms | ±2.5% |

### `emitter`

| Library | small | medium | large | max spread |
|---|---|---|---|---|
| `glaucus` | 20.21 µs | 249.64 µs | 11.78 ms | ±2.3% |
| `rust_yaml` | 206.55 µs | 3.07 ms | 188.44 ms | ±2.2% |
| `yaml_rust2` | 48.32 µs | 483.79 µs | 24.65 ms | ±2.1% |

### `end_to_end`

Group `roundtrip_compare`:

| Library | small | medium | large | max spread |
|---|---|---|---|---|
| `glaucus` | 234.43 µs | 2.30 ms | 115.20 ms | ±2.1% |
| `rust_yaml` | 702.81 µs | 11.64 ms | 5673.80 ms | ±2.2% |
| `yaml_rust2` | 345.48 µs | 3.52 ms | 174.07 ms | ±2.3% |

Group `full_pipeline_node` (two glaucus entry points, not competing libraries):

| Path | small | medium | large | max spread |
|---|---|---|---|---|
| `compose_all` | 221.26 µs | 2.05 ms | 102.88 ms | ±2.6% |
| `facade` | 211.89 µs | 2.04 ms | 102.71 ms | ±2.6% |

Group `roundtrip_node`:

| Case | median | spread |
|---|---|---|
| `small` | 448.59 µs | ±1.9% |
| `medium` | 4.39 ms | ±1.6% |
| `large` | 216.68 ms | ±1.6% |

Group `typed_struct`:

| Case | median | spread |
|---|---|---|
| `deserialize` | 227.79 µs | ±2.3% |
| `roundtrip` | 270.59 µs | ±2.9% |

### `serde`

Group `serde_deserialize`:

| Library | small | medium | large | max spread |
|---|---|---|---|---|
| `glaucus` | 234.79 µs | 2.27 ms | 114.13 ms | ±2.7% |
| `noyalib` | 247.05 µs | 2.33 ms | 115.35 ms | ±2.7% |
| `serde_json` | 29.71 µs | 314.42 µs | 14.11 ms | ±3.8% |
| `serde_saphyr` | 540.81 µs | 5.21 ms | 263.83 ms | ±2.5% |
| `serde_yaml_ng` | 306.70 µs | 3.18 ms | 151.26 ms | ±3.9% |

Group `serde_serialize`:

| Library | small | medium | large | max spread |
|---|---|---|---|---|
| `glaucus` | 43.25 µs | 525.33 µs | 30.14 ms | ±2.8% |
| `noyalib` | 65.23 µs | 679.35 µs | 32.85 ms | ±3.8% |
| `serde_json` | 12.78 µs | 154.82 µs | 7.19 ms | ±2.8% |
| `serde_saphyr` | 105.13 µs | 1.08 ms | 52.50 ms | ±4.2% |
| `serde_yaml_ng` | 147.92 µs | 1.52 ms | 72.15 ms | ±2.7% |

---

## Commands

```bash
# One target at a time, never concurrently.
cargo bench -p glaucus-bench --bench <target> -- --measurement-time 8 --warm-up-time 2

# Targets: scanner parser composer emitter end_to_end serde
```

`-p glaucus-bench` rather than `--workspace`: `--output-format` and the other
Criterion flags are rejected by the default libtest harnesses that `--workspace`
also sweeps in, which aborts the run after the release build.

---

## Standing caveats

- **serde\_saphyr has no native value type.** Both its deserialize and serialize
  rows use `serde_json::Value` as the data carrier, adding marginal JSON-side
  overhead but keeping the comparison consistent within the group.
- **ARM baseline still outstanding.** A Linux aarch64 data point (`raspi5`)
  remains unmeasured; NEON path validation for the `simd` feature gate needs it.
- **`serde_json` rows are a speed-of-light reference**, parsing JSON rather than
  YAML. They bound what the fixture data costs to materialise; they are not a
  YAML comparison.
