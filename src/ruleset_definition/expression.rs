//! Refs and expressions of a Ruleset Definition (ADR-0015, ADR-0016).
//!
//! A ref names a param, and a param may take several card types, so a ref is checked against every
//! card type its param may take: its **possible card types**. Checking is depth-first, left to
//! right: all the operands of an expression are checked (each with its own errors) before the type
//! rule of their operator.

use super::draft::{ExpressionDraft, RefDraft};
use super::error::{DefinitionError, DefinitionErrorKind, RefType, ReferenceTarget};
use super::model::{
    Amount, CardRef, CardType, CardTypeId, CardValue, Expression, FieldType, FieldValue,
    ParamAlternative, ParamName, PlayerRef, PlayerRefSelector, PlayerStateSchema, PlayerTarget,
    ValueRef, Zone, ZoneCount, ZoneId,
};
use crate::definition_path::DefinitionPath;
use crate::id::is_valid_id;

pub(super) fn fail<T>(
    path: DefinitionPath,
    kind: DefinitionErrorKind,
) -> Result<T, DefinitionError> {
    Err(DefinitionError { path, kind })
}

/// A name a ref may start with, and the card types the card behind it may have.
#[derive(Debug, Clone)]
pub(super) struct ScopedParam {
    pub(super) name: ParamName,
    /// The possible card types, in declaration order, without repeats. Never empty when the scope
    /// is built; `Refs::narrowed` may leave it empty, and the caller rejects that as
    /// `ImpossibleCardType` before the scope is used.
    pub(super) types: Vec<CardTypeId>,
}

impl ScopedParam {
    /// A param that takes the card types of its alternatives, in order, each once.
    pub(super) fn of_alternatives(name: ParamName, alternatives: &[ParamAlternative]) -> Self {
        let mut types: Vec<CardTypeId> = Vec::with_capacity(alternatives.len());
        for alternative in alternatives {
            if !types.contains(&alternative.card_type) {
                types.push(alternative.card_type.clone());
            }
        }
        Self { name, types }
    }
}

/// Where an expression or a Player-state write stands, which says which Player selectors it may
/// use (ADR-0041).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    /// An action: its params' `where`, its costs and its effects. `active` is the Active Player,
    /// `opponent` the acting Player's opponent.
    Action,
    /// A Card Effect: `controller` is the owner of `self`, `opponent` the controller's opponent.
    CardEffect,
    /// A win condition.
    WinCondition,
}

/// What the refs of an action (or of a win condition) are checked against.
#[derive(Debug, Clone)]
pub(super) struct Refs<'a> {
    pub(super) zones: &'a [Zone],
    pub(super) card_types: &'a [CardType],
    pub(super) player_state: &'a [PlayerStateSchema],
    /// The number of Players of the ruleset: `opponent` is defined for exactly 2.
    pub(super) players: u32,
    pub(super) scope: Scope,
    pub(super) params: Vec<ScopedParam>,
    /// What a ref to a name nothing declares is a dangling reference to: a param, or for a win
    /// condition a card type.
    pub(super) missing: ReferenceTarget,
    /// Whether `self`, the carrying card, is in scope: only in a Card Effect, where the
    /// `controller` of a `draw` is its owner.
    pub(super) has_self: bool,
    /// Whether a Choice may stand in the effects: only in a triggered Card Effect, which can pause
    /// for a decision (ADR-0033). An activated ability has `self` but no Choice (ADR-0038).
    pub(super) choices: bool,
}

impl Refs<'_> {
    /// The possible card types of `param`; empty if it is not in scope.
    pub(super) fn types_of(&self, param: &ParamName) -> &[CardTypeId] {
        self.params
            .iter()
            .find(|candidate| &candidate.name == param)
            .map_or(&[], |scoped| scoped.types.as_slice())
    }

    /// The same scope where `param` may take only the card types that are also in `types`: its own
    /// possible card types that `types` lists, in the param's order (ADR-0032). The result may be
    /// empty, which the caller reports; a param that is not in scope is left out of scope.
    pub(super) fn narrowed(&self, param: &ParamName, types: &[CardTypeId]) -> Self {
        let mut narrowed = self.clone();
        if let Some(scoped) = narrowed
            .params
            .iter_mut()
            .find(|candidate| &candidate.name == param)
        {
            scoped.types.retain(|own| types.contains(own));
        }
        narrowed
    }

    fn card_type(&self, id: &CardTypeId) -> Option<&CardType> {
        self.card_types.iter().find(|card_type| &card_type.id == id)
    }
}

