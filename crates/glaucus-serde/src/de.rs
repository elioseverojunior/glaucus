// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! YAML deserializer.
//!
//! Walks a [`Node`] tree and drives serde's `Visitor` protocol.
//! Supports the full YAML 1.2 Core Schema type resolution.

use crate::error::Error;
use glaucus_ast::node::Node;
use glaucus_core::types::{ScalarStyle, Tag};
use serde::de::{self, DeserializeOwned, DeserializeSeed, IntoDeserializer, Visitor};

/// Deserializes a `Node` tree into a Rust type.
pub struct Deserializer<'a> {
    node: &'a Node<'a>,
    yaml_1_1: bool,
}

impl<'a> Deserializer<'a> {
    /// Creates a new deserializer from a `Node` reference.
    ///
    /// Uses YAML 1.2 Core Schema bool/null resolution (default).
    #[must_use]
    pub const fn from_node(node: &'a Node<'a>) -> Self {
        Self {
            node,
            yaml_1_1: false,
        }
    }

    /// Creates a deserializer with YAML 1.1 bool/null compatibility set.
    ///
    /// When `yaml_1_1` is `true`, boolean recognition is widened to the YAML
    /// 1.1 set: `y/Y/yes/Yes/YES/on/On/ON` resolve to `true`;
    /// `n/N/no/No/NO/off/Off/OFF` resolve to `false`.
    #[must_use]
    pub const fn from_node_with(node: &'a Node<'a>, yaml_1_1: bool) -> Self {
        Self { node, yaml_1_1 }
    }
}

fn parse_integer(value: &str) -> Option<i64> {
    let (negative, digits) = value.strip_prefix('-').map_or_else(
        || {
            value
                .strip_prefix('+')
                .map_or((false, value), |rest| (false, rest))
        },
        |rest| (true, rest),
    );

    if digits.is_empty() {
        return None;
    }

    let abs = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()?
    } else if let Some(oct) = digits
        .strip_prefix("0o")
        .or_else(|| digits.strip_prefix("0O"))
    {
        i64::from_str_radix(oct, 8).ok()?
    } else {
        digits.parse::<i64>().ok()?
    };

    if negative { Some(-abs) } else { Some(abs) }
}

fn parse_unsigned(value: &str) -> Option<u64> {
    if value.starts_with('-') {
        return None;
    }
    let digits = value.strip_prefix('+').unwrap_or(value);

    if digits.is_empty() {
        return None;
    }

    if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16).ok();
    }
    digits
        .strip_prefix("0o")
        .or_else(|| digits.strip_prefix("0O"))
        .map_or_else(
            || digits.parse::<u64>().ok(),
            |oct| u64::from_str_radix(oct, 8).ok(),
        )
}

/// A YAML core-schema tag, resolved to the type it asserts.
///
/// An explicit tag is the ONE mechanism a YAML author has to override implicit
/// resolution — `!!str 123` to keep a version number text, `!!int "456"` to read
/// a quoted field as a number. Ignoring it turns the author's explicit intent
/// into a silently different value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CoreTag {
    Str,
    Int,
    Float,
    Bool,
    Null,
    Binary,
}

impl CoreTag {
    /// Classifies a resolved tag URI, or `None` for anything not in the core
    /// schema — custom and application tags must keep flowing through normal
    /// resolution rather than being reinterpreted here.
    fn from_uri(uri: &str) -> Option<Self> {
        match uri.strip_prefix("tag:yaml.org,2002:")? {
            "str" => Some(Self::Str),
            "int" => Some(Self::Int),
            "float" => Some(Self::Float),
            "bool" => Some(Self::Bool),
            "null" => Some(Self::Null),
            "binary" => Some(Self::Binary),
            _ => None,
        }
    }
}

