//! Paying the costs and running the effects of an Action Definition (ADR-0015, ADR-0016).
//!
//! Everything here runs on the Execution's clone of the GameState, so an Action that fails half
//! way changes nothing (ADR-0002). Refs were resolved to a field or an instance state when the
//! definition was validated; here they only read that value.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::event::Event;
use super::execution::Execution;
use super::expression;
use super::state::CardInstanceId;
use super::turn_cycle;
use crate::player::PlayerId;
use crate::ruleset_definition::{
    Cost, Effect, EffectPlayer, FieldValue, ParamName, StateName, ValueRef, ZoneId,
};

/// Pays one cost on behalf of `player`, the acting Player.
pub(super) fn pay(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    player: PlayerId,
    cost: &Cost,
) -> Result<(), Rejection> {
    match cost {
        Cost::Tap { zone, count } => tap(ruleset, execution, bound, player, zone, count),
    }
}

/// Taps the first `count` untapped cards of the acting Player's `zone`, in list order. A card is
/// untapped when its card type declares `tapped` and the value is false.
fn tap(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    player: PlayerId,
    zone: &ZoneId,
    count: &ValueRef,
) -> Result<(), Rejection> {
    let card = bound.card(count.param())?;
    let FieldValue::Int(value) = expression::read(ruleset, &execution.state, card, count.value())?
    else {
        return Err(Rejection::CardDataMismatch { card });
    };
    let Ok(required) = u64::try_from(value) else {
        return Err(Rejection::NegativeCount { value });
    };

    let tapped = StateName::new("tapped");
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

/// Runs one effect. An `if` runs the effects of the branch its condition picks, on the state the
/// effects before it produced; `end_turn` is the Turn cycle's.
pub(super) fn run(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    effect: &Effect,
) -> Result<(), Rejection> {
    match effect {
        Effect::EndTurn => turn_cycle::end_turn(ruleset.definition(), execution),
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
            change_state(execution, card, state, value.clone())
        }
        Effect::AddState {
            card,
            state,
            amount,
        } => {
            let card = bound.card(card.param())?;
            let Some(FieldValue::Int(current)) = execution
                .state
                .card(card)
                .and_then(|instance| instance.state_value(state))
            else {
                return Err(Rejection::CardDataMismatch { card });
            };
            let Some(sum) = current.checked_add(*amount) else {
                return Err(Rejection::StateOverflow {
                    card,
                    state: state.clone(),
                });
            };
            change_state(execution, card, state, FieldValue::Int(sum))
        }
        Effect::Draw {
            player,
            from,
            to,
            count,
        } => {
            let player = match player {
                EffectPlayer::Controller => {
                    let card = bound.card(&ParamName::new("self"))?;
                    let Some(instance) = execution.state.card(card) else {
                        return Err(Rejection::CardDataMismatch { card });
                    };
                    instance.owner()
                }
            };
            turn_cycle::draw(execution, player, from, to, count.get())
                .map_err(Rejection::EventRejected)
        }
        Effect::If {
            condition,
            then,
            otherwise,
        } => {
            let branch = if expression::holds(ruleset, &execution.state, bound, condition)? {
                then
            } else {
                otherwise
            };
            for effect in branch {
                run(ruleset, execution, bound, effect)?;
            }
            Ok(())
        }
    }
}

/// Sets the instance state `state` of `card` to `value` if it changes: an unchanged value emits no
/// Event.
fn change_state(
    execution: &mut Execution,
    card: CardInstanceId,
    state: &StateName,
    value: FieldValue,
) -> Result<(), Rejection> {
    let current = execution
        .state
        .card(card)
        .and_then(|instance| instance.state_value(state));
    if current == Some(&value) {
        return Ok(());
    }
    execution
        .apply(Event::CardStateChanged {
            card,
            state: state.clone(),
            value,
        })
        .map_err(Rejection::EventRejected)
}