/// A ref taken apart: the param it names and, for `<param>.<name>`, the name.
struct ParsedRef<'a> {
    param: &'a str,
    name: Option<&'a str>,
}

/// `<id>` or `<id>.<id>`.
fn parse_ref<'a>(
    reference: &'a RefDraft,
    at: &DefinitionPath,
) -> Result<ParsedRef<'a>, DefinitionError> {
    let invalid = || DefinitionError {
        path: at.clone(),
        kind: DefinitionErrorKind::InvalidReference {
            reference: reference.0.clone(),
        },
    };
    let (param, name) = match reference.0.split_once('.') {
        Some((param, name)) => (param, Some(name)),
        None => (reference.0.as_str(), None),
    };
    if !is_valid_id(param) || name.is_some_and(|name| !is_valid_id(name)) {
        return Err(invalid());
    }
    Ok(ParsedRef { param, name })
}

/// The param a ref names, or the dangling reference.
fn find_param<'r>(
    refs: &'r Refs<'_>,
    param: &str,
    at: &DefinitionPath,
) -> Result<&'r ScopedParam, DefinitionError> {
    refs.params
        .iter()
        .find(|declared| declared.name.as_str() == param)
        .ok_or_else(|| DefinitionError {
            path: at.clone(),
            kind: DefinitionErrorKind::DanglingReference {
                target: refs.missing,
                id: param.to_string(),
            },
        })
}

