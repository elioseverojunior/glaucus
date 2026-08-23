// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core YAML data types.
//!
//! Defines source-location, style, and shared types used throughout
//! the pipeline. The tree types (`Node`, `Scalar`, `Sequence`, `Mapping`)
//! are defined in and re-exported from `glaucus-ast`.

use std::fmt;

// ─── Source Location ────────────────────────────────────────────────

/// A byte offset and line/column position within a YAML source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// Byte offset from the start of the input.
    pub offset: usize,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number (in bytes, not characters).
    pub column: u32,
}

impl Position {
    /// Creates a new position at the start of the input.
    #[must_use]
    pub const fn start() -> Self {
        Self {
            offset: 0,
            line: 1,
            column: 1,
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A span covering a range in the source input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Start position (inclusive).
    pub start: Position,
    /// End position (exclusive).
    pub end: Position,
}

impl Span {
    /// Creates a zero-width span at the given position.
    #[must_use]
    pub const fn point(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Merges two spans into one that covers both.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        let start = if self.start.offset <= other.start.offset {
            self.start
        } else {
            other.start
        };
        let end = if self.end.offset >= other.end.offset {
            self.end
        } else {
            other.end
        };
        Self { start, end }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

// ─── Tags ───────────────────────────────────────────────────────────

/// A YAML tag (e.g. `!!str`, `!!int`, or a custom tag URI).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tag<'a> {
    /// The resolved tag URI.
    pub value: std::borrow::Cow<'a, str>,
    /// Source span of the tag in the input.
    pub span: Span,
}

impl Tag<'_> {
    /// Converts this tag into a `'static` lifetime by taking ownership of borrowed data.
    #[must_use]
    pub fn into_owned(self) -> Tag<'static> {
        Tag {
            value: std::borrow::Cow::Owned(self.value.into_owned()),
            span: self.span,
        }
    }
}

// ─── Styles ─────────────────────────────────────────────────────────

/// The presentation style of a scalar value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarStyle {
    /// No quotes — plain scalar.
    Plain,
    /// Single-quoted scalar (`'...'`).
    SingleQuoted,
    /// Double-quoted scalar (`"..."`).
    DoubleQuoted,
    /// Literal block scalar (`|`).
    Literal,
    /// Folded block scalar (`>`).
    Folded,
}

/// The presentation style of a collection (mapping or sequence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollectionStyle {
    /// Block style (indentation-based).
    Block,
    /// Flow style (inline, JSON-like with `{}` or `[]`).
    Flow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_start() {
        let pos = Position::start();
        assert_eq!(pos.offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn position_display() {
        let pos = Position {
            offset: 42,
            line: 3,
            column: 7,
        };
        assert_eq!(pos.to_string(), "3:7");
    }

    #[test]
    fn span_point() {
        let pos = Position::start();
        let span = Span::point(pos);
        assert_eq!(span.start, span.end);
    }

    #[test]
    fn span_merge() {
        let a = Span {
            start: Position {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Position {
                offset: 5,
                line: 1,
                column: 6,
            },
        };
        let b = Span {
            start: Position {
                offset: 3,
                line: 1,
                column: 4,
            },
            end: Position {
                offset: 10,
                line: 2,
                column: 3,
            },
        };
        let merged = a.merge(b);
        assert_eq!(merged.start.offset, 0);
        assert_eq!(merged.end.offset, 10);
    }

    #[test]
    fn span_merge_reversed() {
        // other starts before self, and self ends after other:
        // exercises the `other.start` and `self.end` branches of merge.
        let a = Span {
            start: Position {
                offset: 8,
                line: 2,
                column: 1,
            },
            end: Position {
                offset: 20,
                line: 3,
                column: 5,
            },
        };
        let b = Span {
            start: Position {
                offset: 2,
                line: 1,
                column: 3,
            },
            end: Position {
                offset: 12,
                line: 2,
                column: 5,
            },
        };
        let merged = a.merge(b);
        assert_eq!(merged.start.offset, 2);
        assert_eq!(merged.end.offset, 20);
    }

    #[test]
    fn span_display() {
        let span = Span {
            start: Position {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Position {
                offset: 5,
                line: 1,
                column: 6,
            },
        };
        assert_eq!(span.to_string(), "1:1..1:6");
    }

    #[test]
    fn tag_into_owned() {
        use std::borrow::Cow;
        let tag = Tag {
            value: Cow::Borrowed("!!str"),
            span: Span::point(Position::start()),
        };
        let owned: Tag<'static> = tag.into_owned();
        assert!(matches!(owned.value, Cow::Owned(_)));
        assert_eq!(&*owned.value, "!!str");
    }
}

/// The YAML version in force for a single document.
///
/// Document-scoped by definition: a `%YAML` directive applies to the document it
/// introduces and to nothing after it, so this resets at every document boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum YamlVersion {
    /// YAML 1.1 semantics — notably the extended boolean spellings
    /// (`yes`/`no`/`on`/`off`/`y`/`n`).
    V1_1,
    /// YAML 1.2 semantics. The default when no directive is present.
    #[default]
    V1_2,
}

impl YamlVersion {
    /// Selects semantics from the `major.minor` of a `%YAML` directive.
    ///
    /// `1.1` selects 1.1 semantics. Every other `1.x` selects 1.2, because YAML
    /// 1.2.2 says a 1.x document should be processed by the most recent 1.x
    /// processor available — so an unknown future minor is read as 1.2 rather
    /// than rejected.
    #[must_use]
    pub const fn from_directive(major: u8, minor: u8) -> Self {
        if major == 1 && minor == 1 {
            Self::V1_1
        } else {
            Self::V1_2
        }
    }

    /// Whether YAML 1.1 scalar resolution applies.
    #[must_use]
    pub const fn is_1_1(self) -> bool {
        matches!(self, Self::V1_1)
    }
}

#[cfg(test)]
mod yaml_version_tests {
    use super::YamlVersion;

    #[test]
    fn only_1_1_selects_1_1_semantics() {
        assert_eq!(YamlVersion::from_directive(1, 1), YamlVersion::V1_1);
    }

    #[test]
    fn every_other_1_x_selects_1_2() {
        // 1.2.2: a 1.x document is processed by the most recent 1.x processor.
        for minor in [0u8, 2, 3, 9, 255] {
            assert_eq!(
                YamlVersion::from_directive(1, minor),
                YamlVersion::V1_2,
                "1.{minor} should select 1.2"
            );
        }
    }

    #[test]
    fn defaults_to_1_2() {
        assert_eq!(YamlVersion::default(), YamlVersion::V1_2);
        assert!(!YamlVersion::default().is_1_1());
    }
}
