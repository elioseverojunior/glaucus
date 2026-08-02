// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal YAML Language Server core: transport-agnostic dispatch + framing.
//!
//! Moved here from the now-retired `glaucus-lsp` satellite crate (Task 13
//! deleted that crate's directory entirely) so the stdio drive loop (see
//! [`serve`]) is testable in-process instead of only reachable by spawning a
//! separate binary.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, Write};

/// An outgoing LSP message produced by handling an incoming one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outgoing {
    /// A response to a request, carrying the request `id` and a `result`.
    Response {
        /// The request id this responds to.
        id: Value,
        /// The result payload.
        result: Value,
    },
    /// A server-initiated notification (method + params).
    Notification {
        /// The notification method name.
        method: String,
        /// The notification params.
        params: Value,
    },
}

/// LSP server state: open documents keyed by URI.
#[derive(Default)]
pub struct Server {
    /// Open documents (uri -> full text).
    docs: HashMap<String, String>,
}

impl Server {
    /// Creates an empty server.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles one incoming message. `id` is `Some` for requests, `None` for
    /// notifications. Returns the outgoing messages to emit (responses + notifications).
    pub fn handle(&mut self, method: &str, id: Option<Value>, params: &Value) -> Vec<Outgoing> {
        match method {
            "initialize" => vec![Outgoing::Response {
                id: id.unwrap_or(Value::Null),
                result: json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "documentFormattingProvider": true
                    }
                }),
            }],
            "initialized" | "exit" => vec![],
            "shutdown" => vec![Outgoing::Response {
                id: id.unwrap_or(Value::Null),
                result: Value::Null,
            }],
            "textDocument/didOpen" => self.did_open(params),
            "textDocument/didChange" => self.did_change(params),
            "textDocument/formatting" => vec![Outgoing::Response {
                id: id.unwrap_or(Value::Null),
                result: self.format(params),
            }],
            // Unknown request -> null result; unknown notification -> nothing.
            _ => id.map_or_else(Vec::new, |i| {
                vec![Outgoing::Response {
                    id: i,
                    result: Value::Null,
                }]
            }),
        }
    }

    fn did_open(&mut self, params: &Value) -> Vec<Outgoing> {
        let uri = params["textDocument"]["uri"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let text = params["textDocument"]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        self.docs.insert(uri.clone(), text);
        self.publish_diagnostics(&uri)
    }

    fn did_change(&mut self, params: &Value) -> Vec<Outgoing> {
        let uri = params["textDocument"]["uri"]
            .as_str()
            .unwrap_or("")
            .to_string();
        // Full sync: take the last contentChange's full text.
        if let Some(text) = params["contentChanges"]
            .as_array()
            .and_then(|c| c.last())
            .and_then(|c| c["text"].as_str())
            .map(str::to_string)
        {
            self.docs.insert(uri.clone(), text);
        }
        self.publish_diagnostics(&uri)
    }

    fn publish_diagnostics(&self, uri: &str) -> Vec<Outgoing> {
        let text = self.docs.get(uri).map_or("", String::as_str);
        let diagnostics: Vec<Value> = match crate::from_str_node(text) {
            Ok(_) => Vec::new(),
            Err(e) => {
                // glaucus Position: line is 1-based, column is 1-based.
                // LSP positions are both 0-based.
                let (line, ch) = e.span.map_or((0u64, 0u64), |s| {
                    (
                        u64::from(s.start.line.saturating_sub(1)),
                        u64::from(s.start.column.saturating_sub(1)),
                    )
                });
                vec![json!({
                    "range": {
                        "start": { "line": line, "character": ch },
                        "end":   { "line": line, "character": ch + 1 }
                    },
                    "severity": 1,
                    "source": "glaucus",
                    "message": e.to_string(),
                })]
            }
        };
        vec![Outgoing::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: json!({ "uri": uri, "diagnostics": diagnostics }),
        }]
    }

    fn format(&self, params: &Value) -> Value {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        let Some(text) = self.docs.get(uri) else {
            return json!([]);
        };
        let formatted = crate::cst::Document::parse(text).reformatted();
        let end_line = text.matches('\n').count() as u64 + 1;
        json!([{
            "range": {
                "start": { "line": 0, "character": 0 },
                "end":   { "line": end_line, "character": 0 }
            },
            "newText": formatted,
        }])
    }
}

