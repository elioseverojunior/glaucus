// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Safe YAML formatting: trailing-whitespace trim plus a single final newline.

/// Formats `src`, preserving comments, indentation and scalar content.
///
/// # Errors
///
/// Returns the parse error if `src` is not valid YAML — a formatter must not
/// touch unparseable input. The error carries a span, so callers can point at
/// the offending line and column.
pub fn format_str(src: &str) -> crate::error::Result<String> {
    crate::from_str_node(src)?;
    Ok(crate::cst::Document::parse(src).reformatted())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_trailing_whitespace() {
        assert_eq!(format_str("a: 1   \nb: 2\n").unwrap(), "a: 1\nb: 2\n");
    }

    #[test]
    fn rejects_invalid_yaml() {
        assert!(format_str("a: [1, 2").is_err());
    }

    #[test]
    fn idempotent() {
        let once = format_str("x: y  \n").unwrap();
        assert_eq!(format_str(&once).unwrap(), once);
    }

    #[test]
    fn preserves_comments() {
        assert_eq!(format_str("a: 1  # c\n").unwrap(), "a: 1  # c\n");
    }
}
