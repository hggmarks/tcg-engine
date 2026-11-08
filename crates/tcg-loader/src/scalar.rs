//! The scalar values of the files: a bool, an integer or a text, each keeping the kind that the
//! YAML scalar has. A quoted `"5"` is a text and an unquoted `5` is an integer; the engine checks
//! the kind against the declared field type without coercion.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::Deserialize;
use tcg_engine::ruleset_definition::FieldValue;

pub(crate) fn from_bool(value: bool) -> FieldValue {
    FieldValue::Bool(value)
}

pub(crate) fn from_i64(value: i64) -> FieldValue {
    FieldValue::Int(value)
}

pub(crate) fn from_u64<E: de::Error>(value: u64) -> Result<FieldValue, E> {
    i64::try_from(value)
        .map(FieldValue::Int)
        .map_err(|_| E::custom("the integer does not fit in 64 bits"))
}

pub(crate) fn from_str(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

/// A [`FieldValue`] read from a YAML scalar.
pub(crate) struct ScalarDto(pub(crate) FieldValue);

impl<'de> Deserialize<'de> for ScalarDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ScalarVisitor;

        impl Visitor<'_> for ScalarVisitor {
            type Value = ScalarDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a bool, an integer or a text")
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<ScalarDto, E> {
                Ok(ScalarDto(from_bool(value)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<ScalarDto, E> {
                Ok(ScalarDto(from_i64(value)))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<ScalarDto, E> {
                from_u64(value).map(ScalarDto)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<ScalarDto, E> {
                Ok(ScalarDto(from_str(value)))
            }
        }

        deserializer.deserialize_any(ScalarVisitor)
    }
}

/// A `u32` written as an unquoted integer. A quoted `"3"` is a text, which `serde-saphyr` would
/// otherwise coerce into a number when asked for one.
pub(crate) fn strict_u32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    struct UnsignedVisitor;

    impl Visitor<'_> for UnsignedVisitor {
        type Value = u32;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an unquoted non-negative integer")
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<u32, E> {
            u32::try_from(value).map_err(|_| E::custom("the integer does not fit in 32 bits"))
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<u32, E> {
            u32::try_from(value)
                .map_err(|_| E::custom("the integer must be between 0 and 4294967295"))
        }
    }

    deserializer.deserialize_any(UnsignedVisitor)
}

/// `max_copies`: an absent key is `None` (through `#[serde(default)]`), and a present key must be
/// an unquoted integer; `~` is an error, not `None`.
pub(crate) fn strict_present_u32<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    strict_u32(deserializer).map(Some)
}

/// A bool written as an unquoted `true` or `false`; a quoted `"true"` is a text.
pub(crate) fn strict_bool<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    struct BoolVisitor;

    impl Visitor<'_> for BoolVisitor {
        type Value = bool;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an unquoted `true` or `false`")
        }

        fn visit_bool<E: de::Error>(self, value: bool) -> Result<bool, E> {
            Ok(value)
        }
    }

    deserializer.deserialize_any(BoolVisitor)
}
