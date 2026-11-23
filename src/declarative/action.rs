//! Actions of the declarative Ruleset and how it runs them.
//!
//! An Action names the Player who takes it, an action the ruleset declares and one argument for
//! each of its params. The Ruleset checks it in a fixed order that starts with whether the Game is
//! over (see [`Rejection`]), then pays the action's costs and runs its effects against a clone of
//! the GameState, so a rejected Action changes nothing (ADR-0002). An effect may be an `if` that
//! picks the effects to run from an expression (ADR-0016).

use std::fmt;

use super::action_effects;
use super::arguments::{self, BoundCards};
use super::creation::DeclarativeRuleset;
use super::decision::{Answer, DecisionKind};
use super::event::Event;
use super::execution::Execution;
use super::game_end;
use super::stack::{BoundCard, StackItem, StackSource};
use super::state::{ApplyError, CardInstanceId, GameState};
use super::triggers;
use crate::card_catalog::{ActivatedCardEffect, CardEffect};
use crate::player::PlayerId;
use crate::ruleset::{EventTrace, Ruleset, Transition};
use crate::ruleset_definition::{
    ActionId, CardTypeId, FieldName, ParamName, PhaseKind, PlayerStateId, StateName, Window,
    WindowId, ZoneId,
};

/// An Action of the declarative Ruleset: a Declared Action, an Answer to the decision a Resolution
/// is paused at (ADR-0033), a Pass, which resolves the top item of the Stack (ADR-0037), or an
/// Activate, which uses an activated ability (ADR-0038). While a decision is pending only Answers
/// are Legal. While none is and a Response Window is open, a Pass, the window's Declared Actions
/// and the Activates of cards in the Active Zone are, if the window allows abilities; with no
/// window open only Declared Actions are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `player` takes the declared action `action`, with one argument for each of its params, in
    /// param order.
    Declared {
        player: PlayerId,
        action: ActionId,
        /// One per param of the action, in param order.
        arguments: Vec<Argument>,
    },
    /// `player` answers the pending decision.
    Answer { player: PlayerId, answer: Answer },
    /// `player`, who has Priority, passes: the top item of the Stack resolves, or Fizzles.
    Pass { player: PlayerId },
    /// `player`, who has Priority, uses the activated ability of `card` at Ability Index `ability`
    /// (ADR-0038): its cost is paid at once and its effects are pushed as a Stack Item.
    Activate {
        player: PlayerId,
        card: CardInstanceId,
        ability: u32,
    },
}

