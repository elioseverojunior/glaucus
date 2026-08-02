// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `tools/list` registry and `tools/call` dispatch table.
//!
//! Split out of the parent `mcp` module (which owns JSON-RPC framing, the
//! `Server`, and the drive loop) so each file stays focused on one concern:
//! protocol dispatch versus the tools themselves.

use serde_json::{Value, json};

/// Returns the list of tools advertised by this server.
///
/// Each entry is a JSON object with `name`, `description`, and `inputSchema`
/// following the MCP `Tool` schema.
#[must_use]
pub fn tools_list() -> Vec<Value> {
    vec![
        json!({
            "name": "yaml_parse",
            "description": "Parse YAML and report whether it is valid; on failure returns the error with line/column.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "YAML source to parse." }
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "yaml_validate",
            "description": "Validate YAML against a JSON-Schema (YAML/JSON source); returns span-anchored diagnostics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text":   { "type": "string", "description": "YAML source to validate." },
                    "schema": { "type": "string", "description": "JSON Schema (as YAML or JSON string)." }
                },
                "required": ["text", "schema"]
            }
        }),
        json!({
            "name": "yaml_format",
            "description": "Format YAML safely (trailing-whitespace trim + final newline), preserving comments.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "YAML source to format." }
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "yaml_edit",
            "description": "Set a value at a dotted path (comment-preserving); inserts a top-level key if the path is absent.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text":  { "type": "string", "description": "YAML source to edit." },
                    "path":  { "type": "string", "description": "Dotted key path, e.g. `settings.debug`." },
                    "value": { "type": "string", "description": "New scalar value to write." }
                },
                "required": ["text", "path", "value"]
            }
        }),
    ]
}

/// Dispatches a `tools/call` request by tool name. Returns the MCP
/// content-envelope `Value`.
pub(super) fn call(name: &str, args: &Value) -> Value {
    match name {
        "yaml_parse" => tool_parse(args),
        "yaml_validate" => tool_validate(args),
        "yaml_format" => tool_format(args),
        "yaml_edit" => tool_edit(args),
        other => content_error(format!("unknown tool: {other}")),
    }
}

/// Wraps a successful tool text payload in the MCP content envelope.
fn content_ok(text: impl Into<String>) -> Value {
    json!({ "content": [ { "type": "text", "text": text.into() } ], "isError": false })
}

/// Wraps a tool failure in the MCP content envelope with `isError: true`.
fn content_error(text: impl Into<String>) -> Value {
    json!({ "content": [ { "type": "text", "text": text.into() } ], "isError": true })
}

fn tool_parse(args: &Value) -> Value {
    let text = args["text"].as_str().unwrap_or("");
    match crate::from_str_node(text) {
        Ok(_) => content_ok("valid"),
        Err(e) => {
            let loc = e.span.map_or_else(String::new, |s| {
                format!("{}:{} ", s.start.line, s.start.column)
            });
            content_error(format!("{loc}{e}"))
        }
    }
}

fn tool_validate(args: &Value) -> Value {
    let text = args["text"].as_str().unwrap_or("");
    let schema_src = args["schema"].as_str().unwrap_or("");
    let schema_node = match crate::from_str_node(schema_src) {
        Ok(n) => n,
        Err(e) => return content_error(format!("schema parse error: {e}")),
    };
    let sc = crate::schema::Schema::from_node(&schema_node);
    let data_node = match crate::from_str_node(text) {
        Ok(n) => n,
        Err(e) => return content_error(format!("parse error: {e}")),
    };
    let diags: Vec<Value> = match crate::schema::validate(&data_node, &sc) {
        Ok(()) => Vec::new(),
        Err(errs) => errs
            .into_iter()
            .map(|e| {
                json!({
                    "path": e.path,
                    "line": e.span.start.line,
                    "column": e.span.start.column,
                    "message": e.message,
                })
            })
            .collect(),
    };
    // Compact JSON array as the text payload (machine-readable for the agent).
    content_ok(Value::Array(diags).to_string())
}

fn tool_format(args: &Value) -> Value {
    let text = args["text"].as_str().unwrap_or("");
    match crate::from_str_node(text) {
        Ok(_) => content_ok(crate::cst::Document::parse(text).reformatted()),
        Err(e) => content_error(format!("parse error: {e}")),
    }
}

