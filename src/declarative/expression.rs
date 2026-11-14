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
use super::state::{CardInstanceId, GameState};
use crate::ruleset_definition::{CardValue, Expression, FieldValue};

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
