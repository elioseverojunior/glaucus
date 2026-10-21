// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A `serde::Deserializer` over a single parser scalar event.
//!
//! Fills a typed target without a `Node` tree existing.

use std::borrow::Cow;

use glaucus_core::types::{ScalarStyle, Tag, YamlVersion};
use serde::de::{self, Visitor};

use crate::de::{CoreTag, Resolved, classify_plain, visit_core_tagged_value};
use crate::error::Error;

fn err(msg: impl std::fmt::Display) -> Error {
    <Error as de::Error>::custom(msg)
}

/// One scalar event, ready to be deserialised.
///
/// Holds the event's `Cow` rather than a `&str` so a **borrowed** scalar can be
/// handed to serde as borrowed. That is the point of the whole epic: the text
/// already points into the input, and copying it here would give up the saving
/// streaming exists to capture.
pub(crate) struct ScalarDeserializer<'de> {
    value: Cow<'de, str>,
    style: ScalarStyle,
    tag: Option<Tag<'de>>,
    version: YamlVersion,
}

impl<'de> ScalarDeserializer<'de> {
    pub(crate) const fn new(
        value: Cow<'de, str>,
        style: ScalarStyle,
        tag: Option<Tag<'de>>,
        version: YamlVersion,
    ) -> Self {
        Self {
            value,
            style,
            tag,
            version,
        }
    }

    /// The core-schema tag this scalar carries, if any.
    fn core_tag(&self) -> Option<CoreTag> {
        self.tag.as_ref().and_then(|t| CoreTag::from_uri(&t.value))
    }

    /// Hands the text to serde, **borrowed where it is borrowed**.
    ///
    /// `visit_borrowed_str` is what lets a `#[serde(borrow)] &'de str` field
    /// deserialise without a copy. `visit_str` would compile and pass every test
    /// that does not borrow, while silently forfeiting the reason to stream — so
    /// the distinction is load-bearing rather than stylistic.
    fn visit_text<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Cow::Borrowed(s) => visitor.visit_borrowed_str(s),
            Cow::Owned(s) => visitor.visit_string(s),
        }
    }
}

impl ScalarDeserializer<'_> {
    /// The value as an integer, or the error the wide method would give.
    fn int(&self) -> Result<i64, Error> {
        glaucus_core::schema::resolve_int(&self.value)
            .ok_or_else(|| err(format!("expected integer, found `{}`", self.value)))
    }

    /// The value as an unsigned integer.
    fn uint(&self) -> Result<u64, Error> {
        glaucus_core::schema::resolve_uint(&self.value)
            .ok_or_else(|| err(format!("expected unsigned, found `{}`", self.value)))
    }

    /// The value as a float.
    fn float(&self) -> Result<f64, Error> {
        glaucus_core::schema::resolve_float(&self.value)
            .ok_or_else(|| err(format!("expected float, found `{}`", self.value)))
    }
}

