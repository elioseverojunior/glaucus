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

    /// Returns the underlying glaucus-core error, if this is a core error.
    #[must_use]
    pub const fn as_core(&self) -> Option<&glaucus_core::error::Error> {
        match &self.inner {
            ErrorInner::Core(e) => Some(e),
            ErrorInner::Custom(_) => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            ErrorInner::Core(e) => write!(f, "{e}"),
            ErrorInner::Custom(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.inner {
            ErrorInner::Core(e) => Some(e),
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
