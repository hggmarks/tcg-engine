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
