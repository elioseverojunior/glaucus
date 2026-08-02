// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal YAML MCP server core: transport-agnostic dispatch + newline framing.
//!
//! Implements the [Model Context Protocol](https://modelcontextprotocol.io/)
//! JSON-RPC 2.0 over newline-delimited transport (one JSON object per line).
//!
//! Moved here from the now-retired `glaucus-mcp` satellite crate (Task 13
//! deleted that crate's directory entirely) so the stdio drive loop (see
//! [`serve`]) is testable in-process instead of only reachable by spawning a
//! separate binary.
//!
//! The `tools/list` registry and `tools/call` implementations live in the
//! (private) `tools` submodule — re-exported here as [`tools_list`] — keeping
//! this file to JSON-RPC framing, dispatch, and the drive loop.

use serde_json::{Value, json};
use std::io::{BufRead, Write};

mod tools;

pub use tools::tools_list;

// ─── Protocol types ───────────────────────────────────────────────────────────

/// An outgoing MCP message produced by handling an incoming request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outgoing {
    /// A successful JSON-RPC 2.0 response.
    Response {
        /// The request id this responds to.
        id: Value,
        /// The result payload.
        result: Value,
    },
    /// A JSON-RPC 2.0 error response.
    Error {
        /// The request id this error responds to.
        id: Value,
        /// The error code (JSON-RPC standard: -32600 invalid, -32601 method not found, etc.).
        code: i64,
        /// Human-readable error message.
        message: String,
    },
}

// ─── Server ──────────────────────────────────────────────────────────────────

/// MCP server. Each request is handled purely; the server holds no
/// cross-request state in this subset of the protocol.
#[derive(Default)]
pub struct Server;

impl Server {
    /// Creates a new MCP server.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Handles one incoming JSON-RPC request and returns the outgoing messages.
    ///
    /// `id` is `Some` for requests (which expect a response), `None` for
    /// notifications (which do not).
    pub fn handle(&mut self, method: &str, id: Option<Value>, params: &Value) -> Vec<Outgoing> {
        match method {
            "initialize" => vec![response(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "glaucus-mcp", "version": env!("CARGO_PKG_VERSION") }
                }),
            )],
            "notifications/initialized" => vec![],
            "ping" => vec![response(id, json!({}))],
            "tools/list" => vec![response(id, json!({ "tools": tools_list() }))],
            "tools/call" => vec![response(id, Self::tools_call(params))],
            // Unknown request -> method-not-found; unknown notification -> nothing.
            _ => id.map_or_else(Vec::new, |i| {
                vec![error_response(
                    Some(i),
                    -32601,
                    format!("method not found: {method}"),
                )]
            }),
        }
    }

    /// Dispatches a `tools/call` request. Returns the MCP content-envelope `Value`.
    fn tools_call(params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = &params["arguments"];
        tools::call(name, args)
    }
}

// ─── Framing ─────────────────────────────────────────────────────────────────

/// Frames a JSON body as a single newline-terminated line for MCP transport.
///
/// MCP uses newline-delimited JSON-RPC (one JSON object per `\n`) rather than
/// the `Content-Length` header framing used by LSP.
#[must_use]
pub fn frame(body: &str) -> String {
    format!("{body}\n")
}

// ─── Envelope helpers ─────────────────────────────────────────────────────────

/// Builds a successful JSON-RPC [`Outgoing::Response`] for the given `id` and `result`.
///
/// This is the transport-layer success builder. The tool-result *content*
/// envelope (`{ "content": [...], "isError": .. }`) is built separately by the
/// `content_ok` / `content_error` helpers and carried as this response's `result`.
#[must_use]
pub fn response(id: Option<Value>, result: Value) -> Outgoing {
    Outgoing::Response {
        id: id.unwrap_or(Value::Null),
        result,
    }
}

/// Builds a JSON-RPC [`Outgoing::Error`] for the given `id`, error `code`, and `message`.
#[must_use]
pub fn error_response(id: Option<Value>, code: i64, message: impl Into<String>) -> Outgoing {
    Outgoing::Error {
        id: id.unwrap_or(Value::Null),
        code,
        message: message.into(),
    }
}