fn parse_float(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        ".inf" | "+.inf" => Some(f64::INFINITY),
        "-.inf" => Some(f64::NEG_INFINITY),
        ".nan" => Some(f64::NAN),

        // The digit guard exists because Rust's `f64::from_str` also accepts
        // `inf`, `infinity` and `nan`, which YAML 1.2 §10.2 does NOT recognise:
        // the float production admits exactly `[-+]?\.(inf|Inf|INF)` and
        // `\.(nan|NaN|NAN)`, matched above, and the leading dot is mandatory.
        // Bare `inf` / `nan` / `infinity` are plain strings.
        //
        // Requiring at least one ASCII digit is a complete separator rather than a
        // heuristic: every other form in the YAML float grammar
        // (`[-+]?( \.[0-9]+ | [0-9]+(\.[0-9]*)? )([eE][-+]?[0-9]+)?`) contains a
        // digit by construction, while the three bare words contain none.
        //
        // This is the Norway problem's shape -- implicit resolution firing on a
        // word that should stay text -- and worse in consequence. A float infinity
        // has no JSON representation, so an `inf` that resolved as a float becomes
        // `null` on the way out. The value does not change type; it vanishes.
        _ if value.bytes().any(|b| b.is_ascii_digit()) => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn err(msg: impl std::fmt::Display) -> Error {
    <Error as de::Error>::custom(msg)
}

// ─── Deserializer trait impl ────────────────────────────────────────

impl<'de> de::Deserializer<'de> for &mut Deserializer<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node {
            Node::Sequence(_) => self.deserialize_seq(visitor),
            Node::Mapping(_) => self.deserialize_map(visitor),
            Node::Scalar(s) => {
                let value = &*s.value;

                // An explicit core-schema tag overrides everything below --
                // including the style shortcut, since `!!int "456"` is precisely
                // the author overriding what the quotes would otherwise imply.
                if let Some(tag) = self.core_tag() {
                    return self.visit_core_tagged(tag, visitor);
                }

                // Quoted scalars are always strings.
                // For Cow::Owned, call visit_string to pass the String directly
                // (avoids the extra to_owned() that visit_str would trigger in
                // the visitor). For Cow::Borrowed, visit_str is fine.
                if s.style != ScalarStyle::Plain {
                    return match &s.value {
                        std::borrow::Cow::Owned(v) => visitor.visit_string(v.clone()),
                        std::borrow::Cow::Borrowed(_) => visitor.visit_str(value),
                    };
                }

                // Null
                if Deserializer::is_null(value) {
                    return visitor.visit_unit();
                }

                // Bool
                if self.is_bool_true(value) {
                    return visitor.visit_bool(true);
                }
                if self.is_bool_false(value) {
                    return visitor.visit_bool(false);
                }

                // Integer (try unsigned first for large positive values)
                if let Some(u) = parse_unsigned(value) {
                    // Prefer the signed visitor when the value fits, so small
                    // positive integers deserialize into `i64` targets.
                    if let Ok(i) = i64::try_from(u) {
                        return visitor.visit_i64(i);
                    }
                    return visitor.visit_u64(u);
                }
                if let Some(i) = parse_integer(value) {
                    return visitor.visit_i64(i);
                }

                // Float
                if let Some(f) = parse_float(value) {
                    return visitor.visit_f64(f);
                }

                // Plain string fallback.
                // For Cow::Owned, visit_string avoids an extra clone.
                match &s.value {
                    std::borrow::Cow::Owned(v) => visitor.visit_string(v.clone()),
                    std::borrow::Cow::Borrowed(_) => visitor.visit_str(value),
                }
            }
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let value = self.scalar_value()?;
        if self.is_bool_true(value) {
            visitor.visit_bool(true)
        } else if self.is_bool_false(value) {
            visitor.visit_bool(false)
        } else {
            Err(err(format!("expected boolean, found `{value}`")))
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let i = self.parse_int()?;
        visitor.visit_i8(i.try_into().map_err(|_| err(format!("{i} overflows i8")))?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let i = self.parse_int()?;
        visitor.visit_i16(
            i.try_into()
                .map_err(|_| err(format!("{i} overflows i16")))?,
        )
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let i = self.parse_int()?;
        visitor.visit_i32(
            i.try_into()
                .map_err(|_| err(format!("{i} overflows i32")))?,
        )
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i64(self.parse_int()?)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let u = self.parse_uint()?;
        visitor.visit_u8(u.try_into().map_err(|_| err(format!("{u} overflows u8")))?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let u = self.parse_uint()?;
        visitor.visit_u16(
            u.try_into()
                .map_err(|_| err(format!("{u} overflows u16")))?,
        )
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let u = self.parse_uint()?;
        visitor.visit_u32(
            u.try_into()
                .map_err(|_| err(format!("{u} overflows u32")))?,
        )
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u64(self.parse_uint()?)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        // YAML has one float type; narrowing to the `f32` the caller asked for
        // is the whole point of this method, and matches what serde_json does.
        visitor.visit_f32(self.parse_float_val()? as f32)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_f64(self.parse_float_val()?)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let value = self.scalar_value()?;
        let mut chars = value.chars();
        let c = chars
            .next()
            .ok_or_else(|| err("expected a character, found empty string"))?;
        if chars.next().is_some() {
            return Err(err(format!("expected a single character, found `{value}`")));
        }
        visitor.visit_char(c)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_str(self.scalar_value()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_string(self.scalar_value()?.to_owned())
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        // `!!binary` is base64 in the source and bytes in the data model, so a
        // bytes-shaped target must get the decoded payload rather than the
        // encoding. Untagged scalars keep passing their UTF-8 through unchanged.
        if self.core_tag() == Some(CoreTag::Binary) {
            return self.visit_core_tagged(CoreTag::Binary, visitor);
        }
        visitor.visit_bytes(self.scalar_value()?.as_bytes())
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Node::Scalar(s) = self.node
            && s.style == ScalarStyle::Plain
            && Deserializer::is_null(&s.value)
        {
            return visitor.visit_none();
        }
        visitor.visit_some(self)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Node::Scalar(s) = self.node
            && s.style == ScalarStyle::Plain
            && Deserializer::is_null(&s.value)
        {
            return visitor.visit_unit();
        }
        Err(err("expected null"))
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node {
            Node::Sequence(s) => visitor.visit_seq(SeqAccess::new(&s.items, self.yaml_1_1)),
            _ => Err(err("expected a sequence")),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node {
            Node::Mapping(m) => visitor.visit_map(MapAccess::new(&m.entries, self.yaml_1_1)),
            _ => Err(err("expected a mapping")),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        // `serde_yaml` drop-in: an externally-tagged non-unit variant may arrive as
        // a YAML local tag on the data node, e.g. `!Variant\n- 1`. Accept that form
        // alongside glaucus's own single-key mapping form (`Variant:\n  - 1`).
        let yaml_1_1 = self.yaml_1_1;
        if let Some(variant) = self.node.tag().and_then(external_variant_tag) {
            return visitor.visit_enum(EnumAccess::Tagged(variant, self.node, yaml_1_1));
        }
        match self.node {
            // Plain scalar → unit variant
            Node::Scalar(_) => visitor.visit_enum(EnumAccess::Scalar(self.node, yaml_1_1)),
            // Single-key mapping → newtype/struct variant
            Node::Mapping(m) if m.entries.len() == 1 => visitor.visit_enum(EnumAccess::Mapping(
                &m.entries[0].0,
                &m.entries[0].1,
                yaml_1_1,
            )),
            Node::Mapping(_) => Err(err("expected a single-key mapping for enum variant")),
            Node::Sequence(_) => Err(err("expected a scalar or mapping for enum, found sequence")),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
}

// ─── Helper methods ─────────────────────────────────────────────────

impl Deserializer<'_> {
    // ── Scalar type resolution (YAML 1.2 Core Schema §10.3.2) ──────

    fn is_null(value: &str) -> bool {
        matches!(value, "null" | "Null" | "NULL" | "~" | "")
    }

    fn is_bool_true(&self, value: &str) -> bool {
        matches!(value, "true" | "True" | "TRUE")
            || (self.yaml_1_1
                && matches!(
                    value,
                    "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON"
                ))
    }

    fn is_bool_false(&self, value: &str) -> bool {
        matches!(value, "false" | "False" | "FALSE")
            || (self.yaml_1_1
                && matches!(
                    value,
                    "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF"
                ))
    }

    fn scalar_value(&self) -> Result<&str, Error> {
        match self.node {
            Node::Scalar(s) => Ok(&s.value),
            _ => Err(err("expected a scalar value")),
        }
    }

    /// The core-schema tag on this node, if it carries one.
    fn core_tag(&self) -> Option<CoreTag> {
        match self.node {
            Node::Scalar(s) => s.tag.as_ref().and_then(|t| CoreTag::from_uri(&t.value)),
            _ => None,
        }
    }

    /// Resolves a scalar according to its explicit core-schema tag.
    ///
    /// The tag wins over BOTH implicit resolution and the scalar's presentation
    /// style. That is the whole point: quoting normally forces a string, and
    /// `!!int "456"` is the author saying otherwise.
    ///
    /// Content that cannot satisfy the tag is an error rather than a coercion. A
    /// tag is an assertion about the data, so `!!int "abc"` is a defect in the
    /// document, and inventing a value for it would reintroduce exactly the class
    /// of silent wrongness this resolution exists to remove.
    fn visit_core_tagged<'de, V>(&self, tag: CoreTag, visitor: V) -> Result<V::Value, Error>
    where
        V: Visitor<'de>,
    {
        let value = self.scalar_value()?;

        match tag {
            // Any content is valid `!!str`; that is what makes it the escape hatch.
            CoreTag::Str => visitor.visit_str(value),

            CoreTag::Int => {
                // Early returns rather than `map_or_else`: both arms would need to
                // capture `visitor`, and it can only be moved into one closure.
                if let Some(u) = parse_unsigned(value) {
                    if let Ok(i) = i64::try_from(u) {
                        return visitor.visit_i64(i);
                    }
                    return visitor.visit_u64(u);
                }
                parse_integer(value).map_or_else(
                    || Err(err(format!("`!!int` scalar is not an integer: `{value}`"))),
                    |i| visitor.visit_i64(i),
                )
            }

            CoreTag::Float => parse_float(value).map_or_else(
                || Err(err(format!("`!!float` scalar is not a float: `{value}`"))),
                |f| visitor.visit_f64(f),
            ),

            // Reuses the deserialiser's own bool recognition, so `!!bool` widens
            // to the YAML 1.1 spellings exactly when `yaml_1_1` is set and not
            // otherwise. A second, divergent notion of "boolean" here would be a
            // bug farm.
            CoreTag::Bool => {
                if self.is_bool_true(value) {
                    return visitor.visit_bool(true);
                }
                if self.is_bool_false(value) {
                    return visitor.visit_bool(false);
                }
                Err(err(format!("`!!bool` scalar is not a boolean: `{value}`")))
            }

            CoreTag::Null => {
                if Self::is_null(value) {
                    return visitor.visit_unit();
                }
                Err(err(format!("`!!null` scalar is not null: `{value}`")))
            }

            CoreTag::Binary => crate::base64::decode(value).map_or_else(
                || Err(err("`!!binary` scalar is not valid base64")),
                |bytes| visitor.visit_byte_buf(bytes),
            ),
        }
    }

    fn parse_int(&self) -> Result<i64, Error> {
        let value = self.scalar_value()?;
        parse_integer(value).ok_or_else(|| err(format!("expected integer, found `{value}`")))
    }

    fn parse_uint(&self) -> Result<u64, Error> {
        let value = self.scalar_value()?;
        parse_unsigned(value)
            .ok_or_else(|| err(format!("expected unsigned integer, found `{value}`")))
    }

    fn parse_float_val(&self) -> Result<f64, Error> {
        let value = self.scalar_value()?;
        parse_float(value).ok_or_else(|| err(format!("expected float, found `{value}`")))
    }
}

// ─── SeqAccess ──────────────────────────────────────────────────────

struct SeqAccess<'a> {
    iter: std::slice::Iter<'a, Node<'a>>,
    yaml_1_1: bool,
}

impl<'a> SeqAccess<'a> {
    fn new(items: &'a [Node<'a>], yaml_1_1: bool) -> Self {
        Self {
            iter: items.iter(),
            yaml_1_1,
        }
    }
}

impl<'de> de::SeqAccess<'de> for SeqAccess<'_> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        match self.iter.next() {
            Some(node) => {
                let mut de = Deserializer::from_node_with(node, self.yaml_1_1);
                seed.deserialize(&mut de).map(Some)
            }
            None => Ok(None),
        }
    }
}

// ─── MapAccess ──────────────────────────────────────────────────────

struct MapAccess<'a> {
    iter: std::slice::Iter<'a, (Node<'a>, Node<'a>)>,
    pending_value: Option<&'a Node<'a>>,
    yaml_1_1: bool,
}

impl<'a> MapAccess<'a> {
    fn new(entries: &'a [(Node<'a>, Node<'a>)], yaml_1_1: bool) -> Self {
        Self {
            iter: entries.iter(),
            pending_value: None,
            yaml_1_1,
        }
    }
}

impl<'de> de::MapAccess<'de> for MapAccess<'_> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match self.iter.next() {
            Some((key, value)) => {
                self.pending_value = Some(value);
                let mut de = Deserializer::from_node_with(key, self.yaml_1_1);
                seed.deserialize(&mut de).map(Some)
            }
            None => Ok(None),
        }
    }

    // serde's `MapAccess` contract guarantees `next_key_seed` is called first
    // and that it stored a value; a violation is a bug in the caller, not in
    // the YAML being read.
    #[allow(clippy::expect_used)]
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let node = self
            .pending_value
            .take()
            .expect("next_value_seed called before next_key_seed");
        let mut de = Deserializer::from_node_with(node, self.yaml_1_1);
        seed.deserialize(&mut de)
    }
}

