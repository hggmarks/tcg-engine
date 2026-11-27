//! Evaluating the expressions of a Ruleset Definition against a GameState (ADR-0016).
//!
//! Validation guarantees every operand has the type its operator needs and every ref was resolved
//! to a field or an instance state, so evaluation only reads. A value of another type than the
//! one needed is an internal inconsistency, never expected.
//!
//! [`read`] is the one place a ref is resolved, and it is a derived read (ADR-0024): the value of
//! an int field is its base value plus the Field Modifiers that are active at that moment, computed
//! here and stored nowhere. The `while` of a Field Modifier is evaluated in [`ReadMode::Base`], which
//! never looks for modifiers, so deriving a value never needs a derived value.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::modifiers;
use super::state::{ApplyError, CardInstanceId, GameState};
use crate::player::PlayerId;
use crate::ruleset_definition::{
    Amount, AmountOperand, CardValue, Expression, FieldValue, ParamName, PlayerRef,
    PlayerRefSelector, PlayerTarget,
};
use crate::turn::TurnNumber;

/// Which value a ref reads.
#[derive(Debug, Clone, Copy)]
enum ReadMode {
    /// The base value plus the active Field Modifiers.
    Derived,
    /// The base value alone: the `while` of a Field Modifier reads this way.
    Base,
}

/// Whether a bool expression holds, reading derived values. `bound` gives the card behind each
/// param it reads.
pub(super) fn holds(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    expression: &Expression,
) -> Result<bool, Rejection> {
    holds_in(ruleset, state, bound, expression, ReadMode::Derived)
}

/// Whether a bool expression holds, reading base values only (ADR-0024).
pub(super) fn holds_base(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    expression: &Expression,
) -> Result<bool, Rejection> {
    holds_in(ruleset, state, bound, expression, ReadMode::Base)
}

fn holds_in(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    expression: &Expression,
    mode: ReadMode,
) -> Result<bool, Rejection> {
    match evaluate(ruleset, state, bound, expression, mode)? {
        FieldValue::Bool(holds) => Ok(holds),
        FieldValue::Int(_) | FieldValue::Text(_) => Err(Rejection::ExpressionTypeMismatch),
    }
}

fn evaluate(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    expression: &Expression,
    mode: ReadMode,
) -> Result<FieldValue, Rejection> {
    match expression {
        Expression::Literal(value) => Ok(value.clone()),
        Expression::Value(reference) => {
            let card = bound.card(reference.param())?;
            read_in(ruleset, state, card, reference.value(), mode)
        }
        Expression::PlayerValue(reference) => read_player_state(state, bound, reference),
        Expression::ZoneCount(count) => {
            let selected = match count.player() {
                PlayerRefSelector::Active => Selected::Active,
                PlayerRefSelector::Controller => Selected::Controller,
                PlayerRefSelector::Opponent => Selected::Opponent,
                PlayerRefSelector::Judged => Selected::Judged,
            };
            let player = selected_player(state, bound, selected)?;
            let cards = state
                .zone(player, count.zone())
                .ok_or(Rejection::EventRejected(ApplyError::UnknownZone {
                    player,
                    zone: count.zone().clone(),
                }))?;
            i64::try_from(cards.len())
                .map(FieldValue::Int)
                .map_err(|_| Rejection::CountOverflow {
                    player,
                    zone: count.zone().clone(),
                })
        }
        Expression::FirstTurn => Ok(FieldValue::Bool(
            state.turn_number() == Some(TurnNumber::FIRST),
        )),
        Expression::Eq(left, right) => {
            let left = evaluate(ruleset, state, bound, left, mode)?;
            let right = evaluate(ruleset, state, bound, right, mode)?;
            if left.field_type() != right.field_type() {
                return Err(Rejection::ExpressionTypeMismatch);
            }
            Ok(FieldValue::Bool(left == right))
        }
        Expression::Gte(left, right) => {
            let (left, right) = ints(ruleset, state, bound, left, right, mode)?;
            Ok(FieldValue::Bool(left >= right))
        }
        Expression::Lte(left, right) => {
            let (left, right) = ints(ruleset, state, bound, left, right, mode)?;
            Ok(FieldValue::Bool(left <= right))
        }
        Expression::And(operands) => {
            for operand in operands {
                if !holds_in(ruleset, state, bound, operand, mode)? {
                    return Ok(FieldValue::Bool(false));
                }
            }
            Ok(FieldValue::Bool(true))
        }
        Expression::IsCardType { card, card_type } => {
            let card = bound.card(card.param())?;
            let definition = state
                .card(card)
                .and_then(|instance| ruleset.catalog().card(instance.definition()))
                .ok_or(Rejection::CardDataMismatch { card })?;
            Ok(FieldValue::Bool(definition.card_type() == card_type))
        }
    }
}

/// The Player a selector names, for an expression that reads against `bound` (ADR-0041): `active`
/// is the Active Player; `controller` is the owner of `self`; `opponent` is the opponent of the
/// acting Player in an action and of the owner of `self` in a Card Effect.
fn selected_player(
    state: &GameState,
    bound: &BoundCards,
    selector: Selected,
) -> Result<PlayerId, Rejection> {
    let controller = || -> Result<PlayerId, Rejection> {
        let card = bound.card(&ParamName::new("self"))?;
        state
            .card(card)
            .map(|instance| instance.owner())
            .ok_or(Rejection::CardDataMismatch { card })
    };
    match selector {
        // Only a Player-state win condition judges a Player; a Judged ref anywhere else has none:
        // an internal inconsistency.
        Selected::Judged => bound.judged().ok_or(Rejection::ExpressionTypeMismatch),
        Selected::Active => Ok(state.active_player()),
        Selected::Controller => controller(),
        Selected::Opponent => {
            let of = match bound.acting() {
                Some(acting) => acting,
                None => controller()?,
            };
            state
                .opponent_of(of)
                .ok_or(Rejection::EventRejected(ApplyError::UnknownPlayer {
                    player: of,
                }))
        }
    }
}

