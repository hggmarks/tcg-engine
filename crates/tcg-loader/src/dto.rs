//! The private data-transfer types that mirror the ruleset file, and their conversion into the
//! engine's plain-data draft.
//!
//! Rules for every type here: `deny_unknown_fields`, no `flatten`, no `untagged`, no `HashMap`.
//! Anything where order matters is a `Vec`.

use std::fmt;

use crate::scalar;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::Deserialize;
use tcg_engine::ruleset_definition::{
    CardTypeDraft, DeckInputDraft, FieldDraft, FieldTypeDraft, FieldValue, InitializerDraft,
    InstanceStateDraft, MetaDraft, ParametersDraft, RulesetDraft, Visibility, ZoneDraft,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RulesetDto {
    meta: MetaDto,
    parameters: ParametersDto,
    zones: Vec<ZoneDto>,
    card_types: Vec<CardTypeDto>,
    inputs: Vec<InputDto>,
    // The behavior sections are not read, only noticed. `present` also counts an empty value.
    #[serde(default, deserialize_with = "present")]
    setup: bool,
    #[serde(default, deserialize_with = "present")]
    turn: bool,
    #[serde(default, deserialize_with = "present")]
    actions: bool,
    #[serde(default, deserialize_with = "present")]
    win_conditions: bool,
}

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    IgnoredAny::deserialize(deserializer)?;
    Ok(true)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetaDto {
    id: String,
    version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParametersDto {
    players: u32,
    starting_player: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneDto {
    id: String,
    visibility: VisibilityDto,
    ordered: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum VisibilityDto {
    Hidden,
    Owner,
    Public,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardTypeDto {
    id: String,
    fields: Vec<FieldDto>,
    instance_state: Vec<StateDto>,
    effects: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldDto {
    name: String,
    #[serde(rename = "type")]
    field_type: FieldTypeDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FieldTypeDto {
    Int,
    Bool,
    Text,
    // The spelling of the two types below is provisional until a ruleset needs them.
    Enum,
    EnumSet,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateDto {
    name: String,
    #[serde(rename = "type")]
    state_type: FieldTypeDto,
    init: InitDto,
}

/// `init: false`, `init: 5`, `init: "text"` or `init: { from_field: life }`. A scalar keeps its
/// YAML type; the engine checks it against the declared type without coercion.
enum InitDto {
    Literal(FieldValue),
    FromField(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FromFieldDto {
    from_field: String,
}

impl<'de> Deserialize<'de> for InitDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct InitVisitor;

        impl<'de> Visitor<'de> for InitVisitor {
            type Value = InitDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a bool, an integer, a text or a map `{ from_field: <name> }`")
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<InitDto, E> {
                Ok(InitDto::Literal(scalar::from_bool(value)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<InitDto, E> {
                Ok(InitDto::Literal(scalar::from_i64(value)))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<InitDto, E> {
                scalar::from_u64(value).map(InitDto::Literal)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<InitDto, E> {
                Ok(InitDto::Literal(scalar::from_str(value)))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<InitDto, A::Error> {
                // The derived struct reports an unknown key as `unknown_field`, with the key in
                // the error path.
                let dto = FromFieldDto::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(InitDto::FromField(dto.from_field))
            }
        }

        deserializer.deserialize_any(InitVisitor)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputDto {
    id: String,
    card_types: Vec<String>,
    size: u32,
    // An absent key means "no limit"; an explicit `~` is an error, which `Option<u32>` alone would
    // accept as `None`.
    #[serde(default, deserialize_with = "present_max_copies")]
    max_copies: Option<u32>,
    zone: String,
}

fn present_max_copies<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u32>, D::Error> {
    u32::deserialize(deserializer).map(Some)
}

impl RulesetDto {
    /// The first behavior section present, in the order setup, turn, actions, win_conditions.
    pub(crate) fn unsupported_section(&self) -> Option<&'static str> {
        [
            ("setup", self.setup),
            ("turn", self.turn),
            ("actions", self.actions),
            ("win_conditions", self.win_conditions),
        ]
        .into_iter()
        .find_map(|(name, present)| present.then_some(name))
    }

    pub(crate) fn into_draft(self) -> RulesetDraft {
        RulesetDraft {
            meta: MetaDraft {
                id: self.meta.id,
                version: self.meta.version,
            },
            parameters: ParametersDraft {
                players: self.parameters.players,
                starting_player: self.parameters.starting_player,
            },
            zones: self.zones.into_iter().map(ZoneDto::into_draft).collect(),
            card_types: self
                .card_types
                .into_iter()
                .map(CardTypeDto::into_draft)
                .collect(),
            inputs: self.inputs.into_iter().map(InputDto::into_draft).collect(),
        }
    }
}

impl ZoneDto {
    fn into_draft(self) -> ZoneDraft {
        ZoneDraft {
            id: self.id,
            visibility: match self.visibility {
                VisibilityDto::Hidden => Visibility::Hidden,
                VisibilityDto::Owner => Visibility::Owner,
                VisibilityDto::Public => Visibility::Public,
            },
            ordered: self.ordered,
        }
    }
}

impl FieldTypeDto {
    fn into_draft(self) -> FieldTypeDraft {
        match self {
            Self::Int => FieldTypeDraft::Int,
            Self::Bool => FieldTypeDraft::Bool,
            Self::Text => FieldTypeDraft::Text,
            Self::Enum => FieldTypeDraft::Enum,
            Self::EnumSet => FieldTypeDraft::EnumSet,
        }
    }
}

impl CardTypeDto {
    fn into_draft(self) -> CardTypeDraft {
        CardTypeDraft {
            id: self.id,
            fields: self
                .fields
                .into_iter()
                .map(|field| FieldDraft {
                    name: field.name,
                    field_type: field.field_type.into_draft(),
                })
                .collect(),
            instance_state: self
                .instance_state
                .into_iter()
                .map(|state| InstanceStateDraft {
                    name: state.name,
                    state_type: state.state_type.into_draft(),
                    init: match state.init {
                        InitDto::Literal(value) => InitializerDraft::Literal(value),
                        InitDto::FromField(name) => InitializerDraft::FromField(name),
                    },
                })
                .collect(),
            effects: self.effects,
        }
    }
}

impl InputDto {
    fn into_draft(self) -> DeckInputDraft {
        DeckInputDraft {
            id: self.id,
            card_types: self.card_types,
            size: self.size,
            max_copies: self.max_copies,
            zone: self.zone,
        }
    }
}
