//! The state being changed and the Events applied to it so far. Game creation and every
//! State Transition build their Event Trace through it (ADR-0010): each Event is applied to the
//! state the previous one produced, and recorded only if it was accepted.

use super::action::Rejection;
use super::event::Event;
use super::state::{ApplyError, CardInstanceId, GameState};
use crate::player::PlayerId;
use crate::ruleset_definition::{FieldType, FieldValue, PlayerSelector, PlayerStateId, ZoneId};

/// Why a setup step, an automatic step or a Player-state write could not be done (ADR-0041).
#[derive(Debug)]
pub(super) enum StepError {
    /// An Event the step generated was rejected: an internal inconsistency, never expected.
    Apply(ApplyError),
    /// Adding to the int Player state `state` of `player` would go beyond what an int can hold, in
    /// either direction.
    PlayerStateOverflow {
        player: PlayerId,
        state: PlayerStateId,
    },
}

impl From<ApplyError> for StepError {
    fn from(error: ApplyError) -> Self {
        Self::Apply(error)
    }
}

impl From<StepError> for Rejection {
    fn from(error: StepError) -> Self {
        match error {
            StepError::Apply(error) => Rejection::EventRejected(error),
            StepError::PlayerStateOverflow { player, state } => {
                Rejection::PlayerStateOverflow { player, state }
            }
        }
    }
}

pub(super) struct Execution {
    pub(super) state: GameState,
    pub(super) events: Vec<Event>,
}

impl Execution {
    /// An execution that starts from `state` with no Events yet.
    pub(super) fn new(state: GameState) -> Self {
        Self {
            state,
            events: Vec::new(),
        }
    }

    /// Applies `event` to the state and records it in the trace.
    pub(super) fn apply(&mut self, event: Event) -> Result<(), ApplyError> {
        self.state.apply(&event)?;
        self.events.push(event);
        Ok(())
    }

    /// The Players a selector chooses, in seat order.
    pub(super) fn select(&self, selector: PlayerSelector) -> Vec<PlayerId> {
        match selector {
            PlayerSelector::Each => self.state.player_order().players().to_vec(),
            PlayerSelector::Active => vec![self.state.active_player()],
        }
    }

    /// Sets the Player state `state` of `player` to `value`, if it changes: an unchanged value
    /// emits no Event.
    pub(super) fn set_player_state(
        &mut self,
        player: PlayerId,
        state: &PlayerStateId,
        value: &FieldValue,
    ) -> Result<(), ApplyError> {
        if self.state.player_state_value(player, state) == Some(value) {
            return Ok(());
        }
        self.apply(Event::PlayerStateChanged {
            player,
            state: state.clone(),
            value: value.clone(),
        })
    }

    /// Adds `amount` to the int Player state `state` of `player`, with checked arithmetic, and
    /// sets the sum if it differs from the current value.
    pub(super) fn add_player_state(
        &mut self,
        player: PlayerId,
        state: &PlayerStateId,
        amount: i64,
    ) -> Result<(), StepError> {
        let current = match self.state.player_state_value(player, state) {
            Some(FieldValue::Int(current)) => *current,
            Some(other) => {
                return Err(StepError::Apply(ApplyError::PlayerStateTypeMismatch {
                    player,
                    state: state.clone(),
                    expected: FieldType::Int,
                    found: other.field_type(),
                }))
            }
            None => {
                return Err(StepError::Apply(ApplyError::UnknownPlayerState {
                    player,
                    state: state.clone(),
                }))
            }
        };
        let Some(sum) = current.checked_add(amount) else {
            return Err(StepError::PlayerStateOverflow {
                player,
                state: state.clone(),
            });
        };
        Ok(self.set_player_state(player, state, &FieldValue::Int(sum))?)
    }

    /// The cards of a zone, bottom first.
    pub(super) fn cards_of(
        &self,
        player: PlayerId,
        zone: &ZoneId,
    ) -> Result<Vec<CardInstanceId>, ApplyError> {
        let Some(zones) = self.state.zones(player) else {
            return Err(ApplyError::UnknownPlayer { player });
        };
        zones
            .iter()
            .find(|held| held.zone() == zone)
            .map(|held| held.cards().to_vec())
            .ok_or_else(|| ApplyError::UnknownZone {
                player,
                zone: zone.clone(),
            })
    }
}