/// A ref that must be a card: `{ ref: <param> }`.
pub(super) fn validate_card_ref(
    reference: &RefDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<CardRef, DefinitionError> {
    let parsed = parse_ref(reference, &at)?;
    let param = find_param(refs, parsed.param, &at)?;
    match parsed.name {
        None => Ok(CardRef {
            param: param.name.clone(),
        }),
        Some(name) => {
            let (_, found) = resolve_name(refs, param, name, &at)?;
            fail(
                at,
                DefinitionErrorKind::ReferenceTypeMismatch {
                    expected: RefType::Card,
                    found: RefType::Value(found),
                },
            )
        }
    }
}

/// A ref that must be a value of type `expected`: `{ ref: <param>.<name> }`.
pub(super) fn validate_value_ref(
    reference: &RefDraft,
    expected: FieldType,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<ValueRef, DefinitionError> {
    let parsed = parse_ref(reference, &at)?;
    let param = find_param(refs, parsed.param, &at)?;
    let Some(name) = parsed.name else {
        return fail(
            at,
            DefinitionErrorKind::ReferenceTypeMismatch {
                expected: RefType::Value(expected),
                found: RefType::Card,
            },
        );
    };
    let (value, found) = resolve_name(refs, param, name, &at)?;
    if found != expected {
        return fail(
            at,
            DefinitionErrorKind::ReferenceTypeMismatch {
                expected: RefType::Value(expected),
                found: RefType::Value(found),
            },
        );
    }
    Ok(ValueRef {
        param: param.name.clone(),
        value,
    })
}

/// What `name` is on every possible card type of `param`: a field or an instance state, and its
/// type. The card types are checked in order: a type that declares the name as both is ambiguous,
/// a type that lacks it is dangling, and a kind or type that differs from the first card type's is
/// inconsistent.
fn resolve_name(
    refs: &Refs<'_>,
    param: &ScopedParam,
    name: &str,
    at: &DefinitionPath,
) -> Result<(CardValue, FieldType), DefinitionError> {
    let mut first: Option<(&CardTypeId, CardValue, FieldType)> = None;
    for id in &param.types {
        let card_type = refs.card_type(id);
        let field = card_type.and_then(|card_type| {
            card_type
                .fields
                .iter()
                .find(|field| field.name.as_str() == name)
        });
        let state = card_type.and_then(|card_type| {
            card_type
                .instance_state
                .iter()
                .find(|state| state.name.as_str() == name)
        });
        let (value, found) = match (field, state) {
            (Some(_), Some(_)) => {
                return fail(
                    at.clone(),
                    DefinitionErrorKind::AmbiguousReference {
                        name: name.to_string(),
                    },
                )
            }
            (Some(field), None) => (CardValue::Field(field.name.clone()), field.field_type),
            (None, Some(state)) => (CardValue::State(state.name.clone()), state.state_type),
            (None, None) => {
                return fail(
                    at.clone(),
                    DefinitionErrorKind::DanglingReference {
                        target: ReferenceTarget::FieldOrInstanceState,
                        id: name.to_string(),
                    },
                )
            }
        };
        match &first {
            None => first = Some((id, value, found)),
            Some((first_id, first_value, first_found)) => {
                if *first_value != value || *first_found != found {
                    return fail(
                        at.clone(),
                        DefinitionErrorKind::InconsistentReference {
                            name: name.to_string(),
                            card_type: first_id.as_str().to_string(),
                            other: id.as_str().to_string(),
                        },
                    );
                }
            }
        }
    }
    match first {
        Some((_, value, found)) => Ok((value, found)),
        None => fail(
            at.clone(),
            DefinitionErrorKind::DanglingReference {
                target: refs.missing,
                id: param.name.as_str().to_string(),
            },
        ),
    }
}

/// An expression that must be bool: a `where`, an `if` or a `when`.
pub(super) fn check_condition(
    draft: &ExpressionDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<Expression, DefinitionError> {
    let (expression, found) = check_expression(draft, refs, at.clone())?;
    if found != FieldType::Bool {
        return fail(
            at,
            DefinitionErrorKind::ExpressionTypeMismatch {
                expected: FieldType::Bool,
                found,
            },
        );
    }
    Ok(expression)
}

/// An expression and its type. `at` is its path; the operands of an operator are at the operator's
/// key and their position.
fn check_expression(
    draft: &ExpressionDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<(Expression, FieldType), DefinitionError> {
    match draft {
        ExpressionDraft::Literal(value) => {
            Ok((Expression::Literal(value.clone()), value.field_type()))
        }
        ExpressionDraft::Ref(reference) => {
            let parsed = parse_ref(reference, &at)?;
            // `player` is reserved: a ref that starts with it reads Player state (ADR-0041).
            if parsed.param == PLAYER {
                return check_player_ref(&parsed, None, reference, refs, at);
            }
            let param = find_param(refs, parsed.param, &at)?;
            let Some(name) = parsed.name else {
                return fail(
                    at,
                    DefinitionErrorKind::CardRefNotAValue {
                        reference: reference.0.clone(),
                    },
                );
            };
            let (value, found) = resolve_name(refs, param, name, &at)?;
            Ok((
                Expression::Value(ValueRef {
                    param: param.name.clone(),
                    value,
                }),
                found,
            ))
        }
        ExpressionDraft::PlayerRef {
            reference,
            selector,
        } => {
            let parsed = parse_ref(reference, &at)?;
            // The `player:` key belongs to a Player ref only.
            if parsed.param != PLAYER {
                return fail(
                    at,
                    DefinitionErrorKind::InvalidReference {
                        reference: reference.0.clone(),
                    },
                );
            }
            check_player_ref(&parsed, Some(selector), reference, refs, at)
        }
        ExpressionDraft::Count { zone, player } => {
            if refs.scope == Scope::WinCondition {
                return fail(
                    at,
                    DefinitionErrorKind::OperatorNotInWinCondition {
                        operator: "count".to_string(),
                    },
                );
            }
            let at = at.key("count");
            if !refs
                .zones
                .iter()
                .any(|declared| declared.id.as_str() == zone)
            {
                return fail(
                    at.key("zone"),
                    DefinitionErrorKind::DanglingReference {
                        target: ReferenceTarget::Zone,
                        id: zone.clone(),
                    },
                );
            }
            let Some(player) = player else {
                return fail(at, DefinitionErrorKind::MissingPlayerSelector);
            };
            let player = check_selector(player, refs, at.key("player"))?;
            Ok((
                Expression::ZoneCount(ZoneCount {
                    zone: ZoneId::new(zone),
                    player,
                }),
                FieldType::Int,
            ))
        }
        ExpressionDraft::FirstTurn => {
            if refs.scope == Scope::WinCondition {
                return fail(
                    at,
                    DefinitionErrorKind::OperatorNotInWinCondition {
                        operator: "first_turn".to_string(),
                    },
                );
            }
            Ok((Expression::FirstTurn, FieldType::Bool))
        }
        ExpressionDraft::Eq(left, right) => {
            let at = at.key("eq");
            let (left, expected) = check_expression(left, refs, at.clone().index(0))?;
            let (right, found) = check_expression(right, refs, at.clone().index(1))?;
            if expected != found {
                return fail(
                    at.index(1),
                    DefinitionErrorKind::ExpressionTypeMismatch { expected, found },
                );
            }
            Ok((
                Expression::Eq(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::Gt(left, right) => {
            let (left, right) = check_ints("gt", left, right, refs, at)?;
            Ok((
                Expression::Gt(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::Lt(left, right) => {
            let (left, right) = check_ints("lt", left, right, refs, at)?;
            Ok((
                Expression::Lt(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::Add(left, right) => {
            let (left, right) = check_ints("add", left, right, refs, at)?;
            Ok((
                Expression::Add(Box::new(left), Box::new(right)),
                FieldType::Int,
            ))
        }
        ExpressionDraft::Sub(left, right) => {
            let (left, right) = check_ints("sub", left, right, refs, at)?;
            Ok((
                Expression::Sub(Box::new(left), Box::new(right)),
                FieldType::Int,
            ))
        }
        ExpressionDraft::Gte(left, right) => {
            let (left, right) = check_ints("gte", left, right, refs, at)?;
            Ok((
                Expression::Gte(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::Lte(left, right) => {
            let (left, right) = check_ints("lte", left, right, refs, at)?;
            Ok((
                Expression::Lte(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::And(operands) => {
            let at = at.key("and");
            let mut checked = Vec::with_capacity(operands.len());
            let mut types = Vec::with_capacity(operands.len());
            for (index, operand) in operands.iter().enumerate() {
                let (operand, found) = check_expression(operand, refs, at.clone().index(index))?;
                checked.push(operand);
                types.push(found);
            }
            if let Some((index, &found)) = types
                .iter()
                .enumerate()
                .find(|(_, found)| **found != FieldType::Bool)
            {
                return fail(
                    at.index(index),
                    DefinitionErrorKind::ExpressionTypeMismatch {
                        expected: FieldType::Bool,
                        found,
                    },
                );
            }
            Ok((Expression::And(checked), FieldType::Bool))
        }
        ExpressionDraft::IsCardType { card, card_type } => {
            let at = at.key("is_card_type");
            let card = validate_card_ref(card, refs, at.clone().index(0))?;
            if refs.card_type(&CardTypeId::new(card_type)).is_none() {
                return fail(
                    at.index(1),
                    DefinitionErrorKind::DanglingReference {
                        target: ReferenceTarget::CardType,
                        id: card_type.clone(),
                    },
                );
            }
            Ok((
                Expression::IsCardType {
                    card,
                    card_type: CardTypeId::new(card_type),
                },
                FieldType::Bool,
            ))
        }
    }
}

/// An `amount` of an `add_state` of an action, a cost or a Card Effect (ADR-0043): any Expression
/// that is an int. `at` is its path; an operand of `add` or `sub` is at `add[i]` or `sub[i]`.
pub(super) fn check_amount(
    draft: &ExpressionDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<Amount, DefinitionError> {
    let (expression, found) = check_expression(draft, refs, at.clone())?;
    if found != FieldType::Int {
        return fail(
            at,
            DefinitionErrorKind::ExpressionTypeMismatch {
                expected: FieldType::Int,
                found,
            },
        );
    }
    Ok(Amount(expression))
}

/// The amount of an `add_state` in a setup or an automatic step: an int literal, nothing binds a
/// card or names a Player to read there (ADR-0041, ADR-0043).
pub(super) fn check_literal_amount(
    draft: &ExpressionDraft,
    at: DefinitionPath,
) -> Result<i64, DefinitionError> {
    match draft {
        ExpressionDraft::Literal(FieldValue::Int(value)) => Ok(*value),
        _ => fail(at, DefinitionErrorKind::InvalidAmount),
    }
}

/// The first name of a ref to Player state.
pub(super) const PLAYER: &str = "player";

/// The schema of the Player state `id`, or the dangling reference at `at`.
pub(super) fn find_player_state<'a>(
    schemas: &'a [PlayerStateSchema],
    id: &str,
    at: DefinitionPath,
) -> Result<&'a PlayerStateSchema, DefinitionError> {
    match schemas.iter().find(|schema| schema.id.as_str() == id) {
        Some(schema) => Ok(schema),
        None => fail(
            at,
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::PlayerState,
                id: id.to_string(),
            },
        ),
    }
}

/// `opponent` is defined for exactly 2 Players.
fn check_two_players(players: u32, at: DefinitionPath) -> Result<(), DefinitionError> {
    if players == 2 {
        Ok(())
    } else {
        fail(
            at,
            DefinitionErrorKind::OpponentNeedsTwoPlayers {
                players: u64::from(players),
            },
        )
    }
}

/// A Player ref `player.<id>` with the text of its `player:` key, if it has one (ADR-0041). The
/// state is checked first, then the selector, which the scope limits to the Players it can name.
fn check_player_ref(
    parsed: &ParsedRef<'_>,
    selector: Option<&String>,
    reference: &RefDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<(Expression, FieldType), DefinitionError> {
    let Some(name) = parsed.name else {
        return fail(
            at,
            DefinitionErrorKind::InvalidReference {
                reference: reference.0.clone(),
            },
        );
    };
    let schema = find_player_state(refs.player_state, name, at.clone())?;
    // A win condition reads the Player it judges: no selector, and the ref is Judged.
    if refs.scope == Scope::WinCondition {
        if selector.is_some() {
            return fail(at, DefinitionErrorKind::PlayerSelectorInWinCondition);
        }
        return Ok((
            Expression::PlayerValue(PlayerRef {
                selector: PlayerRefSelector::Judged,
                state: schema.id.clone(),
            }),
            schema.state_type,
        ));
    }
    let Some(selector) = selector else {
        return fail(at, DefinitionErrorKind::MissingPlayerSelector);
    };
    let selected = check_selector(selector, refs, at.clone())?;
    Ok((
        Expression::PlayerValue(PlayerRef {
            selector: selected,
            state: schema.id.clone(),
        }),
        schema.state_type,
    ))
}

/// The Player a selector names in a Player ref or a `count`, which the scope limits to the Players it
/// can name (ADR-0041, ADR-0042).
fn check_selector(
    selector: &str,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<PlayerRefSelector, DefinitionError> {
    match selector {
        "active" => Ok(PlayerRefSelector::Active),
        "opponent" => {
            check_two_players(refs.players, at)?;
            Ok(PlayerRefSelector::Opponent)
        }
        "controller" if refs.scope == Scope::CardEffect => Ok(PlayerRefSelector::Controller),
        "controller" => fail(at, DefinitionErrorKind::ControllerOutsideCardEffect),
        other => fail(
            at,
            DefinitionErrorKind::UnknownPlayerSelector {
                selector: other.to_string(),
            },
        ),
    }
}

/// The Players a write of Player state targets, checked for its scope (ADR-0041): `each`, `active`
/// and `opponent` anywhere, `controller` in a Card Effect only. `at` is the `player` key.
pub(super) fn validate_target(
    selector: &str,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<PlayerTarget, DefinitionError> {
    match selector {
        "each" => Ok(PlayerTarget::Each),
        "active" => Ok(PlayerTarget::Active),
        "opponent" => {
            check_two_players(refs.players, at)?;
            Ok(PlayerTarget::Opponent)
        }
        "controller" if refs.scope == Scope::CardEffect => Ok(PlayerTarget::Controller),
        "controller" => fail(at, DefinitionErrorKind::ControllerOutsideCardEffect),
        other => fail(
            at,
            DefinitionErrorKind::UnknownPlayerSelector {
                selector: other.to_string(),
            },
        ),
    }
}

/// The two operands of `gt`, `gte`, `lt`, `lte`, `add` or `sub`: both int, the first that is not
/// reported.
fn check_ints(
    operator: &'static str,
    left: &ExpressionDraft,
    right: &ExpressionDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<(Expression, Expression), DefinitionError> {
    let at = at.key(operator);
    let (left, left_type) = check_expression(left, refs, at.clone().index(0))?;
    let (right, right_type) = check_expression(right, refs, at.clone().index(1))?;
    for (index, found) in [left_type, right_type].into_iter().enumerate() {
        if found != FieldType::Int {
            return fail(
                at.index(index),
                DefinitionErrorKind::ExpressionTypeMismatch {
                    expected: FieldType::Int,
                    found,
                },
            );
        }
    }
    Ok((left, right))
}

/// Whether an expression reads Player state anywhere.
pub(super) fn reads_player_state(expression: &Expression) -> bool {
    match expression {
        Expression::PlayerValue(_) => true,
        Expression::Literal(_)
        | Expression::Value(_)
        | Expression::IsCardType { .. }
        | Expression::ZoneCount(_)
        | Expression::FirstTurn => false,
        Expression::Eq(left, right)
        | Expression::Gt(left, right)
        | Expression::Gte(left, right)
        | Expression::Lt(left, right)
        | Expression::Lte(left, right)
        | Expression::Add(left, right)
        | Expression::Sub(left, right) => reads_player_state(left) || reads_player_state(right),
        Expression::And(operands) => operands.iter().any(reads_player_state),
    }
}

/// The params an expression reads, each once, in the order a depth-first, left-to-right walk first
/// meets them.
pub(super) fn params_read(expression: &Expression) -> Vec<&ParamName> {
    fn walk<'a>(expression: &'a Expression, found: &mut Vec<&'a ParamName>) {
        fn note<'a>(found: &mut Vec<&'a ParamName>, param: &'a ParamName) {
            if !found.contains(&param) {
                found.push(param);
            }
        }
        match expression {
            Expression::Literal(_)
            | Expression::PlayerValue(_)
            | Expression::ZoneCount(_)
            | Expression::FirstTurn => {}
            Expression::Value(value) => note(found, &value.param),
            Expression::IsCardType { card, .. } => note(found, &card.param),
            Expression::Eq(left, right)
            | Expression::Gt(left, right)
            | Expression::Gte(left, right)
            | Expression::Lt(left, right)
            | Expression::Lte(left, right)
            | Expression::Add(left, right)
            | Expression::Sub(left, right) => {
                walk(left, found);
                walk(right, found);
            }
            Expression::And(operands) => {
                for operand in operands {
                    walk(operand, found);
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(expression, &mut found);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(ids: &[&str]) -> Vec<CardTypeId> {
        ids.iter().map(|id| CardTypeId::new(*id)).collect()
    }

    fn scope(params: &[(&str, &[&str])]) -> Refs<'static> {
        Refs {
            zones: &[],
            card_types: &[],
            player_state: &[],
            players: 2,
            scope: Scope::Action,
            params: params
                .iter()
                .map(|(name, ids)| ScopedParam {
                    name: ParamName::new(*name),
                    types: types(ids),
                })
                .collect(),
            missing: ReferenceTarget::Param,
            has_self: false,
            choices: false,
        }
    }

    #[test]
    fn narrowing_intersects_the_possible_card_types() {
        let refs = scope(&[("p", &["a", "b"]), ("q", &["a"])]);
        let p = ParamName::new("p");

        // [a, b] narrowed by [b, c] is [b]: c is not something the param can be.
        let narrowed = refs.narrowed(&p, &types(&["b", "c"]));
        assert_eq!(narrowed.types_of(&p), types(&["b"]).as_slice());

        // The order is the param's, not the tested list's.
        let ordered = scope(&[("p", &["b", "a"])]);
        let narrowed = ordered.narrowed(&p, &types(&["a", "b"]));
        assert_eq!(narrowed.types_of(&p), types(&["b", "a"]).as_slice());

        // Nothing in common leaves nothing: the caller decides that is an error.
        let narrowed = refs.narrowed(&p, &types(&["c"]));
        assert!(narrowed.types_of(&p).is_empty());

        // Another param is unchanged, and so is the scope the narrowing started from.
        let narrowed = refs.narrowed(&p, &types(&["b"]));
        assert_eq!(
            narrowed.types_of(&ParamName::new("q")),
            types(&["a"]).as_slice()
        );
        assert_eq!(refs.types_of(&p), types(&["a", "b"]).as_slice());

        // A param that is not in scope stays out of scope.
        let absent = ParamName::new("absent");
        let narrowed = refs.narrowed(&absent, &types(&["a"]));
        assert!(narrowed.types_of(&absent).is_empty());
        assert_eq!(narrowed.types_of(&p), types(&["a", "b"]).as_slice());
    }
}
