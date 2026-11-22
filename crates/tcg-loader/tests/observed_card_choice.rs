//! Acceptance tests for ticket #76, observing a Card Choice (ADR-0034, ADR-0028 to ADR-0030): the
//! Deciding Player's Observed Legal Actions name candidates by Observed Card and round-trip; every
//! other Viewer sees the card an Answer names by the Taken Action rule (the Observed Card it holds
//! after the transition, else before, else hidden); the Answer in the Taken Action and in the
//! Observed `DecisionAnswered` is one value.
//!
//! The Effects Ruleset and its real catalog (`snare_vine`, `mind_leech`), plus `hand_picker`, a
//! test-only card that picks a card from its controller's own hand and does nothing with it,
//! appended to the catalog text. Games are driven Event by Event through one Observer per Viewer,
//! so an arrangement is made of Events both Observers follow.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Answer, Argument, CardInstanceId, DecisionKind, DeclarativeRuleset, Event, GameState,
    ObservationError, ObservedAction, ObservedAnswer, ObservedApplyError, ObservedCard,
    ObservedEvent, ObservedGameState, ObservedPendingDecision, ObservedTransition, Observer,
    TakenAction, TakenArgument, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, FieldValue, StateName, ZoneId};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");

/// The controller picks a creature from their own hand, and nothing follows. Costs 0.
const HAND_PICKER: &str = "
  - id: hand_picker
    type: creature
    fields: { name: \"Hand Picker\", cost: 0, power: 1 }
    effects:
      - on: { enter: field }
        do: [ { choose_card: { name: t, decider: controller, zone: hand, card_type: creature } } ]
";

/// Four of each of the three cards and then real creatures, 50 in all.
fn deck_yaml(leader: &str) -> String {
    format!(
        "inputs:
  - input: leader
    cards:
      - {{ card: {leader}, count: 1 }}
  - input: main_deck
    cards:
      - {{ card: snare_vine,   count: 4 }}
      - {{ card: mind_leech,   count: 4 }}
      - {{ card: hand_picker,  count: 4 }}
      - {{ card: goblin_scout, count: 4 }}
      - {{ card: moss_sprite,  count: 4 }}
      - {{ card: ember_hound,  count: 4 }}
      - {{ card: river_otter,  count: 4 }}
      - {{ card: stone_golem,  count: 4 }}
      - {{ card: shadow_cat,   count: 4 }}
      - {{ card: iron_guard,   count: 4 }}
      - {{ card: wind_hawk,    count: 4 }}
      - {{ card: thorn_boar,   count: 4 }}
      - {{ card: frost_wolf,   count: 2 }}
  - input: mana_deck
    cards:
      - {{ card: mana, count: 10 }}
"
    )
}

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(p(seat))
}

fn play(player: u32, card: CardInstanceId) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new("play_creature"),
        arguments: vec![Argument::Card(card)],
    }
}

fn answer_card(player: u32, card: CardInstanceId) -> Action {
    Action::Answer {
        player: p(player),
        answer: Answer::Card(card),
    }
}

fn pending(decider: u32) -> ObservedPendingDecision {
    ObservedPendingDecision {
        decider: p(decider),
        kind: DecisionKind::Card,
    }
}

/// The Observed Answer that names the card by this handle.
fn named(handle: ObservedCard) -> ObservedAnswer {
    ObservedAnswer::Card(TakenArgument::Card(handle))
}

fn ruleset() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(EFFECTS_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, &format!("{EFFECTS_CATALOG}{HAND_PICKER}"))
        .expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    ["captain_ardel", "warden_ilse"]
        .iter()
        .map(|leader| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), &deck_yaml(leader))
                .expect("the test deck is legal")
        })
        .collect()
}

/// A Game followed by one Observer per Viewer, in seat order.
struct Game {
    ruleset: DeclarativeRuleset,
    state: GameState,
    observers: Vec<Observer>,
    observed: Vec<ObservedGameState>,
}

