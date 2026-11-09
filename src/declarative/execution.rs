//! The state being changed and the Events applied to it so far. Game creation and every
//! State Transition build their Event Trace through it (ADR-0010): each Event is applied to the
//! state the previous one produced, and recorded only if it was accepted.

use super::event::Event;
use super::state::{ApplyError, CardInstanceId, GameState};
use crate::player::PlayerId;
use crate::ruleset_definition::{PlayerSelector, ZoneId};

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
