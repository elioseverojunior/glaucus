<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# glaucus-cli

**Flag-free install path for the `glaucus` CLI.**

This package exists so `cargo install glaucus-cli` works without having to
remember `--features cli`. It has no code of its own — its only file is a
one-line `main` that calls `glaucus::cli::process::main()` — and it depends
on [`glaucus`](../glaucus) with the `cli` feature already enabled. It
installs the exact same `glaucus` binary as:

```sh
cargo install glaucus --features cli
```

Installing both `glaucus-cli` and `glaucus --features cli` is redundant:
pick whichever install command you find easier to remember, not both.

## Where `glaucus-fmt`, `glaucus-validate`, `glaucus-lsp`, and `glaucus-mcp` went

Those four crates were published once, at `0.0.1-1`, and are now yanked and
superseded. Their functionality was folded into the `glaucus` crate itself,
behind the `cli` feature, as subcommands of the one `glaucus` binary and as
library modules:

| Former crate       | Now                                    |
| ------------------- | --------------------------------------- |
| `glaucus-fmt`       | `glaucus fmt` / `glaucus::fmt`           |
| `glaucus-validate`  | `glaucus validate` / `glaucus::validate` |
| `glaucus-lsp`       | `glaucus lsp` / `glaucus::lsp`           |
| `glaucus-mcp`       | `glaucus mcp` / `glaucus::mcp`           |

`cargo install glaucus-cli` (or `glaucus --features cli`) installs all four
as subcommands of one binary — no need to install the old crates, which will
receive no further releases.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE-2.0) or
[MIT](LICENSE-MIT), at your option.