impl Game {
    /// Seed 1's opening, followed by both Viewers.
    fn new() -> Game {
        let ruleset = ruleset();
        let creation = ruleset
            .create_game(&decks(&ruleset), Seed(1))
            .expect("the test decks create a game");
        let mut observers = Vec::new();
        let mut observed = Vec::new();
        for seat in 0..2 {
            let mut observer = ruleset
                .observer(viewer(seat), &creation.empty, &[])
                .expect("the Observer is built");
            ruleset
                .follow_events(
                    &mut observer,
                    &creation.empty,
                    creation.opening.trace.events(),
                )
                .expect("the creation trace is followed");
            observed.push(
                ruleset
                    .observe(&observer, &creation.opening.state)
                    .expect("the Opening Position is observed"),
            );
            observers.push(observer);
        }
        Game {
            ruleset,
            state: creation.opening.state,
            observers,
            observed,
        }
    }

    /// The `n`th card of `player` made from `id`.
    fn card(&self, player: u32, id: &str, n: usize) -> CardInstanceId {
        self.state
            .cards()
            .iter()
            .filter(|card| card.owner() == p(player) && card.definition().as_str() == id)
            .nth(n)
            .unwrap_or_else(|| panic!("player {player} has no card {n} of `{id}`"))
            .id()
    }

    /// The cards in a zone of a player.
    fn in_zone(&self, player: u32, id: &str) -> Vec<CardInstanceId> {
        self.state
            .zone(p(player), &zone(id))
            .expect("the zone exists")
            .to_vec()
    }

    /// Moves the card to `to` of its owner with an Event both Viewers follow: an arrangement.
    fn put(&mut self, card: CardInstanceId, to: &str) {
        let owner = self.state.card(card).expect("the card exists").owner();
        let from = self
            .state
            .zone_of(card)
            .expect("the card has a zone")
            .clone();
        if from == zone(to) {
            return;
        }
        let events = [Event::CardMoved {
            card,
            player: owner,
            from,
            to: zone(to),
        }];
        let next = self
            .state
            .apply_trace(&events)
            .expect("the arrangement applies");
        for (observer, observed) in self.observers.iter_mut().zip(&mut self.observed) {
            self.ruleset
                .follow_events(observer, &self.state, &events)
                .expect("the arrangement is followed");
            *observed = self
                .ruleset
                .observe(observer, &next)
                .expect("the arranged GameState is observed");
        }
        self.state = next;
    }

    /// Takes `action`: it must be accepted. Returns what each Viewer observed.
    fn take(&mut self, action: &Action) -> (Vec<Event>, Vec<ObservedTransition>) {
        let transition = self
            .ruleset
            .transition(&self.state, action)
            .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"));
        let seen: Vec<ObservedTransition> = self
            .observers
            .iter_mut()
            .map(|observer| {
                self.ruleset
                    .follow_transition(observer, &self.state, action, &transition)
                    .expect("the transition is followed")
            })
            .collect();
        for (observed, seen) in self.observed.iter_mut().zip(&seen) {
            observed.clone_from(&seen.state);
        }
        let events = transition.trace.into_events();
        self.state = transition.state;
        (events, seen)
    }

    /// The Observed Card `seat` holds for `card` now.
    fn handle(&self, seat: usize, card: CardInstanceId) -> ObservedCard {
        self.observers[seat]
            .observed_card(card)
            .unwrap_or_else(|| panic!("viewer {seat} holds no handle for {card:?}"))
    }

    /// The Observed Legal Actions of `seat`.
    fn listed(&self, seat: usize) -> Vec<ObservedAction> {
        self.ruleset
            .observed_legal_actions(&self.observers[seat], &self.state)
            .expect("the Observed Legal Actions are listed")
    }
}

/// A Game in which Player 0 has played `snare_vine` against two creatures in Player 1's field, and
/// the Resolution is paused. Returns the creatures, in ascending Card Instance Id.
fn snare_paused() -> (Game, Vec<CardInstanceId>, Vec<ObservedTransition>) {
    let mut game = Game::new();
    let snare = game.card(0, "snare_vine", 0);
    game.put(snare, "hand");
    let mut creatures = vec![
        game.card(1, "moss_sprite", 0),
        game.card(1, "goblin_scout", 0),
    ];
    for &creature in &creatures {
        game.put(creature, "field");
    }
    creatures.sort();
    let (_, seen) = game.take(&play(0, snare));
    (game, creatures, seen)
}