impl Action {
    /// The Player who takes the Action.
    pub fn player(&self) -> PlayerId {
        match self {
            Self::Declared { player, .. }
            | Self::Answer { player, .. }
            | Self::Pass { player }
            | Self::Activate { player, .. } => *player,
        }
    }
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
    /// The Player is in the Game but is not the Deciding Player: the Active Player, or the one a
    /// pending decision asks.
    NotDecidingPlayer {
        player: PlayerId,
        deciding: PlayerId,
    },
    /// A Declared Action while a decision is pending: only its Answers are accepted.
    DeclaredWhileDecisionPending,
    /// An Answer, but no decision is pending.
    NoDecisionPending,
    /// The Answer picks option `index`, and the pending Choice has `options`.
    OptionOutOfRange { index: u32, options: u32 },
    /// The Answer is of another kind than the pending decision: `kind` is the one pending.
    WrongAnswerKind { kind: DecisionKind },
    /// The Answer picks a card that is not a Candidate of the pending Card Choice, unknown ids
    /// included (ADR-0034).
    CardNotCandidate { card: CardInstanceId },
    /// The ruleset declares no action with this id.
    UnknownAction { action: ActionId },
    /// A Pass while a decision is pending: only an Answer is accepted.
    PassWhileDecisionPending,
    /// A Pass, but no Response Window is open: the Stack is empty (ADR-0037).
    PassWithoutWindow,
    /// A Declared Action while a Response Window is open, but the window does not list it.
    ActionNotInWindow { action: ActionId },
    /// An Activate while a decision is pending: only an Answer is accepted (ADR-0038).
    ActivateWhileDecisionPending,
    /// An Activate, but no Response Window is open: the Stack is empty, which includes the Action
    /// phase (ADR-0038).
    ActivateWithoutWindow,
    /// An Activate in the open Response Window `window`, whose declaration does not have
    /// `abilities: true` (ADR-0038).
    AbilitiesNotInWindow { window: WindowId },
    /// An Activate whose Ability Index is out of range for the card's definition, or names an
    /// effect that is not activated (ADR-0038).
    NotAnActivatedAbility { card: CardInstanceId, ability: u32 },
    /// A `tap: { card }` cost on a card that is tapped already.
    CardAlreadyTapped { card: CardInstanceId },
    /// The action is declared, but the current phase does not list it: there is no current phase,
    /// the ruleset does not know it, or it is not a phase with actions. *(Amended by ADR-0037: or
    /// the action is listed only by a Response Window and no window is open.)*
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
    /// Adding to the int Player state `state` of `player` would go beyond what an int can hold, in
    /// either direction (ADR-0041).
    PlayerStateOverflow {
        player: PlayerId,
        state: PlayerStateId,
    },
    /// A derived read of the int field `field` of `card` would go beyond what an int can hold: its
    /// base value plus the Field Modifiers that are active, or the sum of those (ADR-0024).
    DerivedValueOverflow {
        card: CardInstanceId,
        field: FieldName,
    },
    /// Ending the Turn would take the Turn Number above its maximum.
    TurnNumberOverflow,
    /// Resolving triggered Card Effects took more than `cap` trigger resolutions in one Resolution,
    /// which spans the Answers of its pauses (ADR-0023, ADR-0033): the whole transition is refused
    /// and the GameState is untouched.
    TriggerCapExceeded { cap: u32 },
    /// An Event the Ruleset generated was rejected: the GameState does not fit this ruleset, an
    /// internal inconsistency that is never expected.
    EventRejected(ApplyError),
    /// The catalog or the GameState does not fit the ruleset: a card's definition is missing, or a
    /// ref reads a value the card does not have or of another type. An internal inconsistency that
    /// is never expected.
    CardDataMismatch { card: CardInstanceId },
    /// An effect of a ruleset action paused at a Choice. Validation refuses a Choice there, so this
    /// is an internal inconsistency that is never expected.
    ChoiceOutsideTriggeredEffect,
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
            Self::NotDecidingPlayer { player, deciding } => write!(
                f,
                "Player {} cannot act: Player {} is the Deciding Player",
                player.0, deciding.0
            ),
            Self::DeclaredWhileDecisionPending => {
                write!(f, "a decision is pending: only an Answer is accepted")
            }
            Self::NoDecisionPending => write!(f, "no decision is pending to answer"),
            Self::PassWhileDecisionPending => {
                write!(f, "a decision is pending: only an Answer is accepted")
            }
            Self::PassWithoutWindow => write!(f, "no Response Window is open to pass in"),
            Self::ActivateWhileDecisionPending => {
                write!(f, "a decision is pending: only an Answer is accepted")
            }
            Self::ActivateWithoutWindow => {
                write!(f, "no Response Window is open to activate an ability in")
            }
            Self::AbilitiesNotInWindow { window } => write!(
                f,
                "the open Response Window `{}` does not allow abilities",
                window.as_str()
            ),
            Self::NotAnActivatedAbility { card, ability } => write!(
                f,
                "the effect {ability} of the card instance {} is not an activated ability",
                card.0
            ),
            Self::ActionNotInWindow { action } => write!(
                f,
                "the open Response Window does not list the action `{}`",
                action.as_str()
            ),
            Self::CardAlreadyTapped { card } => {
                write!(f, "the card instance {} is tapped already", card.0)
            }
            Self::OptionOutOfRange { index, options } => {
                write!(
                    f,
                    "option {index} is out of range: the Choice has {options}"
                )
            }
            Self::WrongAnswerKind { kind } => {
                let asked = match kind {
                    DecisionKind::Option => "an option",
                    DecisionKind::Card => "a card",
                };
                write!(
                    f,
                    "the pending decision asks for {asked}, not this kind of Answer"
                )
            }
            Self::CardNotCandidate { card } => {
                write!(f, "the card instance {} is not a candidate", card.0)
            }
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
            Self::PlayerStateOverflow { player, state } => write!(
                f,
                "adding to the state `{}` of Player {} overflows",
                state.as_str(),
                player.0
            ),
            Self::DerivedValueOverflow { card, field } => write!(
                f,
                "reading the field `{}` of the card instance {} overflows",
                field.as_str(),
                card.0
            ),
            Self::TurnNumberOverflow => write!(f, "the Turn Number cannot go any higher"),
            Self::TriggerCapExceeded { cap } => write!(
                f,
                "triggered Card Effects took more than {cap} resolutions in one Resolution"
            ),
            Self::EventRejected(error) => write!(f, "an Event was rejected: {error}"),
            Self::CardDataMismatch { card } => write!(
                f,
                "the card instance {} does not fit the ruleset data",
                card.0
            ),
            Self::ChoiceOutsideTriggeredEffect => {
                write!(f, "an effect of a ruleset action paused at a Choice")
            }
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
        let player = action.player();
        if !state.player_order().contains(player) {
            return Err(Rejection::UnknownPlayer { player });
        }
        if state.deciding_player() != player {
            return Err(Rejection::NotDecidingPlayer {
                player,
                deciding: state.deciding_player(),
            });
        }
        match action {
            Action::Declared {
                player,
                action: id,
                arguments,
            } => {
                if state.pending_decision().is_some() {
                    return Err(Rejection::DeclaredWhileDecisionPending);
                }
                self.declare(state, *player, id, arguments)
            }
            Action::Answer { player, answer } => self.answer(state, *player, *answer),
            Action::Pass { .. } => {
                if state.pending_decision().is_some() {
                    return Err(Rejection::PassWhileDecisionPending);
                }
                self.pass(state)
            }
            Action::Activate {
                player,
                card,
                ability,
            } => {
                if state.pending_decision().is_some() {
                    return Err(Rejection::ActivateWhileDecisionPending);
                }
                self.activate(state, *player, *card, *ability)
            }
        }
    }
}

