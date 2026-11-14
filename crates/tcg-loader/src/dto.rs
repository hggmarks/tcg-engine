//! The private data-transfer types that mirror the ruleset file, and their conversion into the
//! engine's plain-data draft.
//!
//! Rules for every type here: `deny_unknown_fields`, no `flatten`, no `untagged`, no `HashMap`.
//! Anything where order matters is a `Vec`.

use std::fmt;

use crate::scalar::{self, ScalarDto};
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use tcg_engine::ruleset_definition::{
    ActionDraft, AutomaticStepDraft, CardTypeDraft, ConditionDraft, CostDraft, DeckInputDraft,
    EffectDraft, EffectsDraft, ExpressionDraft, FieldDraft, FieldTypeDraft, FieldValue,
    InitializerDraft, InstanceStateDraft, MetaDraft, ParamAlternativeDraft, ParamCardsDraft,
    ParamDraft, ParametersDraft, PhaseDraft, RefDraft, RulesetDraft, SetupStepDraft, TurnDraft,
    Visibility, WinConditionDraft, WinConditionWhenDraft, ZoneDraft,
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
    #[serde(default)]
    win_conditions: Vec<WinConditionDto>,
    // An absent key is `None`; a present key must be a block.
    #[serde(default, deserialize_with = "some")]
    effects: Option<EffectsDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectsDto {
    active_zone: String,
}

/// A value of a key that is present: unlike `Option`, a null is not accepted as `None`.
pub(crate) fn some<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WinConditionDto {
    id: String,
    when: WinConditionWhenDto,
    loser: String,
}

/// The `when` of a win condition: a single-key map. `draw_from_empty: <zone>` is the one condition
/// that is not an expression; any other key is the operator of an expression, and an unknown
/// operator is an unknown key of the map.
enum WinConditionWhenDto {
    DrawFromEmpty(String),
    Expression(ExpressionDto),
}

impl<'de> Deserialize<'de> for WinConditionWhenDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct WhenVisitor;

        impl<'de> Visitor<'de> for WhenVisitor {
            type Value = WinConditionWhenDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("`draw_from_empty: <zone>` or an expression")
            }

            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<WinConditionWhenDto, A::Error> {
                let Some(key) = map.next_key::<String>()? else {
                    return Err(de::Error::custom("a `when` names one condition"));
                };
                if key == "draw_from_empty" {
                    let zone = map.next_value()?;
                    only_one_key(&mut map, "a `when` names one condition")?;
                    return Ok(WinConditionWhenDto::DrawFromEmpty(zone));
                }
                read_operator(&key, &mut map).map(WinConditionWhenDto::Expression)
            }

            // A scalar is an expression too: validation rejects what is not a bool.
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<WinConditionWhenDto, E> {
                Ok(literal_when(scalar::from_bool(value)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<WinConditionWhenDto, E> {
                Ok(literal_when(scalar::from_i64(value)))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<WinConditionWhenDto, E> {
                scalar::from_u64(value).map(literal_when)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<WinConditionWhenDto, E> {
                Ok(literal_when(scalar::from_str(value)))
            }
        }

        deserializer.deserialize_any(WhenVisitor)
    }
}

fn literal_when(value: FieldValue) -> WinConditionWhenDto {
    WinConditionWhenDto::Expression(ExpressionDto::Literal(value))
}

/// Fails if the map has another key: a single-key map names one operator.
fn only_one_key<'de, A: MapAccess<'de>>(
    map: &mut A,
    message: &'static str,
) -> Result<(), A::Error> {
    if map.next_key::<IgnoredAny>()?.is_some() {
        return Err(de::Error::custom(message));
    }
    Ok(())
}

/// An expression: a literal scalar, `{ ref: <text> }`, or a single-key map with an operator
/// (`eq`, `gte`, `lte` over two expressions, `and` over a list, `is_card_type` over a card ref and
/// a card type id). The operands of `eq`, `gte`, `lte` and `is_card_type` are exactly two. An
/// unknown operator is an unknown key of the map. No `untagged`: a visitor reads the shape.
pub(crate) enum ExpressionDto {
    Literal(FieldValue),
    Ref(String),
    Eq(Box<ExpressionDto>, Box<ExpressionDto>),
    Gte(Box<ExpressionDto>, Box<ExpressionDto>),
    Lte(Box<ExpressionDto>, Box<ExpressionDto>),
    And(Vec<ExpressionDto>),
    IsCardType(String, String),
}

/// `ref` and the operators, for the error of an unknown one.
const EXPRESSION_KEYS: &[&str] = &["ref", "eq", "gte", "lte", "and", "is_card_type"];

/// Reads the value of the operator `key`, whose key was just read from `map`, and checks that it
/// was the only key.
fn read_operator<'de, A: MapAccess<'de>>(
    key: &str,
    map: &mut A,
) -> Result<ExpressionDto, A::Error> {
    let expression = match key {
        "ref" => ExpressionDto::Ref(map.next_value()?),
        "eq" => {
            let Pair(left, right) = map.next_value()?;
            ExpressionDto::Eq(Box::new(left), Box::new(right))
        }
        "gte" => {
            let Pair(left, right) = map.next_value()?;
            ExpressionDto::Gte(Box::new(left), Box::new(right))
        }
        "lte" => {
            let Pair(left, right) = map.next_value()?;
            ExpressionDto::Lte(Box::new(left), Box::new(right))
        }
        "and" => ExpressionDto::And(map.next_value()?),
        "is_card_type" => {
            let CardTypeTest(card, card_type) = map.next_value()?;
            ExpressionDto::IsCardType(card, card_type)
        }
        other => return Err(de::Error::unknown_variant(other, EXPRESSION_KEYS)),
    };
    only_one_key(map, "an expression names one operator")?;
    Ok(expression)
}

impl<'de> Deserialize<'de> for ExpressionDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ExpressionVisitor;

        impl<'de> Visitor<'de> for ExpressionVisitor {
            type Value = ExpressionDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an expression: a bool, an integer, a text or a map with an operator")
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<ExpressionDto, E> {
                Ok(ExpressionDto::Literal(scalar::from_bool(value)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<ExpressionDto, E> {
                Ok(ExpressionDto::Literal(scalar::from_i64(value)))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<ExpressionDto, E> {
                scalar::from_u64(value).map(ExpressionDto::Literal)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<ExpressionDto, E> {
                Ok(ExpressionDto::Literal(scalar::from_str(value)))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ExpressionDto, A::Error> {
                let Some(key) = map.next_key::<String>()? else {
                    return Err(de::Error::custom("an expression names one operator"));
                };
                read_operator(&key, &mut map)
            }
        }

        deserializer.deserialize_any(ExpressionVisitor)
    }
}

/// The two operands of `eq`, `gte` and `lte`: a list of exactly two expressions.
struct Pair(ExpressionDto, ExpressionDto);

impl<'de> Deserialize<'de> for Pair {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PairVisitor;

        impl<'de> Visitor<'de> for PairVisitor {
            type Value = Pair;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of exactly two operands")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Pair, A::Error> {
                let left = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let right = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                if seq.next_element::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(3, &self));
                }
                Ok(Pair(left, right))
            }
        }

        deserializer.deserialize_seq(PairVisitor)
    }
}

/// The operands of `is_card_type`: a list of exactly a card ref and a card type id.
struct CardTypeTest(String, String);

impl<'de> Deserialize<'de> for CardTypeTest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TestVisitor;

        impl<'de> Visitor<'de> for TestVisitor {
            type Value = CardTypeTest;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of a card ref and a card type id")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<CardTypeTest, A::Error> {
                let card: RefDto = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let card_type = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                if seq.next_element::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(3, &self));
                }
                Ok(CardTypeTest(card.reference, card_type))
            }
        }

        deserializer.deserialize_seq(TestVisitor)
    }
}

