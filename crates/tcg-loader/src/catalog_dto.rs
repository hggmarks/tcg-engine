//! The private data-transfer types that mirror the card catalog file, and their conversion into the
//! engine's plain-data draft. The same rules as in `dto.rs` apply.

use std::fmt;

use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::Deserialize;
use tcg_engine::card_catalog::{CardCatalogDraft, CardDraft, CatalogMetaDraft, EffectDraft};
use tcg_engine::ruleset_definition::FieldValue;

use crate::scalar::ScalarDto;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogDto {
    meta: CatalogMetaDto,
    cards: Vec<CardDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogMetaDto {
    ruleset: String,
    ruleset_version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardDto {
    id: String,
    #[serde(rename = "type")]
    card_type: String,
    fields: FieldsDto,
    #[serde(default)]
    effects: Vec<EffectDto>,
}

/// The `fields` mapping, read into a list so that the order of the file is kept.
struct FieldsDto(Vec<(String, FieldValue)>);

impl<'de> Deserialize<'de> for FieldsDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldsVisitor;

        impl<'de> Visitor<'de> for FieldsVisitor {
            type Value = FieldsDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping from field names to bools, integers or texts")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<FieldsDto, A::Error> {
                let mut fields = Vec::new();
                while let Some(name) = map.next_key::<String>()? {
                    let value: ScalarDto = map.next_value()?;
                    fields.push((name, value.0));
                }
                Ok(FieldsDto(fields))
            }
        }

        deserializer.deserialize_map(FieldsVisitor)
    }
}

/// An effect, `{ <primitive>: <arguments> }`. No primitive exists yet, so only the name is kept and
/// the arguments are not read.
struct EffectDto(String);

impl<'de> Deserialize<'de> for EffectDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EffectVisitor;

        impl<'de> Visitor<'de> for EffectVisitor {
            type Value = EffectDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping with exactly one key, the name of the effect")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<EffectDto, A::Error> {
                let Some(primitive) = map.next_key::<String>()? else {
                    return Err(de::Error::custom("an effect must name a primitive"));
                };
                map.next_value::<IgnoredAny>()?;
                if map.next_key::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom(
                        "an effect must name exactly one primitive",
                    ));
                }
                Ok(EffectDto(primitive))
            }
        }

        deserializer.deserialize_map(EffectVisitor)
    }
}

impl CatalogDto {
    pub(crate) fn into_draft(self) -> CardCatalogDraft {
        CardCatalogDraft {
            meta: CatalogMetaDraft {
                ruleset: self.meta.ruleset,
                ruleset_version: self.meta.ruleset_version,
            },
            cards: self
                .cards
                .into_iter()
                .map(|card| CardDraft {
                    id: card.id,
                    card_type: card.card_type,
                    fields: card.fields.0,
                    effects: card
                        .effects
                        .into_iter()
                        .map(|effect| EffectDraft {
                            primitive: effect.0,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
