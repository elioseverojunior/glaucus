// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]
use glaucus_ast::composer::Composer;
use glaucus_core::error::ParserConfig;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let cfg = ParserConfig {
            merge_keys: true,
            ..Default::default()
        };
        // merge resolution must be panic-free on arbitrary input (Err is fine)
        for doc in Composer::with_config(s, cfg) {
            let _ = doc;
        }
    }
});
