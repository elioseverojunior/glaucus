// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Glaucus YAML AST — the `Node` representation tree, composer, and emitter.
//!
//! Built on the `glaucus-core` front-end (scanner → parser → events).

pub mod composer;
pub mod emitter;
pub mod node;

pub use composer::{Composer, compose_all};
pub use emitter::{EmitterConfig, emit, emit_to_string};
pub use node::{Mapping, Node, Scalar, Sequence};
