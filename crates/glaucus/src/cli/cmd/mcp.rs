// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus mcp` — MCP server over stdio.

use crate::cli::env::Env;
use crate::cli::exit;

/// Runs the server until stdin closes or an `exit` message arrives.
#[must_use]
pub(crate) fn run(env: &mut Env<'_>) -> u8 {
    crate::mcp::serve(env.stdin, env.stdout);
    exit::OK
}

#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn empty_stdin_exits_cleanly() {
        let (code, _out, _err) = drive(&["glaucus", "mcp"], "");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn blank_lines_are_skipped() {
        let (code, out, _err) = drive(&["glaucus", "mcp"], "\n\n");
        assert_eq!(code, exit::OK);
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_json_produces_an_error_object() {
        let (code, out, _err) = drive(&["glaucus", "mcp"], "not json\n");
        assert_eq!(code, exit::OK);
        assert!(out.contains("error"), "no error object:\n{out}");
    }
}
