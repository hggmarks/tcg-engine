//! Following transitions: the Observed Events of an Event Trace and the Observed Transition of an
//! accepted Action (ADR-0029).
//!
//! The fold applies each Event to a working copy of the GameState before it reads the card it
//! reveals, so a card that comes into sight carries its definition and instance state at that
//! point of the trace, not at the end of it. Both functions are all-or-nothing: they fold a clone
//! of the Observer and commit it only on success.

use super::error::ObservationError;
use super::observed_event::{
    ObservedAnswer, ObservedEvent, ObservedTransition, TakenAction, TakenArgument,
};
use super::observed_state::{self, observed_item};
use super::observer::{sees, Observer, Sight};
use crate::declarative::action::{Action, Argument};
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::decision::Answer;
use crate::declarative::event::Event;
use crate::declarative::state::{CardInstanceId, GameState};
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
    /// [`follow_events`](Self::follow_events). The GameState the trace leads to from `previous`
    /// must be `transition.state`, else the error is `ObserverOutOfStep`; the next Observed
    /// GameState is built before the Observer is committed, so on `Err` it is unchanged.
    pub fn follow_transition(
        &self,
        observer: &mut Observer,
        previous: &GameState,
        action: &Action,
        transition: &Transition<GameState, Event>,
    ) -> Result<ObservedTransition, ObservationError> {
        let mut folded = observer.clone();
        let mut working = previous.clone();
        let events = self.project(&mut folded, &mut working, transition.trace.events())?;
        if working != transition.state {
            return Err(ObservationError::ObserverOutOfStep);
        }
        let taken = match action {
            Action::Declared {
                player,
                action,
                arguments,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        let Argument::Card(card) = argument;
                        taken_argument(&folded, observer, *card)
                    })
                    .collect();
                TakenAction::Declared {
                    player: *player,
                    action: action.clone(),
                    arguments,
                }
            }
            Action::Pass { player } => TakenAction::Pass { player: *player },
            Action::Activate {
                player,
                card,
                ability,
            } => {
                let handle = folded
                    .observed_card(*card)
                    .or_else(|| observer.observed_card(*card))
                    .ok_or(ObservationError::ObserverOutOfStep)?;
                TakenAction::Activate {
                    player: *player,
                    card: handle,
                    ability: *ability,
                }
            }
            Action::Answer { player, answer } => TakenAction::Answer {
                player: *player,
                answer: match answer {
                    Answer::Option(index) => ObservedAnswer::Option(*index),
                    Answer::Card(card) => {
                        ObservedAnswer::Card(taken_argument(&folded, observer, *card))
                    }
                },
            },
        };
        let state = observed_state::observe(self, &folded, &transition.state)?;
        *observer = folded;
        Ok(ObservedTransition {
            action: taken,
            events,
            state,
        })
    }

    /// Applies each Event to `working`, folds it into `observer` and projects it. Every Observed
    /// Event is read from the Viewer's sight at its own Event, so the Observed Events of a trace
    /// are the same however the trace is sliced.
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
            (
                Event::PlayerStateChanged {
                    player,
                    state,
                    value,
                },
                Sight::NoCard,
            ) => ObservedEvent::PlayerStateChanged {
                player: *player,
                state: state.clone(),
                value: value.clone(),
            },
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
            (Event::DecisionPending { decider, kind, .. }, Sight::NoCard) => {
                ObservedEvent::DecisionPending {
                    decider: *decider,
                    kind: *kind,
                }
            }
            (Event::DecisionAnswered { player, answer }, Sight::NoCard) => {
                ObservedEvent::DecisionAnswered {
                    player: *player,
                    answer: match answer {
                        Answer::Option(index) => ObservedAnswer::Option(*index),
                        // The card as the Viewer sees it at this Event, which changes no sight:
                        // the handle the Observer holds now, else hidden (ADR-0034).
                        Answer::Card(card) => ObservedAnswer::Card(
                            observer
                                .observed_card(*card)
                                .map_or(TakenArgument::HiddenCard, TakenArgument::Card),
                        ),
                    },
                }
            }
            (Event::ItemPushed { item }, Sight::NoCard) => ObservedEvent::ItemPushed {
                item: observed_item(observer, item)?,
            },
            (Event::ItemResolved { item }, Sight::NoCard) => ObservedEvent::ItemResolved {
                item: observed_item(observer, item)?,
            },
            (Event::ItemFizzled { item }, Sight::NoCard) => ObservedEvent::ItemFizzled {
                item: observed_item(observer, item)?,
            },
            _ => return Err(out_of_step()),
        };
        Ok(Some(projected))
    }
}