/// A Game in which Player 0 has played `mind_leech` and Player 1 is asked to pick a creature from
/// its hand. Returns Player 1's hand creatures, in ascending Card Instance Id.
fn leech_paused() -> (Game, Vec<CardInstanceId>, Vec<ObservedTransition>) {
    let mut game = Game::new();
    let leech = game.card(0, "mind_leech", 0);
    game.put(leech, "hand");
    let creatures = {
        let mut hand = game.in_zone(1, "hand");
        hand.sort();
        hand
    };
    assert!(!creatures.is_empty());
    let (_, seen) = game.take(&play(0, leech));
    (game, creatures, seen)
}

/// A Game in which Player 0 has played `hand_picker` and is asked to pick a card from its own hand,
/// which Player 1 cannot see. Returns the creatures of that hand, in ascending Card Instance Id.
fn hand_paused() -> (Game, Vec<CardInstanceId>, Vec<ObservedTransition>) {
    let mut game = Game::new();
    let picker = game.card(0, "hand_picker", 0);
    game.put(picker, "hand");
    let (_, seen) = game.take(&play(0, picker));
    let mut hand = game.in_zone(0, "hand");
    hand.retain(|&card| card != picker);
    hand.sort();
    assert!(!hand.is_empty());
    (game, hand, seen)
}

// ---- what a Viewer sees of the pause

#[test]
fn every_viewer_sees_a_pending_card_decision_and_its_decider() {
    let (_, _, seen) = snare_paused();
    for (seat, seen) in seen.iter().enumerate() {
        assert_eq!(
            seen.state.pending_decision(),
            Some(&pending(0)),
            "viewer {seat}"
        );
        assert_eq!(seen.state.deciding_player(), p(0), "viewer {seat}");
        assert_eq!(
            seen.events.last(),
            Some(&ObservedEvent::DecisionPending {
                decider: p(0),
                kind: DecisionKind::Card
            }),
            "viewer {seat}"
        );
        // Neither the Paused Resolution nor its bound names are observed.
        let text = format!("{seen:?}");
        for hidden in ["paused", "Paused", "bound", "BoundName", "QueuedTrigger"] {
            assert!(!text.contains(hidden), "{hidden} in {text}");
        }
    }

    // The other Player decides for the `mind_leech`, and both Viewers see who.
    let (_, _, seen) = leech_paused();
    for (seat, seen) in seen.iter().enumerate() {
        assert_eq!(
            seen.state.pending_decision(),
            Some(&pending(1)),
            "viewer {seat}"
        );
        assert_eq!(seen.state.deciding_player(), p(1), "viewer {seat}");
    }
}

// ---- the Deciding Player's Observed Legal Actions

#[test]
fn the_decider_names_candidates_by_handle_and_round_trips() {
    let (mut game, creatures, _) = snare_paused();

    // The Deciding Player lists one Answer per candidate, in the engine's order, each naming the
    // card by the handle the Viewer holds; the other Viewer lists none.
    let expected: Vec<ObservedAction> = creatures
        .iter()
        .map(|&card| ObservedAction::Answer {
            player: p(0),
            answer: named(game.handle(0, card)),
        })
        .collect();
    assert_eq!(expected.len(), 2);
    assert_eq!(game.listed(0), expected);
    assert!(game.listed(1).is_empty());

    // Translating them back gives the engine's Legal Actions, in the same order.
    let translated: Vec<Action> = expected
        .iter()
        .map(|observed| {
            game.ruleset
                .translate_action(&game.observers[0], observed)
                .expect("the Observed Action translates")
        })
        .collect();
    assert_eq!(translated, game.ruleset.legal_actions(&game.state));
    assert_eq!(
        translated,
        creatures
            .iter()
            .map(|&card| answer_card(0, card))
            .collect::<Vec<_>>()
    );

    // Another Viewer's Answer is not translated.
    assert_eq!(
        game.ruleset
            .translate_action(&game.observers[1], &expected[0]),
        Err(ObservationError::ActionOfAnotherPlayer { player: p(0) })
    );

    // Taking the translated Answer: the Taken Action and the first Observed Event carry one value,
    // the card as each Viewer holds it, and no Observed value names a Card Instance Id.
    let chosen = creatures[0];
    let (events, seen) = game.take(&translated[0]);
    assert_eq!(
        events[0],
        Event::DecisionAnswered {
            player: p(0),
            answer: Answer::Card(chosen)
        }
    );
    for (seat, seen) in seen.iter().enumerate() {
        let answer = named(game.handle(seat, chosen));
        assert_eq!(
            seen.action,
            TakenAction::Answer {
                player: p(0),
                answer
            },
            "viewer {seat}"
        );
        assert_eq!(
            seen.events.first(),
            Some(&ObservedEvent::DecisionAnswered {
                player: p(0),
                answer
            }),
            "viewer {seat}"
        );
        assert!(
            !format!("{seen:?}").contains("CardInstanceId"),
            "viewer {seat}"
        );
    }
}