impl DeclarativeRuleset {
    /// A Declared Action: pays the costs, runs the effects, then resolves the triggers.
    fn declare(
        &self,
        state: &GameState,
        player: PlayerId,
        id: &ActionId,
        arguments: &[Argument],
    ) -> Result<Transition<GameState, Event>, Rejection> {
        let definition = self.definition();
        let Some(declared) = definition.action(id) else {
            return Err(Rejection::UnknownAction { action: id.clone() });
        };
        // While a window is open its `actions` are what can be taken; otherwise the phase's.
        let window = self.open_window(state);
        match window {
            Some(window) if !window.actions().contains(id) => {
                return Err(Rejection::ActionNotInWindow { action: id.clone() });
            }
            None => {
                let listed =
                    current_action_phase(self, state).is_some_and(|actions| actions.contains(id));
                if !listed {
                    return Err(Rejection::ActionNotInCurrentPhase { action: id.clone() });
                }
            }
            Some(_) => {}
        }

        let bound = arguments::bind(self, state, declared, player, arguments)?;

        let mut execution = Execution::new(state.clone());
        for cost in declared.costs() {
            action_effects::pay(self, &mut execution, &bound, player, cost)?;
        }
        // An action taken in a window, or one that opens a window, is pushed: its cost is paid, and
        // its effects wait on the Stack as an item (ADR-0037).
        let pushed = window.is_some() || self.definition().window_opened_by(id).is_some();
        if pushed {
            // The zones are read once the cost was paid: a cost may have moved the card.
            let mut cards = Vec::with_capacity(declared.params().len());
            for param in declared.params() {
                let card = bound.card(param.name())?;
                let Some(zone) = execution.state.zone_of(card) else {
                    return Err(Rejection::CardDataMismatch { card });
                };
                cards.push(BoundCard {
                    param: param.name().clone(),
                    card,
                    zone: zone.clone(),
                    left: false,
                });
            }
            let item = StackItem {
                controller: player,
                source: StackSource::Action(id.clone()),
                bound: cards,
            };
            execution
                .apply(Event::ItemPushed { item })
                .map_err(Rejection::EventRejected)?;
        } else {
            for effect in declared.effects() {
                action_effects::run(self, &mut execution, &bound, effect)?;
            }
        }
        let paused = triggers::resolve(self, &mut execution)?;
        self.finish(execution, paused)
    }

