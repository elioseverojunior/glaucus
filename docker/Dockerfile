# syntax=docker/dockerfile:1

# SPDX-FileCopyrightText: Glaucus contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# Multi-stage Dockerfile for multi-architecture builds (linux/amd64, linux/arm64)

# Base images. BOTH declared here, before the first FROM: only a global-scope
# ARG is visible to a FROM line. Declared after a stage instead, the runtime
# tag resolved to empty -- "failed to parse stage name
# docker.io/library/debian:" -- because the declaration belonged to that stage
# rather than to the build.
#
# Two names rather than one, because a `--build-arg` overrides the default in
# every stage declaring that name. A single shared `IMAGE_TAG` meant
# `docker:build` passing the toolchain version produced
# `debian:1.97-trixie`, which does not exist.
ARG IMAGE_TAG_BUILD=1.97-slim-trixie@sha256:5c6f46a6e4472ab1ca7ba7d494e6677f2f219ebc02f32025d3986f057635ec9c
ARG IMAGE_TAG_RUNTIME=trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd
ARG APP_VERSION=0.1.0

# Dependency stage

FROM --platform=$BUILDPLATFORM docker.io/library/rust:${IMAGE_TAG_BUILD} AS dependencies

SHELL ["/bin/bash", "-eou", "pipefail", "-c"]

ARG IMAGE_TAG_BUILD
ARG APP_VERSION
ARG TARGETPLATFORM
ARG BUILDPLATFORM

ENV BUILDPLATFORM=$BUILDPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM
ENV BUILD_PATH=/usr/src/app

# 1. Install system packages and cross-compilation toolchains. `git` is here
# for `crates/glaucus/build.rs`, which shells out to `git rev-parse`/`git
# status` in the `builder` stage below to stamp `GIT_COMMIT` -- the slim Rust
# base image does not ship it.
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
 --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  apt-get update
  apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    gcc \
    g++ \
    cmake \
    curl \
    git \
    build-essential \
    clang \
    mold

  case "$TARGETPLATFORM" in
    "linux/arm64")
      dpkg --add-architecture arm64
      apt-get update
      apt-get install -y --no-install-recommends \
        gcc-aarch64-linux-gnu \
        g++-aarch64-linux-gnu \
        libc6-dev-arm64-cross \
        libssl-dev:arm64
      ;;
    "linux/amd64")
      echo "Native amd64 platform, no cross-compilation tools needed."
      ;;
    *)
      echo "Unsupported target platform: $TARGETPLATFORM"
      exit 1
      ;;
  esac
EOF

# 2. Add correct Rust targets based on platform
RUN <<EOF
  #!/bin/bash
  set -exou pipefail
  case "$TARGETPLATFORM" in
    "linux/arm64")
      rustup target add aarch64-unknown-linux-gnu
      ;;
    "linux/amd64")
      rustup target add x86_64-unknown-linux-gnu
      ;;
  esac
EOF

WORKDIR ${BUILD_PATH}

# 3. Copy manifests for EVERY workspace member, not just the one(s) actually
# built. `[workspace] members = ["crates/*"]` in the root Cargo.toml is a
# glob: cargo loads every matched directory's Cargo.toml to resolve the
# workspace, even when the eventual build is scoped with `-p` to a single
# package -- omitting one fails at manifest-resolution time, before
# compilation starts, with "failed to load manifest for workspace member
# `.../crates/X`". No root `build.rs`: the root is a VIRTUAL workspace
# (`[workspace]` only), so a build script there would never run -- it
# belongs to `glaucus`, copied explicitly below so this stage's placeholder
# build actually exercises it (Task 15 added it; skipping it here would cache
# a build that never ran the real provenance-capturing script).
COPY Cargo.toml Cargo.lock ./
COPY crates/glaucus/Cargo.toml crates/glaucus/Cargo.toml
COPY crates/glaucus/build.rs crates/glaucus/build.rs
COPY crates/glaucus-ast/Cargo.toml crates/glaucus-ast/Cargo.toml
COPY crates/glaucus-bench/Cargo.toml crates/glaucus-bench/Cargo.toml
COPY crates/glaucus-cli/Cargo.toml crates/glaucus-cli/Cargo.toml
COPY crates/glaucus-core/Cargo.toml crates/glaucus-core/Cargo.toml
COPY crates/glaucus-cst/Cargo.toml crates/glaucus-cst/Cargo.toml
COPY crates/glaucus-fuzz/Cargo.toml crates/glaucus-fuzz/Cargo.toml
COPY crates/glaucus-schema/Cargo.toml crates/glaucus-schema/Cargo.toml
COPY crates/glaucus-serde/Cargo.toml crates/glaucus-serde/Cargo.toml
COPY crates/glaucus-wasm/Cargo.toml crates/glaucus-wasm/Cargo.toml
COPY crates/glaucus-yaml-test-suite/Cargo.toml crates/glaucus-yaml-test-suite/Cargo.toml
COPY .cargo/config.toml ./.cargo/config.toml

