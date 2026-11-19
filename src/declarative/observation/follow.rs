//! Following transitions: the Observed Events of an Event Trace and the Observed Transition of an
//! accepted Action (ADR-0029).
//!
//! The fold applies each Event to a working copy of the GameState before it reads the card it
//! reveals, so a card that comes into sight carries its definition and instance state at that
//! point of the trace, not at the end of it. Both functions are all-or-nothing: they fold a clone
//! of the Observer and commit it only on success.

use super::error::ObservationError;
use super::observed_event::{ObservedEvent, ObservedTransition, TakenAction, TakenArgument};
use super::observed_state;
use super::observer::{sees, Observer, Sight};
use crate::declarative::action::{Action, Argument};
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::event::Event;
use crate::declarative::state::GameState;
use crate::ruleset::Transition;

impl DeclarativeRuleset {
    /// Folds `events`, which apply in order to `previous` (the GameState `observer` has followed),
    /// into `observer`; returns their Observed Events in trace order. Every Event gives one Observed
    /// Event, except a state change of a card the Viewer cannot see, which gives none.
    ///
    /// All-or-nothing: on `Err` (`ObserverOutOfStep` for an Event that does not apply to the
    /// GameState or does not fit the Observer, or `ObservedCardOverflow`) the Observer is what it
    /// was before the call.
    pub fn follow_events(
        &self,
        observer: &mut Observer,
        previous: &GameState,
        events: &[Event],
    ) -> Result<Vec<ObservedEvent>, ObservationError> {
        let mut folded = observer.clone();
        let observed = self.project(&mut folded, &mut previous.clone(), events)?;
        *observer = folded;
        Ok(observed)
    }

    /// One accepted transition: `action` taken in `previous` (the GameState `observer` has
    /// followed) produced `transition`. Folds its trace into `observer` and returns the Observed
    /// Transition: the Taken Action, the Observed Events and the next Observed GameState.
    ///
    /// A card argument of the Taken Action is the Observed Card the Viewer holds after the
    /// transition; else the one it held before (now retired); else hidden. All-or-nothing, like
    /// [`follow_events`](Self::follow_events): the next Observed GameState is built before the
    /// Observer is committed, so a `transition.state` that does not fit leaves it unchanged.
    pub fn follow_transition(
        &self,
        observer: &mut Observer,
        previous: &GameState,
        action: &Action,
        transition: &Transition<GameState, Event>,
    ) -> Result<ObservedTransition, ObservationError> {
        let mut folded = observer.clone();
        let events = self.project(
            &mut folded,
            &mut previous.clone(),
            transition.trace.events(),
        )?;
        let arguments = action
            .arguments
            .iter()
            .map(|argument| {
                let Argument::Card(card) = argument;
                match folded
                    .observed_card(*card)
                    .or(observer.observed_card(*card))
                {
                    Some(handle) => TakenArgument::Card(handle),
                    None => TakenArgument::HiddenCard,
                }
            })
            .collect();
        let state = observed_state::observe(self, &folded, &transition.state)?;
        *observer = folded;
        Ok(ObservedTransition {
            action: TakenAction {
                player: action.player,
                action: action.action.clone(),
                arguments,
            },
            events,
            state,
        })
    }

    /// Applies each Event to `working`, folds it into `observer` and projects it.
    fn project(
        &self,
        observer: &mut Observer,
        working: &mut GameState,
        events: &[Event],
    ) -> Result<Vec<ObservedEvent>, ObservationError> {
        let mut observed = Vec::new();
        for event in events {
            working
                .apply(event)
                .map_err(|_| ObservationError::ObserverOutOfStep)?;
            let sight = observer.follow(self, event)?;
            if let Some(projected) = self.project_event(observer, working, event, sight)? {
                observed.push(projected);
            }
        }
        Ok(observed)
    }

