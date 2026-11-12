//! Evaluating the expressions of a Ruleset Definition against a GameState (ADR-0016).
//!
//! Validation guarantees every operand has the type its operator needs and every ref was resolved
//! to a field or an instance state, so evaluation only reads. A value of another type than the
//! one needed is an internal inconsistency, never expected.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::state::{CardInstanceId, GameState};
use crate::ruleset_definition::{CardValue, Expression, FieldValue};

/// Whether a bool expression holds. `bound` gives the card behind each param it reads.
pub(super) fn holds(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    expression: &Expression,
) -> Result<bool, Rejection> {
    match evaluate(ruleset, state, bound, expression)? {
        FieldValue::Bool(holds) => Ok(holds),
        FieldValue::Int(_) | FieldValue::Text(_) => Err(Rejection::ExpressionTypeMismatch),
    }
}

fn evaluate(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    bound: &BoundCards,
    expression: &Expression,
) -> Result<FieldValue, Rejection> {
    match expression {
        Expression::Literal(value) => Ok(value.clone()),
        Expression::Value(reference) => {
            let card = bound.card(reference.param())?;
            read(ruleset, state, card, reference.value())
        }
        Expression::Eq(left, right) => {
            let left = evaluate(ruleset, state, bound, left)?;
            let right = evaluate(ruleset, state, bound, right)?;
            if left.field_type() != right.field_type() {
                return Err(Rejection::ExpressionTypeMismatch);
            }
            Ok(FieldValue::Bool(left == right))
        }
        Expression::Gte(left, right) => {
            let (left, right) = ints(ruleset, state, bound, left, right)?;
            Ok(FieldValue::Bool(left >= right))
        }
        Expression::Lte(left, right) => {
            let (left, right) = ints(ruleset, state, bound, left, right)?;
            Ok(FieldValue::Bool(left <= right))
        }
        Expression::And(operands) => {
            for operand in operands {
                if !holds(ruleset, state, bound, operand)? {
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
) -> Result<(i64, i64), Rejection> {
    let left = evaluate(ruleset, state, bound, left)?;
    let right = evaluate(ruleset, state, bound, right)?;
    match (left, right) {
        (FieldValue::Int(left), FieldValue::Int(right)) => Ok((left, right)),
        _ => Err(Rejection::ExpressionTypeMismatch),
    }
}

/// The value a ref reads on `card`: a field of its definition or its instance state.
pub(super) fn read(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    card: CardInstanceId,
    value: &CardValue,
) -> Result<FieldValue, Rejection> {
    let mismatch = || Rejection::CardDataMismatch { card };
    let instance = state.card(card).ok_or_else(mismatch)?;
    match value {
        CardValue::Field(name) => ruleset
            .catalog()
            .card(instance.definition())
            .and_then(|definition| definition.field(name))
            .cloned()
            .ok_or_else(mismatch),
        CardValue::State(name) => instance.state_value(name).cloned().ok_or_else(mismatch),
    }
}
