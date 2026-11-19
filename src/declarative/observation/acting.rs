//! Acting from an Observation (ADR-0030): a Viewer's Legal Actions with Observed Cards, and an
//! Observed Action turned back into the engine Action. The mapping is read only through the
//! Observer; legality stays with `transition`.

use super::error::ObservationError;
use super::observer::{sees, Observer};
use super::viewer::{ObservedCard, Viewer};
use crate::declarative::action::{Action, Argument};
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::state::GameState;
use crate::player::PlayerId;
use crate::ruleset::LegalActions;
use crate::ruleset_definition::ActionId;

/// An Action as a Viewer expresses it (ADR-0030): the engine Action with an Observed Card for each
/// card argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedAction {
    pub player: PlayerId,
    pub action: ActionId,
    /// One per param of the action, in param order.
    pub arguments: Vec<ObservedArgument>,
}

/// The value an Observed Action supplies for one param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedArgument {
    /// An Observed Card.
    Card(ObservedCard),
}

impl DeclarativeRuleset {
    /// The Viewer's Legal Actions in Listing Order, each card argument as its Observed Card.
    ///
    /// It is `legal_actions(state)` filtered to the Actions whose Player is the Viewer, so the
    /// engine's order is kept and a Player that is not the Active Player gets an empty list. The
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
            if legal.player != me {
                continue;
            }
            let mut arguments = Vec::with_capacity(legal.arguments.len());
            for &Argument::Card(id) in &legal.arguments {
                let instance = state.card(id).ok_or(ObservationError::ObserverOutOfStep)?;
                let zone = state
                    .zone_of(id)
                    .ok_or(ObservationError::ObserverOutOfStep)?;
                if !sees(self, viewer, instance.owner(), zone)? {
                    return Err(ObservationError::LegalActionNamesUnseenCard {
                        action: legal.action,
                    });
                }
                let handle = observer
                    .observed_card(id)
                    .ok_or(ObservationError::ObserverOutOfStep)?;
                arguments.push(ObservedArgument::Card(handle));
            }
            listed.push(ObservedAction {
                player: legal.player,
                action: legal.action,
                arguments,
            });
        }
        Ok(listed)
    }

    /// The engine Action an Observed Action stands for. The checks come in this order: the
    /// Observed Action's Player is the Viewer (`ActionOfAnotherPlayer`); then each argument in
    /// order, a handle the Viewer does not hold is `UnknownObservedCard`. The action id, the arity
    /// and the legality are not checked: `transition` decides them.
    pub fn translate_action(
        &self,
        observer: &Observer,
        action: &ObservedAction,
    ) -> Result<Action, ObservationError> {
        let Viewer::Player(me) = observer.viewer();
        if action.player != me {
            return Err(ObservationError::ActionOfAnotherPlayer {
                player: action.player,
            });
        }
        let mut arguments = Vec::with_capacity(action.arguments.len());
        for &ObservedArgument::Card(card) in &action.arguments {
            let id = observer
                .card_instance(card)
                .ok_or(ObservationError::UnknownObservedCard { card })?;
            arguments.push(Argument::Card(id));
        }
        Ok(Action {
            player: action.player,
            action: action.action.clone(),
            arguments,
        })
    }
}