    /// The Observed Event of `event` given what it did to the Viewer's sight; `working` is the
    /// GameState after `event`. `None` for a state change of a card the Viewer cannot see.
    fn project_event(
        &self,
        observer: &Observer,
        working: &GameState,
        event: &Event,
        sight: Sight,
    ) -> Result<Option<ObservedEvent>, ObservationError> {
        let out_of_step = || ObservationError::ObserverOutOfStep;
        let projected = match (event, sight) {
            (
                Event::CardCreated {
                    owner,
                    definition,
                    zone,
                    state,
                    ..
                },
                Sight::Appeared(card),
            ) => ObservedEvent::CardCreated {
                card,
                owner: *owner,
                definition: definition.clone(),
                zone: zone.clone(),
                state: state.clone(),
            },
            (Event::CardCreated { owner, zone, .. }, Sight::Unseen) => {
                ObservedEvent::UnseenCardCreated {
                    owner: *owner,
                    zone: zone.clone(),
                }
            }
            (
                Event::CardMoved {
                    player, from, to, ..
                },
                Sight::Kept(card),
            ) => ObservedEvent::CardMoved {
                card,
                player: *player,
                from: from.clone(),
                to: to.clone(),
            },
            (
                Event::CardMoved {
                    card: id,
                    player,
                    from,
                    to,
                },
                Sight::Appeared(card),
            ) => {
                let revealed = working.card(*id).ok_or_else(out_of_step)?;
                ObservedEvent::CardMovedIntoSight {
                    card,
                    player: *player,
                    from: from.clone(),
                    to: to.clone(),
                    definition: revealed.definition().clone(),
                    state: revealed.state().to_vec(),
                }
            }
            (
                Event::CardMoved {
                    player, from, to, ..
                },
                Sight::Disappeared(card),
            ) => ObservedEvent::CardMovedOutOfSight {
                card,
                player: *player,
                from: from.clone(),
                to: to.clone(),
            },
            (
                Event::CardMoved {
                    player, from, to, ..
                },
                Sight::Unseen,
            ) => ObservedEvent::UnseenCardMoved {
                player: *player,
                from: from.clone(),
                to: to.clone(),
            },
            (
                Event::ZoneShuffled {
                    player,
                    zone,
                    order,
                    ..
                },
                Sight::NoCard,
            ) => {
                if sees(self, observer.viewer(), *player, zone)? {
                    let mut handles = Vec::new();
                    for id in order {
                        handles.push(observer.observed_card(*id).ok_or_else(out_of_step)?);
                    }
                    ObservedEvent::ZoneShuffled {
                        player: *player,
                        zone: zone.clone(),
                        order: handles,
                    }
                } else {
                    ObservedEvent::UnseenZoneShuffled {
                        player: *player,
                        zone: zone.clone(),
                    }
                }
            }
            (Event::CardStateChanged { state, value, .. }, Sight::Kept(card)) => {
                ObservedEvent::CardStateChanged {
                    card,
                    state: state.clone(),
                    value: value.clone(),
                }
            }
            (Event::CardStateChanged { .. }, Sight::Unseen) => return Ok(None),
            (Event::TurnBegan { player, turn }, Sight::NoCard) => ObservedEvent::TurnBegan {
                player: *player,
                turn: *turn,
            },
            (Event::PhaseBegan { phase }, Sight::NoCard) => ObservedEvent::PhaseBegan {
                phase: phase.clone(),
            },
            (Event::DrawFromEmptyZone { player, zone }, Sight::NoCard) => {
                ObservedEvent::DrawFromEmptyZone {
                    player: *player,
                    zone: zone.clone(),
                }
            }
            (Event::GameEnded { losses }, Sight::NoCard) => ObservedEvent::GameEnded {
                losses: losses.clone(),
            },
            _ => return Err(out_of_step()),
        };
        Ok(Some(projected))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_catalog::{CardCatalog, CardCatalogDraft, CardId, CatalogMetaDraft};
    use crate::declarative::observation::viewer::{ObservedCard, Viewer};
    use crate::declarative::state::CardInstanceId;
    use crate::player::{PlayerId, PlayerOrder};
    use crate::random::{RandomState, Seed};
    use crate::ruleset::Observations;
    use crate::ruleset_definition::{
        MetaDraft, ParametersDraft, RulesetDefinition, RulesetDraft, Visibility, ZoneDraft, ZoneId,
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
                    ordered: true,
                },
                ZoneDraft {
                    id: "hand".to_string(),
                    visibility: Visibility::Owner,
                    ordered: true,
                },
            ],
            card_types: Vec::new(),
            inputs: Vec::new(),
            setup: Vec::new(),
            actions: Vec::new(),
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

    fn empty() -> GameState {
        GameState::empty(
            PlayerOrder::new(vec![PlayerId(0), PlayerId(1)]).expect("two Players"),
            PlayerId(0),
            &[ZoneId::new("field"), ZoneId::new("hand")],
            RandomState::from_seed(Seed(1)),
        )
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

    fn shuffled(player: u32, zone: &str, order: &[u32], seed: u64) -> Event {
        Event::ZoneShuffled {
            player: PlayerId(player),
            zone: ZoneId::new(zone),
            order: order.iter().copied().map(CardInstanceId).collect(),
            random_state: RandomState::from_seed(Seed(seed)),
        }
    }

    #[test]
    fn a_shuffle_of_a_seen_zone_gives_the_new_order_of_observed_cards_and_of_an_unseen_zone_no_order(
    ) {
        let ruleset = ruleset();
        let start = empty();
        let viewer = Viewer::Player(PlayerId(0));
        let mut observer = ruleset
            .observer(viewer, &start, &[])
            .expect("the Viewer is a Player");

        // Seat 0 sees the public field (cards 0, 1, 2: handles 0, 1, 2) and not seat 1's hand.
        let creation = [
            created(0, 0, "field"),
            created(1, 0, "field"),
            created(2, 0, "field"),
            created(3, 1, "hand"),
            created(4, 1, "hand"),
        ];
        ruleset
            .follow_events(&mut observer, &start, &creation)
            .expect("the creation is followed");
        let created_state = start.apply_trace(&creation).expect("the cards are created");

        let shuffles = [
            shuffled(0, "field", &[2, 0, 1], 9),
            shuffled(1, "hand", &[4, 3], 10),
        ];
        let observed = ruleset
            .follow_events(&mut observer, &created_state, &shuffles)
            .expect("the shuffles are followed");

        assert_eq!(
            observed,
            vec![
                ObservedEvent::ZoneShuffled {
                    player: PlayerId(0),
                    zone: ZoneId::new("field"),
                    order: vec![ObservedCard(2), ObservedCard(0), ObservedCard(1)],
                },
                ObservedEvent::UnseenZoneShuffled {
                    player: PlayerId(1),
                    zone: ZoneId::new("hand"),
                },
            ]
        );
    }
}
