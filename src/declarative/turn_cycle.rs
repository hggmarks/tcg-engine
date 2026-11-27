//! The Turn cycle: beginning a Turn, running automatic phases, moving between action phases and
//! ending a Turn (ADR-0012, ADR-0042).
//!
//! A Turn begins with `TurnBegan`; then each phase from the first begins with `PhaseBegan` and, if
//! it is automatic, runs its steps in order. The first phase with actions is where the Game waits
//! for the Active Player. `advance_phase` enters each phase after the current one up to the next
//! phase with actions, where the Game waits again. `end_turn` runs every automatic phase after the
//! current one, passes over the action phases among them (a phase begins only when the Game enters
//! it, and the Game waits in an action phase it enters), then begins the next Player's Turn, all
//! as one transition.

use super::action::Rejection;
use super::event::Event;
use super::execution::{Execution, StepError};
use super::state::ApplyError;
use crate::player::PlayerId;
use crate::ruleset_definition::{
    AutomaticStep, Condition, FieldValue, Phase, PhaseKind, Position, RulesetDefinition, StateName,
    ZoneId,
};
use crate::turn::TurnNumber;

/// Begins Turn `turn` for `player`, and runs its phases until the one that waits for actions. A
/// ruleset without a turn has no phases to run.
pub(super) fn begin_turn(
    definition: &RulesetDefinition,
    execution: &mut Execution,
    player: PlayerId,
    turn: TurnNumber,
) -> Result<(), StepError> {
    execution.apply(Event::TurnBegan { player, turn })?;
    let Some(structure) = definition.turn() else {
        return Ok(());
    };
    for phase in structure.phases() {
        enter_phase(execution, phase)?;
        if matches!(phase.kind(), PhaseKind::Actions(_)) {
            break;
        }
    }
    Ok(())
}

/// Enters a phase: `PhaseBegan`, and then its steps in order if it is automatic.
fn enter_phase(execution: &mut Execution, phase: &Phase) -> Result<(), StepError> {
    execution.apply(Event::PhaseBegan {
        phase: phase.id().clone(),
    })?;
    if let PhaseKind::Automatic(steps) = phase.kind() {
        run_steps(execution, steps)?;
    }
    Ok(())
}

/// The phases after the current one: where the Game is waiting, which is an action phase of the
/// Turn. Rejected, like a Game that has no Turn in progress, when it is none.
fn phases_after_current<'a>(
    definition: &'a RulesetDefinition,
    execution: &Execution,
) -> Result<&'a [Phase], Rejection> {
    let current = execution.state.phase();
    let phases: &[Phase] = definition
        .turn()
        .map_or(&[], |structure| structure.phases());
    let position = phases
        .iter()
        .position(|phase| Some(phase.id()) == current)
        .ok_or(Rejection::EventRejected(ApplyError::NoTurnInProgress))?;
    Ok(phases.get(position.saturating_add(1)..).unwrap_or(&[]))
}

/// Moves to the next action phase of the Turn: enters each phase after the current one, running
/// the automatic ones, and stops at the first phase with actions (ADR-0042). A load-checked
/// Ruleset never takes this from the last action phase; if it happens, the Action is rejected
/// before any Event.
pub(super) fn advance_phase(
    definition: &RulesetDefinition,
    execution: &mut Execution,
) -> Result<(), Rejection> {
    let rest = phases_after_current(definition, execution)?;
    let Some(target) = rest
        .iter()
        .position(|phase| matches!(phase.kind(), PhaseKind::Actions(_)))
    else {
        return Err(Rejection::NoLaterActionPhase);
    };
    for phase in rest.iter().take(target.saturating_add(1)) {
        enter_phase(execution, phase)?;
    }
    Ok(())
}

/// Ends the Turn in progress: runs every phase after the current one, then begins the Turn of the
/// next Player in seat order, one Turn Number later.
pub(super) fn end_turn(
    definition: &RulesetDefinition,
    execution: &mut Execution,
) -> Result<(), Rejection> {
    let event_rejected = Rejection::EventRejected;
    let active = execution.state.active_player();
    let Some(next_player) = execution.state.player_order().next_after(active) else {
        return Err(event_rejected(ApplyError::UnknownPlayer { player: active }));
    };
    let Some(turn) = execution.state.turn_number() else {
        return Err(event_rejected(ApplyError::NoTurnInProgress));
    };
    let Some(next_turn) = turn.checked_next() else {
        return Err(Rejection::TurnNumberOverflow);
    };

    // Every phase after the current one: the automatic ones run, and the action phases are passed
    // over without beginning (ADR-0042).
    for phase in phases_after_current(definition, execution)? {
        if matches!(phase.kind(), PhaseKind::Automatic(_)) {
            enter_phase(execution, phase)?;
        }
    }
    Ok(begin_turn(definition, execution, next_player, next_turn)?)
}