# 4. Generate placeholder sources and build dependencies, caching both target
# architectures correctly.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
 --mount=type=cache,target=/usr/local/cargo/git \
 --mount=type=cache,target=${BUILD_PATH}/target,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  # Placeholder sources for every crate the shipped binary actually depends
  # on: `glaucus-cli` -> `glaucus` (feature "cli") -> `glaucus-ast`/`-core`/
  # `-cst`/`-schema`/`-serde`. Empty `lib.rs` files compile to a trivial rlib
  # -- nothing here needs to be real, since `COPY . .` in the `builder` stage
  # below overwrites every one of them with the actual source.
  mkdir -p crates/glaucus/src crates/glaucus-ast/src crates/glaucus-core/src \
    crates/glaucus-cst/src crates/glaucus-schema/src crates/glaucus-serde/src \
    crates/glaucus-cli/src
  touch crates/glaucus-ast/src/lib.rs crates/glaucus-core/src/lib.rs \
    crates/glaucus-cst/src/lib.rs crates/glaucus-schema/src/lib.rs \
    crates/glaucus-serde/src/lib.rs crates/glaucus/src/lib.rs
  echo 'fn main() {}' > crates/glaucus/src/main.rs
  echo 'fn main() { println!("===> Preparing Cargo Dependencies! <==="); }' \
    > crates/glaucus-cli/src/main.rs

  # `glaucus-wasm`, `glaucus-bench`, `glaucus-fuzz` and
  # `glaucus-yaml-test-suite` are dev/CI-only harnesses (`publish = false` on
  # the last three) that `glaucus-cli` never depends on, so this build never
  # compiles them -- but cargo still resolves every EXPLICIT `[lib]`/
  # `[[bin]]`/`[[bench]]`/`[[test]]` table's path while loading the
  # workspace, before any unit is selected for compilation, and fails the
  # whole load if one is missing, even for this unrelated `-p glaucus-cli`
  # build. Empty files satisfy that check; none of these are ever compiled.
  mkdir -p crates/glaucus-wasm/src crates/glaucus-bench/src \
    crates/glaucus-bench/benches crates/glaucus-fuzz/fuzz_targets \
    crates/glaucus-yaml-test-suite/tests
  touch crates/glaucus-wasm/src/lib.rs crates/glaucus-bench/src/lib.rs
  touch crates/glaucus-bench/benches/scanner.rs \
    crates/glaucus-bench/benches/parser.rs \
    crates/glaucus-bench/benches/composer.rs \
    crates/glaucus-bench/benches/emitter.rs \
    crates/glaucus-bench/benches/serde.rs \
    crates/glaucus-bench/benches/end_to_end.rs
  touch crates/glaucus-fuzz/fuzz_targets/fuzz_scanner.rs \
    crates/glaucus-fuzz/fuzz_targets/fuzz_parser.rs \
    crates/glaucus-fuzz/fuzz_targets/fuzz_round_trip.rs \
    crates/glaucus-fuzz/fuzz_targets/fuzz_limits.rs \
    crates/glaucus-fuzz/fuzz_targets/fuzz_serde.rs \
    crates/glaucus-fuzz/fuzz_targets/fuzz_cst_roundtrip.rs \
    crates/glaucus-fuzz/fuzz_targets/fuzz_lossless_edit.rs \
    crates/glaucus-fuzz/fuzz_targets/fuzz_merge_keys.rs
  touch crates/glaucus-yaml-test-suite/tests/yaml_test_suite.rs \
    crates/glaucus-yaml-test-suite/tests/round_trip_fidelity.rs

  # Build targeting the exact architecture matching the multi-arch flow,
  # scoped with `--release -p glaucus-cli` to mirror the `builder` stage's
  # real build exactly -- that is what makes the cache this stage populates
  # the cache that stage reuses, rather than warming a debug profile or
  # crates nothing here needs.
  case "$TARGETPLATFORM" in
    "linux/arm64")
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      cargo build --release --locked -p glaucus-cli --target aarch64-unknown-linux-gnu
      ;;
    "linux/amd64")
      # Mold needs clang flag inside cargo to match your .cargo/config.toml
      cargo build --release --locked -p glaucus-cli --target x86_64-unknown-linux-gnu
      ;;
  esac
EOF

# Build stage
FROM dependencies AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM
ARG APP_VERSION

ENV BUILDPLATFORM=$BUILDPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM
ENV BUILD_PATH=/usr/src/app
ENV RUSTUP_PROFILE=minimal
ENV APP_VERSION=${APP_VERSION}

WORKDIR ${BUILD_PATH}

