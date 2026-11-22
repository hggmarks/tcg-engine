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
    /// The Event names no single card.
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
            Event::CardStateChanged { card, .. } => {
                Ok(self.observed_card(*card).map_or(Sight::Unseen, Sight::Kept))
            }
            Event::ZoneShuffled { .. }
            | Event::TurnBegan { .. }
            | Event::PhaseBegan { .. }
            | Event::DrawFromEmptyZone { .. }
            | Event::GameEnded { .. }
            | Event::DecisionPending { .. }
            | Event::DecisionAnswered { .. }
            | Event::ItemPushed { .. }
            | Event::ItemResolved { .. }
            | Event::ItemFizzled { .. } => Ok(Sight::NoCard),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_catalog::{CardCatalog, CardCatalogDraft, CardId, CatalogMetaDraft};
    use crate::ruleset_definition::{
        FieldValue, MetaDraft, ParametersDraft, RulesetDefinition, RulesetDraft, StateName,
        ZoneDraft,
    };

    /// Two Players, a `field` every Viewer sees and a `hand` only its owner sees; no cards defined.
    fn ruleset() -> DeclarativeRuleset {
        let definition = RulesetDefinition::validate(RulesetDraft {
            meta: MetaDraft {
                id: "unit".to_string(),
                version: 1,
            },
            parameters: ParametersDraft {
                players: 2,
                starting_player: 0,
            },
            zones: vec![
                ZoneDraft {
                    id: "field".to_string(),
                    visibility: Visibility::Public,
                    ordered: false,
                },
                ZoneDraft {
                    id: "hand".to_string(),
                    visibility: Visibility::Owner,
                    ordered: false,
                },
            ],
            card_types: Vec::new(),
            inputs: Vec::new(),
            setup: Vec::new(),
            actions: Vec::new(),
            windows: Vec::new(),
            turn: None,
            win_conditions: Vec::new(),
            effects: None,
        })
        .expect("the unit ruleset is valid");
        let catalog = CardCatalog::validate(
            &definition,
            CardCatalogDraft {
                meta: CatalogMetaDraft {
                    ruleset: "unit".to_string(),
                    ruleset_version: 1,
                },
                cards: Vec::new(),
            },
        )
        .expect("the unit catalog is valid");
        DeclarativeRuleset::new(definition, catalog)
    }

    fn created(card: u32, owner: u32, zone: &str) -> Event {
        Event::CardCreated {
            card: CardInstanceId(card),
            owner: PlayerId(owner),
            definition: CardId::new("any"),
            zone: ZoneId::new(zone),
            state: Vec::new(),
        }
    }

    fn changed(card: u32) -> Event {
        Event::CardStateChanged {
            card: CardInstanceId(card),
            state: StateName::new("tapped"),
            value: FieldValue::Bool(true),
        }
    }

    #[test]
    fn a_card_state_change_keeps_the_sight_of_a_held_card_and_leaves_an_unseen_one_unseen() {
        let ruleset = ruleset();
        let mut observer = Observer::new(Viewer::Player(PlayerId(0)));
        // Seat 0 sees card 0 (public field) and not card 1 (seat 1's hand).
        let appeared = observer.follow(&ruleset, &created(0, 0, "field"));
        assert_eq!(appeared, Ok(Sight::Appeared(ObservedCard(0))));
        let hidden = observer.follow(&ruleset, &created(1, 1, "hand"));
        assert_eq!(hidden, Ok(Sight::Unseen));
        let before = observer.clone();

        assert_eq!(
            observer.follow(&ruleset, &changed(0)),
            Ok(Sight::Kept(ObservedCard(0)))
        );
        assert_eq!(observer, before);
        assert_eq!(observer.follow(&ruleset, &changed(1)), Ok(Sight::Unseen));
        assert_eq!(observer, before);
        // A card the Observer never heard of is unseen too, never a panic.
        assert_eq!(observer.follow(&ruleset, &changed(9)), Ok(Sight::Unseen));
        assert_eq!(observer, before);
    }
}
