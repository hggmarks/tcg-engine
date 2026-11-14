//! Actions of the declarative Ruleset and how it runs them.
//!
//! An Action names the Player who takes it, an action the ruleset declares and one argument for
//! each of its params. The Ruleset checks it in a fixed order that starts with whether the Game is
//! over (see [`Rejection`]), then pays the action's costs and runs its effects against a clone of
//! the GameState, so a rejected Action changes nothing (ADR-0002). An effect may be an `if` that
//! picks the effects to run from an expression (ADR-0016).

use std::fmt;

use super::action_effects;
use super::arguments;
use super::creation::DeclarativeRuleset;
use super::event::Event;
use super::execution::Execution;
use super::game_end;
use super::state::{ApplyError, CardInstanceId, GameState};
use super::triggers;
use crate::player::PlayerId;
use crate::ruleset::{EventTrace, Ruleset, Transition};
use crate::ruleset_definition::{ActionId, CardTypeId, ParamName, PhaseKind, StateName, ZoneId};

/// An Action of the declarative Ruleset: `player` takes the declared action `action`, with one
/// argument for each of its params, in param order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub player: PlayerId,
    pub action: ActionId,
    /// One per param of the action, in param order.
    pub arguments: Vec<Argument>,
}

/// The value an Action supplies for one param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Argument {
    /// A card instance.
    Card(CardInstanceId),
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
    /// The Action has a different number of arguments than the action has params.
    ArgumentCountMismatch { expected: u64, found: u64 },
    /// The argument for `param` names a card instance that does not exist.
    UnknownCard {
        param: ParamName,
        card: CardInstanceId,
    },
    /// The card for `param` belongs to another Player than the one acting.
    NotCardOwner {
        param: ParamName,
        card: CardInstanceId,
        owner: PlayerId,
    },
    /// The card for `param` belongs to the acting Player, but the param takes only an Opponent's
    /// card (`owner: opponent`).
    NotOpponentCard {
        param: ParamName,
        card: CardInstanceId,
    },
    /// The card for `param` is not in the param's `zone` of its owner.
    CardNotInZone {
        param: ParamName,
        card: CardInstanceId,
        zone: ZoneId,
    },
    /// The card for `param` is not of the param's card type.
    WrongCardType {
        param: ParamName,
        card: CardInstanceId,
        expected: CardTypeId,
        found: CardTypeId,
    },
    /// The card for `param` is of the right owner, zone and card type, but the `where` condition of
    /// the param does not hold for it.
    WhereConditionNotMet {
        param: ParamName,
        card: CardInstanceId,
    },
    /// The param has several alternatives and the card fits none of them.
    NoAlternativeMatched {
        param: ParamName,
        card: CardInstanceId,
    },
    /// A cost resolved to a negative count.
    NegativeCount { value: i64 },
    /// A tap cost needs `required` untapped cards in the acting Player's `zone` and there are only
    /// `available`.
    NotEnoughUntappedCards {
        zone: ZoneId,
        required: u64,
        available: u64,
    },
    /// Adding to the instance state `state` of `card` would go beyond what an int can hold, in
    /// either direction.
    StateOverflow {
        card: CardInstanceId,
        state: StateName,
    },
    /// Ending the Turn would take the Turn Number above its maximum.
    TurnNumberOverflow,
    /// Resolving triggered Card Effects took more than `cap` trigger resolutions in one
    /// transition (ADR-0023): the whole transition is refused and the GameState is untouched.
    TriggerCapExceeded { cap: u32 },
    /// An Event the Ruleset generated was rejected: the GameState does not fit this ruleset, an
    /// internal inconsistency that is never expected.
    EventRejected(ApplyError),
    /// The catalog or the GameState does not fit the ruleset: a card's definition is missing, or a
    /// ref reads a value the card does not have or of another type. An internal inconsistency that
    /// is never expected.
    CardDataMismatch { card: CardInstanceId },
    /// An expression has a value of another type than its operator or slot needs. Validation
    /// guarantees the types, so this is an internal inconsistency that is never expected.
    ExpressionTypeMismatch,
    /// A ref names a param that no argument was bound to. Validation guarantees every ref names a
    /// param of its action, so this is an internal inconsistency that is never expected.
    UnboundParam { param: ParamName },
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
            Self::ArgumentCountMismatch { expected, found } => {
                write!(f, "the action takes {expected} arguments, found {found}")
            }
            Self::UnknownCard { param, card } => write!(
                f,
                "the card instance {} for `{}` does not exist",
                card.0,
                param.as_str()
            ),
            Self::NotCardOwner { param, card, owner } => write!(
                f,
                "the card instance {} for `{}` belongs to Player {}",
                card.0,
                param.as_str(),
                owner.0
            ),
            Self::NotOpponentCard { param, card } => write!(
                f,
                "the card instance {} for `{}` belongs to the acting Player, not to an opponent",
                card.0,
                param.as_str()
            ),
            Self::CardNotInZone { param, card, zone } => write!(
                f,
                "the card instance {} for `{}` is not in the zone `{}` of its owner",
                card.0,
                param.as_str(),
                zone.as_str()
            ),
            Self::WrongCardType {
                param,
                card,
                expected,
                found,
            } => write!(
                f,
                "the card instance {} for `{}` is a `{}`, not a `{}`",
                card.0,
                param.as_str(),
                found.as_str(),
                expected.as_str()
            ),
            Self::WhereConditionNotMet { param, card } => write!(
                f,
                "the card instance {} for `{}` does not meet the `where` condition",
                card.0,
                param.as_str()
            ),
            Self::NoAlternativeMatched { param, card } => write!(
                f,
                "the card instance {} for `{}` fits none of its alternatives",
                card.0,
                param.as_str()
            ),
            Self::NegativeCount { value } => {
                write!(f, "a cost resolved to the negative count {value}")
            }
            Self::NotEnoughUntappedCards {
                zone,
                required,
                available,
            } => write!(
                f,
                "the cost needs {required} untapped cards in `{}`, found {available}",
                zone.as_str()
            ),
            Self::StateOverflow { card, state } => write!(
                f,
                "adding to the state `{}` of the card instance {} overflows",
                state.as_str(),
                card.0
            ),
            Self::TurnNumberOverflow => write!(f, "the Turn Number cannot go any higher"),
            Self::TriggerCapExceeded { cap } => write!(
                f,
                "triggered Card Effects took more than {cap} resolutions in one transition"
            ),
            Self::EventRejected(error) => write!(f, "an Event was rejected: {error}"),
            Self::CardDataMismatch { card } => write!(
                f,
                "the card instance {} does not fit the ruleset data",
                card.0
            ),
            Self::ExpressionTypeMismatch => write!(
                f,
                "an expression has a value of another type than its operator needs"
            ),
            Self::UnboundParam { param } => {
                write!(f, "the param `{}` has no argument", param.as_str())
            }
        }
    }
}