// ─── EnumAccess ─────────────────────────────────────────────────────

/// Extracts an external enum variant name from a YAML local tag (`!Variant`).
///
/// Returns `None` for core-schema shorthand tags (`!!str`) and global/URI tags
/// (`tag:yaml.org,2002:int`), which denote a *data type* rather than a serde
/// variant — those must flow through normal scalar/mapping resolution.
fn external_variant_tag<'a>(tag: &'a Tag<'_>) -> Option<&'a str> {
    let name = tag.value.strip_prefix('!')?;
    if name.is_empty() || name.starts_with('!') {
        return None;
    }
    Some(name)
}

enum EnumAccess<'a> {
    /// Plain scalar → unit variant name.
    Scalar(&'a Node<'a>, bool),
    /// Single-key mapping → variant name is the key, data is the value.
    Mapping(&'a Node<'a>, &'a Node<'a>, bool),
    /// Local-tag form (`!Variant`) → variant name from the tag, data is the node.
    Tagged(&'a str, &'a Node<'a>, bool),
}

impl<'de> de::EnumAccess<'de> for EnumAccess<'_> {
    type Error = Error;
    type Variant = VariantAccess<'static>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        match self {
            EnumAccess::Scalar(node, yaml_1_1) => {
                let mut de = Deserializer::from_node_with(node, yaml_1_1);
                let variant = seed.deserialize(&mut de)?;
                Ok((variant, VariantAccess::Unit))
            }
            EnumAccess::Mapping(key, value, yaml_1_1) => {
                let mut de = Deserializer::from_node_with(key, yaml_1_1);
                let variant = seed.deserialize(&mut de)?;
                // We need to stash the value node for the VariantAccess to consume.
                // Since VariantAccess can't borrow the node (lifetime issues),
                // we convert the value to an owned Node<'static>.
                let owned = value.clone().into_owned();
                Ok((variant, VariantAccess::Value(owned, yaml_1_1)))
            }
            EnumAccess::Tagged(name, node, yaml_1_1) => {
                // The variant name comes from the tag; the whole node is the data.
                let de = IntoDeserializer::<Error>::into_deserializer(name);
                let variant = seed.deserialize(de)?;
                let owned = node.clone().into_owned();
                Ok((variant, VariantAccess::Value(owned, yaml_1_1)))
            }
        }
    }
}

enum VariantAccess<'a> {
    Unit,
    Value(Node<'a>, bool),
}

impl<'de> de::VariantAccess<'de> for VariantAccess<'_> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self {
            VariantAccess::Unit => Ok(()),
            VariantAccess::Value(..) => Err(err("expected unit variant")),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        match self {
            VariantAccess::Value(ref node, yaml_1_1) => {
                let mut de = Deserializer::from_node_with(node, yaml_1_1);
                seed.deserialize(&mut de)
            }
            VariantAccess::Unit => Err(err("expected newtype variant")),
        }
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Error> {
        match self {
            VariantAccess::Value(ref node, yaml_1_1) => {
                let mut de = Deserializer::from_node_with(node, yaml_1_1);
                de::Deserializer::deserialize_seq(&mut de, visitor)
            }
            VariantAccess::Unit => Err(err("expected tuple variant")),
        }
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self {
            VariantAccess::Value(ref node, yaml_1_1) => {
                let mut de = Deserializer::from_node_with(node, yaml_1_1);
                de::Deserializer::deserialize_map(&mut de, visitor)
            }
            VariantAccess::Unit => Err(err("expected struct variant")),
        }
    }
}

// ─── Public convenience ─────────────────────────────────────────────

/// Parses the first YAML document and converts it to `Value` directly from
/// the composer's `Node<'_>` tree via `into_owned()`, bypassing serde dispatch.
///
/// This avoids the per-scalar overhead of `deserialize_any` → `visit_str` /
/// `visit_i64` etc. when the caller simply needs a generic value tree.
fn from_str_value_direct(input: &str) -> crate::error::Result<crate::value::Value> {
    // An empty stream is a null document, not an error -- see
    // `glaucus_ast::composer::compose_one`.
    let node = glaucus_ast::composer::compose_one(input)?;
    Ok(crate::value::Value::new(node.into_owned()))
}

/// Deserializes a YAML string into a Rust type.
///
/// When `T` is [`Value`](crate::value::Value), a fast direct-conversion path
/// is used: the `Node<'_>` produced by the composer is converted to
/// `Node<'static>` via `into_owned()` without going through serde's visitor
/// protocol.  For all other types the standard serde `Deserializer` path is
/// used unchanged.
///
/// # Errors
///
/// Returns an error if the input is not well-formed YAML, if a configured
/// resource limit is exceeded, or if the document does not match `T`.
///
/// # Panics
///
/// Cannot panic in practice. The `Value` fast path downcasts only after
/// `TypeId::of::<T>()` has matched `Value`, so the downcast is infallible.
#[allow(clippy::expect_used)]
pub fn from_str<T: DeserializeOwned + 'static>(input: &str) -> crate::error::Result<T> {
    // Fast path: when T = Value, bypass serde's visitor dispatch entirely.
    // TypeId::of::<T>() is a compile-time constant under monomorphisation;
    // the branch is eliminated by the optimiser for every T ≠ Value.
    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<crate::value::Value>() {
        let value = from_str_value_direct(input)?;
        // `Box<dyn Any>::downcast` is safe here: the TypeId check above
        // guarantees T is exactly Value, so the downcast will always succeed.
        let boxed: Box<dyn std::any::Any> = Box::new(value);
        return Ok(*boxed
            .downcast::<T>()
            .expect("TypeId of T equals TypeId of Value, downcast is infallible"));
    }

    // An empty stream is a null document, not an error -- see
    // `glaucus_ast::composer::compose_one`. The version comes back alongside the
    // node because a `%YAML 1.1` directive selects 1.1 scalar resolution for this
    // document, and the node alone cannot say which schema applies to it.
    let (node, version) = glaucus_ast::composer::compose_one_versioned(
        input,
        glaucus_core::error::ParserConfig::default(),
    )?;
    let mut de = Deserializer::from_node_with(&node, version.is_1_1());
    T::deserialize(&mut de)
}

/// Deserializes a YAML string into a Rust type, reading from a `Read` source.
///
/// # Errors
///
/// Returns an error if the input is not well-formed YAML, if a configured
/// resource limit is exceeded, or if the document does not match `T`.
///
/// Also returns an error if reading from the source fails.
pub fn from_reader<T: DeserializeOwned + 'static>(
    mut reader: impl std::io::Read,
) -> crate::error::Result<T> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf).map_err(|e| {
        Error::core(glaucus_core::error::Error::spanless(
            glaucus_core::error::ErrorKind::UnexpectedToken {
                expected: "readable input".into(),
                found: format!("I/O error: {e}").into(),
            },
        ))
    })?;
    from_str(&buf)
}

/// Deserializes a YAML string into a Rust type with custom parser configuration.
///
/// Use this to control strictness (lenient vs strict), resource limits, and schema.
///
/// # Errors
///
/// Returns an error if the input is not well-formed YAML, if a configured
/// resource limit is exceeded, or if the document does not match `T`.
///
/// # Panics
///
/// Cannot panic in practice. The `Value` fast path downcasts only after
/// `TypeId::of::<T>()` has matched `Value`, so the downcast is infallible.
#[allow(clippy::expect_used)]
pub fn from_str_with<T: DeserializeOwned + 'static>(
    input: &str,
    config: glaucus_core::error::ParserConfig,
) -> crate::error::Result<T> {
    // Fast path for Value: same as from_str but with config-driven parsing.
    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<crate::value::Value>() {
        let node = glaucus_ast::composer::compose_one_with(input, config)?;
        let value = crate::value::Value::new(node.into_owned());
        let boxed: Box<dyn std::any::Any> = Box::new(value);
        return Ok(*boxed
            .downcast::<T>()
            .expect("TypeId of T equals TypeId of Value, downcast is infallible"));
    }

    // The config flag and the in-document directive are both honoured, and either
    // one alone is enough. The flag is for callers whose input carries no
    // directive; the directive is for documents that declare their own version.
    // Neither can turn the other off.
    let yaml_1_1 = config.yaml_1_1;
    let (node, version) = glaucus_ast::composer::compose_one_versioned(input, config)?;
    let mut de = Deserializer::from_node_with(&node, yaml_1_1 || version.is_1_1());
    T::deserialize(&mut de)
}

