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
ARG IMAGE_TAG_BUILD=1.99-slim-trixie@sha256:4df2ae8cfdfebdfc13a65da7f253b73d68983dc0339f2719bc17a279c913b13d
ARG IMAGE_TAG_RUNTIME=trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f

# Dependency stage

FROM --platform=$BUILDPLATFORM docker.io/library/rust:${IMAGE_TAG_BUILD} AS dependencies

SHELL ["/bin/bash", "-eou", "pipefail", "-c"]

ARG IMAGE_TAG_BUILD
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

  # A native build (BUILDPLATFORM == TARGETPLATFORM) needs no cross toolchain:
  # Debian's own gcc/binutils already ship the triplet-prefixed names the
  # builder stage exports (`aarch64-linux-gnu-gcc` on an arm64 host). That is
  # how docker-release.yml builds each architecture -- on a runner of that
  # architecture -- so only a local `--platform linux/arm64` build on an amd64
  # host still cross-compiles.
  case "$BUILDPLATFORM -> $TARGETPLATFORM" in
    "linux/amd64 -> linux/amd64" | "linux/arm64 -> linux/arm64")
      echo "Native $TARGETPLATFORM build, no cross-compilation tools needed."
      ;;
    "linux/amd64 -> linux/arm64")
      dpkg --add-architecture arm64
      apt-get update
      apt-get install -y --no-install-recommends \
        gcc-aarch64-linux-gnu \
        g++-aarch64-linux-gnu \
        libc6-dev-arm64-cross \
        libssl-dev:arm64
      ;;
    *)
      echo "Unsupported build: $BUILDPLATFORM -> $TARGETPLATFORM (build on a host of the target architecture)"
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

ENV BUILDPLATFORM=$BUILDPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM
ENV BUILD_PATH=/usr/src/app
ENV RUSTUP_PROFILE=minimal

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

  # Re-stamp every workspace source as newest-wins. Cargo does NOT hash the
  # contents of path/workspace crates -- it records their source list in
  # `.fingerprint/<pkg>-<hash>/dep-lib-<name>` and compares each file's MTIME
  # against the recorded build time. `COPY . .` above restores real sources
  # carrying their original (older) mtimes from the build context, while the
  # `dependencies` stage's placeholders were `touch`ed to build time -- so
  # "new content, old mtime" reads as fresh and cargo links the stale artefact.
  #
  # This is the name-agnostic half of the guard: the sweep below is keyed on the
  # `glaucus-` prefix and would silently skip a workspace crate named anything
  # else, which is the same silent-staleness failure it exists to prevent. This
  # line needs no such list. Registry dependencies live in
  # /usr/local/cargo/registry and are untouched, so their cache still hits.
  find crates \( -name '*.rs' -o -name 'Cargo.toml' \) -exec touch {} +

  # Drop EVERY artefact belonging to a workspace crate before compiling the
  # real sources. This is a correctness requirement, not a tidiness one: the
  # `dependencies` stage `touch`es placeholder `lib.rs` files, so they carry
  # build-time mtimes, and `COPY . .` above then restores the real sources with
  # their ORIGINAL (older) mtimes from the build context. Cargo's freshness
  # check is mtime-based -- source older than the cached artefact reads as
  # fresh -- so it happily links the EMPTY placeholder rlibs and the build dies
  # with `E0432: unresolved imports ... no \`error\` in the root`.
  #
  # The previous globs could not prevent that: rlibs live in `release/deps/`,
  # which `target/*/release/libglaucus*` does not reach, and nothing removed
  # the `.fingerprint/` entries that actually decide freshness. Deleting the
  # fingerprints is what makes this deterministic rather than mtime-dependent.
  #
  # This never fired before only because a stray `export RUSTFLAGS` in the
  # amd64 branch below (since removed) changed every fingerprint hash and
  # forced a full rebuild -- one bug masking the other.
  #
  # Both `target/release` and `target/<triple>/release` are swept: with
  # `--target`, build scripts are compiled for the HOST into the former while
  # the crates themselves land in the latter.
  for d in target/release target/*/release; do
    [ -d "$d" ] || continue
    rm -rf "$d"/.fingerprint/glaucus-* "$d"/build/glaucus-* \
           "$d"/deps/*glaucus* "$d"/glaucus "$d"/glaucus.d "$d"/incremental
  done

  case "$TARGETPLATFORM" in
    "linux/arm64")
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
      export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
      export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
      export PKG_CONFIG_ALLOW_CROSS=1
      export TARGET=aarch64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu

      cargo build --release --locked -p glaucus-cli --target aarch64-unknown-linux-gnu
      cp -v target/aarch64-unknown-linux-gnu/release/glaucus /out/glaucus
      ;;

    "linux/amd64")
      export TARGET=x86_64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

      # No `export RUSTFLAGS` here, deliberately. `.cargo/config.toml` already
      # gives this triple `linker = "clang"` and `rustflags = ["-D", "warnings",
      # "-C", "link-arg=-fuse-ld=mold"]`, so mold was always applied. Setting
      # the environment variable does not ADD to that -- cargo resolves
      # rustflags first-match-wins (env, then `[target.<triple>]`, then
      # `[build]`) and never merges, so the export REPLACED the config and
      # silently dropped `-D warnings` from every release image.
      #
      # It also broke the `dependencies` stage above: that stage builds with the
      # config's flags, this one built with different ones, so every fingerprint
      # missed. A verified build recompiled 49 crates here -- serde_core,
      # anstyle, clap_lex and the rest of the third-party tree the cache stage
      # exists to pre-warm. Removing the line restores the gate and the cache
      # in one go; `-C linker=clang` was redundant with the `linker` key, which
      # RUSTFLAGS never overrode anyway.
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
# `-eo pipefail`, NOT `-exo`. The `x` traced this wrapper's own execution onto
# the container's stderr on every single run: a verified image emitted
# "+ case \"${1:-}\" in" and "+ exec tini -- /usr/local/bin/glaucus fmt a.yaml"
# ahead of the command's real output -- 108 bytes of noise before a one-line
# summary. stderr is where this CLI writes diagnostics, so the trace corrupted
# the very stream anything parsing `glaucus` output would read.
#
# `-eo pipefail`, not `-e pipefail`: the latter takes `pipefail` as a
# POSITIONAL parameter rather than an option, overwriting "$@" with `pipefail`
# before the dispatch below can read it.
set -eo pipefail

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