// ─── Drive loop ───────────────────────────────────────────────────────────────

/// Whether the drive loop in [`serve`] should keep reading or stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// Keep reading the next line.
    Continue,
    /// Stop the loop; no more lines will be read.
    Exit,
}

/// Runs the MCP server loop against `input`/`output` until stdin closes
/// (EOF) or the client sends an `exit` message.
///
/// This is the framing + dispatch loop moved out of the now-retired
/// `glaucus-mcp` binary's `main`, taking injected streams instead of locking
/// process stdio so it can be driven by tests.
///
/// Reads are line-delimited (one JSON object per line via [`BufRead::read_line`]);
/// writes are newline-[`frame`]d. Unlike [`crate::lsp::serve`], an `exit`
/// message is still dispatched to [`Server::handle`] (so a request-style
/// `exit` with an `id` gets its `method not found` reply) before the loop
/// stops — that asymmetry matches the retired `glaucus-mcp` binary exactly.
pub fn serve(input: &mut dyn BufRead, output: &mut dyn Write) {
    let mut server = Server::new();
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if handle_line(&mut server, trimmed, output) == Flow::Exit {
            break;
        }
    }
}

/// Parses one line as a JSON-RPC message, dispatches it to `server`, writes
/// the resulting outgoing message(s), and reports whether [`serve`] should
/// stop reading further lines.
///
/// A line that fails to parse as JSON emits a `-32700` parse-error object
/// (with `id: null`) and the loop continues.
fn handle_line(server: &mut Server, line: &str, output: &mut dyn Write) -> Flow {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            write_parse_error(output, &e);
            return Flow::Continue;
        }
    };
    let method = msg["method"].as_str().unwrap_or("").to_string();
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    for outgoing in server.handle(&method, id, &params) {
        write_outgoing(output, outgoing);
    }
    let _ = output.flush();

    if method == "exit" {
        Flow::Exit
    } else {
        Flow::Continue
    }
}

/// Writes a JSON-RPC `-32700` parse-error object for a line that failed to
/// deserialize, matching the format the retired `glaucus-mcp` binary emitted.
///
/// Built from the same [`error_response`] + [`write_outgoing`] path every
/// other error reply goes through, rather than assembling the envelope by
/// hand a second time.
fn write_parse_error(output: &mut dyn Write, error: &serde_json::Error) {
    let outgoing = error_response(None, -32700, format!("parse error: {error}"));
    write_outgoing(output, outgoing);
    let _ = output.flush();
}

