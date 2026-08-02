// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `glaucus lsp` — language server over stdio.

use crate::cli::env::Env;
use crate::cli::exit;

/// Runs the server until stdin closes.
#[must_use]
pub(crate) fn run(env: &mut Env<'_>) -> u8 {
    crate::lsp::serve(env.stdin, env.stdout);
    exit::OK
}

#[cfg(test)]
mod tests {
    use crate::cli::exit;
    use crate::cli::tests::drive;

    #[test]
    fn empty_stdin_exits_cleanly() {
        let (code, _out, _err) = drive(&["glaucus", "lsp"], "");
        assert_eq!(code, exit::OK);
    }

    #[test]
    fn a_single_request_is_answered_on_stdout() {
        // Content-Length framing, initialize request.
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let msg = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        let (code, out, _err) = drive(&["glaucus", "lsp"], &msg);
        assert_eq!(code, exit::OK);
        assert!(out.contains("Content-Length:"), "no framed reply:\n{out}");
    }
}