#[test]
fn the_opponent_lists_and_takes_the_card_answers_of_an_opponent_decider() {
    let (mut game, creatures, _) = leech_paused();

    // The decider is the non-active Player, who holds a handle for every card of its own hand.
    assert!(game.listed(0).is_empty());
    let expected: Vec<ObservedAction> = creatures
        .iter()
        .map(|&card| ObservedAction::Answer {
            player: p(1),
            answer: named(game.handle(1, card)),
        })
        .collect();
    assert_eq!(game.listed(1), expected);
    let chosen = game
        .ruleset
        .translate_action(&game.observers[1], &expected[1])
        .expect("the Observed Action translates");
    assert_eq!(chosen, answer_card(1, creatures[1]));
    let (_, seen) = game.take(&chosen);
    assert_eq!(
        seen[1].action,
        TakenAction::Answer {
            player: p(1),
            answer: named(game.handle(1, creatures[1]))
        }
    );
}

// ---- what the other Viewer sees of the Answer

#[test]
fn the_other_viewer_sees_a_card_moved_to_a_public_trash() {
    let (mut game, creatures, _) = leech_paused();
    let chosen = creatures[0];
    // Player 0 does not see Player 1's hand: no handle for the card yet.
    assert_eq!(game.observers[0].observed_card(chosen), None);
    let kept = game.handle(1, chosen);

    let (_, seen) = game.take(&answer_card(1, chosen));

    // The trash is public: Player 0 sees the card arrive, under a handle it holds only after the
    // transition. The Taken Action names the card by it (the Taken Action rule: after, else
    // before), but the Observed `DecisionAnswered` is read from Player 0's sight at that Event,
    // which does not see the card yet: hidden. The decider keeps the handle it had, so its two
    // values coincide.
    let arrived = game.handle(0, chosen);
    assert_eq!(game.handle(1, chosen), kept);
    for (seat, taken, event) in [
        (
            0,
            named(arrived),
            ObservedAnswer::Card(TakenArgument::HiddenCard),
        ),
        (1, named(kept), named(kept)),
    ] {
        assert_eq!(
            seen[seat].action,
            TakenAction::Answer {
                player: p(1),
                answer: taken
            },
            "viewer {seat}"
        );
        assert_eq!(
            seen[seat].events.first(),
            Some(&ObservedEvent::DecisionAnswered {
                player: p(1),
                answer: event
            }),
            "viewer {seat}"
        );
    }
    assert!(matches!(
        seen[0].events.get(1),
        Some(ObservedEvent::CardMovedIntoSight { card, player, to, .. })
            if *card == arrived && *player == p(1) && *to == zone("trash")
    ));
    assert!(matches!(
        seen[1].events.get(1),
        Some(ObservedEvent::CardMoved { card, to, .. })
            if *card == kept && *to == zone("trash")
    ));
}

#[test]
fn the_other_viewer_sees_a_tapped_field_creature() {
    let (mut game, creatures, _) = snare_paused();
    let chosen = creatures[1];

    let (_, seen) = game.take(&answer_card(0, chosen));

    // The field is public: both Viewers hold a handle for the creature before and after, see it
    // tapped, and the Answer names it.
    for (seat, seen) in seen.iter().enumerate() {
        let handle = game.handle(seat, chosen);
        let answer = named(handle);
        assert_eq!(
            seen.action,
            TakenAction::Answer {
                player: p(0),
                answer
            },
            "viewer {seat}"
        );
        assert_eq!(
            seen.events,
            [
                ObservedEvent::DecisionAnswered {
                    player: p(0),
                    answer
                },
                ObservedEvent::CardStateChanged {
                    card: handle,
                    state: StateName::new("tapped"),
                    value: FieldValue::Bool(true)
                }
            ],
            "viewer {seat}"
        );
        assert_eq!(seen.state.pending_decision(), None, "viewer {seat}");
    }
}