fn tool_edit(args: &Value) -> Value {
    let text = args["text"].as_str().unwrap_or("");
    let path = args["path"].as_str().unwrap_or("");
    let value = args["value"].as_str().unwrap_or("");
    let mut doc = crate::cst::Document::parse(text);
    match doc.set(path, value) {
        Ok(()) => content_ok(doc.to_string()),
        Err(crate::cst::SetError::PathNotFound) if !path.contains('.') => {
            match doc.insert(path, value) {
                Ok(()) => content_ok(doc.to_string()),
                Err(e) => content_error(e.to_string()),
            }
        }
        Err(e) => content_error(e.to_string()),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_contains_four_tools() {
        let tools = tools_list();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names.len(), 4);
        assert!(names.contains(&"yaml_parse"));
        assert!(names.contains(&"yaml_validate"));
        assert!(names.contains(&"yaml_format"));
        assert!(names.contains(&"yaml_edit"));
    }

    #[test]
    fn parse_valid_is_ok() {
        let r = call("yaml_parse", &json!({ "text": "a: 1\n" }));
        assert_eq!(r["isError"], json!(false));
    }

    #[test]
    fn parse_invalid_is_error_with_location() {
        let r = call("yaml_parse", &json!({ "text": "a: [1, 2\n" }));
        assert_eq!(r["isError"], json!(true));
        assert!(r["content"][0]["text"].as_str().unwrap().contains(':'));
    }

    #[test]
    fn parse_invalid_without_a_span_omits_the_location_prefix() {
        // Empty input is `UnexpectedEof`, built via `Error::spanless` (see
        // `glaucus::from_str_node`) — the one parse-error path with no span,
        // exercising the `e.span.map_or_else` fallback (no "line:col " prefix)
        // that `parse_invalid_is_error_with_location` does not reach.
        let r = call("yaml_parse", &json!({ "text": "" }));
        assert_eq!(r["isError"], json!(true));
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(!text.contains(':'), "unexpected location prefix:\n{text}");
    }

    #[test]
    fn validate_reports_type_error() {
        let r = call(
            "yaml_validate",
            &json!({
                "text": "age: notnum\n",
                "schema": "type: object\nproperties:\n  age: {type: integer}\n"
            }),
        );
        let text = r["content"][0]["text"].as_str().unwrap();
        let diags: Value = serde_json::from_str(text).unwrap();
        assert_eq!(diags.as_array().unwrap().len(), 1);
        assert!(diags[0]["message"].is_string());
        assert!(diags[0]["line"].is_number());
    }

    #[test]
    fn validate_ok_is_empty_array() {
        let r = call(
            "yaml_validate",
            &json!({
                "text": "name: glaucus\n",
                "schema": "type: object\nrequired: [name]\nproperties:\n  name: {type: string}\n"
            }),
        );
        let diags: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(diags.as_array().unwrap().len(), 0);
        assert_eq!(r["isError"], json!(false));
    }

    #[test]
    fn format_trims_trailing_ws_and_preserves_comment() {
        let r = call("yaml_format", &json!({ "text": "a: 1   # c\n" }));
        assert_eq!(r["isError"], json!(false));
        // The scanner bakes trailing spaces before a comment into the token;
        // reformatted() preserves inline comments verbatim, so the exact input
        // is unchanged (no trailing ws after the comment, comment preserved).
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("# c"));
        assert!(!text.ends_with("  \n"));
    }

    #[test]
    fn format_invalid_is_error() {
        let r = call("yaml_format", &json!({ "text": "a: [1, 2" }));
        assert_eq!(r["isError"], json!(true));
    }

    #[test]
    fn edit_sets_existing_value_preserving_comment() {
        let r = call(
            "yaml_edit",
            &json!({ "text": "a: 1  # keep\nb: 2\n", "path": "a", "value": "9" }),
        );
        assert_eq!(r["isError"], json!(false));
        assert_eq!(
            r["content"][0]["text"].as_str().unwrap(),
            "a: 9  # keep\nb: 2\n"
        );
    }

    #[test]
    fn edit_inserts_absent_toplevel_key() {
        let r = call(
            "yaml_edit",
            &json!({ "text": "a: 1\n", "path": "b", "value": "2" }),
        );
        assert_eq!(r["isError"], json!(false));
        assert!(r["content"][0]["text"].as_str().unwrap().contains("b: 2"));
    }

    #[test]
    fn unknown_tool_is_error() {
        let r = call("nope", &json!({}));
        assert_eq!(r["isError"], json!(true));
    }

    #[test]
    fn validate_invalid_schema_is_error() {
        // A malformed schema document hits the schema parse-error arm.
        let r = call(
            "yaml_validate",
            &json!({ "text": "a: 1\n", "schema": "schema: [1, 2\n" }),
        );
        assert_eq!(r["isError"], json!(true));
        assert!(
            r["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("schema parse error")
        );
    }

    #[test]
    fn validate_invalid_data_is_error() {
        // A valid schema but malformed data hits the data parse-error arm.
        let r = call(
            "yaml_validate",
            &json!({ "text": "a: [1, 2\n", "schema": "type: object\n" }),
        );
        assert_eq!(r["isError"], json!(true));
        assert!(
            r["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("parse error")
        );
    }

    #[test]
    fn edit_insert_into_non_mapping_root_is_error() {
        // set → PathNotFound (no dot) → insert → NotAMapping (scalar root) → the
        // inner insert Err arm.
        let r = call(
            "yaml_edit",
            &json!({ "text": "just a scalar\n", "path": "newkey", "value": "v" }),
        );
        assert_eq!(r["isError"], json!(true));
    }

    #[test]
    fn edit_unresolved_dotted_path_is_error() {
        // A dotted path that does not resolve returns PathNotFound WITH a dot,
        // which falls through to the outer set Err arm (no insert attempt).
        let r = call(
            "yaml_edit",
            &json!({ "text": "a: 1\n", "path": "a.b.c", "value": "v" }),
        );
        assert_eq!(r["isError"], json!(true));
    }
}
