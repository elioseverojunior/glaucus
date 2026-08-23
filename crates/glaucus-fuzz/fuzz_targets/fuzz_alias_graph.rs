// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzes anchor/alias graphs against the alias-materialisation budget.
//!
//! `fuzz_limits` clamps every limit at once, which means an input is usually
//! refused by whichever cap happens to trip first — often `max_node_count` on
//! the source document, long before any alias is resolved. This target does the
//! opposite: it leaves the source-shaped limits generous and constrains only
//! `max_total_alias_nodes`, so the alias path is the one under pressure and the
//! budget is the thing actually being exercised.
//!
//! The property under test is not "the input is rejected" — plenty of anchor
//! graphs are perfectly legal. It is that the composer always **terminates with
//! a value or an error, never a panic and never unbounded growth**, whatever
//! shape of anchor, alias, cycle, or forward reference the fuzzer invents.

#![no_main]
use glaucus_ast::composer::Composer;
use glaucus_core::error::ParserConfig;
use glaucus_core::limits::ResourceLimits;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let config = ParserConfig {
            limits: ResourceLimits {
                // Deep enough that nesting is not what rejects the input.
                max_depth: 64,
                // Generous: the point is to let aliases actually be resolved, so
                // the occurrence counter must not pre-empt the materialisation
                // budget. These two count different things and this target is
                // about the second one.
                max_alias_expansions: 100_000,
                max_document_size: 65_536,
                max_key_length: 1_024,
                // Generous relative to the alias budget below, so rejection comes
                // from materialisation rather than from source size.
                max_node_count: 100_000,
                // The limit under test. Small, so a bomb trips it quickly and the
                // fuzzer is not spending its budget allocating.
                max_total_alias_nodes: 512,
                // Generous: anchors are the raw material this target needs.
                max_anchors: 4_096,
                max_anchor_name_length: 256,
            },
            ..Default::default()
        };
        for result in Composer::with_config(s, config) {
            let _ = result;
        }
    }
});
