//! The private data-transfer types that mirror the card catalog file, and their conversion into the
//! engine's plain-data draft. The same rules as in `dto.rs` apply.

use std::fmt;

use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::Deserialize;
use tcg_engine::card_catalog::{
    CardCatalogDraft, CardDraft, CardEffectDraft, CatalogMetaDraft, ModifyDraft,
    TriggerMatcherDraft,
};
use tcg_engine::ruleset_definition::FieldValue;

use crate::dto::{some, EffectDto, ExpressionDto};
use crate::scalar::{self, ScalarDto};

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
    #[serde(deserialize_with = "scalar::strict_u32")]
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
    effects: Vec<CardEffectDto>,
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

/// A Card Effect, one element of `effects`: the keys of the triggered shape (`on`, `if`, `do`) and
/// of the continuous shape (`while`, `modify`), all optional. Which shape it is, and whether the
/// keys mix, is the engine's to decide; only an unknown key is a syntax error. A key that is
/// present must have a value, so `on:` with nothing after it is an error, not an absent key.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardEffectDto {
    #[serde(default, deserialize_with = "some")]
    on: Option<MatcherDto>,
    #[serde(default, rename = "if", deserialize_with = "some")]
    condition: Option<ExpressionDto>,
    #[serde(default, rename = "do", deserialize_with = "some")]
    body: Option<Vec<EffectDto>>,
    #[serde(default, rename = "while", deserialize_with = "some")]
    while_condition: Option<ExpressionDto>,
    #[serde(default, deserialize_with = "some")]
    modify: Option<ModifyDto>,
}

/// `modify: { field, amount }`; `amount` is an unquoted integer.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModifyDto {
    field: String,
    #[serde(deserialize_with = "scalar::strict_i64")]
    amount: i64,
}

/// `{}`: the value of a matcher that takes no argument.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyDto {}

/// The value of `on`: a single-key map, `{ enter: <zone> }` or `{ turn_start: {} }`. The key of any
/// other matcher is kept, so that the engine rejects it and a code frontend gets the same error.
enum MatcherDto {
    Enter(String),
    TurnStart,
    Other(String),
}

impl<'de> Deserialize<'de> for MatcherDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MatcherVisitor;

        impl<'de> Visitor<'de> for MatcherVisitor {
            type Value = MatcherDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping with exactly one key, the name of the matcher")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MatcherDto, A::Error> {
                let Some(key) = map.next_key::<String>()? else {
                    return Err(de::Error::custom("`on` must name a matcher"));
                };
                let matcher = match key.as_str() {
                    "enter" => MatcherDto::Enter(map.next_value()?),
                    "turn_start" => {
                        let EmptyDto {} = map.next_value()?;
                        MatcherDto::TurnStart
                    }
                    _ => {
                        map.next_value::<IgnoredAny>()?;
                        MatcherDto::Other(key)
                    }
                };
                if map.next_key::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("`on` must name exactly one matcher"));
                }
                Ok(matcher)
            }
        }

        deserializer.deserialize_map(MatcherVisitor)
    }
}

impl CardEffectDto {
    fn into_draft(self) -> CardEffectDraft {
        CardEffectDraft {
            on: self.on.map(|matcher| match matcher {
                MatcherDto::Enter(zone) => TriggerMatcherDraft::Enter(zone),
                MatcherDto::TurnStart => TriggerMatcherDraft::TurnStart,
                MatcherDto::Other(key) => TriggerMatcherDraft::Other(key),
            }),
            condition: self.condition.map(ExpressionDto::into_draft),
            body: self.body.map(EffectDto::into_drafts),
            while_condition: self.while_condition.map(ExpressionDto::into_draft),
            modify: self.modify.map(|modify| ModifyDraft {
                field: modify.field,
                amount: modify.amount,
            }),
        }
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
                        .map(CardEffectDto::into_draft)
                        .collect(),
                })
                .collect(),
        }
    }
}
