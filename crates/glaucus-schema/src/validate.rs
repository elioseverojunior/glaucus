// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::schema::{JsonType, Schema};
use glaucus_ast::node::Node;
use glaucus_core::types::{ScalarStyle, Span};

/// A single validation failure, anchored to the offending node's source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    /// JSON-pointer-style path to the offending node (e.g. `/items/0/age`).
    pub path: String,
    /// Source span of the offending node.
    pub span: Span,
    /// Human-readable message.
    pub message: String,
}

/// Validates `data` against `schema`. Returns all violations (`Ok` if none).
///
/// # Errors
///
/// Returns every [`SchemaError`] found, so the caller can report them
/// together. Validation itself never fails — a non-conforming document is a
/// result, not an error condition.
pub fn validate(data: &Node<'_>, schema: &Schema) -> Result<(), Vec<SchemaError>> {
    let mut errors = Vec::new();
    validate_node(data, schema, String::new(), &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Classifies a plain scalar string into a [`JsonType`].
fn classify_scalar(value: &str) -> JsonType {
    match value {
        "null" | "Null" | "NULL" | "~" | "" => JsonType::Null,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => JsonType::Boolean,
        _ => {
            if value.parse::<i64>().is_ok() {
                JsonType::Integer
            } else if value.parse::<f64>().is_ok() {
                JsonType::Number
            } else {
                JsonType::String
            }
        }
    }
}

/// Returns the [`JsonType`] of a node.
///
/// Style decides this before the value's text does. Per the YAML 1.2 Core
/// Schema, only a *plain* scalar undergoes non-specific tag resolution to
/// bool/int/float/null — an explicitly quoted scalar (`'...'`/`"..."`) or a
/// block scalar (`|`/`>`) is unambiguously `string`, no matter what its text
/// looks like. Skipping this and always falling through to `classify_scalar`
/// is what let `port: "8080"` validate as an integer while
/// `coerce_to_schema` (which reads the raw CST text and does respect
/// quoting) rewrote that very same value under `--fix` — two codepaths
/// disagreeing about one document's type. Do not "simplify" this back to a
/// single `classify_scalar(&s.value)` call; that reintroduces exactly that
/// contradiction.
fn json_type_of(node: &Node<'_>) -> JsonType {
    match node {
        Node::Scalar(s) if s.style == ScalarStyle::Plain => classify_scalar(&s.value),
        Node::Scalar(_) => JsonType::String,
        Node::Sequence(_) => JsonType::Array,
        Node::Mapping(_) => JsonType::Object,
    }
}

/// Returns `true` if `actual` satisfies `expected`, with the special rule that
/// `Integer` satisfies `Number`.
fn type_matches(expected: JsonType, actual: JsonType) -> bool {
    expected == actual || (expected == JsonType::Number && actual == JsonType::Integer)
}

/// Core recursive validator — appends any violations to `errors`.
/// Checks the keywords that constrain a scalar's magnitude or length:
/// `minimum`/`maximum` and `minLength`/`maxLength`. Non-scalars and scalars
/// that do not parse as a number simply skip the checks that cannot apply.
fn validate_scalar_bounds(
    node: &Node<'_>,
    schema: &Schema,
    path: &str,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    // ── numeric bounds ────────────────────────────────────────────────────────
    if (schema.minimum.is_some() || schema.maximum.is_some())
        && let Some(text) = node.as_str()
        && let Ok(num) = text.parse::<f64>()
    {
        if let Some(min) = schema.minimum
            && num < min
        {
            errors.push(SchemaError {
                path: path.to_string(),
                span,
                message: format!("value {num} is less than minimum {min}"),
            });
        }
        if let Some(max) = schema.maximum
            && num > max
        {
            errors.push(SchemaError {
                path: path.to_string(),
                span,
                message: format!("value {num} exceeds maximum {max}"),
            });
        }
    }

    // ── string length ─────────────────────────────────────────────────────────
    if (schema.min_length.is_some() || schema.max_length.is_some())
        && let Some(text) = node.as_str()
    {
        let len = text.chars().count();
        if let Some(min) = schema.min_length
            && len < min
        {
            errors.push(SchemaError {
                path: path.to_string(),
                span,
                message: format!("string length {len} is less than minLength {min}"),
            });
        }
        if let Some(max) = schema.max_length
            && len > max
        {
            errors.push(SchemaError {
                path: path.to_string(),
                span,
                message: format!("string length {len} exceeds maxLength {max}"),
            });
        }
    }
}

/// Checks the object keywords: `minProperties`/`maxProperties`, `required`,
/// and `properties`/`additionalProperties`, recursing into each declared
/// property schema. A non-mapping node skips all of them.
fn validate_object(
    node: &Node<'_>,
    schema: &Schema,
    path: &str,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    // ── object keywords ───────────────────────────────────────────────────────
    if let Some(entries) = node.as_mapping() {
        // minProperties / maxProperties
        let prop_count = entries.len();
        if let Some(min) = schema.min_properties
            && prop_count < min
        {
            errors.push(SchemaError {
                path: path.to_string(),
                span,
                message: format!("object has {prop_count} properties, minimum is {min}"),
            });
        }
        if let Some(max) = schema.max_properties
            && prop_count > max
        {
            errors.push(SchemaError {
                path: path.to_string(),
                span,
                message: format!("object has {prop_count} properties, maximum is {max}"),
            });
        }

        // required
        for required_key in &schema.required {
            let present = entries
                .iter()
                .any(|(k, _)| k.as_str().is_some_and(|s| s == required_key));
            if !present {
                errors.push(SchemaError {
                    path: path.to_string(),
                    span,
                    message: format!("required property {required_key:?} is missing"),
                });
            }
        }

        // properties + additionalProperties
        for (key_node, value_node) in entries {
            let key_str = key_node.as_str().unwrap_or("");
            let child_path = format!("{path}/{key_str}");

            if let Some(prop_schema) = schema.properties.get(key_str) {
                validate_node(value_node, prop_schema, child_path, errors);
            } else if schema.additional_properties == Some(false) {
                errors.push(SchemaError {
                    path: child_path,
                    span: value_node.span(),
                    message: format!("additional property {key_str:?} is not allowed"),
                });
            }
        }
    }
}

fn validate_node(node: &Node<'_>, schema: &Schema, path: String, errors: &mut Vec<SchemaError>) {
    let span = node.span();
    let actual_type = json_type_of(node);

    // ── type ──────────────────────────────────────────────────────────────────
    if let Some(types) = &schema.types
        && !types.iter().any(|&t| type_matches(t, actual_type))
    {
        let type_names: Vec<&str> = types.iter().copied().map(json_type_name).collect();
        errors.push(SchemaError {
            path,
            span,
            message: format!(
                "expected type {} but got {}",
                type_names.join(" or "),
                json_type_name(actual_type)
            ),
        });
        // Return early: further keyword checks would be noisy / wrong type.
        return;
    }

    // ── const ─────────────────────────────────────────────────────────────────
    if let Some(expected) = &schema.const_value
        && node.as_str().is_none_or(|s| s != expected)
    {
        errors.push(SchemaError {
            path: path.clone(),
            span,
            message: format!("value must equal const {expected:?}"),
        });
    }

    // ── enum ──────────────────────────────────────────────────────────────────
    if let Some(allowed) = &schema.enum_values
        && let Some(text) = node.as_str()
        && !allowed.iter().any(|v| v == text)
    {
        errors.push(SchemaError {
            path: path.clone(),
            span,
            message: format!(
                "value {:?} is not one of [{}]",
                text,
                allowed
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    validate_scalar_bounds(node, schema, &path, span, errors);

    validate_object(node, schema, &path, span, errors);

    // ── array keywords ────────────────────────────────────────────────────────
    if let Some(items) = node.as_sequence() {
        let item_count = items.len();

        if let Some(min) = schema.min_items
            && item_count < min
        {
            errors.push(SchemaError {
                path: path.clone(),
                span,
                message: format!("array has {item_count} items, minimum is {min}"),
            });
        }
        if let Some(max) = schema.max_items
            && item_count > max
        {
            errors.push(SchemaError {
                path: path.clone(),
                span,
                message: format!("array has {item_count} items, maximum is {max}"),
            });
        }

        if let Some(item_schema) = &schema.items {
            for (idx, item_node) in items.iter().enumerate() {
                validate_node(item_node, item_schema, format!("{path}/{idx}"), errors);
            }
        }
    }
}

/// Returns a human-readable name for a [`JsonType`].
const fn json_type_name(t: JsonType) -> &'static str {
    match t {
        JsonType::Null => "null",
        JsonType::Boolean => "boolean",
        JsonType::Integer => "integer",
        JsonType::Number => "number",
        JsonType::String => "string",
        JsonType::Array => "array",
        JsonType::Object => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Schema;

    fn node(s: &str) -> Node<'static> {
        glaucus_ast::composer::Composer::new(s)
            .next()
            .unwrap()
            .unwrap()
            .into_owned()
    }

    fn schema(s: &str) -> Schema {
        Schema::from_node(&node(s))
    }

    #[test]
    fn valid_object_passes() {
        let sc = schema(
            "type: object\nrequired: [name]\nproperties:\n  name: {type: string}\n  age: {type: integer}\n",
        );
        assert!(validate(&node("name: alice\nage: 30\n"), &sc).is_ok());
    }

    #[test]
    fn missing_required_fails_with_span() {
        let sc = schema("type: object\nrequired: [name]\n");
        let data = node("age: 30\n");
        let errs = validate(&data, &sc).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("name") && e.message.contains("required"))
        );
    }

    #[test]
    fn wrong_type_fails() {
        let sc = schema("type: object\nproperties:\n  age: {type: integer}\n");
        let errs = validate(&node("age: not_a_number\n"), &sc).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.to_lowercase().contains("integer")
                    || e.message.to_lowercase().contains("type"))
        );
    }

    #[test]
    fn enum_and_numeric_bounds() {
        let sc = schema(
            "type: object\nproperties:\n  color: {enum: [red, green, blue]}\n  n: {type: integer, minimum: 1, maximum: 10}\n",
        );
        assert!(validate(&node("color: red\nn: 5\n"), &sc).is_ok());
        assert!(validate(&node("color: purple\nn: 5\n"), &sc).is_err());
        assert!(validate(&node("color: red\nn: 99\n"), &sc).is_err());
    }

    #[test]
    fn additional_properties_false_rejects_extras() {
        let sc = schema(
            "type: object\nadditionalProperties: false\nproperties:\n  a: {type: integer}\n",
        );
        assert!(validate(&node("a: 1\n"), &sc).is_ok());
        assert!(validate(&node("a: 1\nb: 2\n"), &sc).is_err());
    }

    #[test]
    fn array_items_and_length() {
        let sc = schema("type: array\nminItems: 1\nmaxItems: 3\nitems: {type: integer}\n");
        assert!(validate(&node("[1, 2, 3]\n"), &sc).is_ok());
        assert!(validate(&node("[]\n"), &sc).is_err());
        assert!(validate(&node("[1, 2, 3, 4]\n"), &sc).is_err());
        assert!(validate(&node("[1, two, 3]\n"), &sc).is_err()); // wrong item type
    }

    #[test]
    fn integer_satisfies_number_type() {
        let sc = schema("type: number\n");
        assert!(validate(&node("42\n"), &sc).is_ok());
    }

    #[test]
    fn string_length_bounds() {
        let sc = schema("type: string\nminLength: 2\nmaxLength: 4\n");
        assert!(validate(&node("abc\n"), &sc).is_ok());
        assert!(validate(&node("a\n"), &sc).is_err());
        assert!(validate(&node("abcde\n"), &sc).is_err());
    }

    #[test]
    fn float_scalar_classified_as_number() {
        // A non-integer float scalar exercises classify_scalar's Number branch
        // and satisfies a `type: number` schema.
        let sc = schema("type: number\n");
        assert!(validate(&node("1.5\n"), &sc).is_ok());
        // The mismatch path also names the actual type "number".
        let strict = schema("type: integer\n");
        let errs = validate(&node("1.5\n"), &strict).unwrap_err();
        assert!(errs[0].message.contains("number"));
    }

    #[test]
    fn const_mismatch_reports_error() {
        // A value differing from `const` triggers the const-check error arm.
        let sc = schema("const: fixed\n");
        assert!(validate(&node("fixed\n"), &sc).is_ok());
        let errs = validate(&node("other\n"), &sc).unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("const")));
    }

    #[test]
    fn minimum_violation_reported() {
        // A value below `minimum` exercises the `num < min` branch.
        let sc = schema("type: integer\nminimum: 10\n");
        let errs = validate(&node("3\n"), &sc).unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("less than minimum")));
    }

    #[test]
    fn min_and_max_properties_violations() {
        let too_few = schema("type: object\nminProperties: 2\n");
        let errs = validate(&node("a: 1\n"), &too_few).unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("minimum is 2")));

        let too_many = schema("type: object\nmaxProperties: 1\n");
        let errs = validate(&node("a: 1\nb: 2\n"), &too_many).unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("maximum is 1")));
    }

    #[test]
    fn json_type_name_covers_all_variants() {
        // Drive type-mismatch errors whose expected/actual names span every
        // JsonType arm of json_type_name.
        // expected null, actual boolean
        let e = validate(&node("true\n"), &schema("type: 'null'\n")).unwrap_err();
        assert!(e[0].message.contains("null") && e[0].message.contains("boolean"));
        // expected integer, actual string
        let e = validate(&node("hello\n"), &schema("type: integer\n")).unwrap_err();
        assert!(e[0].message.contains("integer") && e[0].message.contains("string"));
        // expected array, actual object
        let e = validate(&node("a: 1\n"), &schema("type: array\n")).unwrap_err();
        assert!(e[0].message.contains("array") && e[0].message.contains("object"));
        // expected object, actual array
        let e = validate(&node("[1]\n"), &schema("type: object\n")).unwrap_err();
        assert!(e[0].message.contains("object") && e[0].message.contains("array"));
    }

    // ── Style-aware type classification ──────────────────────────────
    //
    // Per the YAML 1.2 Core Schema, only a *plain* scalar undergoes
    // non-specific tag resolution to bool/int/float/null. An explicitly
    // quoted or block scalar is unambiguously `string`, regardless of what
    // its text looks like.

    #[test]
    fn double_quoted_integer_like_string_is_not_an_integer() {
        // Without the style check, `port: "8080"` validated as an integer
        // while `coerce_to_schema` (which reads the raw CST text and does
        // respect quoting) rewrites that very same value under `--fix` — two
        // codepaths disagreeing about one document's type.
        let sc = schema("type: object\nproperties:\n  port: {type: integer}\n");
        let errs = validate(&node("port: \"8080\"\n"), &sc).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("integer") && e.message.contains("string"))
        );
    }

    #[test]
    fn plain_integer_like_scalar_still_validates() {
        // The companion case: an unquoted scalar keeps resolving through
        // classify_scalar, so `type: integer` still accepts plain `8080`.
        let sc = schema("type: object\nproperties:\n  port: {type: integer}\n");
        assert!(validate(&node("port: 8080\n"), &sc).is_ok());
    }

    #[test]
    fn literal_block_scalar_is_always_a_string() {
        // `|-` (strip chomping) makes the scalar's text exactly "123", with
        // no trailing newline to otherwise explain a type mismatch — the
        // failure below can only come from the block style itself.
        let sc = schema("type: object\nproperties:\n  n: {type: integer}\n");
        let errs = validate(&node("n: |-\n  123\n"), &sc).unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("string")));

        let string_sc = schema("type: object\nproperties:\n  n: {type: string}\n");
        assert!(validate(&node("n: |-\n  123\n"), &string_sc).is_ok());
    }
}
