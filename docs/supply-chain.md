<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Supply-Chain Security Posture

This document maps every supply-chain and security control in the Glaucus
repository to the mechanism that implements it and where it runs.

## Control Matrix

| Control | Mechanism | Local mise task | Where it runs in CI |
|---------|-----------|-----------------|---------------------|
| **Memory safety** | `#![forbid(unsafe_code)]` on glaucus-core / glaucus-ast / glaucus-cst | — (compiler-enforced) | every build |
| **Undefined behaviour** | Miri with `-Zmiri-strict-provenance` | _no task defined_ | **not wired** — see below |
| **Dependency advisories** | `cargo audit` against the RustSec advisory database | `mise run cargo:audit` | `cicd.yml` — `lint` job, via the hk `audit` step |
| **License & ban policy** | `cargo deny check` (license allowlist, multiple-versions ban, unknown-registries ban) | `mise run cargo:deny` | `cicd.yml` — `lint` job, via the hk `deny` step |
| **Secret scanning** | `gitleaks` | `hk run gitleaks` | `cicd.yml` — `lint` job, via the hk `gitleaks` step |
| **Supply-chain audits** | `cargo vet` — records in `supply-chain/audits.toml` | `mise run cargo:vet` | **not wired** — see below |
| **License compliance** | REUSE 3.3 — every file covered via `REUSE.toml` globs + `LICENSES/` | `mise run comply` | `cicd.yml` — `lint` job, via the hk `comply` step |
| **Static analysis** | CodeQL | CI only | `cicd.yml` — `sast` job → `codeql.yml` |
| **Build provenance** | SLSA Level 3 via `slsa-framework/slsa-github-generator` | CI only | `publish.yml` |
| **Artifact signing** | sigstore/cosign keyless signing (`.cosign.bundle` per artifact) | CI only | `publish.yml` |
| **Project scorecard** | OpenSSF Scorecard (`ossf/scorecard-action`); weekly schedule | CI only | `scorecards.yml` |
| **Fuzzing** | 9 `cargo-fuzz` targets — `fuzz_scanner`, `fuzz_parser`, `fuzz_round_trip`, `fuzz_limits`, `fuzz_alias_graph`, `fuzz_serde`, `fuzz_cst_roundtrip`, `fuzz_lossless_edit`, `fuzz_merge_keys` | `mise run fuzz:all` | `nightly.yml` — `fuzz` job |
| **Test coverage** | `cargo-tarpaulin`, LLVM engine, **code and doctests both at 100%** (see below) | `mise run coverage` | `cicd.yml` — `tests` job |
| **Spec conformance** | Official YAML test suite, 735/735, enforced at a 100% floor | `mise run test` | `cicd.yml` — `tests` job |

### Coverage: one invocation, code and doctests alike

The gate is a single tarpaulin run, and it covers **both** unit/integration tests
and doctests against the same 100% floor — a doctest is documentation that
executes, so it is held to the same standard as the code it documents:

```sh
cargo tarpaulin --workspace --engine llvm \
  --features glaucus/cli \
  --run-types Tests --run-types Doctests \
  --fail-under 100
```

The threshold is declared once, in `mise.toml` as `RUST_COVERAGE_THRESHOLD = "100"`,
and every other consumer reads it rather than restating it: the `coverage` task
defaults `--threshold` to it, and `cicd.yml` passes
`${{ vars.RUST_COVERAGE_THRESHOLD || '100' }}`. `codecov.yml` sets `target: 100%`
with `threshold: 0%` on project, patch and every component, so no regression is
tolerated there either.

### Controls that are NOT wired into CI

Recorded explicitly rather than left to be inferred from the table, because a
security document that overstates its own posture is worse than one that admits a
gap:

- **Miri** — no `miri` task exists in `mise.toml` and `nightly.yml` defines only a
  `fuzz` job. The Miri configuration survives as a comment in that workflow.
- **`cargo vet`** — the `cargo:vet` task exists and is deliberately non-blocking
  (`|| true`) until the dependency tree is certified, but nothing invokes it in CI.

## Notes

**Ordering in the lint job** — `cargo audit` intentionally runs before
`cargo deny`. `cargo audit` clones the RustSec advisory database
into `~/.cargo/advisory-db`; `cargo deny`'s `advisories` check reuses that
directory and will fail if it is non-empty when the clone is attempted.

**cargo vet is non-blocking** — the `cargo:vet` task carries
`continue-on-error: true` until the dependency audit set is fully populated
with `cargo vet certify` entries. Remove that flag once the tree is audited.

**REUSE compliance** — `REUSE.toml` uses glob patterns to cover every file
class (Rust sources carry SPDX headers inline; config files, docs, and CI
workflows are covered by catch-all globs). Run `reuse lint` or `mise run comply`
to verify compliance locally.

**SLSA / cosign are release-only** — these controls run on tag pushes
(`on: push: tags: ['v*']`) and are not part of the PR gate. They operate on
the published `.tar.gz` / `.sha256` release assets.

**Miri scope** — Miri tests `glaucus-core` and `glaucus-ast`. `glaucus-serde` and
`glaucus-cst` pull in `serde` proc-macros which Miri does not yet fully support;
they are intentionally excluded.
