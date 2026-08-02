// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Exit-code taxonomy. Kept identical to the retired `glaucus-fmt` so existing
//! scripts keep working.

/// Success, no findings.
pub const OK: u8 = 0;
/// Findings: unformatted under `--check`, schema violations, parse errors.
pub const FINDINGS: u8 = 1;
/// Usage error. Matches clap's own default.
pub const USAGE: u8 = 2;
/// I/O failure: unreadable file, permission denied.
pub const IO: u8 = 3;
