//! Ending the Game: evaluating the win conditions (ADR-0014).
//!
//! The win conditions are evaluated once at the end of every accepted transition and of Game
//! creation, over the Events of that transition only, never one Event at a time: so a loss that
//! happens together with another is not turned into a win for whoever lost second.

use super::event::Event;
use super::execution::Execution;
use super::state::{ApplyError, Loss};
use crate::player::PlayerId;
use crate::ruleset_definition::{Loser, RulesetDefinition, WinCondition, WinConditionWhen};

/// Ends the Game if some Player lost in the transition `execution` has run, by applying one
/// `GameEnded` Event.
///
/// For each Player in seat order, their reason is the first win condition, in declaration order,
/// that holds for them: a Player loses once. If any Player lost, the Game ends and the others win;
/// if all of them lost, it is a drawn game.
pub(super) fn end_game_if_lost(
    definition: &RulesetDefinition,
    execution: &mut Execution,
) -> Result<(), ApplyError> {
    let losses: Vec<Loss> = execution
        .state
        .player_order()
        .players()
        .iter()
        .filter_map(|&player| {
            definition
                .win_conditions()
                .iter()
                .find(|condition| loses(condition, player, &execution.events))
                .map(|condition| Loss {
                    player,
                    reason: condition.id().clone(),
                })
        })
        .collect();
    if losses.is_empty() {
        return Ok(());
    }
    execution.apply(Event::GameEnded { losses })
}

/// Whether `condition` makes `player` lose, given the Events of the transition.
fn loses(condition: &WinCondition, player: PlayerId, events: &[Event]) -> bool {
    match (condition.when(), condition.loser()) {
        // The Owner of a `draw_from_empty` is the Player who drew.
        (WinConditionWhen::DrawFromEmpty(zone), Loser::Owner) => events.iter().any(|event| {
            matches!(
                event,
                Event::DrawFromEmptyZone { player: drew, zone: from }
                    if *drew == player && from == zone
            )
        }),
    }
}