/// Frames and writes one outgoing message as a JSON-RPC 2.0 object.
fn write_outgoing(output: &mut dyn Write, outgoing: Outgoing) {
    let obj = match outgoing {
        Outgoing::Response { id, result } => {
            json!({ "jsonrpc": "2.0", "id": id, "result": result })
        }
        Outgoing::Error { id, code, message } => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    };
    let _ = output.write_all(frame(&obj.to_string()).as_bytes());
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frame_appends_newline() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let framed = frame(body);
        assert_eq!(framed, format!("{body}\n"));
        assert!(framed.ends_with('\n'));
    }

    #[test]
    fn initialize_returns_protocol_version_and_capabilities() {
        let mut s = Server::new();
        let out = s.handle("initialize", Some(json!(1)), &json!({}));
        let result = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Response { id, result } if *id == json!(1) => Some(result),
                _ => None,
            })
            .expect("initialize response");
        assert_eq!(result["protocolVersion"], json!("2024-11-05"));
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["serverInfo"]["name"], json!("glaucus-mcp"));
    }

    #[test]
    fn initialized_notification_is_silent() {
        let mut s = Server::new();
        assert!(
            s.handle("notifications/initialized", None, &json!(null))
                .is_empty()
        );
    }

    #[test]
    fn ping_returns_empty_object() {
        let mut s = Server::new();
        let out = s.handle("ping", Some(json!(2)), &json!(null));
        assert!(out.iter().any(
            |o| matches!(o, Outgoing::Response { id, result } if *id == json!(2) && result == &json!({}))
        ));
    }

    #[test]
    fn tools_list_response_contains_four_tools() {
        let mut s = Server::new();
        let out = s.handle("tools/list", Some(json!(3)), &json!({}));
        let result = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Response { id, result } if *id == json!(3) => Some(result),
                _ => None,
            })
            .expect("tools/list response");
        let tools = result["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 4);
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"yaml_parse"));
        assert!(names.contains(&"yaml_validate"));
        assert!(names.contains(&"yaml_format"));
        assert!(names.contains(&"yaml_edit"));
    }

    #[test]
    fn unknown_method_returns_error() {
        let mut s = Server::new();
        let out = s.handle("nonexistent/method", Some(json!(4)), &json!({}));
        assert!(out.iter().any(
            |o| matches!(o, Outgoing::Error { id, code, .. } if *id == json!(4) && *code == -32601)
        ));
    }

    #[test]
    fn unknown_notification_is_silent() {
        let mut s = Server::new();
        let out = s.handle("$/something", None, &json!({}));
        assert!(out.is_empty());
    }

    // ─── `tools/call` dispatch (per-tool behaviour lives in `tools::tests`) ──

    #[test]
    fn tools_call_dispatches_to_the_named_tool() {
        let mut s = Server::new();
        let out = s.handle(
            "tools/call",
            Some(json!(7)),
            &json!({ "name": "yaml_parse", "arguments": { "text": "a: 1\n" } }),
        );
        let result = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Response { id, result } if *id == json!(7) => Some(result),
                _ => None,
            })
            .expect("tools/call response");
        assert_eq!(result["isError"], json!(false));
    }

    // ─── `serve` and its drive-loop helpers ──────────────────────────────

    #[test]
    fn serve_returns_immediately_on_empty_input() {
        let mut input: &[u8] = b"";
        let mut output: Vec<u8> = Vec::new();
        serve(&mut input, &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn serve_skips_blank_lines_then_exits_on_eof() {
        let mut input: &[u8] = b"\n\n";
        let mut output: Vec<u8> = Vec::new();
        serve(&mut input, &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn serve_emits_parse_error_for_malformed_json_then_continues() {
        let mut input: &[u8] = b"not json\n";
        let mut output: Vec<u8> = Vec::new();
        serve(&mut input, &mut output);
        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("-32700"), "no parse error:\n{rendered}");
        assert!(rendered.contains("parse error"));
        assert!(rendered.ends_with('\n'), "not newline-framed:\n{rendered}");
    }

    #[test]
    fn serve_writes_a_framed_response_for_a_request() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let msg = format!("{body}\n");
        let mut input: &[u8] = msg.as_bytes();
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("\"protocolVersion\""));
        assert!(rendered.ends_with('\n'));
    }

    #[test]
    fn serve_writes_a_framed_error_for_an_unknown_method() {
        let body = r#"{"jsonrpc":"2.0","id":5,"method":"nonexistent/method","params":{}}"#;
        let msg = format!("{body}\n");
        let mut input: &[u8] = msg.as_bytes();
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("-32601"));
        assert!(rendered.contains("method not found"));
    }

    #[test]
    fn serve_exits_on_exit_notification_without_a_response() {
        let mut input: &[u8] = b"{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}\n";
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        assert!(output.is_empty(), "a bare exit notification is silent");
    }

    #[test]
    fn serve_writes_response_before_exiting_when_exit_request_has_an_id() {
        // "exit" is not a recognised MCP method, so a request-style exit
        // (carrying an id) still gets a method-not-found reply — dispatched
        // through `Server::handle` — before the loop stops.
        let mut input: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"exit\"}\n";
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("-32601"));
    }

    #[test]
    fn serve_stops_reading_further_lines_after_exit() {
        // A second line after "exit" must never be reached.
        let mut input: &[u8] =
            b"{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        assert!(output.is_empty(), "the line after exit must not run");
    }
}