#[test]
fn the_other_viewer_does_not_see_a_card_chosen_in_a_hand_that_stays_hidden() {
    let (mut game, hand, _) = hand_paused();
    let chosen = hand[0];
    // Player 1 never saw this card, and does not see Player 0's hand.
    assert_eq!(game.observers[1].observed_card(chosen), None);
    // Nothing is listed for it.
    assert!(game.listed(1).is_empty());
    assert_eq!(game.listed(0).len(), hand.len());

    let (_, seen) = game.take(&answer_card(0, chosen));

    // The card stays in a hand Player 1 cannot see: the Answer names a hidden card, in the Taken
    // Action and in the Observed `DecisionAnswered` alike, and there is no other Observed Event.
    let hidden = ObservedAnswer::Card(TakenArgument::HiddenCard);
    assert_eq!(
        seen[1].action,
        TakenAction::Answer {
            player: p(0),
            answer: hidden
        }
    );
    assert_eq!(
        seen[1].events,
        [ObservedEvent::DecisionAnswered {
            player: p(0),
            answer: hidden
        }]
    );
    assert_eq!(game.observers[1].observed_card(chosen), None);
    // The decider names its own card by handle.
    let own = named(game.handle(0, chosen));
    assert_eq!(
        seen[0].action,
        TakenAction::Answer {
            player: p(0),
            answer: own
        }
    );
    assert_eq!(
        seen[0].events,
        [ObservedEvent::DecisionAnswered {
            player: p(0),
            answer: own
        }]
    );
}

#[test]
fn translate_action_refuses_a_hidden_card_answer() {
    let (game, _, _) = hand_paused();
    let observer = &game.observers[1];

    // A hidden card is not a card a Viewer can name.
    assert_eq!(
        game.ruleset.translate_action(
            observer,
            &ObservedAction::Answer {
                player: p(1),
                answer: ObservedAnswer::Card(TakenArgument::HiddenCard)
            }
        ),
        Err(ObservationError::AnswerNamesHiddenCard)
    );
    // A handle the Viewer does not hold is the usual error.
    let unknown = ObservedCard(9_999);
    assert_eq!(
        game.ruleset.translate_action(
            observer,
            &ObservedAction::Answer {
                player: p(1),
                answer: named(unknown)
            }
        ),
        Err(ObservationError::UnknownObservedCard { card: unknown })
    );
    // A handle the Viewer holds maps back to the card: here a card of Player 1's own hand.
    let own = game.in_zone(1, "hand")[0];
    assert_eq!(
        game.ruleset.translate_action(
            observer,
            &ObservedAction::Answer {
                player: p(1),
                answer: named(game.handle(1, own))
            }
        ),
        Ok(answer_card(1, own))
    );
    // Another Player's Answer is not translated, whatever it names.
    assert_eq!(
        game.ruleset.translate_action(
            observer,
            &ObservedAction::Answer {
                player: p(0),
                answer: ObservedAnswer::Card(TakenArgument::HiddenCard)
            }
        ),
        Err(ObservationError::ActionOfAnotherPlayer { player: p(0) })
    );
}

// ---- slicing a trace

