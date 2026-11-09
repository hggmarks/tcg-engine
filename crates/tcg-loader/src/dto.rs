//! The private data-transfer types that mirror the ruleset file, and their conversion into the
//! engine's plain-data draft.
//!
//! Rules for every type here: `deny_unknown_fields`, no `flatten`, no `untagged`, no `HashMap`.
//! Anything where order matters is a `Vec`.

use std::fmt;

use crate::scalar::{self, ScalarDto};
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::Deserialize;
use tcg_engine::ruleset_definition::{
    ActionDraft, AutomaticStepDraft, CardTypeDraft, ConditionDraft, DeckInputDraft, EffectDraft,
    FieldDraft, FieldTypeDraft, FieldValue, InitializerDraft, InstanceStateDraft, MetaDraft,
    ParametersDraft, PhaseDraft, RulesetDraft, SetupStepDraft, TurnDraft, Visibility, ZoneDraft,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RulesetDto {
    meta: MetaDto,
    parameters: ParametersDto,
    zones: Vec<ZoneDto>,
    card_types: Vec<CardTypeDto>,
    inputs: Vec<InputDto>,
    #[serde(default)]
    setup: Vec<SetupStepDto>,
    #[serde(default)]
    actions: Vec<ActionDto>,
    // An absent key is `None`; a present key must be a turn, so `turn:` with nothing after it is an
    // error, not `None`.
    #[serde(default, deserialize_with = "some")]
    turn: Option<TurnDto>,
    // `win_conditions` is not read, only noticed. `present` also counts an empty value.
    #[serde(default, deserialize_with = "present")]
    win_conditions: bool,
}

/// A value of a key that is present: unlike `Option`, a null is not accepted as `None`.
fn some<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    IgnoredAny::deserialize(deserializer)?;
    Ok(true)
}

/// One Setup Step: a single-key map `{ <primitive>: { ... } }`. An unknown primitive is an unknown
/// key of the list element.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SetupStepDto {
    Shuffle(ShuffleDto),
    Draw(DrawDto),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShuffleDto {
    player: String,
    zone: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrawDto {
    player: String,
    from: String,
    to: String,
    #[serde(deserialize_with = "scalar::strict_u32")]
    count: u32,
}

/// `{}`: the argument of a primitive that has none.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyDto {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionDto {
    id: String,
    effects: Vec<EffectDto>,
}

/// One effect of an action: a single-key map `{ <primitive>: { ... } }`.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum EffectDto {
    EndTurn(EmptyDto),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnDto {
    phases: Vec<PhaseDto>,
}

/// A phase has `automatic` (a list of steps) or `actions` (a list of action ids); the engine checks
/// that it has exactly one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PhaseDto {
    id: String,
    #[serde(default, deserialize_with = "some")]
    automatic: Option<Vec<AutomaticStepDto>>,
    #[serde(default, deserialize_with = "some")]
    actions: Option<Vec<String>>,
}

