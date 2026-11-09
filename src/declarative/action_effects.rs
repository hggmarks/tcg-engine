//! Paying the costs and running the effects of an Action Definition (ADR-0015).
//!
//! Everything here runs on the Execution's clone of the GameState, so an Action that fails half
//! way changes nothing (ADR-0002). Refs were resolved to a field or an instance state when the
//! definition was validated; here they only read that value.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::event::Event;
use super::execution::Execution;
use super::state::CardInstanceId;
use crate::ruleset_definition::{CardValue, Cost, Effect, FieldValue, StateName, ValueRef, ZoneId};

/// Pays one cost.
pub(super) fn pay(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    cost: &Cost,
) -> Result<(), Rejection> {
    match cost {
        Cost::Tap { zone, count } => tap(ruleset, execution, bound, zone, count),
    }
}

/// Taps the first `count` untapped cards of the acting Player's `zone`, in list order. A card is
/// untapped when its card type declares `tapped` and the value is false.
fn tap(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    zone: &ZoneId,
    count: &ValueRef,
) -> Result<(), Rejection> {
    let card = bound.card(count.param())?;
    let FieldValue::Int(value) = read(ruleset, execution, card, count.value())? else {
        return Err(Rejection::CardDataMismatch { card });
    };
    let Ok(required) = u64::try_from(value) else {
        return Err(Rejection::NegativeCount { value });
    };

    let tapped = StateName::new("tapped");
    let player = execution.state.active_player();
    let untapped: Vec<CardInstanceId> = execution
        .cards_of(player, zone)
        .map_err(Rejection::EventRejected)?
        .into_iter()
        .filter(|&candidate| {
            execution
                .state
                .card(candidate)
                .and_then(|instance| instance.state_value(&tapped))
                == Some(&FieldValue::Bool(false))
        })
        .collect();
    let available = u64::try_from(untapped.len()).unwrap_or(u64::MAX);
    if required > available {
        return Err(Rejection::NotEnoughUntappedCards {
            zone: zone.clone(),
            required,
            available,
        });
    }
    let required = usize::try_from(required).unwrap_or(usize::MAX);
    for card in untapped.into_iter().take(required) {
        execution
            .apply(Event::CardStateChanged {
                card,
                state: tapped.clone(),
                value: FieldValue::Bool(true),
            })
            .map_err(Rejection::EventRejected)?;
    }
    Ok(())
}

/// Runs one effect other than `end_turn`, which the Turn cycle owns.
pub(super) fn run(
    execution: &mut Execution,
    bound: &BoundCards,
    effect: &Effect,
) -> Result<(), Rejection> {
    match effect {
        Effect::EndTurn => Ok(()),
        Effect::Move { card, to } => {
            let card = bound.card(card.param())?;
            let (Some(from), Some(instance)) =
                (execution.state.zone_of(card), execution.state.card(card))
            else {
                return Err(Rejection::CardDataMismatch { card });
            };
            let event = Event::CardMoved {
                card,
                player: instance.owner(),
                from: from.clone(),
                to: to.clone(),
            };
            execution.apply(event).map_err(Rejection::EventRejected)
        }
        Effect::SetState { card, state, value } => {
            let card = bound.card(card.param())?;
            let current = execution
                .state
                .card(card)
                .and_then(|instance| instance.state_value(state));
            if current == Some(value) {
                return Ok(());
            }
            execution
                .apply(Event::CardStateChanged {
                    card,
                    state: state.clone(),
                    value: value.clone(),
                })
                .map_err(Rejection::EventRejected)
        }
    }
}

/// The value a ref reads on `card`: a field of its definition or its instance state.
fn read(
    ruleset: &DeclarativeRuleset,
    execution: &Execution,
    card: CardInstanceId,
    value: &CardValue,
) -> Result<FieldValue, Rejection> {
    let mismatch = || Rejection::CardDataMismatch { card };
    let instance = execution.state.card(card).ok_or_else(mismatch)?;
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
