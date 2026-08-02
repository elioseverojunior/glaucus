<!--
SPDX-FileCopyrightText: Glaucus contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

## What

<!-- Short description of the change. -->

## Why

<!-- Motivation. Link an issue if one exists: closes #NNN -->

## Testing

<!-- How was this validated? Local lint? `act` run? Real workflow trigger? -->

## Checklist

- [ ] Conventional Commits used in commit messages
- [ ] `mise run pr:ready` passes (fmt, clippy, tests, doctests)
- [ ] Coverage still at the `RUST_COVERAGE_THRESHOLD` in `mise.toml`
- [ ] New `.rs` files carry the 2-line SPDX header (`mise run comply:fix <paths>`)
- [ ] Tests added as inline `#[cfg(test)] mod tests`, written before the code
- [ ] Breaking change? If yes, describe migration below

### Breaking change notes

<!-- Leave empty if not applicable. -->
