// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Serde integration for the glaucus YAML library.
//!
//! Provides serialization/deserialization between YAML text and Rust types
//! via serde's `Serialize` and `Deserialize` traits.
//!
//! # Quick Start
//!
//! ```
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Deserialize, Serialize, PartialEq)]
//! struct Config {
//!     name: String,
//!     debug: bool,
//! }
//!
//! let yaml = "name: glaucus\ndebug: true\n";
//! let config: Config = glaucus_serde::from_str(yaml).unwrap();
//! assert_eq!(config.name, "glaucus");
//!
//! let output = glaucus_serde::to_string(&config).unwrap();
//! assert!(output.contains("name: glaucus"));
//! ```

#![forbid(unsafe_code)]

/// Base64 decoding for `!!binary` scalars. Internal: the decoder exists to
/// serve tag resolution, not as a general-purpose codec.
mod base64;
pub mod borrowed;
pub mod de;
pub mod error;
pub mod ser;
mod stream;
// The streaming serializer drives the public `to_string`/`to_string_with` and
// `to_writer`/`to_writer_with` paths (no intermediate `Node` tree).
pub(crate) mod stream_ser;
pub mod styled;
pub mod value;

// Top-level re-exports for convenience.
pub use borrowed::BorrowedValue;
pub use de::{
    from_reader, from_reader_with, from_str, from_str_multi, from_str_multi_with, from_str_with,
};
pub use error::{Error, Result};
pub use ser::{to_string, to_string_with, to_writer, to_writer_with};
pub use styled::{FlowMap, FlowSeq, FoldStr, LitStr};
pub use value::Value;