    /// The Response Window that is open: the one whose `opened_by` holds the source of the bottom
    /// item of the Stack. `None` when the Stack is empty. Derived, never stored (ADR-0037).
    pub(super) fn open_window(&self, state: &GameState) -> Option<&Window> {
        // An ability is never at the bottom: it can only be used inside a window. Never a panic.
        let StackSource::Action(opener) = &state.stack().first()?.source else {
            return None;
        };
        self.definition().window_opened_by(opener)
    }

    /// An Activate: uses the activated ability of `card` at Ability Index `ability` for `player`,
    /// who has Priority (ADR-0038). The checks run in this order: a window is open, it allows
    /// abilities, the card exists, the index is an activated effect, the Player owns the card, the
    /// card is in the Active Zone, then each cost in order. The costs are paid at once with `self`
    /// bound to the card, then the ability is pushed as a Stack Item that binds `self` in the zone
    /// the card is in once the cost was paid, then the triggers the cost woke resolve.
    fn activate(
        &self,
        state: &GameState,
        player: PlayerId,
        card: CardInstanceId,
        ability: u32,
    ) -> Result<Transition<GameState, Event>, Rejection> {
        let Some(window) = self.open_window(state) else {
            return Err(Rejection::ActivateWithoutWindow);
        };
        if !window.abilities() {
            return Err(Rejection::AbilitiesNotInWindow {
                window: window.id().clone(),
            });
        }
        let this = ParamName::new("self");
        let Some(instance) = state.card(card) else {
            return Err(Rejection::UnknownCard { param: this, card });
        };
        let activated = self.activated_effect(state, card, ability)?;
        if instance.owner() != player {
            return Err(Rejection::NotCardOwner {
                param: this,
                card,
                owner: instance.owner(),
            });
        }
        let Some(active) = self.definition().effects_active_zone() else {
            return Err(Rejection::CardDataMismatch { card });
        };
        if state.zone_of(card) != Some(active) {
            return Err(Rejection::CardNotInZone {
                param: this,
                card,
                zone: active.clone(),
            });
        }

        let bound = BoundCards::of(this.clone(), card);
        let mut execution = Execution::new(state.clone());
        for cost in activated.costs() {
            action_effects::pay(self, &mut execution, &bound, player, cost)?;
        }
        // The zone is read once the cost was paid: a cost may have moved the card.
        let Some(zone) = execution.state.zone_of(card) else {
            return Err(Rejection::CardDataMismatch { card });
        };
        let item = StackItem {
            controller: player,
            source: StackSource::Ability { card, ability },
            bound: vec![BoundCard {
                param: this,
                card,
                zone: zone.clone(),
                left: false,
            }],
        };
        execution
            .apply(Event::ItemPushed { item })
            .map_err(Rejection::EventRejected)?;
        let paused = triggers::resolve(self, &mut execution)?;
        self.finish(execution, paused)
    }

    /// The activated Card Effect at Ability Index `ability` of `card`: a typed Rejection when the
    /// catalog has no such card or the index is not an activated effect.
    fn activated_effect(
        &self,
        state: &GameState,
        card: CardInstanceId,
        ability: u32,
    ) -> Result<&ActivatedCardEffect, Rejection> {
        let instance = state
            .card(card)
            .ok_or(Rejection::CardDataMismatch { card })?;
        let definition = self
            .catalog()
            .card(instance.definition())
            .ok_or(Rejection::CardDataMismatch { card })?;
        let index = usize::try_from(ability).ok();
        match index.and_then(|index| definition.effects().get(index)) {
            Some(CardEffect::Activated(activated)) => Ok(activated),
            _ => Err(Rejection::NotAnActivatedAbility { card, ability }),
        }
    }