/// Deserializes a YAML string into a Rust type from a reader, with custom parser configuration.
///
/// # Errors
///
/// Returns an error if the input is not well-formed YAML, if a configured
/// resource limit is exceeded, or if the document does not match `T`.
///
/// Also returns an error if reading from the source fails.
pub fn from_reader_with<T: DeserializeOwned + 'static>(
    mut reader: impl std::io::Read,
    config: glaucus_core::error::ParserConfig,
) -> crate::error::Result<T> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf).map_err(|e| {
        Error::core(glaucus_core::error::Error::spanless(
            glaucus_core::error::ErrorKind::UnexpectedToken {
                expected: "readable input".into(),
                found: format!("I/O error: {e}").into(),
            },
        ))
    })?;
    from_str_with(&buf, config)
}

/// Deserializes all YAML documents from a string into a `Vec<T>`.
///
/// Each document is composed into a `Node`, then deserialized to `T`.
/// An empty stream produces an empty `Vec`.
///
/// # Errors
///
/// Returns an error if the input is not well-formed YAML, if a configured
/// resource limit is exceeded, or if the document does not match `T`.
///
/// # Panics
///
/// Cannot panic in practice. The `Value` fast path downcasts only after
/// `TypeId::of::<T>()` has matched `Value`, so the downcast is infallible.
#[allow(clippy::expect_used)]
pub fn from_str_multi<T: DeserializeOwned + 'static>(input: &str) -> crate::error::Result<Vec<T>> {
    // Fast path for Vec<Value>.
    if std::any::TypeId::of::<T>() == std::any::TypeId::of::<crate::value::Value>() {
        let nodes = glaucus_ast::composer::compose_all(input)?;
        let values: Vec<crate::value::Value> = nodes
            .into_iter()
            .map(|node| crate::value::Value::new(node.into_owned()))
            .collect();
        let boxed: Box<dyn std::any::Any> = Box::new(values);
        return Ok(*boxed
            .downcast::<Vec<T>>()
            .expect("TypeId of T equals TypeId of Value, downcast is infallible"));
    }

    let nodes = glaucus_ast::composer::compose_all(input)?;
    nodes
        .iter()
        .map(|node| {
            let mut de = Deserializer::from_node(node);
            T::deserialize(&mut de)
        })
        .collect()
}

