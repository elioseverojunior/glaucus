// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deserialization straight from the parser event stream, without a node tree.
//!
//! Built alongside the tree path rather than replacing it, and kept honest by
//! `tests/differential.rs`, which compares the two across a corpus.

pub(crate) mod scalar;
pub(crate) mod tape;
pub(crate) mod value;