/// Runs steps in order.
fn run_steps(execution: &mut Execution, steps: &[AutomaticStep]) -> Result<(), StepError> {
    for step in steps {
        run_step(execution, step)?;
    }
    Ok(())
}

fn run_step(execution: &mut Execution, step: &AutomaticStep) -> Result<(), StepError> {
    match step {
        AutomaticStep::SetPlayerState {
            player,
            state,
            value,
        } => {
            for player in execution.select(*player) {
                execution.set_player_state(player, state, value)?;
            }
        }
        AutomaticStep::AddPlayerState {
            player,
            state,
            amount,
        } => {
            for player in execution.select(*player) {
                execution.add_player_state(player, state, *amount)?;
            }
        }
        AutomaticStep::Untap { player, zones } => {
            let tapped = StateName::new("tapped");
            for player in execution.select(*player) {
                for zone in zones {
                    set_state_in_zone(execution, player, zone, &tapped, &FieldValue::Bool(false))?;
                }
            }
        }
        AutomaticStep::SetState {
            player,
            zone,
            state,
            value,
        } => {
            for player in execution.select(*player) {
                set_state_in_zone(execution, player, zone, state, value)?;
            }
        }
        AutomaticStep::Draw {
            player,
            from,
            to,
            count,
        } => {
            for player in execution.select(*player) {
                draw(execution, player, from, to, count.get())?;
            }
        }
        AutomaticStep::Move {
            player,
            from,
            to,
            count,
            position,
        } => {
            for player in execution.select(*player) {
                for _ in 0..count.get() {
                    let moved = match position {
                        Position::Top => move_top_card(execution, player, from, to)?,
                    };
                    if !moved {
                        break;
                    }
                }
            }
        }
        AutomaticStep::If {
            condition,
            then,
            otherwise,
        } => {
            let branch = if holds(execution, *condition) {
                then
            } else {
                otherwise
            };
            run_steps(execution, branch)?;
        }
    }
    Ok(())
}

/// Whether a condition holds in the current state.
fn holds(execution: &Execution, condition: Condition) -> bool {
    match condition {
        Condition::FirstTurn => execution.state.turn_number() == Some(TurnNumber::FIRST),
    }
}

/// Draws for `player`: moves `count` cards one at a time from the Top of `from` onto the Top of
/// `to`. An empty `from` emits `DrawFromEmptyZone` and stops (ADR-0014).
pub(super) fn draw(
    execution: &mut Execution,
    player: PlayerId,
    from: &ZoneId,
    to: &ZoneId,
    count: u32,
) -> Result<(), ApplyError> {
    for _ in 0..count {
        if !move_top_card(execution, player, from, to)? {
            execution.apply(Event::DrawFromEmptyZone {
                player,
                zone: from.clone(),
            })?;
            break;
        }
    }
    Ok(())
}

/// Moves the Top card of `from` onto the Top of `to`. Returns false, changing nothing, if `from`
/// is empty.
fn move_top_card(
    execution: &mut Execution,
    player: PlayerId,
    from: &ZoneId,
    to: &ZoneId,
) -> Result<bool, ApplyError> {
    let Some(card) = execution.cards_of(player, from)?.last().copied() else {
        return Ok(false);
    };
    execution.apply(Event::CardMoved {
        card,
        player,
        from: from.clone(),
        to: to.clone(),
    })?;
    Ok(true)
}

/// Sets `state` to `value` on the cards of a zone, bottom first, that have the state and do not
/// have that value yet.
fn set_state_in_zone(
    execution: &mut Execution,
    player: PlayerId,
    zone: &ZoneId,
    state: &StateName,
    value: &FieldValue,
) -> Result<(), ApplyError> {
    let changing: Vec<_> = execution
        .cards_of(player, zone)?
        .into_iter()
        .filter(|&card| {
            execution
                .state
                .card(card)
                .and_then(|instance| instance.state_value(state))
                .is_some_and(|current| current != value)
        })
        .collect();
    for card in changing {
        execution.apply(Event::CardStateChanged {
            card,
            state: state.clone(),
            value: value.clone(),
        })?;
    }
    Ok(())
}
