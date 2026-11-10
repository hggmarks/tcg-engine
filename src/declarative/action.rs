//! Actions of the declarative Ruleset and how it runs them.
//!
//! An Action names the Player who takes it and an action the ruleset declares. The Ruleset checks
//! it in a fixed order that starts with whether the Game is over (see [`Rejection`]), then runs the action's effects against a clone of the
//! GameState, so a rejected Action changes nothing (ADR-0002).

use std::fmt;

use super::creation::DeclarativeRuleset;
use super::event::Event;
use super::execution::Execution;
use super::game_end;
use super::state::{ApplyError, GameState};
use super::turn_cycle;
use crate::player::PlayerId;
use crate::ruleset::{EventTrace, Ruleset, Transition};
use crate::ruleset_definition::{ActionId, Effect, PhaseKind};

/// An Action of the declarative Ruleset: `player` takes the declared action `action`. Parameters
/// and costs arrive with ticket [10].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub player: PlayerId,
    pub action: ActionId,
}

/// The reason the declarative Ruleset refuses an Action, in the order the checks run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// The Game has ended: every Action is rejected, whoever takes it and whatever it is.
    GameOver,
    /// The Player is not in the Game.
    UnknownPlayer { player: PlayerId },
    /// The Player is in the Game but it is not their Turn.
    NotActivePlayer { player: PlayerId, active: PlayerId },
    /// The ruleset declares no action with this id.
    UnknownAction { action: ActionId },
    /// The action is declared, but the current phase does not list it: there is no current phase,
    /// the ruleset does not know it, or it is not a phase with actions.
    ActionNotInCurrentPhase { action: ActionId },
    /// Ending the Turn would take the Turn Number above its maximum.
    TurnNumberOverflow,
    /// An Event the Ruleset generated was rejected: the GameState does not fit this ruleset, an
    /// internal inconsistency that is never expected.
    EventRejected(ApplyError),
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameOver => write!(f, "the Game is over"),
            Self::UnknownPlayer { player } => write!(f, "Player {} is not in the Game", player.0),
            Self::NotActivePlayer { player, active } => write!(
                f,
                "Player {} cannot act: it is the Turn of Player {}",
                player.0, active.0
            ),
            Self::UnknownAction { action } => {
                write!(f, "the ruleset has no action `{}`", action.as_str())
            }
            Self::ActionNotInCurrentPhase { action } => write!(
                f,
                "the action `{}` is not allowed in the current phase",
                action.as_str()
            ),
            Self::TurnNumberOverflow => write!(f, "the Turn Number cannot go any higher"),
            Self::EventRejected(error) => write!(f, "an Event was rejected: {error}"),
        }
    }
}

impl std::error::Error for Rejection {}

impl Ruleset for DeclarativeRuleset {
    type State = GameState;
    type Action = Action;
    type Event = Event;
    type Rejection = Rejection;

    fn transition(
        &self,
        state: &GameState,
        action: &Action,
    ) -> Result<Transition<GameState, Event>, Rejection> {
        if state.outcome().is_some() {
            return Err(Rejection::GameOver);
        }
        if !state.player_order().contains(action.player) {
            return Err(Rejection::UnknownPlayer {
                player: action.player,
            });
        }
        if state.active_player() != action.player {
            return Err(Rejection::NotActivePlayer {
                player: action.player,
                active: state.active_player(),
            });
        }
        let definition = self.definition();
        let Some(declared) = definition.action(&action.action) else {
            return Err(Rejection::UnknownAction {
                action: action.action.clone(),
            });
        };
        let listed_in_current_phase = state
            .phase()
            .and_then(|current| {
                definition
                    .turn()?
                    .phases()
                    .iter()
                    .find(|phase| phase.id() == current)
            })
            .is_some_and(|phase| match phase.kind() {
                PhaseKind::Actions(actions) => actions.contains(&action.action),
                PhaseKind::Automatic(_) => false,
            });
        if !listed_in_current_phase {
            return Err(Rejection::ActionNotInCurrentPhase {
                action: action.action.clone(),
            });
        }

        let mut execution = Execution::new(state.clone());
        for effect in declared.effects() {
            match effect {
                Effect::EndTurn => turn_cycle::end_turn(definition, &mut execution)?,
            }
        }
        game_end::end_game_if_lost(definition, &mut execution).map_err(Rejection::EventRejected)?;
        Ok(Transition {
            state: execution.state,
            trace: EventTrace::new(execution.events),
        })
    }
}