impl<'de> de::Deserializer<'de> for ScalarDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        // An explicit core-schema tag overrides both implicit resolution and the
        // style shortcut: `!!int "456"` is the author overriding what the quotes
        // would otherwise imply.
        if let Some(tag) = self.core_tag() {
            return visit_core_tagged_value(tag, &self.value, self.version, visitor);
        }

        // Quoting is the author stating the value is text.
        if self.style != ScalarStyle::Plain {
            return self.visit_text(visitor);
        }

        // The ordering is shared with the tree path so the two cannot drift.
        match classify_plain(&self.value, self.version) {
            Resolved::Null => visitor.visit_unit(),
            Resolved::Bool(b) => visitor.visit_bool(b),
            Resolved::I64(i) => visitor.visit_i64(i),
            Resolved::U64(u) => visitor.visit_u64(u),
            Resolved::F64(f) => visitor.visit_f64(f),
            Resolved::Str => self.visit_text(visitor),
        }
    }

    /// The raw text, never classified.
    ///
    /// A `&str` target asked for text, so `1` is the one-character string rather
    /// than the integer — forwarding to `deserialize_any` would resolve it and
    /// fail the target.
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.visit_text(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.visit_text(visitor)
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match classify_plain(&self.value, self.version) {
            Resolved::Bool(b) => visitor.visit_bool(b),
            _ => Err(err(format!("expected boolean, found `{}`", self.value))),
        }
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i64(self.int()?)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u64(self.uint()?)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_f64(self.float()?)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.style == ScalarStyle::Plain && glaucus_core::schema::is_null(&self.value) {
            return visitor.visit_none();
        }
        visitor.visit_some(self)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.style == ScalarStyle::Plain && glaucus_core::schema::is_null(&self.value) {
            return visitor.visit_unit();
        }
        Err(err("expected null"))
    }

    // The narrow integer types resolve exactly as the wide ones do and then
    // narrow, so a value that does not fit reports the overflow rather than
    // being silently truncated.
    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let i = self.int()?;
        visitor.visit_i8(i.try_into().map_err(|_| err(format!("{i} overflows i8")))?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let i = self.int()?;
        visitor.visit_i16(
            i.try_into()
                .map_err(|_| err(format!("{i} overflows i16")))?,
        )
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let i = self.int()?;
        visitor.visit_i32(
            i.try_into()
                .map_err(|_| err(format!("{i} overflows i32")))?,
        )
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let u = self.uint()?;
        visitor.visit_u8(u.try_into().map_err(|_| err(format!("{u} overflows u8")))?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let u = self.uint()?;
        visitor.visit_u16(
            u.try_into()
                .map_err(|_| err(format!("{u} overflows u16")))?,
        )
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let u = self.uint()?;
        visitor.visit_u32(
            u.try_into()
                .map_err(|_| err(format!("{u} overflows u32")))?,
        )
    }

    #[allow(clippy::cast_possible_truncation)]
    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        // YAML has one float type; narrowing to the `f32` the caller asked for
        // is the whole point of this method, and matches what serde_json does.
        visitor.visit_f32(self.float()? as f32)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let mut chars = self.value.chars();
        let c = chars
            .next()
            .ok_or_else(|| err("expected a character, found empty string"))?;
        if chars.next().is_some() {
            return Err(err(format!(
                "expected a single character, found `{}`",
                self.value
            )));
        }
        visitor.visit_char(c)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        // `!!binary` is base64 in the source and bytes in the data model, so a
        // bytes-shaped target must get the decoded payload rather than the
        // encoding. Untagged scalars keep passing their UTF-8 through unchanged.
        if self.core_tag() == Some(CoreTag::Binary) {
            return visit_core_tagged_value(CoreTag::Binary, &self.value, self.version, visitor);
        }
        visitor.visit_bytes(self.value.as_bytes())
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_bytes(visitor)
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

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    // Collections and enums never reach here: `EventDeserializer` inspects the
    // event kind first and only builds a `ScalarDeserializer` for a scalar, so a
    // sequence-shaped request against a scalar is refused before this point.
    serde::forward_to_deserialize_any! {
        i128 u128 seq tuple tuple_struct map struct enum ignored_any
    }
}

#[cfg(test)]
mod tests {
    use super::ScalarDeserializer;
    use glaucus_core::types::{ScalarStyle, Tag, YamlVersion};
    use serde::Deserialize;
    use serde::de::Deserializer as _;
    use std::borrow::Cow;

