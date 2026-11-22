//! Acting from an Observation (ADR-0030): a Viewer's Legal Actions with Observed Cards, and an
//! Observed Action turned back into the engine Action. The mapping is read only through the
//! Observer; legality stays with `transition`.

use super::error::ObservationError;
use super::observed_event::{ObservedAnswer, TakenArgument};
use super::observer::{sees, Observer};
use super::viewer::{ObservedCard, Viewer};
use crate::declarative::action::{Action, Argument};
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::decision::Answer;
use crate::declarative::state::GameState;
use crate::player::PlayerId;
use crate::ruleset::LegalActions;
use crate::ruleset_definition::ActionId;

/// An Action as a Viewer expresses it (ADR-0030): the engine Action with an Observed Card for each
/// card argument of a Declared Action, and the option index of an Answer (ADR-0033).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedAction {
    Declared {
        player: PlayerId,
        action: ActionId,
        /// One per param of the action, in param order.
        arguments: Vec<ObservedArgument>,
    },
    Answer {
        player: PlayerId,
        answer: ObservedAnswer,
    },
    /// The Player with Priority passes.
    Pass { player: PlayerId },
}

/// The value an Observed Action supplies for one param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedArgument {
    /// An Observed Card.
    Card(ObservedCard),
}

impl DeclarativeRuleset {
    /// The Viewer's Legal Actions in Listing Order, each card argument as its Observed Card and
    /// each Answer as its option index.
    ///
    /// It is `legal_actions(state)` filtered to the Actions whose Player is the Viewer, so the
    /// engine's order is kept and a Player that is not the Deciding Player gets an empty list. The
    /// checks come in this order: the Viewer is a Player of `state` (`UnknownViewer`); then, per
    /// Legal Action in order and per argument in param order, a card in a zone the Viewer cannot
    /// see is `LegalActionNamesUnseenCard`, and a seen card the Observer holds no handle for, or a
    /// card absent from `state`, is `ObserverOutOfStep`. The first failure fails the whole call.
    pub fn observed_legal_actions(
        &self,
        observer: &Observer,
        state: &GameState,
    ) -> Result<Vec<ObservedAction>, ObservationError> {
        let viewer = observer.viewer();
        let Viewer::Player(me) = viewer;
        if !state.player_order().contains(me) {
            return Err(ObservationError::UnknownViewer { viewer });
        }
        let mut listed = Vec::new();
        for legal in self.legal_actions(state) {
            if legal.player() != me {
                continue;
            }
            let (player, action, arguments) = match legal {
                Action::Pass { player } => {
                    listed.push(ObservedAction::Pass { player });
                    continue;
                }
                Action::Answer {
                    player,
                    answer: Answer::Option(index),
                } => {
                    listed.push(ObservedAction::Answer {
                        player,
                        answer: ObservedAnswer::Option(index),
                    });
                    continue;
                }
                // The decider sees every candidate (checked when the ruleset loads), so it holds a
                // handle for each; a missing one is an inconsistency, not a leak.
                Action::Answer {
                    player,
                    answer: Answer::Card(id),
                } => {
                    let handle = observer
                        .observed_card(id)
                        .ok_or(ObservationError::ObserverOutOfStep)?;
                    listed.push(ObservedAction::Answer {
                        player,
                        answer: ObservedAnswer::Card(TakenArgument::Card(handle)),
                    });
                    continue;
                }
                Action::Declared {
                    player,
                    action,
                    arguments,
                } => (player, action, arguments),
            };
            let mut observed = Vec::with_capacity(arguments.len());
            for &Argument::Card(id) in &arguments {
                let instance = state.card(id).ok_or(ObservationError::ObserverOutOfStep)?;
                let zone = state
                    .zone_of(id)
                    .ok_or(ObservationError::ObserverOutOfStep)?;
                if !sees(self, viewer, instance.owner(), zone)? {
                    return Err(ObservationError::LegalActionNamesUnseenCard { action });
                }
                let handle = observer
                    .observed_card(id)
                    .ok_or(ObservationError::ObserverOutOfStep)?;
                observed.push(ObservedArgument::Card(handle));
            }
            listed.push(ObservedAction::Declared {
                player,
                action,
                arguments: observed,
            });
        }
        Ok(listed)
    }

    /// The engine Action an Observed Action stands for. The checks come in this order: the
    /// Observed Action's Player is the Viewer (`ActionOfAnotherPlayer`); then each argument in
    /// order, a handle the Viewer does not hold is `UnknownObservedCard`; an Answer that names a
    /// hidden card is `AnswerNamesHiddenCard`. The action id, the arity, the option index and the
    /// legality are not checked: `transition` decides them.
    pub fn translate_action(
        &self,
        observer: &Observer,
        action: &ObservedAction,
    ) -> Result<Action, ObservationError> {
        let Viewer::Player(me) = observer.viewer();
        match action {
            ObservedAction::Pass { player } => {
                if *player != me {
                    return Err(ObservationError::ActionOfAnotherPlayer { player: *player });
                }
                Ok(Action::Pass { player: *player })
            }
            ObservedAction::Answer { player, answer } => {
                if *player != me {
                    return Err(ObservationError::ActionOfAnotherPlayer { player: *player });
                }
                let answer = match answer {
                    ObservedAnswer::Option(index) => Answer::Option(*index),
                    ObservedAnswer::Card(TakenArgument::Card(card)) => {
                        let id = observer
                            .card_instance(*card)
                            .ok_or(ObservationError::UnknownObservedCard { card: *card })?;
                        Answer::Card(id)
                    }
                    ObservedAnswer::Card(TakenArgument::HiddenCard) => {
                        return Err(ObservationError::AnswerNamesHiddenCard)
                    }
                };
                Ok(Action::Answer {
                    player: *player,
                    answer,
                })
            }
            ObservedAction::Declared {
                player,
                action,
                arguments,
            } => {
                if *player != me {
                    return Err(ObservationError::ActionOfAnotherPlayer { player: *player });
                }
                let mut translated = Vec::with_capacity(arguments.len());
                for &ObservedArgument::Card(card) in arguments {
                    let id = observer
                        .card_instance(card)
                        .ok_or(ObservationError::UnknownObservedCard { card })?;
                    translated.push(Argument::Card(id));
                }
                Ok(Action::Declared {
                    player: *player,
                    action: action.clone(),
                    arguments: translated,
                })
            }
        }
    }
}
