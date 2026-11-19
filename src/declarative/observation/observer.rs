//! The Observer: one Viewer's fold of the Events of a Game (ADR-0028).
//!
//! It holds the mapping between Observed Cards and Card Instances, which is never shown to the
//! Viewer. The rule per Event is total: a card appears (gets the next handle) when a `CardCreated`
//! puts it, or a `CardMoved` brings it from a zone the Viewer cannot see, into a zone the Viewer
//! sees; it disappears (its handle is retired, never reused) when a `CardMoved` takes it from a seen
//! zone to an unseen one; otherwise it keeps its handle. Every other Event leaves the mapping alone.

use std::collections::BTreeMap;

use super::error::ObservationError;
use super::observed_state::{self, ObservedGameState};
use super::viewer::{ObservedCard, Viewer};
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::event::Event;
use crate::declarative::state::{CardInstanceId, GameState};
use crate::player::PlayerId;
use crate::ruleset::Observations;
use crate::ruleset_definition::{Visibility, ZoneId};

/// A Viewer's fold of the Events applied to the Empty GameState, so far. Built by
/// [`Observations::observer`]; it changes no GameState, Event or Game Record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observer {
    viewer: Viewer,
    by_card: BTreeMap<CardInstanceId, ObservedCard>,
    by_handle: BTreeMap<ObservedCard, CardInstanceId>,
    next: u32,
}

/// What one Event did to the Viewer's sight of its card. Crate-private: the incremental, projecting
/// form of the fold reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sight {
    /// The Event names no card.
    NoCard,
    /// The Event names a card the Viewer does not see, before and after.
    Unseen,
    /// The Viewer sees the card, before and after, under this handle.
    Kept(ObservedCard),
    /// The card came into the Viewer's sight under this new handle.
    Appeared(ObservedCard),
    /// The card left the Viewer's sight; this handle is retired.
    Disappeared(ObservedCard),
}

impl Observer {
    /// The Observer of `viewer` before any Event.
    fn new(viewer: Viewer) -> Observer {
        Observer {
            viewer,
            by_card: BTreeMap::new(),
            by_handle: BTreeMap::new(),
            next: 0,
        }
    }

    /// The Viewer this Observer follows.
    pub fn viewer(&self) -> Viewer {
        self.viewer
    }

    /// The Card Instance the Viewer holds as `card` now; `None` for a handle not held. This is the
    /// mapping an Observed Action is translated through.
    pub fn card_instance(&self, card: ObservedCard) -> Option<CardInstanceId> {
        self.by_handle.get(&card).copied()
    }

    /// The Observed Card the Viewer holds for `card` now; `None` while it is unseen.
    pub fn observed_card(&self, card: CardInstanceId) -> Option<ObservedCard> {
        self.by_card.get(&card).copied()
    }

    /// Folds one Event into the Observer.
    pub(crate) fn follow(
        &mut self,
        ruleset: &DeclarativeRuleset,
        event: &Event,
    ) -> Result<Sight, ObservationError> {
        match event {
            Event::CardCreated {
                card, owner, zone, ..
            } => {
                if self.by_card.contains_key(card) {
                    return Err(ObservationError::ObserverOutOfStep);
                }
                if sees(ruleset, self.viewer, *owner, zone)? {
                    self.appear(*card).map(Sight::Appeared)
                } else {
                    Ok(Sight::Unseen)
                }
            }
            Event::CardMoved {
                card,
                player,
                from,
                to,
            } => {
                let before = sees(ruleset, self.viewer, *player, from)?;
                let after = sees(ruleset, self.viewer, *player, to)?;
                let held = self.observed_card(*card);
                if before != held.is_some() {
                    return Err(ObservationError::ObserverOutOfStep);
                }
                match (held, after) {
                    (Some(handle), true) => Ok(Sight::Kept(handle)),
                    (Some(handle), false) => {
                        self.by_card.remove(card);
                        self.by_handle.remove(&handle);
                        Ok(Sight::Disappeared(handle))
                    }
                    (None, true) => self.appear(*card).map(Sight::Appeared),
                    (None, false) => Ok(Sight::Unseen),
                }
            }
            Event::ZoneShuffled { .. }
            | Event::TurnBegan { .. }
            | Event::PhaseBegan { .. }
            | Event::CardStateChanged { .. }
            | Event::DrawFromEmptyZone { .. }
            | Event::GameEnded { .. } => Ok(Sight::NoCard),
        }
    }

    /// Gives `card` the next handle.
    fn appear(&mut self, card: CardInstanceId) -> Result<ObservedCard, ObservationError> {
        let handle = ObservedCard(self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or(ObservationError::ObservedCardOverflow)?;
        self.by_card.insert(card, handle);
        self.by_handle.insert(handle, card);
        Ok(handle)
    }

    /// How many cards the Viewer holds a handle for now.
    pub(super) fn held(&self) -> usize {
        self.by_handle.len()
    }

    /// The cards the Viewer holds a handle for, by ascending handle.
    pub(super) fn held_cards(&self) -> impl Iterator<Item = (ObservedCard, CardInstanceId)> + '_ {
        self.by_handle.iter().map(|(&handle, &card)| (handle, card))
    }
}

/// Whether `viewer` sees the cards of `owner`'s zone `zone` (ADR-0004): `public` every Viewer,
/// `owner` its owner only, `hidden` nobody, its owner included. A zone the Ruleset does not declare
/// is out of step.
pub(super) fn sees(
    ruleset: &DeclarativeRuleset,
    viewer: Viewer,
    owner: PlayerId,
    zone: &ZoneId,
) -> Result<bool, ObservationError> {
    let declared = ruleset
        .definition()
        .zone(zone)
        .ok_or(ObservationError::ObserverOutOfStep)?;
    Ok(match declared.visibility() {
        Visibility::Public => true,
        Visibility::Owner => viewer == Viewer::Player(owner),
        Visibility::Hidden => false,
    })
}

impl Observations for DeclarativeRuleset {
    type Viewer = Viewer;
    type Observer = Observer;
    type ObservedState = ObservedGameState;
    type ObservationError = ObservationError;

    fn observer(
        &self,
        viewer: Viewer,
        empty: &GameState,
        events: &[Event],
    ) -> Result<Observer, ObservationError> {
        let Viewer::Player(player) = viewer;
        if !empty.player_order().contains(player) {
            return Err(ObservationError::UnknownViewer { viewer });
        }
        let mut observer = Observer::new(viewer);
        for event in events {
            observer.follow(self, event)?;
        }
        Ok(observer)
    }

    fn observe(
        &self,
        observer: &Observer,
        state: &GameState,
    ) -> Result<ObservedGameState, ObservationError> {
        observed_state::observe(self, observer, state)
    }
}