    /// A Pass: the top item of the Stack resolves, or Fizzles when a card it bound has left. A
    /// resolving item is popped first, so its effects never mark it; then the effects run, then the
    /// triggers they wake resolve.
    fn pass(&self, state: &GameState) -> Result<Transition<GameState, Event>, Rejection> {
        let Some(item) = state.stack().last() else {
            return Err(Rejection::PassWithoutWindow);
        };
        let mut execution = Execution::new(state.clone());
        if item.fizzles() {
            execution
                .apply(Event::ItemFizzled { item: item.clone() })
                .map_err(Rejection::EventRejected)?;
        } else {
            execution
                .apply(Event::ItemResolved { item: item.clone() })
                .map_err(Rejection::EventRejected)?;
            let bound = BoundCards::of_item(&item.bound, item.controller);
            let effects = match &item.source {
                StackSource::Action(id) => {
                    let Some(declared) = self.definition().action(id) else {
                        return Err(Rejection::UnknownAction { action: id.clone() });
                    };
                    declared.effects()
                }
                StackSource::Ability { card, ability } => {
                    self.activated_effect(state, *card, *ability)?.body()
                }
            };
            for effect in effects {
                action_effects::run(self, &mut execution, &bound, effect)?;
            }
        }
        let paused = triggers::resolve(self, &mut execution)?;
        self.finish(execution, paused)
    }

    /// An Answer: checks it against what the Choice asks (its kind, then its option or its card),
    /// applies `DecisionAnswered` and goes on with the paused Resolution.
    fn answer(
        &self,
        state: &GameState,
        player: PlayerId,
        answer: Answer,
    ) -> Result<Transition<GameState, Event>, Rejection> {
        let Some(pending) = state.pending_decision() else {
            return Err(Rejection::NoDecisionPending);
        };
        let paused = pending.paused();
        let asked = triggers::asked(self, state, paused)?;
        // The kind the GameState holds must be the one of the Choice at the path.
        let consistent = matches!(
            (&asked, pending.kind()),
            (triggers::Asked::Options(_), DecisionKind::Option)
                | (triggers::Asked::Candidates(_), DecisionKind::Card)
        );
        if !consistent {
            return Err(Rejection::CardDataMismatch {
                card: paused.trigger.card,
            });
        }
        match (asked, answer) {
            (triggers::Asked::Options(options), Answer::Option(index)) => {
                if index >= options {
                    return Err(Rejection::OptionOutOfRange { index, options });
                }
            }
            (triggers::Asked::Candidates(candidates), Answer::Card(card)) => {
                if !candidates.contains(&card) {
                    return Err(Rejection::CardNotCandidate { card });
                }
            }
            _ => {
                return Err(Rejection::WrongAnswerKind {
                    kind: pending.kind(),
                })
            }
        }
        let mut execution = Execution::new(state.clone());
        execution
            .apply(Event::DecisionAnswered { player, answer })
            .map_err(Rejection::EventRejected)?;
        let again = triggers::resume(self, &mut execution, paused, answer)?;
        self.finish(execution, again)
    }

    /// Ends a transition: a pause is applied as the last Event, then the win conditions run.
    fn finish(
        &self,
        mut execution: Execution,
        pause: Option<triggers::Pause>,
    ) -> Result<Transition<GameState, Event>, Rejection> {
        if let Some(pause) = pause {
            execution
                .apply(Event::DecisionPending {
                    decider: pause.decider,
                    kind: pause.kind,
                    paused: pause.paused,
                })
                .map_err(Rejection::EventRejected)?;
        }
        game_end::end_game_if_lost(self, &mut execution)?;
        Ok(Transition {
            state: execution.state,
            trace: EventTrace::new(execution.events),
        })
    }
}