# Copy the actual source code over (invalidating the cache from this point
# forward). This includes `.git` -- `.dockerignore` deliberately does NOT
# exclude it, so `crates/glaucus/build.rs` can run `git rev-parse`/`git
# status` here and stamp a real commit instead of `unknown`. See
# `.dockerignore` for the full reasoning and its trade-offs.
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
 --mount=type=cache,target=/usr/local/cargo/git \
 --mount=type=cache,target=${BUILD_PATH}/target,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  mkdir -p /out
  rm -f rust-toolchain.toml

  # Clean old binary artifacts to ensure fresh build variables. `glaucus-cli`
  # is the package built now, but the OUTPUT binary is still named `glaucus`
  # (`[[bin]] name = "glaucus"` in both crates' Cargo.toml), so this still
  # matches what needs cleaning.
  rm -rf target/*/release/glaucus* target/*/release/libglaucus* \
         target/*/incremental target/*/build/glaucus*

  case "$TARGETPLATFORM" in
    "linux/arm64")
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
      export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
      export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
      export PKG_CONFIG_ALLOW_CROSS=1
      export CARGO_BUILD_VERSION=${APP_VERSION}
      export TARGET=aarch64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu

      cargo build --release --locked -p glaucus-cli --target aarch64-unknown-linux-gnu
      cp -v target/aarch64-unknown-linux-gnu/release/glaucus /out/glaucus
      ;;

    "linux/amd64")
      export CARGO_BUILD_VERSION=${APP_VERSION}
      export TARGET=x86_64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

      # Use mold linker for maximum link speed on amd64
      export RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold ${RUSTFLAGS:-}"

      cargo build --release --locked -p glaucus-cli --target x86_64-unknown-linux-gnu
      cp -v target/x86_64-unknown-linux-gnu/release/glaucus /out/glaucus
      ;;
  esac
EOF

# Runtime stage - minimal image with just the binary and runtime deps.
#
# Braced `${IMAGE_TAG_RUNTIME}`, matching the dependencies stage above: Scorecard's
# pinned-dependencies check resolves the braced form back to the ARG's digest
# but not the bare `$IMAGE_TAG_RUNTIME`, so this line alone read as unpinned.
# The declaration is at the top of the file, not here -- see the note there.
FROM docker.io/library/debian:${IMAGE_TAG_RUNTIME} AS runtime

# Install only runtime dependencies (SSL certs, libssl, tini for signal handling)
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
 --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
  #!/bin/bash
  set -exo pipefail
  apt-get update
  apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    tini
  rm -rf /var/lib/apt/lists/*
EOF

# Fixed-UID non-root user with a REAL home directory (`-m` is actually used
# here: HOME matters for anything the CLI or its dependencies consult -- XDG
# config/cache dirs, `.cache`, etc).
RUN groupadd -g 10000 glaucus &&\
 useradd -u 10000 -g glaucus -d /home/glaucus -m -s /usr/sbin/nologin glaucus

# The bind-mount point: whatever the caller mounts over `/data` is what
# `glaucus fmt`/`validate`/etc. operate on via relative paths. Owned by the
# user this image runs as (not root), so a bind mount lands writable.
RUN mkdir -p /data && chown glaucus:glaucus /data
WORKDIR /data

# Copy the binary from builder with ownership root:glaucus, straight onto
# PATH: `docker run img fmt a.yaml` needs no path to the binary, and
# `/usr/local/bin` is already on PATH for every shell on Debian.
COPY --from=builder --chown=root:glaucus /out/glaucus /usr/local/bin/glaucus

# Security labels for orchestration tools
LABEL org.opencontainers.image.security.no-new-privileges="true" \
      org.opencontainers.image.security.read-only-rootfs="true" \
      org.opencontainers.image.security.capabilities.drop="ALL" \
      org.opencontainers.image.security.run-as-non-root="true" \
      org.opencontainers.image.security.run-as-user="10000" \
      org.opencontainers.image.security.run-as-group="10000"

# Entrypoint wrapper: allows `docker run glaucus:0.1.0 bash` for debugging
RUN <<EOF
#!/bin/bash
set -exo pipefail

# QUOTED delimiter, so `\$@` and `\${1:-}` land in the file literally instead of
# being expanded by THIS shell while the image builds. Unquoted, the wrapper was
# written with the build-time values baked in and every runtime argument was
# discarded: `docker run glaucus --version` ran `/app/glaucus pipefail`.
cat > /entrypoint.sh <<'SCRIPT'
#!/bin/bash
# `-exo pipefail`, not `-ex pipefail`. The latter takes `pipefail` as a
# POSITIONAL parameter rather than an option, so it overwrote "$@" with
# `pipefail` before the dispatch below could read it -- which is how the
# build-time value got in here in the first place.
set -exo pipefail

# If first arg is a shell, exec it directly (for debugging)
case "${1:-}" in
  bash|sh|/bin/bash|/bin/sh)
    exec tini -- "$@"
    ;;
esac

# Default: run glaucus with args
exec tini -- /usr/local/bin/glaucus "$@"
SCRIPT
chmod +x /entrypoint.sh
chown root:glaucus /entrypoint.sh
EOF

# Switch to non-root user
USER glaucus:glaucus

ENTRYPOINT ["/entrypoint.sh"]
CMD ["--help"]