#[test]
fn follow_events_gives_the_same_observed_events_however_a_trace_is_sliced() {
    // The `mind_leech` is played and then answered: two transitions, followed in one call or in two.
    let mut game = Game::new();
    let leech = game.card(0, "mind_leech", 0);
    game.put(leech, "hand");
    let mut hand = game.in_zone(1, "hand");
    hand.sort();
    let chosen = hand[0];
    let before = game.state.clone();
    let first = game
        .ruleset
        .transition(&before, &play(0, leech))
        .expect("the play is accepted");
    let second = game
        .ruleset
        .transition(&first.state, &answer_card(1, chosen))
        .expect("the Answer is accepted");
    let whole: Vec<Event> = first
        .trace
        .events()
        .iter()
        .chain(second.trace.events())
        .cloned()
        .collect();

    for seat in 0..2 {
        let mut together = game.observers[seat].clone();
        let all = game
            .ruleset
            .follow_events(&mut together, &before, &whole)
            .expect("the slice is followed");

        let mut apart = game.observers[seat].clone();
        let mut pieces = game
            .ruleset
            .follow_events(&mut apart, &before, first.trace.events())
            .expect("the first trace is followed");
        pieces.extend(
            game.ruleset
                .follow_events(&mut apart, &first.state, second.trace.events())
                .expect("the second trace is followed"),
        );

        assert_eq!(all, pieces, "viewer {seat}");
        assert_eq!(together, apart, "viewer {seat}");
    }

    // Player 0 does not see the card at the Answer, so a slice never names it by a handle it gets
    // only later: it is hidden in the Observed Event however the trace is sliced.
    let mut observer = game.observers[0].clone();
    let events = game
        .ruleset
        .follow_events(&mut observer, &before, &whole)
        .expect("the slice is followed");
    assert!(events.contains(&ObservedEvent::DecisionAnswered {
        player: p(1),
        answer: ObservedAnswer::Card(TakenArgument::HiddenCard)
    }));
}

#[test]
fn a_card_chosen_in_a_hidden_hand_and_played_later_stays_hidden_in_every_slice() {
    // `hand_picker` picks a goblin that stays in Player 0's hand, which Player 1 cannot see; in a
    // later transition Player 0 plays it, and Player 1 sees it arrive in the public field.
    let mut game = Game::new();
    let picker = game.card(0, "hand_picker", 0);
    let goblin = game.card(0, "goblin_scout", 0);
    game.put(picker, "hand");
    game.put(goblin, "hand");
    let before = game.state.clone();
    let first = game
        .ruleset
        .transition(&before, &play(0, picker))
        .expect("the play is accepted");
    let second = game
        .ruleset
        .transition(&first.state, &answer_card(0, goblin))
        .expect("the Answer is accepted");
    let third = game
        .ruleset
        .transition(&second.state, &play(0, goblin))
        .expect("the later play is accepted");
    let whole: Vec<Event> = [&first, &second, &third]
        .iter()
        .flat_map(|transition| transition.trace.events().iter().cloned())
        .collect();

    let hidden = ObservedEvent::DecisionAnswered {
        player: p(0),
        answer: ObservedAnswer::Card(TakenArgument::HiddenCard),
    };
    // Player 1 never sees the card at the Answer, however the Events are sliced.
    let mut together = game.observers[1].clone();
    let all = game
        .ruleset
        .follow_events(&mut together, &before, &whole)
        .expect("the slice is followed");
    let mut apart = game.observers[1].clone();
    let mut pieces = Vec::new();
    for (previous, transition) in [
        (&before, &first),
        (&first.state, &second),
        (&second.state, &third),
    ] {
        pieces.extend(
            game.ruleset
                .follow_events(&mut apart, previous, transition.trace.events())
                .expect("the trace is followed"),
        );
    }
    assert_eq!(all, pieces);
    assert_eq!(together, apart);
    assert!(all.contains(&hidden), "{all:?}");
    // The card does come into Player 1's sight in the third transition.
    assert!(game.observers[1].observed_card(goblin).is_none());
    assert!(together.observed_card(goblin).is_some());
}

// ---- the fold of Observed Events

#[test]
fn the_observed_fold_applies_a_card_answer() {
    let (mut game, creatures, paused) = snare_paused();
    let chosen = creatures[0];
    let (_, after) = game.take(&answer_card(0, chosen));

    // Applying the Observed Events of the Answer to the paused Observed GameState gives the next
    // one, for each Viewer: the fold clears the Pending Decision.
    for seat in 0..2 {
        assert_eq!(
            paused[seat].state.apply_events(&after[seat].events),
            Ok(after[seat].state.clone()),
            "viewer {seat}"
        );
    }

    // An Answer of another Player than the decider does not fit.
    let wrong = ObservedEvent::DecisionAnswered {
        player: p(1),
        answer: ObservedAnswer::Card(TakenArgument::HiddenCard),
    };
    let mut working = paused[0].state.clone();
    assert_eq!(
        working.apply(&wrong),
        Err(ObservedApplyError::EventDoesNotFit)
    );
    assert_eq!(working, paused[0].state);
}
