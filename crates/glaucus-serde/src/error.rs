// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Error types bridging `glaucus_core` errors to serde's error traits.

use std::fmt;

/// Error type for serde operations.
///
/// Wraps both glaucus-core parsing errors and serde custom messages.
#[derive(Debug)]
pub struct Error {
    inner: ErrorInner,
}

#[derive(Debug)]
enum ErrorInner {
    /// Error from glaucus-core (parsing, composing).
    Core(glaucus_core::error::Error),
    /// Failure reading the input, kept as the original `io::Error`.
    ///
    /// Held as a typed error rather than a message so a caller can still tell
    /// `NotFound` from `PermissionDenied` from a malformed document. Flattening
    /// it into prose makes those three indistinguishable, and "the file is
    /// missing" and "the YAML is wrong" want different handling.
    ///
    /// This lives here rather than in `glaucus_core::ErrorKind` because that enum
    /// derives `Clone` and `PartialEq`, and `std::io::Error` is neither.
    Io(std::io::Error),
    /// Custom message from serde (e.g. "missing field `name`").
    Custom(String),
}

impl Error {
    /// Creates an error from a glaucus-core error.
    #[must_use]
    pub const fn core(err: glaucus_core::error::Error) -> Self {
        Self {
            inner: ErrorInner::Core(err),
        }
    }

    /// Creates an error from a failure to read the input.
    #[must_use]
    pub const fn io(err: std::io::Error) -> Self {
        Self {
            inner: ErrorInner::Io(err),
        }
    }

    /// Returns the underlying glaucus-core error, if this is a core error.
    #[must_use]
    pub const fn as_core(&self) -> Option<&glaucus_core::error::Error> {
        match &self.inner {
            ErrorInner::Core(e) => Some(e),
            ErrorInner::Io(_) | ErrorInner::Custom(_) => None,
        }
    }

    /// Returns the underlying I/O error, if the input could not be read.
    ///
    /// Prefer this to matching on the message: it is the difference between
    /// retrying a transient read and reporting a malformed document.
    #[must_use]
    pub const fn as_io(&self) -> Option<&std::io::Error> {
        match &self.inner {
            ErrorInner::Io(e) => Some(e),
            ErrorInner::Core(_) | ErrorInner::Custom(_) => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            ErrorInner::Core(e) => write!(f, "{e}"),
            ErrorInner::Io(e) => write!(f, "failed to read input: {e}"),
            ErrorInner::Custom(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.inner {
            ErrorInner::Core(e) => Some(e),
            ErrorInner::Io(e) => Some(e),
            // A serde-origin message ("missing field `name`") has nothing beneath
            // it. Inventing a source would be worse than reporting none.
            ErrorInner::Custom(_) => None,
        }
    }
}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self {
            inner: ErrorInner::Custom(msg.to_string()),
        }
    }
}

impl serde::ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self {
            inner: ErrorInner::Custom(msg.to_string()),
        }
    }
}

impl From<glaucus_core::error::Error> for Error {
    fn from(err: glaucus_core::error::Error) -> Self {
        Self::core(err)
    }
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_de_error_display() {
        let err = <Error as serde::de::Error>::custom("missing field `name`");
        assert_eq!(err.to_string(), "missing field `name`");
        assert!(err.as_core().is_none());
    }

    #[test]
    fn custom_ser_error_display() {
        let err = <Error as serde::ser::Error>::custom("key must be a string");
        assert_eq!(err.to_string(), "key must be a string");
    }

    #[test]
    fn core_error_conversion() {
        let core_err =
            glaucus_core::error::Error::spanless(glaucus_core::error::ErrorKind::UnexpectedEof);
        let err = Error::from(core_err);
        assert!(err.as_core().is_some());
        assert!(err.to_string().contains("unexpected end of input"));
    }

    #[test]
    fn error_source_chain() {
        use std::error::Error as StdError;

        let core_err =
            glaucus_core::error::Error::spanless(glaucus_core::error::ErrorKind::UnexpectedEof);
        let err = Error::from(core_err);
        assert!(err.source().is_some());

        let custom = <Error as serde::de::Error>::custom("test");
        assert!(custom.source().is_none());
    }
}
