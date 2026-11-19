//! The Observed GameState: what one Viewer sees of a GameState (ADR-0028).
//!
//! It holds base facts only, never a derived value (Derived Reads are offered beside it), and no
//! Card Instance Id, no Random State and no order of a zone the Viewer cannot see.

use super::error::ObservationError;
use super::observer::{sees, Observer};
use super::viewer::{ObservedCard, Viewer};
use crate::card_catalog::CardId;
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::state::{GameState, TerminalOutcome};
use crate::player::{PlayerId, PlayerOrder};
use crate::ruleset_definition::{FieldValue, PhaseId, StateName, ZoneId};
use crate::turn::TurnNumber;

/// A GameState as one Viewer sees it: the facts every Viewer sees, each zone as the Viewer's
/// Observed Cards or as a count, and the Visible Cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedGameState {
    viewer: Viewer,
    player_order: PlayerOrder,
    active_player: PlayerId,
    turn_number: Option<TurnNumber>,
    phase: Option<PhaseId>,
    outcome: Option<TerminalOutcome>,
    zones: Vec<(PlayerId, Vec<ObservedZone>)>,
    cards: Vec<VisibleCard>,
}

impl ObservedGameState {
    /// The Viewer this is the Observed GameState for.
    pub fn viewer(&self) -> Viewer {
        self.viewer
    }

    /// The Players in seat order.
    pub fn players(&self) -> &[PlayerId] {
        self.player_order.players()
    }

    /// The Player whose Turn it is.
    pub fn active_player(&self) -> PlayerId {
        self.active_player
    }

    /// The Turn Number of the Turn in progress; `None` before the first Turn has begun.
    pub fn turn_number(&self) -> Option<TurnNumber> {
        self.turn_number
    }

    /// The phase of the Turn in progress that began last; `None` when no phase has begun.
    pub fn phase(&self) -> Option<&PhaseId> {
        self.phase.as_ref()
    }

    /// The Terminal Outcome; `None` while the Game is on.
    pub fn outcome(&self) -> Option<&TerminalOutcome> {
        self.outcome.as_ref()
    }

    /// The zones of `player` in declared order; `None` for a Player not in the Game.
    pub fn zones(&self, player: PlayerId) -> Option<&[ObservedZone]> {
        self.zones
            .iter()
            .find(|(owner, _)| *owner == player)
            .map(|(_, zones)| zones.as_slice())
    }

    /// One zone of `player`; `None` for an unknown Player or zone.
    pub fn zone(&self, player: PlayerId, zone: &ZoneId) -> Option<&ObservedZone> {
        self.zones(player)?.iter().find(|held| &held.zone == zone)
    }

    /// Every card the Viewer sees, by ascending Observed Card.
    pub fn cards(&self) -> &[VisibleCard] {
        &self.cards
    }

    /// The visible card the Viewer holds as `card`.
    pub fn card(&self, card: ObservedCard) -> Option<&VisibleCard> {
        self.cards
            .binary_search_by_key(&card, |visible| visible.card)
            .ok()
            .and_then(|position| self.cards.get(position))
    }
}

/// What the Viewer sees of one zone: its cards, or only how many there are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedZone {
    zone: ZoneId,
    view: ZoneView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ZoneView {
    /// The Viewer sees the cards, bottom first.
    Seen(Vec<ObservedCard>),
    /// The Viewer sees only the count: not which cards, nor their order.
    Unseen { count: usize },
}

impl ObservedZone {
    /// The zone.
    pub fn zone(&self) -> &ZoneId {
        &self.zone
    }

    /// How many cards the zone holds, whether or not the Viewer sees them.
    pub fn count(&self) -> usize {
        match &self.view {
            ZoneView::Seen(cards) => cards.len(),
            ZoneView::Unseen { count } => *count,
        }
    }

    /// The Observed Cards of the zone, bottom first; `None` when the Viewer cannot see the zone.
    pub fn cards(&self) -> Option<&[ObservedCard]> {
        match &self.view {
            ZoneView::Seen(cards) => Some(cards),
            ZoneView::Unseen { .. } => None,
        }
    }
}

/// A card the Viewer sees: its Observed Card, its owner, its Card Definition and its instance
/// state as stored. Fields of the definition are read from the catalog, and Derived Reads are asked
/// of the Ruleset; neither is held here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleCard {
    card: ObservedCard,
    owner: PlayerId,
    definition: CardId,
    state: Vec<(StateName, FieldValue)>,
}

impl VisibleCard {
    /// The Observed Card the Viewer holds for it.
    pub fn card(&self) -> ObservedCard {
        self.card
    }

    /// The Player who owns the card.
    pub fn owner(&self) -> PlayerId {
        self.owner
    }

    /// The Card Definition it was made from.
    pub fn definition(&self) -> &CardId {
        &self.definition
    }

    /// The instance state in the order of the card type's schema.
    pub fn state(&self) -> &[(StateName, FieldValue)] {
        &self.state
    }
}

/// The Observed GameState of `state` for the Viewer of `observer`.
pub(super) fn observe(
    ruleset: &DeclarativeRuleset,
    observer: &Observer,
    state: &GameState,
) -> Result<ObservedGameState, ObservationError> {
    let viewer = observer.viewer();
    let Viewer::Player(me) = viewer;
    if !state.player_order().contains(me) {
        return Err(ObservationError::UnknownViewer { viewer });
    }

    let mut zones = Vec::new();
    let mut seen_total = 0usize;
    for &owner in state.player_order().players() {
        let held = state
            .zones(owner)
            .ok_or(ObservationError::ObserverOutOfStep)?;
        let mut observed = Vec::new();
        for stored in held {
            let view = if sees(ruleset, viewer, owner, stored.zone())? {
                let mut handles = Vec::new();
                for &id in stored.cards() {
                    let handle = observer
                        .observed_card(id)
                        .ok_or(ObservationError::ObserverOutOfStep)?;
                    handles.push(handle);
                }
                seen_total = seen_total
                    .checked_add(handles.len())
                    .ok_or(ObservationError::ObserverOutOfStep)?;
                ZoneView::Seen(handles)
            } else {
                ZoneView::Unseen {
                    count: stored.cards().len(),
                }
            };
            observed.push(ObservedZone {
                zone: stored.zone().clone(),
                view,
            });
        }
        zones.push((owner, observed));
    }
    // Every seen card has a handle, so the handles of the seen cards are distinct; the Observer
    // holds no other.
    if seen_total != observer.held() {
        return Err(ObservationError::ObserverOutOfStep);
    }

    let mut cards = Vec::new();
    for (handle, id) in observer.held_cards() {
        let instance = state.card(id).ok_or(ObservationError::ObserverOutOfStep)?;
        cards.push(VisibleCard {
            card: handle,
            owner: instance.owner(),
            definition: instance.definition().clone(),
            state: instance.state().to_vec(),
        });
    }

    Ok(ObservedGameState {
        viewer,
        player_order: state.player_order().clone(),
        active_player: state.active_player(),
        turn_number: state.turn_number(),
        phase: state.phase().cloned(),
        outcome: state.outcome().cloned(),
        zones,
        cards,
    })
}