impl std::error::Error for Rejection {}

/// The actions the current phase lists, or `None` when the Game has no `turn`, no current phase,
/// a phase the definition does not know or one that is automatic.
pub(super) fn current_action_phase<'a>(
    ruleset: &'a DeclarativeRuleset,
    state: &GameState,
) -> Option<&'a [ActionId]> {
    let current = state.phase()?;
    let phase = ruleset
        .definition()
        .turn()?
        .phases()
        .iter()
        .find(|phase| phase.id() == current)?;
    match phase.kind() {
        PhaseKind::Actions(actions) => Some(actions),
        PhaseKind::Automatic(_) => None,
    }
}

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
        let listed_in_current_phase = current_action_phase(self, state)
            .is_some_and(|actions| actions.contains(&action.action));
        if !listed_in_current_phase {
            return Err(Rejection::ActionNotInCurrentPhase {
                action: action.action.clone(),
            });
        }

        let bound = arguments::bind(self, state, declared, action)?;

        let mut execution = Execution::new(state.clone());
        for cost in declared.costs() {
            action_effects::pay(self, &mut execution, &bound, action.player, cost)?;
        }
        for effect in declared.effects() {
            action_effects::run(self, &mut execution, &bound, effect)?;
        }
        triggers::resolve(self, &mut execution)?;
        game_end::end_game_if_lost(self, &mut execution)?;
        Ok(Transition {
            state: execution.state,
            trace: EventTrace::new(execution.events),
        })
    }
}