/// Frames a JSON body with the LSP `Content-Length` header.
#[must_use]
pub fn frame(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

/// Runs the language server loop against `input`/`output` until stdin closes
/// (EOF) or the client sends an `exit` notification.
///
/// This is the framing + dispatch loop moved out of the now-retired
/// `glaucus-lsp` binary's `main`, taking injected streams instead of locking
/// process stdio so it can be driven by tests.
pub fn serve(input: &mut dyn BufRead, output: &mut dyn Write) {
    let mut server = Server::new();
    while let Some(body) = read_message(input) {
        let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if method == "exit" {
            return;
        }
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        for outgoing in server.handle(&method, id, &params) {
            write_outgoing(output, outgoing);
        }
    }
}

/// Reads one `Content-Length`-framed message body from `input`.
///
/// A header with a zero or missing `Content-Length` carries no body; such
/// headers are skipped and the next one is read instead. Returns `None` on
/// EOF, whether hit mid-header or mid-body — the caller should stop serving.
fn read_message(input: &mut dyn BufRead) -> Option<Vec<u8>> {
    loop {
        let content_length = read_content_length(input)?;
        if content_length == 0 {
            continue;
        }
        let mut body = vec![0u8; content_length];
        input.read_exact(&mut body).ok()?;
        return Some(body);
    }
}

/// Reads header lines up to the blank line that ends them, returning the
/// parsed `Content-Length` value (`0` if absent or not a valid integer).
fn read_content_length(input: &mut dyn BufRead) -> Option<usize> {
    let mut header = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        input.read_exact(&mut byte).ok()?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header_text = String::from_utf8_lossy(&header);
    let content_length = header_text
        .split("\r\n")
        .find_map(|line| line.strip_prefix("Content-Length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    Some(content_length)
}

/// Frames and writes one outgoing message, flushing so the client sees it
/// immediately. Write failures are not fatal to the loop: a client that has
/// gone away is caught on the next read instead.
fn write_outgoing(output: &mut dyn Write, outgoing: Outgoing) {
    let obj = match outgoing {
        Outgoing::Response { id, result } => {
            json!({ "jsonrpc": "2.0", "id": id, "result": result })
        }
        Outgoing::Notification { method, params } => {
            json!({ "jsonrpc": "2.0", "method": method, "params": params })
        }
    };
    let framed = frame(&obj.to_string());
    let _ = output.write_all(framed.as_bytes());
    let _ = output.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_roundtrip() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"x"}"#;
        let framed = frame(body);
        assert!(framed.starts_with("Content-Length: "));
        assert!(framed.ends_with(body));
    }

    #[test]
    fn initialize_advertises_capabilities() {
        let mut s = Server::new();
        let out = s.handle("initialize", Some(json!(1)), &json!({}));
        let resp = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Response { id, result } if *id == json!(1) => Some(result),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            resp["capabilities"]["documentFormattingProvider"],
            json!(true)
        );
        assert_eq!(resp["capabilities"]["textDocumentSync"], json!(1));
    }

    #[test]
    fn initialized_notification_is_silent() {
        let mut s = Server::new();
        assert!(s.handle("initialized", None, &json!(null)).is_empty());
    }

    #[test]
    fn exit_via_handle_is_silent() {
        // `serve` intercepts "exit" before dispatch (see
        // `serve_exits_on_exit_notification_without_a_response` below), but
        // `Server::handle` is a public, transport-agnostic entry point in its
        // own right and must handle "exit" the same way if called directly.
        let mut s = Server::new();
        assert!(s.handle("exit", None, &json!(null)).is_empty());
    }

    #[test]
    fn shutdown_returns_null_result() {
        let mut s = Server::new();
        let out = s.handle("shutdown", Some(json!(2)), &json!(null));
        assert!(out.iter().any(
            |o| matches!(o, Outgoing::Response { id, result } if *id == json!(2) && result.is_null())
        ));
    }

    #[test]
    fn unknown_notification_is_silent() {
        let mut s = Server::new();
        assert!(s.handle("$/cancelRequest", None, &json!({})).is_empty());
    }

    #[test]
    fn unknown_request_returns_null_result() {
        // An unrecognized method WITH an id is a request: reply with a null
        // result echoing the id (the `Some(i)` arm of the catch-all).
        let mut s = Server::new();
        let out = s.handle("textDocument/hover", Some(json!(42)), &json!({}));
        assert!(out.iter().any(
            |o| matches!(o, Outgoing::Response { id, result } if *id == json!(42) && result.is_null())
        ));
    }

    #[test]
    fn did_open_invalid_yaml_publishes_diagnostic() {
        let mut s = Server::new();
        let out = s.handle(
            "textDocument/didOpen",
            None,
            &json!({
                "textDocument": { "uri": "file:///t.yaml", "text": "a: [1, 2\n" }
            }),
        );
        let note = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Notification { method, params }
                    if method == "textDocument/publishDiagnostics" =>
                {
                    Some(params)
                }
                _ => None,
            })
            .expect("publishDiagnostics");
        assert_eq!(note["uri"], json!("file:///t.yaml"));
        let diags = note["diagnostics"].as_array().unwrap();
        assert_eq!(diags.len(), 1);
        assert!(diags[0]["range"]["start"]["line"].is_number());
        assert!(diags[0]["message"].is_string());
    }

    #[test]
    fn did_open_valid_yaml_publishes_empty_diagnostics() {
        let mut s = Server::new();
        let out = s.handle(
            "textDocument/didOpen",
            None,
            &json!({
                "textDocument": { "uri": "file:///ok.yaml", "text": "a: 1\n" }
            }),
        );
        let note = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Notification { params, .. } => Some(params),
                Outgoing::Response { .. } => None,
            })
            .unwrap();
        assert_eq!(note["diagnostics"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn did_change_updates_and_rediagnoses() {
        let mut s = Server::new();
        s.handle(
            "textDocument/didOpen",
            None,
            &json!({ "textDocument": { "uri": "file:///c.yaml", "text": "a: 1\n" } }),
        );
        let out = s.handle(
            "textDocument/didChange",
            None,
            &json!({
                "textDocument": { "uri": "file:///c.yaml" },
                "contentChanges": [ { "text": "a: [1, 2\n" } ]
            }),
        );
        let note = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Notification { params, .. } => Some(params),
                Outgoing::Response { .. } => None,
            })
            .unwrap();
        assert_eq!(note["diagnostics"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn formatting_returns_text_edit() {
        let mut s = Server::new();
        s.handle(
            "textDocument/didOpen",
            None,
            &json!({ "textDocument": { "uri": "file:///f.yaml", "text": "a: 1   \n" } }),
        );
        let out = s.handle(
            "textDocument/formatting",
            Some(json!(9)),
            &json!({ "textDocument": { "uri": "file:///f.yaml" } }),
        );
        let edits = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Response { id, result } if *id == json!(9) => Some(result),
                _ => None,
            })
            .unwrap();
        let arr = edits.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["newText"], json!("a: 1\n"));
    }

    #[test]
    fn formatting_unknown_doc_returns_empty() {
        let mut s = Server::new();
        let out = s.handle(
            "textDocument/formatting",
            Some(json!(1)),
            &json!({ "textDocument": { "uri": "file:///nope.yaml" } }),
        );
        let edits = out
            .iter()
            .find_map(|o| match o {
                Outgoing::Response { result, .. } => Some(result),
                Outgoing::Notification { .. } => None,
            })
            .unwrap();
        assert_eq!(edits.as_array().unwrap().len(), 0);
    }

    // ─── `serve` and its framing helpers ─────────────────────────────

    #[test]
    fn serve_returns_immediately_on_empty_input() {
        let mut input: &[u8] = b"";
        let mut output: Vec<u8> = Vec::new();
        serve(&mut input, &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn serve_stops_on_eof_mid_header() {
        // No terminating "\r\n\r\n": EOF hits inside `read_content_length`.
        let mut input: &[u8] = b"Content-Leng";
        let mut output: Vec<u8> = Vec::new();
        serve(&mut input, &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn serve_treats_header_without_content_length_as_empty_and_continues() {
        let exit_notification = frame(r#"{"jsonrpc":"2.0","method":"exit"}"#);
        let mut data = b"\r\n\r\n".to_vec();
        data.extend_from_slice(exit_notification.as_bytes());
        let mut input: &[u8] = &data;
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        assert!(output.is_empty(), "exit must not produce a response");
    }

    #[test]
    fn serve_treats_malformed_content_length_as_zero_and_continues() {
        let exit_notification = frame(r#"{"jsonrpc":"2.0","method":"exit"}"#);
        let mut data = b"Content-Length: not-a-number\r\n\r\n".to_vec();
        data.extend_from_slice(exit_notification.as_bytes());
        let mut input: &[u8] = &data;
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        assert!(output.is_empty());
    }

    #[test]
    fn serve_stops_when_body_shorter_than_declared() {
        let data = b"Content-Length: 50\r\n\r\nshort".to_vec();
        let mut input: &[u8] = &data;
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        assert!(output.is_empty());
    }

    #[test]
    fn serve_skips_invalid_json_body_then_exits() {
        let bad = frame("not json");
        let exit_notification = frame(r#"{"jsonrpc":"2.0","method":"exit"}"#);
        let mut data = bad.into_bytes();
        data.extend_from_slice(exit_notification.as_bytes());
        let mut input: &[u8] = &data;
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        assert!(output.is_empty(), "an unparseable body must not be echoed");
    }

    #[test]
    fn serve_exits_on_exit_notification_without_a_response() {
        let msg = frame(r#"{"jsonrpc":"2.0","method":"exit"}"#);
        let mut input: &[u8] = msg.as_bytes();
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        assert!(output.is_empty());
    }

    #[test]
    fn serve_writes_a_framed_response_for_a_request() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let msg = frame(body);
        let mut input: &[u8] = msg.as_bytes();
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.starts_with("Content-Length:"));
        assert!(rendered.contains("\"capabilities\""));
    }

    #[test]
    fn serve_writes_a_framed_notification_for_did_open() {
        let body = r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":
            {"textDocument":{"uri":"file:///t.yaml","text":"a: 1\n"}}}"#;
        let msg = frame(body);
        let mut input: &[u8] = msg.as_bytes();
        let mut output: Vec<u8> = Vec::new();

        serve(&mut input, &mut output);

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("textDocument/publishDiagnostics"));
    }
}