/// The condition of an `if` step: a single-key map `{ <condition>: {} }`.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConditionDto {
    FirstTurn(EmptyDto),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UntapDto {
    player: String,
    zones: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetStateDto {
    player: String,
    zone: String,
    state: String,
    value: ScalarDto,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveDto {
    player: String,
    from: String,
    to: String,
    #[serde(deserialize_with = "scalar::strict_u32")]
    count: u32,
    position: String,
}

/// One step of an automatic phase: either a single-key map `{ <primitive>: { ... } }`, or the
/// sibling-key form `{ if: <condition>, then: [...], else: [...] }` where `then` is required and
/// `else` is optional.
enum AutomaticStepDto {
    Untap(UntapDto),
    SetState(SetStateDto),
    Draw(DrawDto),
    Move(MoveDto),
    If {
        condition: ConditionDto,
        then: Vec<AutomaticStepDto>,
        otherwise: Vec<AutomaticStepDto>,
    },
}

/// The keys a step may have, for the error of an unknown one.
const STEP_KEYS: &[&str] = &["untap", "set_state", "draw", "move", "if", "then", "else"];

impl<'de> Deserialize<'de> for AutomaticStepDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StepVisitor;

        impl<'de> Visitor<'de> for StepVisitor {
            type Value = AutomaticStepDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a step: one primitive, or `if` with `then` and optionally `else`")
            }

            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<AutomaticStepDto, A::Error> {
                let mut primitive: Option<AutomaticStepDto> = None;
                let mut condition: Option<ConditionDto> = None;
                let mut then: Option<Vec<AutomaticStepDto>> = None;
                let mut otherwise: Option<Vec<AutomaticStepDto>> = None;

                // An unknown key is an unknown variant at the list element, like an unknown Setup
                // Step primitive.
                while let Some(key) = map.next_key::<String>()? {
                    let found = match key.as_str() {
                        "untap" => Some(AutomaticStepDto::Untap(map.next_value()?)),
                        "set_state" => Some(AutomaticStepDto::SetState(map.next_value()?)),
                        "draw" => Some(AutomaticStepDto::Draw(map.next_value()?)),
                        "move" => Some(AutomaticStepDto::Move(map.next_value()?)),
                        "if" => {
                            condition = Some(map.next_value()?);
                            None
                        }
                        "then" => {
                            then = Some(map.next_value()?);
                            None
                        }
                        "else" => {
                            otherwise = Some(map.next_value()?);
                            None
                        }
                        other => return Err(de::Error::unknown_variant(other, STEP_KEYS)),
                    };
                    if let Some(found) = found {
                        if primitive.replace(found).is_some() {
                            return Err(de::Error::custom("a step names only one primitive"));
                        }
                    }
                }

                match (primitive, condition, then, otherwise) {
                    (Some(primitive), None, None, None) => Ok(primitive),
                    (None, Some(condition), Some(then), otherwise) => Ok(AutomaticStepDto::If {
                        condition,
                        then,
                        otherwise: otherwise.unwrap_or_default(),
                    }),
                    (None, Some(_), None, _) => {
                        Err(de::Error::custom("an `if` step needs a `then` list"))
                    }
                    (Some(_), Some(_), _, _) => Err(de::Error::custom(
                        "an `if` step cannot also name a primitive",
                    )),
                    (None, None, Some(_), _) | (None, None, None, Some(_)) => Err(
                        de::Error::custom("`then` and `else` belong to an `if` step"),
                    ),
                    (Some(_), None, _, _) => Err(de::Error::custom(
                        "`then` and `else` belong to an `if` step",
                    )),
                    (None, None, None, None) => {
                        Err(de::Error::custom("a step names a primitive or an `if`"))
                    }
                }
            }
        }

        deserializer.deserialize_map(StepVisitor)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetaDto {
    id: String,
    #[serde(deserialize_with = "scalar::strict_u32")]
    version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParametersDto {
    #[serde(deserialize_with = "scalar::strict_u32")]
    players: u32,
    #[serde(deserialize_with = "scalar::strict_u32")]
    starting_player: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneDto {
    id: String,
    visibility: VisibilityDto,
    #[serde(deserialize_with = "scalar::strict_bool")]
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
    #[serde(deserialize_with = "scalar::strict_bool")]
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
    #[serde(deserialize_with = "scalar::strict_u32")]
    size: u32,
    // An absent key means "no limit"; an explicit `~` is an error, which `Option<u32>` alone would
    // accept as `None`.
    #[serde(default, deserialize_with = "scalar::strict_present_u32")]
    max_copies: Option<u32>,
    zone: String,
}

impl RulesetDto {
    /// The unsupported behavior section present, if any: only `win_conditions` is still not
    /// loaded.
    pub(crate) fn unsupported_section(&self) -> Option<&'static str> {
        self.win_conditions.then_some("win_conditions")
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
            setup: self
                .setup
                .into_iter()
                .map(SetupStepDto::into_draft)
                .collect(),
            actions: self
                .actions
                .into_iter()
                .map(ActionDto::into_draft)
                .collect(),
            turn: self.turn.map(TurnDto::into_draft),
        }
    }
}

impl ActionDto {
    fn into_draft(self) -> ActionDraft {
        ActionDraft {
            id: self.id,
            effects: self
                .effects
                .into_iter()
                .map(|effect| match effect {
                    EffectDto::EndTurn(_) => EffectDraft::EndTurn,
                })
                .collect(),
        }
    }
}

impl TurnDto {
    fn into_draft(self) -> TurnDraft {
        TurnDraft {
            phases: self.phases.into_iter().map(PhaseDto::into_draft).collect(),
        }
    }
}

impl PhaseDto {
    fn into_draft(self) -> PhaseDraft {
        PhaseDraft {
            id: self.id,
            automatic: self.automatic.map(AutomaticStepDto::into_drafts),
            actions: self.actions,
        }
    }
}

impl AutomaticStepDto {
    fn into_drafts(steps: Vec<AutomaticStepDto>) -> Vec<AutomaticStepDraft> {
        steps
            .into_iter()
            .map(AutomaticStepDto::into_draft)
            .collect()
    }

    fn into_draft(self) -> AutomaticStepDraft {
        match self {
            Self::Untap(step) => AutomaticStepDraft::Untap {
                player: step.player,
                zones: step.zones,
            },
            Self::SetState(step) => AutomaticStepDraft::SetState {
                player: step.player,
                zone: step.zone,
                state: step.state,
                value: step.value.0,
            },
            Self::Draw(step) => AutomaticStepDraft::Draw {
                player: step.player,
                from: step.from,
                to: step.to,
                count: step.count,
            },
            Self::Move(step) => AutomaticStepDraft::Move {
                player: step.player,
                from: step.from,
                to: step.to,
                count: step.count,
                position: step.position,
            },
            Self::If {
                condition,
                then,
                otherwise,
            } => AutomaticStepDraft::If {
                condition: match condition {
                    ConditionDto::FirstTurn(_) => ConditionDraft::FirstTurn,
                },
                then: Self::into_drafts(then),
                otherwise: Self::into_drafts(otherwise),
            },
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

impl SetupStepDto {
    fn into_draft(self) -> SetupStepDraft {
        match self {
            Self::Shuffle(step) => SetupStepDraft::Shuffle {
                player: step.player,
                zone: step.zone,
            },
            Self::Draw(step) => SetupStepDraft::Draw {
                player: step.player,
                from: step.from,
                to: step.to,
                count: step.count,
            },
        }
    }
}