/// Deserializes all YAML documents from a string into a `Vec<T>`, with custom parser configuration.
///
/// # Errors
///
/// Returns an error if the input is not well-formed YAML, if a configured
/// resource limit is exceeded, or if the document does not match `T`.
pub fn from_str_multi_with<T: DeserializeOwned + 'static>(
    input: &str,
    config: glaucus_core::error::ParserConfig,
) -> crate::error::Result<Vec<T>> {
    let yaml_1_1 = config.yaml_1_1;
    let nodes: Vec<Node<'_>> = glaucus_ast::composer::Composer::with_config(input, config)
        .collect::<Result<Vec<_>, _>>()?;
    nodes
        .iter()
        .map(|node| {
            let mut de = Deserializer::from_node_with(node, yaml_1_1);
            T::deserialize(&mut de)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn de_string() {
        let result: String = from_str("hello").unwrap();
        assert_eq!(result, "hello");
    }

    // ─── Core-schema tag resolution ─────────────────────────────────

    /// Routes through `deserialize_any`, which is where implicit resolution lives.
    #[derive(Deserialize, PartialEq, Debug)]
    #[serde(untagged)]
    enum TagAny {
        Bool(bool),
        Int(i64),
        Float(f64),
        Text(String),
        Unit,
    }

    /// A target that calls `deserialize_bytes`, so `!!binary` can be observed.
    #[derive(PartialEq, Debug)]
    struct TagBytes(Vec<u8>);

    impl<'de> Deserialize<'de> for TagBytes {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct V;

            impl Visitor<'_> for V {
                type Value = TagBytes;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("binary data")
                }

                fn visit_bytes<E>(self, v: &[u8]) -> Result<TagBytes, E>
                where
                    E: de::Error,
                {
                    Ok(TagBytes(v.to_vec()))
                }

                fn visit_str<E>(self, v: &str) -> Result<TagBytes, E>
                where
                    E: de::Error,
                {
                    Ok(TagBytes(v.as_bytes().to_vec()))
                }
            }

            deserializer.deserialize_bytes(V)
        }
    }

    #[test]
    fn tag_str_keeps_a_number_as_text() {
        // The canonical reason `!!str` exists: force a version number or ZIP code
        // to stay text. Silently producing an integer is a wrong value rather than
        // an error, which is the most dangerous failure mode there is.
        assert_eq!(
            from_str::<TagAny>("!!str 123").unwrap(),
            TagAny::Text("123".into())
        );
        assert_eq!(
            from_str::<TagAny>("!!str 007").unwrap(),
            TagAny::Text("007".into())
        );
        assert_eq!(
            from_str::<TagAny>("!!str true").unwrap(),
            TagAny::Text("true".into())
        );
    }

    #[test]
    fn tag_int_overrides_quoting() {
        // Quoting normally forces a string. An explicit tag is the author's
        // override of implicit resolution and must win over it.
        assert_eq!(
            from_str::<TagAny>("!!int \"456\"").unwrap(),
            TagAny::Int(456)
        );
        assert_eq!(from_str::<i64>("!!int \"456\"").unwrap(), 456);
    }

    #[test]
    fn tag_float_overrides_quoting() {
        let v = from_str::<TagAny>("!!float \"3.15\"").unwrap();
        assert!(matches!(v, TagAny::Float(f) if (f - 3.15).abs() < f64::EPSILON));
    }

    #[test]
    fn tag_bool_overrides_quoting() {
        assert_eq!(
            from_str::<TagAny>("!!bool \"true\"").unwrap(),
            TagAny::Bool(true)
        );
        assert_eq!(
            from_str::<TagAny>("!!bool \"false\"").unwrap(),
            TagAny::Bool(false)
        );
    }

    #[test]
    fn tag_null_overrides_quoting() {
        assert_eq!(from_str::<TagAny>("!!null \"null\"").unwrap(), TagAny::Unit);
        assert_eq!(from_str::<TagAny>("!!null ~").unwrap(), TagAny::Unit);
    }

    #[test]
    fn tag_binary_is_base64_decoded() {
        assert_eq!(
            from_str::<TagBytes>("!!binary SGVsbG8gV29ybGQh").unwrap(),
            TagBytes(b"Hello World!".to_vec())
        );
    }

    #[test]
    fn tag_binary_rejects_malformed_payloads() {
        // Must error rather than silently hand back the undecoded string.
        assert!(from_str::<TagBytes>("!!binary not@valid@base64").is_err());
    }

    #[test]
    fn tag_content_mismatch_is_an_error() {
        // A tag asserts a type. Content that cannot satisfy it is a defect in the
        // document, and failing loudly beats coercing to something arbitrary.
        assert!(from_str::<TagAny>("!!int \"abc\"").is_err());
        assert!(from_str::<TagAny>("!!float \"xyz\"").is_err());
        assert!(from_str::<TagAny>("!!bool \"maybe\"").is_err());
        assert!(from_str::<TagAny>("!!null \"nope\"").is_err());
    }

    #[test]
    fn non_core_schema_tags_fall_through_to_implicit_resolution() {
        // Only the six core-schema tags drive resolution. A `tag:yaml.org,2002:`
        // tag outside that set -- and any application tag -- must keep flowing
        // through normal resolution rather than being reinterpreted.
        assert_eq!(
            from_str::<TagAny>("!!timestamp 2024-01-01").unwrap(),
            TagAny::Text("2024-01-01".into())
        );
        assert_eq!(
            from_str::<TagAny>("!!seq 5").unwrap(),
            TagAny::Int(5),
            "an unrecognised core-schema tag must not suppress implicit resolution"
        );
    }

    #[test]
    fn tag_int_above_i64_max_uses_the_unsigned_visitor() {
        // `parse_unsigned` succeeds but the value does not fit `i64`, so the
        // deserialiser must widen rather than overflow or fall through.
        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(untagged)]
        enum BigInt {
            U(u64),
            Text(String),
        }

        assert_eq!(
            from_str::<BigInt>("!!int 18446744073709551615").unwrap(),
            BigInt::U(u64::MAX)
        );
    }

    #[test]
    fn tag_int_accepts_negative_values() {
        // `parse_unsigned` refuses a leading `-`, so this exercises the signed
        // fallback rather than the unsigned fast path above.
        assert_eq!(
            from_str::<TagAny>("!!int \"-42\"").unwrap(),
            TagAny::Int(-42)
        );
    }

    #[test]
    fn deserialize_bytes_on_a_non_scalar_is_an_error() {
        // `core_tag` has to answer for collections too, and a sequence carries no
        // scalar to decode.
        assert!(from_str::<TagBytes>("[1, 2]").is_err());
    }

    #[test]
    fn untagged_scalars_keep_todays_resolution() {
        // The fallback must be untouched for every document that carries no tag.
        assert_eq!(from_str::<TagAny>("123").unwrap(), TagAny::Int(123));
        assert_eq!(
            from_str::<TagAny>("\"123\"").unwrap(),
            TagAny::Text("123".into())
        );
        assert_eq!(from_str::<TagAny>("true").unwrap(), TagAny::Bool(true));
        assert_eq!(from_str::<TagAny>("null").unwrap(), TagAny::Unit);
    }

    // ─── YAML 1.2 float production ──────────────────────────────────

    #[test]
    fn bare_inf_and_nan_words_are_strings() {
        // YAML 1.2 §10.2 admits exactly `[-+]?\.(inf|Inf|INF)` and
        // `\.(nan|NaN|NAN)` -- the leading dot is mandatory, so the bare words
        // are plain strings. Rust's `f64::from_str` accepts all three spellings,
        // which is where the over-eager resolution came from.
        for word in [
            "inf",
            "Inf",
            "INF",
            "infinity",
            "Infinity",
            "INFINITY",
            "nan",
            "NaN",
            "NAN",
            "-inf",
            "+inf",
            "-infinity",
        ] {
            let as_string: String = from_str(word).unwrap();
            assert_eq!(as_string, word, "`{word}` must deserialise as a string");
            assert!(
                from_str::<f64>(word).is_err(),
                "`{word}` must not deserialise as a float"
            );
        }
    }

    #[test]
    fn bare_inf_is_a_string_under_dynamic_resolution() {
        // `from_str::<String>` goes through `deserialize_string`; the bug lives in
        // `deserialize_any`, which is the path a dynamically typed target takes.
        // An untagged enum routes through `deserialize_any`, so this pins the
        // classification chain itself rather than the typed shortcut.
        //
        // This is why the defect is worse than a plain type confusion: under
        // dynamic resolution `inf` became a float, and a float infinity has no
        // JSON representation -- serialising onward turns it into `null`. The
        // value does not merely change type, it disappears.
        #[derive(Deserialize, PartialEq, Debug)]
        #[serde(untagged)]
        enum Any {
            Float(f64),
            Text(String),
        }

        assert_eq!(
            from_str::<Any>("inf").unwrap(),
            Any::Text("inf".to_owned()),
            "`inf` must not be classified as a float by deserialize_any"
        );
        assert_eq!(from_str::<Any>("nan").unwrap(), Any::Text("nan".to_owned()));
        // The dotted form must still win the float arm.
        assert!(matches!(from_str::<Any>(".inf").unwrap(), Any::Float(f) if f.is_infinite()));
    }

    #[test]
    fn dotted_inf_and_nan_still_resolve_as_floats() {
        assert!(from_str::<f64>(".inf").unwrap().is_infinite());
        assert!(from_str::<f64>(".inf").unwrap().is_sign_positive());
        assert!(from_str::<f64>("+.inf").unwrap().is_infinite());
        assert!(from_str::<f64>("-.inf").unwrap().is_sign_negative());
        assert!(from_str::<f64>(".nan").unwrap().is_nan());
        assert!(from_str::<f64>(".NaN").unwrap().is_nan());
        assert!(from_str::<f64>(".INF").unwrap().is_infinite());
    }

    #[test]
    fn ordinary_floats_are_unaffected_by_the_digit_guard() {
        // Every float in the YAML 1.2 grammar contains at least one digit, which
        // is what makes the guard safe. These pin that down.
        assert!((from_str::<f64>("1.5").unwrap() - 1.5).abs() < f64::EPSILON);
        assert!((from_str::<f64>("-1.5e-3").unwrap() + 0.0015).abs() < 1e-12);
        assert!((from_str::<f64>("1e10").unwrap() - 1e10).abs() < f64::EPSILON);
        assert!((from_str::<f64>(".5").unwrap() - 0.5).abs() < f64::EPSILON);
        assert!(from_str::<f64>("0.0").unwrap().abs() < f64::EPSILON);
        assert!((from_str::<f64>("+3.25").unwrap() - 3.25).abs() < f64::EPSILON);
    }

    #[test]
    fn external_variant_tag_classification() {
        use glaucus_core::types::{Position, Span};
        let span = Span::point(Position::start());
        let tag = |v: &str| Tag {
            value: std::borrow::Cow::Owned(v.to_string()),
            span,
        };
        // A local single-`!` tag names an external enum variant.
        assert_eq!(external_variant_tag(&tag("!Variant")), Some("Variant"));
        // Bare `!` (empty name), core-schema `!!str`, and global URI tags are
        // data-type tags rather than variants → None. This exercises the
        // early-return branch that defers to normal scalar/mapping resolution.
        assert_eq!(external_variant_tag(&tag("!")), None);
        assert_eq!(external_variant_tag(&tag("!!str")), None);
        assert_eq!(external_variant_tag(&tag("tag:yaml.org,2002:int")), None);
    }

    #[test]
    fn de_bool_true() {
        let result: bool = from_str("true").unwrap();
        assert!(result);
    }

    #[test]
    fn de_bool_false() {
        let result: bool = from_str("false").unwrap();
        assert!(!result);
    }

    #[test]
    fn de_integer() {
        let result: i64 = from_str("42").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn de_negative_integer() {
        let result: i64 = from_str("-7").unwrap();
        assert_eq!(result, -7);
    }

    #[test]
    fn de_hex_integer() {
        let result: i64 = from_str("0x2A").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn de_octal_integer() {
        let result: i64 = from_str("0o52").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn de_unsigned() {
        let result: u32 = from_str("100").unwrap();
        assert_eq!(result, 100);
    }

    #[test]
    fn de_float() {
        let result: f64 = from_str("1.23").unwrap();
        assert!((result - 1.23).abs() < f64::EPSILON);
    }

    #[test]
    fn de_float_infinity() {
        let result: f64 = from_str(".inf").unwrap();
        assert!(result.is_infinite() && result.is_sign_positive());
    }

    #[test]
    fn de_float_neg_infinity() {
        let result: f64 = from_str("-.inf").unwrap();
        assert!(result.is_infinite() && result.is_sign_negative());
    }

    #[test]
    fn de_float_nan() {
        let result: f64 = from_str(".nan").unwrap();
        assert!(result.is_nan());
    }

    #[test]
    fn de_null_to_option_none() {
        let result: Option<String> = from_str("null").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn de_value_to_option_some() {
        let result: Option<String> = from_str("hello").unwrap();
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn de_tilde_to_option_none() {
        let result: Option<i32> = from_str("~").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn de_quoted_bool_is_string() {
        let result: String = from_str("'true'").unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn de_quoted_number_is_string() {
        let result: String = from_str("'42'").unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn de_vec() {
        let result: Vec<String> = from_str("- a\n- b\n- c").unwrap();
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn de_vec_integers() {
        let result: Vec<i32> = from_str("- 1\n- 2\n- 3").unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn de_simple_struct() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            host: String,
            port: u16,
        }
        let result: Config = from_str("host: localhost\nport: 8080").unwrap();
        assert_eq!(result.host, "localhost");
        assert_eq!(result.port, 8080);
    }

    #[test]
    fn de_nested_struct() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Database {
            host: String,
            port: u16,
        }
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            database: Database,
        }
        let yaml = "database:\n  host: db.example.com\n  port: 5432";
        let result: Config = from_str(yaml).unwrap();
        assert_eq!(result.database.host, "db.example.com");
        assert_eq!(result.database.port, 5432);
    }

    #[test]
    fn de_struct_with_vec() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            names: Vec<String>,
        }
        let yaml = "names:\n- alice\n- bob";
        let result: Config = from_str(yaml).unwrap();
        assert_eq!(result.names, vec!["alice", "bob"]);
    }

    #[test]
    fn de_option_field_present() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            name: Option<String>,
        }
        let result: Config = from_str("name: hello").unwrap();
        assert_eq!(result.name, Some("hello".to_string()));
    }

    #[test]
    fn de_option_field_null() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            name: Option<String>,
        }
        let result: Config = from_str("name: null").unwrap();
        assert_eq!(result.name, None);
    }

    #[test]
    fn de_unit_enum() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Color {
            Red,
            Blue,
        }
        let result: Color = from_str("Red").unwrap();
        assert_eq!(result, Color::Red);
    }

    #[test]
    fn de_newtype_enum() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Value {
            Text(String),
            Number(i32),
        }
        let result: Value = from_str("Text: hello").unwrap();
        assert_eq!(result, Value::Text("hello".to_string()));
    }

    #[test]
    fn de_struct_enum() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Shape {
            Circle { radius: f64 },
        }
        let yaml = "Circle:\n  radius: 3.5";
        let result: Shape = from_str(yaml).unwrap();
        assert_eq!(result, Shape::Circle { radius: 3.5 });
    }

    // ─── serde_yaml drop-in: `!Variant` external-tag form on input ──────

    #[test]
    fn de_external_tag_newtype_variant() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Value {
            Items(Vec<u32>),
        }
        // serde_yaml emits this for `Value::Items(vec![1])`.
        let result: Value = from_str("!Items\n- 1\n").unwrap();
        assert_eq!(result, Value::Items(vec![1]));
    }

    #[test]
    fn de_external_tag_newtype_scalar_variant() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Value {
            Number(i32),
        }
        // serde_yaml emits `!Number 42` for `Value::Number(42)`.
        let result: Value = from_str("!Number 42\n").unwrap();
        assert_eq!(result, Value::Number(42));
    }

    #[test]
    fn de_external_tag_tuple_variant() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Point {
            Coords(i32, i32),
        }
        let result: Point = from_str("!Coords\n- 1\n- 2\n").unwrap();
        assert_eq!(result, Point::Coords(1, 2));
    }

    #[test]
    fn de_external_tag_struct_variant() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Shape {
            Circle { radius: f64 },
        }
        let result: Shape = from_str("!Circle\nradius: 3.5\n").unwrap();
        assert_eq!(result, Shape::Circle { radius: 3.5 });
    }

    #[test]
    fn de_external_tag_and_mapping_forms_agree() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Shape {
            Circle { radius: f64 },
        }
        // Both glaucus's own mapping form and serde_yaml's tag form parse identically.
        let tagged: Shape = from_str("!Circle\nradius: 3.5\n").unwrap();
        let mapped: Shape = from_str("Circle:\n  radius: 3.5\n").unwrap();
        assert_eq!(tagged, mapped);
    }

    #[test]
    fn de_core_schema_tag_not_treated_as_variant() {
        // `!!str` is a core-schema type tag, not an external variant tag —
        // a tagged scalar must still deserialize as a unit variant by name.
        #[derive(Deserialize, PartialEq, Debug)]
        enum Color {
            Red,
        }
        let result: Color = from_str("!!str Red\n").unwrap();
        assert_eq!(result, Color::Red);
    }

    #[test]
    fn de_non_specific_tag_not_treated_as_variant() {
        // A bare `!` is YAML's non-specific tag (§7.2), not an external
        // variant name. `resolve_tag` yields the literal value "!", so
        // `external_variant_tag` strips the single `!` to an empty name and
        // returns `None` — the scalar must still resolve to a unit variant.
        #[derive(Deserialize, PartialEq, Debug)]
        enum Color {
            Red,
        }
        let result: Color = from_str("! Red\n").unwrap();
        assert_eq!(result, Color::Red);
    }

    #[test]
    fn de_char() {
        let result: char = from_str("a").unwrap();
        assert_eq!(result, 'a');
    }

    #[test]
    fn de_tuple() {
        let result: (i32, String) = from_str("- 42\n- hello").unwrap();
        assert_eq!(result, (42, "hello".to_string()));
    }

    #[test]
    fn de_serde_rename() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            #[serde(rename = "api-key")]
            api_key: String,
        }
        let result: Config = from_str("api-key: secret").unwrap();
        assert_eq!(result.api_key, "secret");
    }

    #[test]
    fn de_serde_default() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            host: String,
            #[serde(default)]
            port: u16,
        }
        let result: Config = from_str("host: localhost").unwrap();
        assert_eq!(result.host, "localhost");
        assert_eq!(result.port, 0);
    }

    #[test]
    fn de_hashmap() {
        use std::collections::HashMap;
        let result: HashMap<String, i32> = from_str("a: 1\nb: 2").unwrap();
        assert_eq!(result.get("a"), Some(&1));
        assert_eq!(result.get("b"), Some(&2));
    }

    #[test]
    fn de_error_type_mismatch() {
        let result: Result<i32, _> = from_str("not_a_number");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected integer"));
    }

    #[test]
    fn de_error_expected_sequence() {
        let result: Result<Vec<i32>, _> = from_str("hello");
        assert!(result.is_err());
    }

    #[test]
    fn de_with_lenient_allows_duplicate_keys() {
        use std::collections::HashMap;
        let config = glaucus_core::error::ParserConfig {
            strictness: glaucus_core::error::Strictness::Lenient,
            ..Default::default()
        };
        let result: HashMap<String, i32> = from_str_with("a: 1\na: 2", config).unwrap();
        // Lenient: last value wins
        assert_eq!(result.get("a"), Some(&2));
    }

    #[test]
    fn de_with_strict_rejects_duplicate_keys() {
        let config = glaucus_core::error::ParserConfig {
            strictness: glaucus_core::error::Strictness::Strict,
            ..Default::default()
        };
        let result: Result<std::collections::HashMap<String, i32>, _> =
            from_str_with("a: 1\na: 2", config);
        assert!(result.is_err());
    }

    #[test]
    fn de_reader_with_works() {
        let config = glaucus_core::error::ParserConfig::default();
        let reader = std::io::Cursor::new(b"hello: world");
        let result: std::collections::HashMap<String, String> =
            from_reader_with(reader, config).unwrap();
        assert_eq!(result.get("hello"), Some(&"world".to_string()));
    }

    #[test]
    fn de_multi_strings() {
        let result: Vec<String> = from_str_multi("---\nhello\n---\nworld\n").unwrap();
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn de_multi_structs() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Item {
            name: String,
        }
        let result: Vec<Item> = from_str_multi("---\nname: a\n---\nname: b\n").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "a");
        assert_eq!(result[1].name, "b");
    }

    #[test]
    fn de_multi_empty_input() {
        let result: Vec<String> = from_str_multi("").unwrap();
        assert!(result.is_empty());
    }

    // ─── Typed integer/float widths (drive deserialize_iN/uN/fN) ──────

    #[test]
    fn de_signed_widths() {
        assert_eq!(from_str::<i8>("5").unwrap(), 5i8);
        assert_eq!(from_str::<i16>("300").unwrap(), 300i16);
        assert_eq!(from_str::<i32>("70000").unwrap(), 70000i32);
        assert_eq!(from_str::<i64>("5").unwrap(), 5i64);
    }

    #[test]
    fn de_unsigned_widths() {
        assert_eq!(from_str::<u8>("5").unwrap(), 5u8);
        assert_eq!(from_str::<u16>("300").unwrap(), 300u16);
        assert_eq!(from_str::<u32>("70000").unwrap(), 70000u32);
        assert_eq!(from_str::<u64>("5").unwrap(), 5u64);
    }

    #[test]
    fn de_f32_width() {
        let v: f32 = from_str("1.5").unwrap();
        assert!((v - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn de_signed_overflow_errors() {
        assert!(from_str::<i8>("999").is_err());
        assert!(from_str::<i16>("99999").is_err());
        assert!(from_str::<i32>("9999999999").is_err());
    }

    #[test]
    fn de_unsigned_overflow_errors() {
        assert!(from_str::<u8>("999").is_err());
        assert!(from_str::<u16>("99999").is_err());
        assert!(from_str::<u32>("9999999999").is_err());
    }

    #[test]
    fn de_plus_prefixed_integer() {
        // '+' prefix arm of parse_integer.
        assert_eq!(from_str::<i64>("+42").unwrap(), 42);
    }

    #[test]
    fn de_sign_only_is_not_integer() {
        // A bare "+" is a plain scalar; as i64 it drives parse_integer, whose
        // empty-digits-after-sign branch returns None → error.
        assert!(from_str::<i64>("+").is_err());
        // As u64 it drives parse_unsigned's empty-digits branch.
        assert!(from_str::<u64>("+").is_err());
    }

    #[test]
    fn de_unsigned_rejects_negative() {
        // parse_unsigned early-returns None for a leading '-'.
        assert!(from_str::<u64>("-5").is_err());
    }

    #[test]
    fn de_unsigned_hex_and_octal() {
        assert_eq!(from_str::<u64>("0xFF").unwrap(), 255);
        assert_eq!(from_str::<u64>("0o17").unwrap(), 15);
    }

    #[test]
    fn de_bool_error_on_non_bool() {
        let r: Result<bool, _> = from_str("notbool");
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("expected boolean"));
    }

    // ─── deserialize_any branches via the Value bridge ────────────────

    #[test]
    fn de_any_null_yields_unit() {
        // deserialize_any → is_null → visit_unit.
        let v: crate::value::Value = from_str("null").unwrap();
        assert!(v.is_scalar());
    }

    #[test]
    fn de_any_large_u64_uses_visit_u64() {
        // Routing through the Value bridge forces deserialize_any; a value
        // greater than i64::MAX takes the visit_u64 branch (not visit_i64).
        let v: crate::value::Value = from_str("18446744073709551615").unwrap();
        assert_eq!(v.as_str(), Some("18446744073709551615"));
    }

    #[test]
    fn de_any_negative_int_and_float() {
        // Drive deserialize_any's visit_i64 (negative) and visit_f64 branches
        // through the Value bridge.
        let neg: crate::value::Value = from_str("-5").unwrap();
        assert_eq!(neg.as_str(), Some("-5"));
        let f: crate::value::Value = from_str("1.5").unwrap();
        assert_eq!(f.as_str(), Some("1.5"));
    }

    // ─── char / scalar errors ─────────────────────────────────────────

    #[test]
    fn de_char_multi_char_errors() {
        let r: Result<char, _> = from_str("ab");
        assert!(r.is_err());
        assert!(
            r.unwrap_err()
                .to_string()
                .contains("expected a single character")
        );
    }

    #[test]
    fn de_char_empty_errors() {
        // Empty quoted scalar → no chars → error.
        let r: Result<char, _> = from_str("''");
        assert!(r.is_err());
    }

    #[test]
    fn de_int_on_sequence_errors() {
        // scalar_value called on a Sequence → "expected a scalar value".
        let r: Result<i64, _> = from_str("- 1\n- 2");
        assert!(r.is_err());
    }

    // ─── bytes / byte_buf ──────────────────────────────────────────────

    #[test]
    fn de_bytes_via_visitor() {
        struct BytesVisitor;
        impl Visitor<'_> for BytesVisitor {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bytes")
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
                Ok(v.to_vec())
            }
        }
        let node = glaucus_ast::composer::Composer::new("hello")
            .next()
            .unwrap()
            .unwrap();
        let mut de = Deserializer::from_node(&node);
        let bytes = de::Deserializer::deserialize_bytes(&mut de, BytesVisitor).unwrap();
        assert_eq!(bytes, b"hello");

        let mut de2 = Deserializer::from_node(&node);
        let buf = de::Deserializer::deserialize_byte_buf(&mut de2, BytesVisitor).unwrap();
        assert_eq!(buf, b"hello");
    }

    // ─── unit / unit struct ────────────────────────────────────────────

    #[test]
    fn de_unit_from_null() {
        let _: () = from_str("null").unwrap();
    }

    #[test]
    fn de_unit_error_on_non_null() {
        let r: Result<(), _> = from_str("hello");
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("expected null"));
    }

    #[test]
    fn de_unit_struct_from_null() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Unit;
        let _: Unit = from_str("null").unwrap();
    }

    // ─── newtype struct ────────────────────────────────────────────────

    #[test]
    fn de_newtype_struct() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Meters(i32);
        let result: Meters = from_str("42").unwrap();
        assert_eq!(result, Meters(42));
    }

    // ─── tuple struct ──────────────────────────────────────────────────

    #[test]
    fn de_tuple_struct() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Pair(i32, String);
        let result: Pair = from_str("- 1\n- two").unwrap();
        assert_eq!(result, Pair(1, "two".to_string()));
    }

    // ─── map error ─────────────────────────────────────────────────────

    #[test]
    fn de_map_on_scalar_errors() {
        use std::collections::HashMap;
        let r: Result<HashMap<String, i32>, _> = from_str("hello");
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("expected a mapping"));
    }

    // ─── enum error branches ───────────────────────────────────────────

    #[test]
    fn de_enum_multi_key_mapping_errors() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum E {
            A(i32),
        }
        let r: Result<E, _> = from_str("A: 1\nB: 2");
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("single-key mapping"));
    }

    #[test]
    fn de_enum_sequence_errors() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum E {
            A(i32),
        }
        let r: Result<E, _> = from_str("- 1\n- 2");
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("found sequence"));
    }

    // ─── ignored_any (unknown fields) ──────────────────────────────────

    #[test]
    fn de_ignores_unknown_fields() {
        #[derive(Deserialize, PartialEq, Debug)]
        struct Config {
            host: String,
        }
        // `extra` is unknown → serde calls deserialize_ignored_any to skip it.
        let result: Config = from_str("host: localhost\nextra: ignored").unwrap();
        assert_eq!(result.host, "localhost");
    }

    // ─── tuple variant (enum) ──────────────────────────────────────────

    #[test]
    fn de_tuple_variant() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum Geom {
            Point(i32, i32),
        }
        let result: Geom = from_str("Point:\n- 3\n- 4").unwrap();
        assert_eq!(result, Geom::Point(3, 4));
    }

    // ─── VariantAccess error arms (Unit vs Value mismatch) ─────────────

    #[test]
    fn de_unit_variant_given_data_errors() {
        // A unit variant declared, but YAML provides a single-key mapping
        // (Value variant) → unit_variant() hits the Value error arm.
        #[derive(Deserialize, PartialEq, Debug)]
        enum E {
            Unit,
        }
        let r: Result<E, _> = from_str("Unit: 1");
        assert!(r.is_err());
    }

    #[test]
    fn de_newtype_variant_given_scalar_errors() {
        // Newtype variant declared, but YAML provides a plain scalar
        // (Unit variant access) → newtype_variant_seed hits the Unit error arm.
        #[derive(Deserialize, PartialEq, Debug)]
        enum E {
            N(i32),
        }
        let r: Result<E, _> = from_str("N");
        assert!(r.is_err());
    }

    #[test]
    fn de_tuple_variant_given_scalar_errors() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum E {
            T(i32, i32),
        }
        let r: Result<E, _> = from_str("T");
        assert!(r.is_err());
    }

    #[test]
    fn de_struct_variant_given_scalar_errors() {
        #[derive(Deserialize, PartialEq, Debug)]
        enum E {
            S { x: i32 },
        }
        let r: Result<E, _> = from_str("S");
        assert!(r.is_err());
    }

    // ─── EOF / from_reader paths ───────────────────────────────────────

    #[test]
    fn de_from_str_empty_input_errors() {
        // Empty stream → Composer::next() is None → UnexpectedEof.
        let r: Result<i32, _> = from_str("");
        assert!(r.is_err());
    }

    #[test]
    fn de_from_str_with_empty_input_errors() {
        let config = glaucus_core::error::ParserConfig::default();
        let r: Result<i32, _> = from_str_with("", config);
        assert!(r.is_err());
    }

    #[test]
    fn de_from_reader_works() {
        let reader = std::io::Cursor::new(b"hello: world");
        let result: std::collections::HashMap<String, String> = from_reader(reader).unwrap();
        assert_eq!(result.get("hello"), Some(&"world".to_string()));
    }

    #[test]
    fn de_from_reader_invalid_utf8_errors() {
        // Invalid UTF-8 bytes → read_to_string fails → I/O error path.
        let reader = std::io::Cursor::new([0xFF, 0xFE, 0x00]);
        let r: Result<String, _> = from_reader(reader);
        assert!(r.is_err());
    }

    #[test]
    fn de_from_reader_with_invalid_utf8_errors() {
        let config = glaucus_core::error::ParserConfig::default();
        let reader = std::io::Cursor::new([0xFF, 0xFE, 0x00]);
        let r: Result<String, _> = from_reader_with(reader, config);
        assert!(r.is_err());
    }

    #[test]
    fn de_multi_with_config() {
        let config = glaucus_core::error::ParserConfig {
            strictness: glaucus_core::error::Strictness::Lenient,
            ..Default::default()
        };
        let result: Vec<String> = from_str_multi_with("---\nhello\n---\nworld\n", config).unwrap();
        assert_eq!(result, vec!["hello", "world"]);
    }

    // ─── YAML 1.1 bool compatibility ──────────────────────────────────

    #[test]
    fn yaml_1_1_resolves_extended_booleans() {
        let cfg = glaucus_core::error::ParserConfig {
            yaml_1_1: true,
            ..Default::default()
        };
        assert!(from_str_with::<bool>("yes", cfg.clone()).unwrap());
        assert!(!from_str_with::<bool>("no", cfg.clone()).unwrap());
        assert!(from_str_with::<bool>("on", cfg.clone()).unwrap());
        assert!(!from_str_with::<bool>("off", cfg.clone()).unwrap());
        assert!(from_str_with::<bool>("Y", cfg).unwrap());
    }

    #[test]
    fn yaml_1_2_default_does_not_treat_yes_as_bool() {
        assert!(from_str::<bool>("yes").is_err());
        assert_eq!(from_str::<String>("yes").unwrap(), "yes");
    }

    #[test]
    fn norway_country_code_stays_string_in_1_2_bool_in_1_1() {
        assert_eq!(from_str::<String>("NO").unwrap(), "NO");
        let cfg = glaucus_core::error::ParserConfig {
            yaml_1_1: true,
            ..Default::default()
        };
        assert!(!from_str_with::<bool>("NO", cfg).unwrap());
    }

    /// An untagged enum forces serde to call `Deserializer::deserialize_any`,
    /// exercising every scalar-classification arm in that method (sequence,
    /// quoted string, null, bool, signed/unsigned integer, large unsigned,
    /// float, and plain-string fallback) — paths the typed `deserialize_*`
    /// fast-paths skip.
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(untagged)]
    enum Any {
        Bool(bool),
        Unsigned(u64),
        Signed(i64),
        Float(f64),
        Text(String),
        List(Vec<i64>),
        Map(std::collections::BTreeMap<String, i64>),
    }

    #[test]
    fn deserialize_any_classifies_bool() {
        assert_eq!(from_str::<Any>("true").unwrap(), Any::Bool(true));
        assert_eq!(from_str::<Any>("false").unwrap(), Any::Bool(false));
    }

    #[test]
    fn deserialize_any_classifies_integers() {
        // Small positive → fits i64.
        assert_eq!(from_str::<Any>("7").unwrap(), Any::Unsigned(7));
        // Negative → signed path.
        assert_eq!(from_str::<Any>("-7").unwrap(), Any::Signed(-7));
        // Larger than i64::MAX → u64 path.
        assert_eq!(
            from_str::<Any>("18446744073709551615").unwrap(),
            Any::Unsigned(u64::MAX)
        );
    }

    #[test]
    fn deserialize_any_classifies_float_and_seq() {
        assert_eq!(from_str::<Any>("1.5").unwrap(), Any::Float(1.5));
        assert_eq!(
            from_str::<Any>("[1, 2, 3]").unwrap(),
            Any::List(vec![1, 2, 3])
        );
    }

    #[test]
    fn deserialize_any_classifies_quoted_and_plain_strings() {
        // Quoted scalar → string even though it looks numeric.
        assert_eq!(
            from_str::<Any>("\"123\"").unwrap(),
            Any::Text("123".to_string())
        );
        // Plain non-numeric scalar → string fallback.
        assert_eq!(
            from_str::<Any>("hello").unwrap(),
            Any::Text("hello".to_string())
        );
    }

    #[test]
    fn deserialize_any_null_yields_unit_in_option() {
        // `~` is null; an Option deserializes via deserialize_option which,
        // for a present value, dispatches through deserialize_any → visit_unit.
        let v: Option<Any> = from_str("~").unwrap();
        assert_eq!(v, None);
    }

    /// A bare `~` deserialized directly into an untagged enum forces serde to
    /// buffer the value through `deserialize_any`, exercising the null →
    /// `visit_unit` arm. No `Any` variant accepts unit, so the result is an
    /// error — but the `deserialize_any` null branch is what we are covering.
    #[test]
    fn deserialize_any_null_branch_via_untagged() {
        assert!(
            from_str::<Any>("~").is_err(),
            "null has no matching untagged variant"
        );
    }

    /// Drives `deserialize_any` on a quoted scalar whose value borrows from the
    /// source (`Cow::Borrowed`), exercising the borrowed-quoted arm that emits
    /// `visit_str` rather than `visit_string`.
    #[test]
    fn deserialize_any_quoted_borrowed_scalar_uses_visit_str() {
        use glaucus_core::types::{Position, ScalarStyle, Span};
        use std::borrow::Cow;
        let node = Node::Scalar(glaucus_ast::node::Scalar {
            value: Cow::Borrowed("123"),
            tag: None,
            style: ScalarStyle::DoubleQuoted,
            span: Span::point(Position::start()),
        });
        let mut de = Deserializer::from_node(&node);
        // `Any` is untagged → routed through `deserialize_any`, hitting the
        // quoted-borrowed arm that calls `visit_str`. Quoted scalars are always
        // strings, even when they look numeric.
        let v: Any = Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(v, Any::Text("123".to_string()));
    }

    /// Drives `deserialize_any` on a sequence node (via the untagged enum,
    /// which buffers through `deserialize_any`) so the `Node::Sequence` arm of
    /// that method is exercised.
    #[test]
    fn deserialize_any_sequence_arm() {
        use glaucus_core::types::{CollectionStyle, Position, ScalarStyle, Span};
        use std::borrow::Cow;
        let span = Span::point(Position::start());
        let item = Node::Scalar(glaucus_ast::node::Scalar {
            value: Cow::Borrowed("1"),
            tag: None,
            style: ScalarStyle::Plain,
            span,
        });
        let node = Node::Sequence(glaucus_ast::node::Sequence {
            items: vec![item],
            tag: None,
            style: CollectionStyle::Flow,
            span,
        });
        let mut de = Deserializer::from_node(&node);
        // `Any` is untagged → serde buffers the value via `deserialize_any`,
        // hitting the `Node::Sequence` arm before re-dispatching to seq.
        let v: Any = Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(v, Any::List(vec![1]));
    }

    /// Drives `deserialize_any` on a mapping node (via the untagged enum) so the
    /// `Node::Mapping` arm of that method is exercised.
    #[test]
    fn deserialize_any_mapping_arm() {
        let v: Any = from_str("k: 5").unwrap();
        let mut expected = std::collections::BTreeMap::new();
        expected.insert("k".to_string(), 5);
        assert_eq!(v, Any::Map(expected));
    }

    #[test]
    fn from_str_value_direct_empty_input_is_null() {
        // Was an EOF error. Empty input is a null document, and the `Value` fast
        // path must agree with the ordinary path rather than keeping its own
        // answer for the same input.
        let v = from_str::<crate::value::Value>("").unwrap();
        assert_eq!(v.as_node().as_str(), Some(""), "empty input should be null");
    }

    #[test]
    fn from_str_with_value_fast_path_handles_empty_input() {
        let cfg = glaucus_core::error::ParserConfig::default();
        // Non-empty: exercises the Value fast path in from_str_with.
        let v = from_str_with::<crate::value::Value>("a: 1", cfg.clone()).unwrap();
        assert!(matches!(v.as_node(), Node::Mapping(_)));
        // Empty: a null document, matching from_str and the node API.
        let empty = from_str_with::<crate::value::Value>("", cfg).unwrap();
        assert_eq!(empty.as_node().as_str(), Some(""));
    }

    #[test]
    fn from_str_multi_value_fast_path() {
        // Multi-document stream into Vec<Value> exercises the fast path.
        let docs = from_str_multi::<crate::value::Value>("a: 1\n---\nb: 2\n").unwrap();
        assert_eq!(docs.len(), 2);
        assert!(matches!(docs[0].as_node(), Node::Mapping(_)));
        assert!(matches!(docs[1].as_node(), Node::Mapping(_)));
    }
}