impl ExpressionDto {
    pub(crate) fn into_draft(self) -> ExpressionDraft {
        let boxed = |expression: Box<ExpressionDto>| Box::new(expression.into_draft());
        match self {
            Self::Literal(value) => ExpressionDraft::Literal(value),
            Self::Ref(reference) => ExpressionDraft::Ref(RefDraft(reference)),
            Self::Eq(left, right) => ExpressionDraft::Eq(boxed(left), boxed(right)),
            Self::Gte(left, right) => ExpressionDraft::Gte(boxed(left), boxed(right)),
            Self::Lte(left, right) => ExpressionDraft::Lte(boxed(left), boxed(right)),
            Self::And(operands) => ExpressionDraft::And(
                operands
                    .into_iter()
                    .map(ExpressionDto::into_draft)
                    .collect(),
            ),
            Self::IsCardType(card, card_type) => ExpressionDraft::IsCardType {
                card: RefDraft(card),
                card_type,
            },
        }
    }
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
pub(crate) struct DrawDto {
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
    #[serde(default)]
    params: Vec<ParamDto>,
    #[serde(default)]
    cost: Vec<CostDto>,
    effects: Vec<EffectDto>,
}

/// A param: the simple form `{ name, zone, card_type, owner?, where? }` or `{ name, one_of: [...] }`
/// with alternatives `{ zone, card_type, owner?, where? }`. The two forms cannot be mixed and one
/// of them is needed. The keys are read into `RawParamDto` and the combination is checked when it
/// becomes a `ParamDto`, so a wrong one is a syntax error at the param.
struct ParamDto(ParamDraft);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParamDto {
    name: String,
    #[serde(default, deserialize_with = "some")]
    zone: Option<String>,
    #[serde(default, deserialize_with = "some")]
    card_type: Option<String>,
    #[serde(default, deserialize_with = "some")]
    owner: Option<String>,
    #[serde(default, rename = "where", deserialize_with = "some")]
    condition: Option<ExpressionDto>,
    #[serde(default, deserialize_with = "some")]
    one_of: Option<Vec<AlternativeDto>>,
}

/// An alternative of a `one_of`: `{ zone, card_type, owner?, where? }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlternativeDto {
    zone: String,
    card_type: String,
    #[serde(default, deserialize_with = "some")]
    owner: Option<String>,
    #[serde(default, rename = "where", deserialize_with = "some")]
    condition: Option<ExpressionDto>,
}

