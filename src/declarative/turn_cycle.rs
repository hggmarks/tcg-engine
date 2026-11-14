//! The Turn cycle: beginning a Turn, running automatic phases, and ending a Turn (ADR-0012).
//!
//! A Turn begins with `TurnBegan`; then each phase from the first begins with `PhaseBegan` and, if
//! it is automatic, runs its steps in order. The first phase with actions is where the Game waits
//! for the Active Player. `end_turn` runs every phase after it, then begins the next Player's Turn,
//! all as one transition.

use super::action::Rejection;
use super::event::Event;
use super::execution::Execution;
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
) -> Result<(), ApplyError> {
    execution.apply(Event::TurnBegan { player, turn })?;
    let Some(structure) = definition.turn() else {
        return Ok(());
    };
    for phase in structure.phases() {
        execution.apply(Event::PhaseBegan {
            phase: phase.id().clone(),
        })?;
        match phase.kind() {
            PhaseKind::Automatic(steps) => run_steps(execution, steps)?,
            PhaseKind::Actions(_) => break,
        }
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

    let current = execution.state.phase().cloned();
    let phases: &[Phase] = definition
        .turn()
        .map_or(&[], |structure| structure.phases());
    let Some(position) = phases
        .iter()
        .position(|phase| Some(phase.id()) == current.as_ref())
    else {
        // The Ruleset only ends a Turn from the phase that waits for actions.
        return Err(event_rejected(ApplyError::NoTurnInProgress));
    };
    // Every phase after the current one; all of them are automatic, because a turn has exactly one
    // phase with actions and the Game is waiting in it.
    for phase in phases.iter().skip(position.saturating_add(1)) {
        execution
            .apply(Event::PhaseBegan {
                phase: phase.id().clone(),
            })
            .map_err(event_rejected)?;
        if let PhaseKind::Automatic(steps) = phase.kind() {
            run_steps(execution, steps).map_err(event_rejected)?;
        }
    }
    begin_turn(definition, execution, next_player, next_turn).map_err(event_rejected)
}

/// Runs steps in order.
fn run_steps(execution: &mut Execution, steps: &[AutomaticStep]) -> Result<(), ApplyError> {
    for step in steps {
        run_step(execution, step)?;
    }
    Ok(())
}

fn run_step(execution: &mut Execution, step: &AutomaticStep) -> Result<(), ApplyError> {
    match step {
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