    /// A plain scalar borrowed from the input, resolved under YAML 1.2.
    fn plain(value: &str) -> ScalarDeserializer<'_> {
        ScalarDeserializer::new(
            Cow::Borrowed(value),
            ScalarStyle::Plain,
            None,
            YamlVersion::V1_2,
        )
    }

    fn quoted(value: &str) -> ScalarDeserializer<'_> {
        ScalarDeserializer::new(
            Cow::Borrowed(value),
            ScalarStyle::DoubleQuoted,
            None,
            YamlVersion::V1_2,
        )
    }

    fn plain_1_1(value: &str) -> ScalarDeserializer<'_> {
        ScalarDeserializer::new(
            Cow::Borrowed(value),
            ScalarStyle::Plain,
            None,
            YamlVersion::V1_1,
        )
    }

    fn tagged<'a>(value: &'a str, uri: &'a str) -> ScalarDeserializer<'a> {
        ScalarDeserializer::new(
            Cow::Borrowed(value),
            ScalarStyle::Plain,
            Some(Tag {
                value: Cow::Borrowed(uri),
                span: glaucus_core::types::Span::point(glaucus_core::types::Position::start()),
            }),
            YamlVersion::V1_2,
        )
    }

    /// Routes through `deserialize_any`, which is where resolution happens.
    #[derive(Deserialize, PartialEq, Debug)]
    #[serde(untagged)]
    enum Any {
        Bool(bool),
        Int(i64),
        Float(f64),
        Text(String),
        Unit,
    }

    fn any(de: ScalarDeserializer<'_>) -> Any {
        Any::deserialize(de).expect("should deserialise")
    }

    // ─── the borrow, which is the point ─────────────────────────────

    /// A `&'de str` target only succeeds if `visit_borrowed_str` was used.
    /// `visit_str` would compile and pass every other test here while silently
    /// forfeiting the saving streaming exists to capture.
    #[test]
    fn a_borrowed_scalar_reaches_serde_borrowed() {
        let input = String::from("hello world");
        let borrowed: &str = <&str>::deserialize(plain(&input)).expect("must borrow");
        assert_eq!(borrowed, "hello world");
        assert!(
            std::ptr::eq(borrowed.as_ptr(), input.as_ptr()),
            "the &str must point INTO the input, not at a copy"
        );
    }

    #[test]
    fn an_owned_scalar_still_deserialises_as_a_string() {
        // A folded scalar arrives owned; it cannot be borrowed, and must not fail.
        let de = ScalarDeserializer::new(
            Cow::Owned("folded text".to_owned()),
            ScalarStyle::Plain,
            None,
            YamlVersion::V1_2,
        );
        assert_eq!(String::deserialize(de).unwrap(), "folded text");
    }

    // ─── resolution, through the shared core resolvers ──────────────

    #[test]
    fn radix_prefixes_resolve() {
        assert_eq!(any(plain("0x1F")), Any::Int(31));
        assert_eq!(any(plain("0X1f")), Any::Int(31));
        assert_eq!(any(plain("0o17")), Any::Int(15));
        assert_eq!(any(plain("-0x10")), Any::Int(-16));
        assert_eq!(
            any(plain("017")),
            Any::Int(17),
            "a bare leading zero is decimal"
        );
    }

    #[test]
    fn booleans_are_case_sensitive_and_version_aware() {
        assert_eq!(any(plain("true")), Any::Bool(true));
        assert_eq!(any(plain("TRUE")), Any::Bool(true));
        assert_eq!(any(plain("tRuE")), Any::Text("tRuE".into()));

        // 1.1 spellings are strings under 1.2 and booleans under 1.1.
        assert_eq!(any(plain("yes")), Any::Text("yes".into()));
        assert_eq!(any(plain_1_1("yes")), Any::Bool(true));
        assert_eq!(any(plain_1_1("no")), Any::Bool(false));
    }

    #[test]
    fn bare_inf_and_nan_stay_strings_dotted_forms_are_floats() {
        for word in ["inf", "Infinity", "nan", "NaN"] {
            assert_eq!(any(plain(word)), Any::Text(word.into()), "{word}");
        }
        assert!(matches!(any(plain(".inf")), Any::Float(f) if f.is_infinite()));
        assert!(
            matches!(any(plain("-.inf")), Any::Float(f) if f.is_infinite() && f.is_sign_negative())
        );
        assert!(matches!(any(plain(".nan")), Any::Float(f) if f.is_nan()));
    }

    #[test]
    fn null_resolves_and_options_see_none() {
        assert_eq!(any(plain("null")), Any::Unit);
        assert_eq!(any(plain("~")), Any::Unit);
        assert_eq!(Option::<String>::deserialize(plain("~")).unwrap(), None);
        assert_eq!(
            Option::<String>::deserialize(plain("text")).unwrap(),
            Some("text".to_owned())
        );
    }

    /// Quoting is the author stating the value is text.
    #[test]
    fn quoted_scalars_stay_strings_regardless_of_content() {
        for content in ["1", "true", "null", "0x1F", ".inf", ""] {
            assert_eq!(
                any(quoted(content)),
                Any::Text(content.into()),
                "{content:?} is quoted and must stay text"
            );
        }
        // A quoted null is not None.
        assert_eq!(
            Option::<String>::deserialize(quoted("~")).unwrap(),
            Some("~".to_owned())
        );
    }

    /// An explicit tag beats both implicit resolution and the style shortcut.
    #[test]
    fn core_schema_tags_drive_resolution() {
        assert_eq!(
            any(tagged("123", "tag:yaml.org,2002:str")),
            Any::Text("123".into())
        );
        assert_eq!(any(tagged("456", "tag:yaml.org,2002:int")), Any::Int(456));
        assert_eq!(
            any(tagged("true", "tag:yaml.org,2002:bool")),
            Any::Bool(true)
        );
        assert_eq!(any(tagged("null", "tag:yaml.org,2002:null")), Any::Unit);
        assert!(Any::deserialize(tagged("abc", "tag:yaml.org,2002:int")).is_err());
    }

    /// A `&str` target asked for TEXT, so `1` is the one-character string.
    /// Forwarding this to `deserialize_any` would resolve it and fail the target.
    #[test]
    fn a_str_target_is_never_classified() {
        assert_eq!(String::deserialize(plain("1")).unwrap(), "1");
        assert_eq!(String::deserialize(plain("true")).unwrap(), "true");
        assert_eq!(String::deserialize(plain("0x1F")).unwrap(), "0x1F");
    }

    #[test]
    fn typed_targets_use_the_shared_resolvers() {
        assert_eq!(i64::deserialize(plain("0x1F")).unwrap(), 31);
        assert_eq!(u64::deserialize(plain("0o17")).unwrap(), 15);
        assert!(
            u64::deserialize(plain("-1")).is_err(),
            "unsigned refuses a minus"
        );
        assert!((f64::deserialize(plain("1.5")).unwrap() - 1.5).abs() < f64::EPSILON);
        assert!(
            bool::deserialize(plain("yes")).is_err(),
            "not a 1.2 boolean"
        );
        assert!(bool::deserialize(plain_1_1("yes")).unwrap());
    }

    #[test]
    fn deserialize_unit_requires_a_plain_null() {
        assert!(
            plain("null")
                .deserialize_unit(serde::de::IgnoredAny)
                .is_ok()
        );
        assert!(
            plain("text")
                .deserialize_unit(serde::de::IgnoredAny)
                .is_err()
        );
        assert!(
            quoted("null")
                .deserialize_unit(serde::de::IgnoredAny)
                .is_err()
        );
    }

    // --- typed entry points ------------------------------------------------
    //
    // `value.rs` always routes through `deserialize_any`, so these are reached
    // only when a caller asks for a concrete numeric type directly. Their
    // failure arms had no coverage until #53 gave the impl its first real
    // caller and the compiler started emitting it.

    /// Reports which `visit_*` the deserialiser chose.
    ///
    /// `Any` cannot answer this: it has no `u64` variant, and adding one would
    /// change what every other test through it asserts.
    struct WhichVisit;

    impl serde::de::Visitor<'_> for WhichVisit {
        type Value = &'static str;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("any scalar")
        }

        fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
            Ok("i64")
        }

        fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
            Ok("u64")
        }
    }

    #[test]
    fn deserialize_any_picks_u64_only_beyond_i64_range() {
        // `u64::deserialize` would route through `deserialize_u64` and prove
        // nothing about the `deserialize_any` resolution ladder, which is the
        // path every collection element takes.
        assert_eq!(plain("1").deserialize_any(WhichVisit).unwrap(), "i64");
        assert_eq!(
            plain("9223372036854775807")
                .deserialize_any(WhichVisit)
                .unwrap(),
            "i64"
        );
        assert_eq!(
            plain("18446744073709551615")
                .deserialize_any(WhichVisit)
                .unwrap(),
            "u64"
        );
    }

    #[test]
    fn a_plain_scalar_beyond_i64_resolves_as_u64() {
        assert_eq!(
            u64::deserialize(plain("18446744073709551615")).unwrap(),
            u64::MAX
        );
    }

    #[test]
    fn asking_for_an_integer_rejects_non_integer_text() {
        let msg = plain("abc")
            .deserialize_i64(serde::de::IgnoredAny)
            .expect_err("non-integer text deserialized as i64")
            .to_string();
        assert!(msg.contains("expected integer, found `abc`"), "{msg}");
    }

    #[test]
    fn asking_for_a_float_rejects_non_float_text() {
        let msg = plain("abc")
            .deserialize_f64(serde::de::IgnoredAny)
            .expect_err("non-float text deserialized as f64")
            .to_string();
        assert!(msg.contains("expected float, found `abc`"), "{msg}");
    }

    // --- reachable only by direct use --------------------------------------
    //
    // `EventDeserializer` handles unit structs and newtype structs itself, and
    // only builds a `ScalarDeserializer` for a scalar. These stay implemented so
    // this remains a correct standalone `Deserializer`, and are called directly
    // so that claim is tested rather than assumed.

    #[test]
    fn an_untagged_scalar_hands_bytes_through_unchanged() {
        // Only `!!binary` is decoded; anything else is its own UTF-8. Reached
        // whenever a bytes-shaped target meets an ordinary scalar.
        #[derive(Debug, PartialEq)]
        struct Bytes(Vec<u8>);

        impl<'de> Deserialize<'de> for Bytes {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct Visit;

                impl serde::de::Visitor<'_> for Visit {
                    type Value = Bytes;

                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("bytes")
                    }

                    fn visit_bytes<E>(self, v: &[u8]) -> Result<Bytes, E> {
                        Ok(Bytes(v.to_vec()))
                    }
                }

                d.deserialize_bytes(Visit)
            }
        }

        assert_eq!(
            Bytes::deserialize(plain("abc")).unwrap(),
            Bytes(b"abc".to_vec())
        );
    }

    #[test]
    fn a_unit_struct_requires_a_plain_null() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Unit;

        assert_eq!(Unit::deserialize(plain("~")).unwrap(), Unit);
        assert!(Unit::deserialize(plain("1")).is_err());
    }

    #[test]
    fn a_newtype_struct_wraps_the_scalar() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Wrapper(i64);

        assert_eq!(Wrapper::deserialize(plain("7")).unwrap(), Wrapper(7));
    }
}