impl AlternativeDto {
    fn into_draft(self) -> ParamAlternativeDraft {
        ParamAlternativeDraft {
            zone: self.zone,
            card_type: self.card_type,
            owner: self.owner,
            condition: self.condition.map(ExpressionDto::into_draft),
        }
    }
}

impl<'de> Deserialize<'de> for ParamDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawParamDto::deserialize(deserializer)?;
        let cards = match (raw.one_of, raw.zone, raw.card_type) {
            (Some(listed), None, None) if raw.owner.is_none() && raw.condition.is_none() => {
                ParamCardsDraft::OneOf(listed.into_iter().map(AlternativeDto::into_draft).collect())
            }
            (Some(_), _, _) => {
                return Err(de::Error::custom(
                    "a param with `one_of` takes no `zone`, `card_type`, `owner` or `where`",
                ))
            }
            (None, Some(zone), Some(card_type)) => ParamCardsDraft::Single(ParamAlternativeDraft {
                zone,
                card_type,
                owner: raw.owner,
                condition: raw.condition.map(ExpressionDto::into_draft),
            }),
            (None, _, _) => {
                return Err(de::Error::custom(
                    "a param needs `zone` and `card_type`, or `one_of`",
                ))
            }
        };
        Ok(ParamDto(ParamDraft {
            name: raw.name,
            cards,
        }))
    }
}

/// `{ ref: <param> }` or `{ ref: <param>.<name> }`. The text is kept as is: the engine parses it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefDto {
    #[serde(rename = "ref")]
    reference: String,
}

/// One cost of an action: a single-key map `{ <primitive>: { ... } }`.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CostDto {
    Tap(TapDto),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TapDto {
    zone: String,
    count: RefDto,
}