/// A card as the Taken Action rule names it: the Observed Card the Viewer holds `after`, else the
/// one it held `before` (now retired), else hidden.
fn taken_argument(after: &Observer, before: &Observer, card: CardInstanceId) -> TakenArgument {
    match after
        .observed_card(card)
        .or_else(|| before.observed_card(card))
    {
        Some(handle) => TakenArgument::Card(handle),
        None => TakenArgument::HiddenCard,
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
            player_state: Vec::new(),
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

    fn empty() -> GameState {
        GameState::empty(
            PlayerOrder::new(vec![PlayerId(0), PlayerId(1)]).expect("two Players"),
            PlayerId(0),
            &[ZoneId::new("field"), ZoneId::new("hand")],
            &[],
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
        let observed_start = ruleset
            .observe(&observer, &start)
            .expect("the Empty GameState is observed");

        // Seat 0 sees the public field (cards 0, 1, 2: handles 0, 1, 2) and not seat 1's hand.
        let creation = [
            created(0, 0, "field"),
            created(1, 0, "field"),
            created(2, 0, "field"),
            created(3, 1, "hand"),
            created(4, 1, "hand"),
        ];
        let observed_creation = ruleset
            .follow_events(&mut observer, &start, &creation)
            .expect("the creation is followed");
        let created_state = start.apply_trace(&creation).expect("the cards are created");

        // Two shuffles, then a draw from seat 0's empty hand.
        let trace = [
            shuffled(0, "field", &[2, 0, 1], 9),
            shuffled(1, "hand", &[4, 3], 10),
            Event::DrawFromEmptyZone {
                player: PlayerId(0),
                zone: ZoneId::new("hand"),
            },
        ];
        let observed = ruleset
            .follow_events(&mut observer, &created_state, &trace)
            .expect("the trace is followed");

        let expected = vec![
            ObservedEvent::ZoneShuffled {
                player: PlayerId(0),
                zone: ZoneId::new("field"),
                order: vec![ObservedCard(2), ObservedCard(0), ObservedCard(1)],
            },
            ObservedEvent::UnseenZoneShuffled {
                player: PlayerId(1),
                zone: ZoneId::new("hand"),
            },
            ObservedEvent::DrawFromEmptyZone {
                player: PlayerId(0),
                zone: ZoneId::new("hand"),
            },
        ];
        assert_eq!(observed, expected);

        // Applying the projected events to the Observed GameState of the Empty GameState gives the
        // Observed GameState of the resulting GameState.
        let final_state = created_state
            .apply_trace(&trace)
            .expect("the trace applies");
        let observed_final = ruleset
            .observe(&observer, &final_state)
            .expect("the resulting GameState is observed");
        let all: Vec<ObservedEvent> = observed_creation.into_iter().chain(observed).collect();
        assert_eq!(
            observed_start.apply_events(&all),
            Ok(observed_final.clone())
        );
        assert_eq!(
            observed_final
                .zone(PlayerId(0), &ZoneId::new("field"))
                .and_then(|zone| zone.cards()),
            Some([ObservedCard(2), ObservedCard(0), ObservedCard(1)].as_slice())
        );
    }
}
