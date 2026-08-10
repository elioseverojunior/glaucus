<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Glaucus

**Safe YAML for Rust** — zero `unsafe` by default, full YAML 1.2.2 spec compliance, high performance.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![codecov](https://codecov.io/gh/elioseverojunior/glaucus/graph/badge.svg?token=C1C23U0Y3G)](https://codecov.io/gh/elioseverojunior/glaucus)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/elioseverojunior/glaucus/badge)](https://securityscorecards.dev/viewer/?uri=github.com/elioseverojunior/glaucus)
[![SLSA 3](https://slsa.dev/images/gh-badge-level3.svg)](https://slsa.dev)

Glaucus is a from-scratch YAML 1.2.2 library built for safety, correctness, and speed. It passes **100% of the official YAML test suite** (735/735 tests) and provides both a serde integration and a low-level node API.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
glaucus = "0.2"
serde = { version = "1", features = ["derive"] }
```

### Deserialize with Serde

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    name: String,
    debug: bool,
    port: u16,
}

let config: Config = glaucus::from_str("
name: my-app
debug: true
port: 8080
").unwrap();

assert_eq!(config.name, "my-app");
assert_eq!(config.port, 8080);
```

### Serialize with Serde

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Point { x: i32, y: i32 }

let yaml = glaucus::to_string(&Point { x: 1, y: 2 }).unwrap();
// x: 1
// y: 2
```

### Round-Trip

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Server {
    host: String,
    port: u16,
}

let original = Server { host: "localhost".into(), port: 443 };
let yaml = glaucus::to_string(&original).unwrap();
let restored: Server = glaucus::from_str(&yaml).unwrap();
assert_eq!(original, restored);
```

### Node API (No Serde)

For dynamic YAML processing without predefined types:

```rust
// Parse
let node = glaucus::from_str_node("hello: world").unwrap();
let entries = node.as_mapping().unwrap();
assert_eq!(entries[0].0.as_str(), Some("hello"));
assert_eq!(entries[0].1.as_str(), Some("world"));

// Emit
let yaml = glaucus::to_string_node(&node);
assert_eq!(yaml, "hello: world\n");
```

### Multi-Document

```rust
let docs = glaucus::from_str_multi("---\nfirst\n---\nsecond\n").unwrap();
assert_eq!(docs.len(), 2);
assert_eq!(docs[0].as_str(), Some("first"));
```

### Read from Files

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Config { name: String }

let file = std::fs::File::open("config.yaml").unwrap();
let config: Config = glaucus::from_reader(file).unwrap();
```

## Command-Line Interface

Glaucus also ships a `glaucus` CLI (`fmt`, `validate`, `parse`, `convert`,
`schema`, `lsp`, `mcp`, `completions`). Install it either of these
equivalent ways:

```sh
cargo install glaucus-cli            # flag-free
cargo install glaucus --features cli # equivalent
```

Installing both is redundant — they produce the same `glaucus` binary. Run
`glaucus --help` for the full command reference.

## Why Glaucus?

### Safety First

- **`#![forbid(unsafe_code)]`** on the core crate — no exceptions
- **Built-in resource limits** protect against adversarial inputs out of the box:

| Protection                        | Default Limit |
| --------------------------------- | ------------- |
| Nesting depth (stack overflow)    | 128 levels    |
| Alias expansions (billion laughs) | 1,024         |
| Document size (memory exhaustion) | 256 MiB       |
| Key length                        | 1,024 bytes   |
| Node count (CPU exhaustion)       | 1,000,000     |

- **Strict mode by default** — duplicate keys are errors, not silent overwrites

### Spec Complete

- **735/735** [Official YAML Test Suite](https://github.com/yaml/yaml-test-suite) cases pass (v2022-01-17)
- Full YAML 1.2.2 support: anchors, aliases, tags, block scalars, flow collections, multi-document streams
- Correct handling of edge cases that trip up other parsers

### Fast

Benchmarked against [yaml-rust2](https://github.com/Ethiraric/yaml-rust2) (the most used Rust YAML parser) on real-world documents (K8s Pod, Helm values, 800-entry config):

| Operation | Small (~210B) | Medium (~3KB) | Large (~119KB) | vs yaml-rust2          |
| --------- | ------------- | ------------- | -------------- | ---------------------- |
| **Parse** | 68 us         | 663 us        | 34.2 ms        | **1.3x – 2.0x faster** |
| **Load**  | 99 us         | 1.2 ms        | 46.3 ms        | **up to 2.2x faster**  |
| **Emit**  | 10.7 us       | 49 us         | 2.5 ms         | **1.8x – 5.4x faster** |

Glaucus reads YAML about **twice as fast** and writes it up to **5x faster**. The emitter processes data at **45–63 MB/s** compared to yaml-rust2's **9–13 MB/s**, making Glaucus particularly well-suited for tools that generate a lot of YAML output.

### Zero-Copy Design

Scalar values use `Cow<'a, str>` — when no transformation is needed (plain scalars), Glaucus borrows directly from the input buffer with zero allocation.

## Crate Architecture

Glaucus is organized as a workspace of focused crates:

```mermaid
graph TD
    USER["Your Code"]
    GLAUCUS["glaucus<br/><i>facade crate</i>"]
    SERDE["glaucus-serde<br/><i>serde integration</i>"]
    CORE["glaucus-core<br/><i>zero dependencies</i>"]
    S["serde"]

    USER --> GLAUCUS
    GLAUCUS --> SERDE
    GLAUCUS --> CORE
    SERDE --> CORE
    SERDE --> S

    style GLAUCUS fill:#4a9eff,color:#fff,stroke:#2970c9
    style CORE fill:#2ea043,color:#fff,stroke:#1a7431
    style SERDE fill:#8957e5,color:#fff,stroke:#6e40c9
    style USER fill:#656d76,color:#fff,stroke:#484f58
    style S fill:#656d76,color:#fff,stroke:#484f58
```

### Processing Pipeline

```mermaid
graph LR
    subgraph Loading["Loading Pipeline"]
        direction LR
        INPUT["&str"] --> SCAN["Scanner"]
        SCAN -->|Token| PARSE["Parser"]
        PARSE -->|Event| COMPOSE["Composer"]
        COMPOSE --> NODE["Node Tree"]
    end

    subgraph Serde["Serde Bridge"]
        direction LR
        NODE -->|Deserializer| T["T"]
        T -->|Serializer| NODE2["Node"]
    end

    subgraph Dumping["Dumping Pipeline"]
        direction LR
        NODE2 --> EMIT["Emitter"]
        EMIT --> OUTPUT["String"]
    end

    style Loading fill:#dafbe1,stroke:#2ea043
    style Serde fill:#e8d5f5,stroke:#8957e5
    style Dumping fill:#ddf4ff,stroke:#4a9eff
```

Each stage is independently usable. You can scan tokens, consume parser events, or work with the composed node tree — whatever level of control you need.

### Scanner

Reads raw `&str` input byte-by-byte and produces a stream of typed tokens. Handles BOM detection, character classification via `[u8; 256]` lookup tables, and tracks source positions for span reporting.

```mermaid
graph LR
    A["&str"] --> B["BOM Detection"]
    B --> C["Character<br/>Classification"]
    C --> D["Token Stream"]
    D --> E["Token + Span"]

    style A fill:#f6f8fa,stroke:#d1d9e0
    style E fill:#2ea043,color:#fff,stroke:#1a7431
```

### Parser

Consumes tokens from the scanner and produces a stream of semantic events. Uses an iterative state machine (no recursion) with depth checking against `ResourceLimits::max_depth`.

```mermaid
graph LR
    A["Token Stream"] --> B["State Machine"]
    B --> C{"Depth<br/>Check"}
    C -->|OK| D["Event Stream"]
    C -->|Exceeded| E["Error"]

    style A fill:#f6f8fa,stroke:#d1d9e0
    style D fill:#2ea043,color:#fff,stroke:#1a7431
    style E fill:#f85149,color:#fff,stroke:#cf222e
```

### Composer

Transforms the event stream into a `Node` tree. Resolves anchors/aliases, enforces duplicate key detection, and checks node count, key length, and alias expansion limits.

```mermaid
graph LR
    A["Event Stream"] --> B["Tree Builder"]
    B --> C["Anchor<br/>Resolution"]
    C --> D{"Limits<br/>Check"}
    D -->|OK| E["Node Tree"]
    D -->|Exceeded| F["Error"]

    style A fill:#f6f8fa,stroke:#d1d9e0
    style E fill:#2ea043,color:#fff,stroke:#1a7431
    style F fill:#f85149,color:#fff,stroke:#cf222e
```

### Emitter

Walks a `Node` tree and writes YAML text. Supports configurable indentation, key sorting, explicit document markers, and all scalar styles (plain, single-quoted, double-quoted, literal block, folded block).

```mermaid
graph LR
    A["Node Tree"] --> B["Style Selection"]
    B --> C["Indentation<br/>Engine"]
    C --> D["YAML String"]

    style A fill:#f6f8fa,stroke:#d1d9e0
    style D fill:#4a9eff,color:#fff,stroke:#2970c9
```

## Advanced Usage

### Custom Emitter Configuration

```rust
use serde::Serialize;
use glaucus::emitter::EmitterConfig;

#[derive(Serialize)]
struct Data { key: String }

let config = EmitterConfig {
    indent: 4,                  // 4 spaces (default: 2)
    sort_keys: true,            // alphabetical keys
    explicit_document: true,    // emit --- markers
    ..EmitterConfig::default()
};

let yaml = glaucus::to_string_with(&Data { key: "val".into() }, &config).unwrap();
assert!(yaml.starts_with("---"));
```

### Working with `Value` (Dynamic Typed)

```rust
// Parse YAML into a dynamic Value, then serialize back
let value: glaucus::Value = glaucus::from_str("
name: glaucus
tags:
  - yaml
  - rust
").unwrap();

assert!(value.is_mapping());
let yaml = glaucus::to_string(&value).unwrap();
```

### Low-Level Pipeline Access

```rust
use glaucus::scanner::Scanner;
use glaucus::parser::Parser;
use glaucus::composer;

// Scan tokens
let scanner = Scanner::new("key: value");
for token in scanner {
    println!("{:?}", token.unwrap());
}

// Parse events
let parser = Parser::new("key: value");
for event in parser {
    println!("{:?}", event.unwrap());
}

// Compose nodes
let nodes = composer::compose_all("key: value").unwrap();
```

## Real-World Examples

Glaucus handles the YAML formats you actually work with:

```rust
use serde::Deserialize;

// Kubernetes manifests
#[derive(Deserialize)]
struct K8sPod {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: Metadata,
}

#[derive(Deserialize)]
struct Metadata {
    name: String,
    #[serde(default)]
    labels: std::collections::BTreeMap<String, String>,
}

let pod: K8sPod = glaucus::from_str("
apiVersion: v1
kind: Pod
metadata:
  name: nginx
  labels:
    app: web
").unwrap();

assert_eq!(pod.kind, "Pod");
assert_eq!(pod.metadata.labels["app"], "web");
```

## Versioning and Releases

Versions come from [GitVersion](https://gitversion.net) and are never hand-edited.

The **crate version** is GitVersion's `MajorMinorPatch`: always a plain
`MAJOR.MINOR.PATCH` with no pre-release part, so every release on crates.io is a
stable one. GitVersion's `SemVer` is deliberately *not* used for it, because it
appends the commits-since-last-release counter (`0.1.0-1`) — and under SemVer
anything after the hyphen makes the whole version a **pre-release**. crates.io
then offers no stable version at all, and `cargo add glaucus` silently resolves
something else.

A release on the default branch pushes four tags:

| Tag | Example | Moves? |
| --- | --- | --- |
| `vX.Y.Z-N` | `v0.1.0-3` | No — a unique marker for that exact commit |
| `vX.Y.Z` | `v0.1.0` | No — the release itself |
| `vX.Y` | `v0.1` | Yes — newest patch in that minor line |
| `vX` | `v0` | Yes — newest release in that major line |

`vX.Y.Z-N` carries GitVersion's `SemVer`, so it names one point in history and
can never collide with a later commit. The GitHub Release and the crates.io
upload both use `vX.Y.Z`; the two shorter tags are floating pointers, so `v0`
always resolves to the newest 0.x release.

Only `vX.Y.Z` becomes a `CHANGELOG.md` section. The release and its `-N` marker
sit on the same commit, and git-cliff resolves one tag per commit — so while
`cliff.toml`'s `tag_pattern` accepted a suffix it preferred the marker and the
changelog read `## [0.1.1-13]`, a heading no release, tag link or crates.io
version ever uses. The pattern now anchors after the patch component, which also
means a genuine pre-release such as `v0.2.0-rc1` gets no section of its own: its
commits stay under `[Unreleased]` until a stable release absorbs them.

Cut a release with `mise run release:prepare`: it computes the next version and
writes it into every file that restates it. It refuses to run outside the
default branch, where GitVersion labels versions with the branch name.

`CHANGELOG.md` is **not** written there. The release pipeline regenerates and
commits it after the crates are published, so the file records a version that
actually reached crates.io rather than a local guess at the next one — and a
release that fails to publish leaves no changelog entry claiming otherwise.

## Minimum Supported Rust Version

Rust **1.88** or later (edition 2024).

Edition 2024 alone would allow 1.85, and no dependency requires more than that,
but the parser uses let-chains — stabilised in 1.88 — so that is the real floor.

The MSRV is the version declared in `rust-version` (`Cargo.toml`), and CI
compiles every publishable crate with exactly that toolchain on every run, so
the number is verified rather than asserted. It is independent of
`rust-toolchain.toml`, which pins the toolchain the project is *developed* with
and tracks stable.

Raising the MSRV is a **minor** version bump accompanied by a `CHANGELOG.md`
entry — never a patch release.

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE-2.0)
- [MIT License](LICENSE-MIT)
