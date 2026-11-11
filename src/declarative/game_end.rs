//! Ending the Game: evaluating the win conditions (ADR-0014, ADR-0016).
//!
//! The win conditions are evaluated once at the end of every accepted transition and of Game
//! creation, never one Event at a time: so a loss that happens together with another is not turned
//! into a win for whoever lost second. `draw_from_empty` reads the Events of that transition; an
//! expression reads the GameState the transition produced.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::event::Event;
use super::execution::Execution;
use super::expression;
use super::state::Loss;
use crate::player::PlayerId;
use crate::ruleset_definition::{Loser, ParamName, WinCondition, WinConditionWhen};

/// Ends the Game if some Player lost in the transition `execution` has run, by applying one
/// `GameEnded` Event.
///
/// For each Player in seat order, their reason is the first win condition, in declaration order,
/// that holds for them: a Player loses once. If any Player lost, the Game ends and the others win;
/// if all of them lost, it is a drawn game.
pub(super) fn end_game_if_lost(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
) -> Result<(), Rejection> {
    let mut losses: Vec<Loss> = Vec::new();
    for &player in execution.state.player_order().players() {
        for condition in ruleset.definition().win_conditions() {
            if loses(ruleset, execution, condition, player)? {
                losses.push(Loss {
                    player,
                    reason: condition.id().clone(),
                });
                break;
            }
        }
    }
    if losses.is_empty() {
        return Ok(());
    }
    execution
        .apply(Event::GameEnded { losses })
        .map_err(Rejection::EventRejected)
}

/// Whether `condition` makes `player` lose, given the Events of the transition and the GameState
/// it produced.
fn loses(
    ruleset: &DeclarativeRuleset,
    execution: &Execution,
    condition: &WinCondition,
    player: PlayerId,
) -> Result<bool, Rejection> {
    match (condition.when(), condition.loser()) {
        // The Owner of a `draw_from_empty` is the Player who drew.
        (WinConditionWhen::DrawFromEmpty(zone), Loser::Owner) => {
            Ok(execution.events.iter().any(|event| {
                matches!(
                    event,
                    Event::DrawFromEmptyZone { player: drew, zone: from }
                        if *drew == player && from == zone
                )
            }))
        }
        // The Owner of an expression is the owner of a card instance of the Subject, in any zone,
        // that makes it true.
        (WinConditionWhen::Expression { subject, condition }, Loser::Owner) => {
            let name = ParamName::new(subject.as_str());
            for instance in execution.state.cards() {
                if instance.owner() != player {
                    continue;
                }
                let is_subject = ruleset
                    .catalog()
                    .card(instance.definition())
                    .is_some_and(|definition| definition.card_type() == subject);
                if !is_subject {
                    continue;
                }
                let bound = BoundCards::of(name.clone(), instance.id());
                if expression::holds(ruleset, &execution.state, &bound, condition)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}