/// A selector of a Player that is one Player.
#[derive(Debug, Clone, Copy)]
enum Selected {
    Judged,
    Active,
    Controller,
    Opponent,
}

/// The value of the Player state a Player ref reads.
fn read_player_state(
    state: &GameState,
    bound: &BoundCards,
    reference: &PlayerRef,
) -> Result<FieldValue, Rejection> {
    let selected = match reference.selector() {
        PlayerRefSelector::Active => Selected::Active,
        PlayerRefSelector::Controller => Selected::Controller,
        PlayerRefSelector::Opponent => Selected::Opponent,
        PlayerRefSelector::Judged => Selected::Judged,
    };
    let player = selected_player(state, bound, selected)?;
    state
        .player_state_value(player, reference.state())
        .cloned()
        .ok_or_else(|| {
            Rejection::EventRejected(ApplyError::UnknownPlayerState {
                player,
                state: reference.state().clone(),
            })
        })
}

/// The int an `amount` evaluates to against the GameState as it stands (ADR-0041). A card's field is
/// read as any expression does, with its active Field Modifiers; an instance state as it stands; a
/// Player's state as it stands. `sub` is checked.
pub(super) fn amount(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    amount: &Amount,
) -> Result<i64, Rejection> {
    match amount {
        Amount::Literal(value) => Ok(*value),
        Amount::Operand(operand) => operand_value(ruleset, state, bound, operand),
        Amount::Sub(left, right) => {
            let left = operand_value(ruleset, state, bound, left)?;
            let right = operand_value(ruleset, state, bound, right)?;
            left.checked_sub(right).ok_or(Rejection::ArithmeticOverflow)
        }
    }
}

fn operand_value(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    operand: &AmountOperand,
) -> Result<i64, Rejection> {
    let value = match operand {
        AmountOperand::Literal(value) => return Ok(*value),
        AmountOperand::Value(reference) => {
            let card = bound.card(reference.param())?;
            read(ruleset, state, card, reference.value())?
        }
        AmountOperand::PlayerValue(reference) => read_player_state(state, bound, reference)?,
    };
    match value {
        FieldValue::Int(value) => Ok(value),
        FieldValue::Bool(_) | FieldValue::Text(_) => Err(Rejection::ExpressionTypeMismatch),
    }
}

/// The Players a Player-state write targets, in seat order for `each` (ADR-0041).
pub(super) fn target_players(
    state: &GameState,
    bound: &BoundCards,
    target: PlayerTarget,
) -> Result<Vec<PlayerId>, Rejection> {
    match target {
        PlayerTarget::Each => Ok(state.player_order().players().to_vec()),
        PlayerTarget::Active => selected_player(state, bound, Selected::Active).map(|p| vec![p]),
        PlayerTarget::Controller => {
            selected_player(state, bound, Selected::Controller).map(|p| vec![p])
        }
        PlayerTarget::Opponent => {
            selected_player(state, bound, Selected::Opponent).map(|p| vec![p])
        }
    }
}

/// Two operands that must be ints.
fn ints(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    left: &Expression,
    right: &Expression,
    mode: ReadMode,
) -> Result<(i64, i64), Rejection> {
    let left = evaluate(ruleset, state, bound, left, mode)?;
    let right = evaluate(ruleset, state, bound, right, mode)?;
    match (left, right) {
        (FieldValue::Int(left), FieldValue::Int(right)) => Ok((left, right)),
        _ => Err(Rejection::ExpressionTypeMismatch),
    }
}

/// The value a ref reads on `card`: a field of its definition with the active Field Modifiers added
/// (ADR-0024), or its instance state.
pub(super) fn read(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    card: CardInstanceId,
    value: &CardValue,
) -> Result<FieldValue, Rejection> {
    read_in(ruleset, state, card, value, ReadMode::Derived)
}

fn read_in(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    card: CardInstanceId,
    value: &CardValue,
    mode: ReadMode,
) -> Result<FieldValue, Rejection> {
    let mismatch = || Rejection::CardDataMismatch { card };
    let instance = state.card(card).ok_or_else(mismatch)?;
    match value {
        CardValue::Field(name) => {
            let base = ruleset
                .catalog()
                .card(instance.definition())
                .and_then(|definition| definition.field(name))
                .cloned()
                .ok_or_else(mismatch)?;
            match (base, mode) {
                (FieldValue::Int(base), ReadMode::Derived) => {
                    let added = modifiers::modifier_sum(ruleset, state, card, name)?;
                    base.checked_add(added).map(FieldValue::Int).ok_or_else(|| {
                        Rejection::DerivedValueOverflow {
                            card,
                            field: name.clone(),
                        }
                    })
                }
                (base, _) => Ok(base),
            }
        }
        CardValue::State(name) => instance.state_value(name).cloned().ok_or_else(mismatch),
    }
}