/// One effect of an action: either a single-key map `{ <primitive>: { ... } }`, or the sibling-key
/// form `{ if: <expression>, then: [...], else: [...] }` where `then` is required and `else` is
/// optional, like an `if` step.
pub(crate) enum EffectDto {
    EndTurn,
    Move(MoveEffectDto),
    SetState(SetStateEffectDto),
    AddState(AddStateEffectDto),
    Draw(DrawDto),
    If {
        condition: ExpressionDto,
        then: Vec<EffectDto>,
        otherwise: Vec<EffectDto>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MoveEffectDto {
    card: RefDto,
    to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetStateEffectDto {
    card: RefDto,
    state: String,
    value: ScalarDto,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddStateEffectDto {
    card: RefDto,
    state: String,
    #[serde(deserialize_with = "scalar::strict_i64")]
    amount: i64,
}

/// The keys an effect may have, for the error of an unknown one.
const EFFECT_KEYS: &[&str] = &[
    "end_turn",
    "move",
    "set_state",
    "add_state",
    "draw",
    "if",
    "then",
    "else",
];

impl<'de> Deserialize<'de> for EffectDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EffectVisitor;

        impl<'de> Visitor<'de> for EffectVisitor {
            type Value = EffectDto;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an effect: one primitive, or `if` with `then` and optionally `else`")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<EffectDto, A::Error> {
                let mut primitive: Option<EffectDto> = None;
                let mut condition: Option<ExpressionDto> = None;
                let mut then: Option<Vec<EffectDto>> = None;
                let mut otherwise: Option<Vec<EffectDto>> = None;

                // An unknown key is an unknown variant at the list element, like an unknown cost
                // or step primitive.
                while let Some(key) = map.next_key::<String>()? {
                    let found = match key.as_str() {
                        "end_turn" => {
                            let EmptyDto {} = map.next_value()?;
                            Some(EffectDto::EndTurn)
                        }
                        "move" => Some(EffectDto::Move(map.next_value()?)),
                        "set_state" => Some(EffectDto::SetState(map.next_value()?)),
                        "add_state" => Some(EffectDto::AddState(map.next_value()?)),
                        "draw" => Some(EffectDto::Draw(map.next_value()?)),
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
                        other => return Err(de::Error::unknown_variant(other, EFFECT_KEYS)),
                    };
                    if let Some(found) = found {
                        if primitive.replace(found).is_some() {
                            return Err(de::Error::custom("an effect names only one primitive"));
                        }
                    }
                }

                match (primitive, condition, then, otherwise) {
                    (Some(primitive), None, None, None) => Ok(primitive),
                    (None, Some(condition), Some(then), otherwise) => Ok(EffectDto::If {
                        condition,
                        then,
                        otherwise: otherwise.unwrap_or_default(),
                    }),
                    (None, Some(_), None, _) => {
                        Err(de::Error::custom("an `if` effect needs a `then` list"))
                    }
                    (Some(_), Some(_), _, _) => Err(de::Error::custom(
                        "an `if` effect cannot also name a primitive",
                    )),
                    (None, None, Some(_), _) | (None, None, None, Some(_)) => Err(
                        de::Error::custom("`then` and `else` belong to an `if` effect"),
                    ),
                    (Some(_), None, _, _) => Err(de::Error::custom(
                        "`then` and `else` belong to an `if` effect",
                    )),
                    (None, None, None, None) => {
                        Err(de::Error::custom("an effect names a primitive or an `if`"))
                    }
                }
            }
        }

        deserializer.deserialize_map(EffectVisitor)
    }
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
            win_conditions: self
                .win_conditions
                .into_iter()
                .map(WinConditionDto::into_draft)
                .collect(),
            effects: self.effects.map(|effects| EffectsDraft {
                active_zone: effects.active_zone,
            }),
        }
    }
}

impl WinConditionDto {
    fn into_draft(self) -> WinConditionDraft {
        WinConditionDraft {
            id: self.id,
            when: match self.when {
                WinConditionWhenDto::DrawFromEmpty(zone) => {
                    WinConditionWhenDraft::DrawFromEmpty { zone }
                }
                WinConditionWhenDto::Expression(expression) => {
                    WinConditionWhenDraft::Expression(expression.into_draft())
                }
            },
            loser: self.loser,
        }
    }
}

impl ActionDto {
    fn into_draft(self) -> ActionDraft {
        ActionDraft {
            id: self.id,
            params: self.params.into_iter().map(|param| param.0).collect(),
            cost: self
                .cost
                .into_iter()
                .map(|cost| match cost {
                    CostDto::Tap(tap) => CostDraft::Tap {
                        zone: tap.zone,
                        count: RefDraft(tap.count.reference),
                    },
                })
                .collect(),
            effects: EffectDto::into_drafts(self.effects),
        }
    }
}

impl EffectDto {
    pub(crate) fn into_drafts(effects: Vec<EffectDto>) -> Vec<EffectDraft> {
        effects.into_iter().map(EffectDto::into_draft).collect()
    }

    fn into_draft(self) -> EffectDraft {
        match self {
            Self::EndTurn => EffectDraft::EndTurn,
            Self::Move(step) => EffectDraft::Move {
                card: RefDraft(step.card.reference),
                to: step.to,
            },
            Self::SetState(step) => EffectDraft::SetState {
                card: RefDraft(step.card.reference),
                state: step.state,
                value: step.value.0,
            },
            Self::AddState(step) => EffectDraft::AddState {
                card: RefDraft(step.card.reference),
                state: step.state,
                amount: step.amount,
            },
            Self::Draw(step) => EffectDraft::Draw {
                player: step.player,
                from: step.from,
                to: step.to,
                count: step.count,
            },
            Self::If {
                condition,
                then,
                otherwise,
            } => EffectDraft::If {
                condition: condition.into_draft(),
                then: Self::into_drafts(then),
                otherwise: Self::into_drafts(otherwise),
            },
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
