//! Acceptance tests for ticket #63: observing a GameState (ADR-0028), on the whole real Effects
//! Ruleset, its real catalog and its two real sample decks, and on the Small Ruleset for the last
//! test.
//!
//! A **Viewer** (a Player of the Game) observes through an **Observer**, a fold of the Events applied
//! to the Empty GameState, which maps **Observed Cards** to Card Instances and is never shown to the
//! Viewer. The **Observed GameState** shows each zone the Viewer can see card by card and every
//! other zone as a count (ADR-0004: `public` all, `owner` its owner, `hidden` nobody). The Toy
//! Ruleset not implementing the capability is a `compile_fail` doctest on `Observations`.
//!
//! The oracles are computed here from the Events and the Ruleset Definition's zone visibility; no
//! test reads an expectation from the code under test.

use std::collections::BTreeMap;

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState,
    ObservationError, ObservedCard, ObservedGameState, Observer, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset::{Observations, Ruleset};
use tcg_engine::ruleset_definition::{
    ActionId, FieldName, FieldValue, PhaseId, Visibility, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");
const EFFECTS_DECK_A: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-a.yaml");
const EFFECTS_DECK_B: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-b.yaml");

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const SMALL_DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const SMALL_DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// The Seed of every Game of these tests. With the Effects sample decks, seat 0 opens with a
/// `battle_veteran` (card 41) and a `banner_bearer` (card 47) in hand, which the derived-read test
/// plays; the test asserts it.
const SEED: u64 = 7;

/// The cards of the derived-read scenario in the Opening Position of `SEED`.
const VETERAN: CardInstanceId = CardInstanceId(41);
const BANNER: CardInstanceId = CardInstanceId(47);

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

fn effects() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(EFFECTS_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, EFFECTS_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn small() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn decks(ruleset: &DeclarativeRuleset, yamls: [&str; 2]) -> Vec<DeckList> {
    yamls
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn create(ruleset: &DeclarativeRuleset, yamls: [&str; 2], seed: u64) -> GameCreation {
    ruleset
        .create_game(&decks(ruleset, yamls), Seed(seed))
        .expect("the sample decks create a game")
}

fn create_effects(ruleset: &DeclarativeRuleset) -> GameCreation {
    create(ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], SEED)
}

/// The Observer of seat `seat` over the Events `events` applied to the Empty GameState.
fn observer_over(
    ruleset: &DeclarativeRuleset,
    creation: &GameCreation,
    seat: u32,
    events: &[Event],
) -> Observer {
    ruleset
        .observer(viewer(seat), &creation.empty, events)
        .expect("the Observer is built")
}

/// The Observer of seat `seat` over the creation trace alone.
fn creation_observer(ruleset: &DeclarativeRuleset, creation: &GameCreation, seat: u32) -> Observer {
    observer_over(ruleset, creation, seat, creation.opening.trace.events())
}

/// What seat `seat` sees of the Opening Position.
fn observe_opening(
    ruleset: &DeclarativeRuleset,
    creation: &GameCreation,
    seat: u32,
) -> ObservedGameState {
    let observer = creation_observer(ruleset, creation, seat);
    ruleset
        .observe(&observer, &creation.opening.state)
        .expect("the Opening Position is observed")
}

fn visibility_of(ruleset: &DeclarativeRuleset, zone_id: &ZoneId) -> Visibility {
    ruleset
        .definition()
        .zone(zone_id)
        .expect("the zone is declared")
        .visibility()
}

/// Whether `viewer_seat` sees the cards of `owner`'s zone `zone_id`, by ADR-0004.
fn sees(ruleset: &DeclarativeRuleset, viewer_seat: u32, owner: u32, zone_id: &ZoneId) -> bool {
    match visibility_of(ruleset, zone_id) {
        Visibility::Public => true,
        Visibility::Owner => viewer_seat == owner,
        Visibility::Hidden => false,
    }
}

fn zone_ids(ruleset: &DeclarativeRuleset) -> Vec<ZoneId> {
    ruleset
        .definition()
        .zones()
        .iter()
        .map(|declared| declared.id().clone())
        .collect()
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

/// The cards seat `seat` finds visible walking `events`, in Event order: the ones a `CardCreated`
/// puts, or a `CardMoved` brings from a zone the Viewer cannot see, into a zone the Viewer sees.
fn appearances(ruleset: &DeclarativeRuleset, seat: u32, events: &[Event]) -> Vec<CardInstanceId> {
    let mut found = Vec::new();
    for event in events {
        match event {
            Event::CardCreated {
                card, owner, zone, ..
            } if sees(ruleset, seat, owner.0, zone) => found.push(*card),
            Event::CardMoved {
                card,
                player,
                from,
                to,
            } if !sees(ruleset, seat, player.0, from) && sees(ruleset, seat, player.0, to) => {
                found.push(*card);
            }
            _ => {}
        }
    }
    found
}

// ---- tests

#[test]
fn each_player_observes_their_hand_and_the_public_zones_at_the_opening_position() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let state = &creation.opening.state;

    for seat in [0, 1] {
        let observer = creation_observer(&ruleset, &creation, seat);
        let observed = ruleset
            .observe(&observer, state)
            .expect("the Opening Position is observed");
        assert_eq!(observed.viewer(), viewer(seat));
        assert_eq!(observed.players(), [p(0), p(1)]);

        // The Players and their zones, in Player Order and then the declared zone order.
        for owner in [0, 1] {
            let listed: Vec<&ZoneId> = observed
                .zones(p(owner))
                .expect("the Player is in the Game")
                .iter()
                .map(|observed_zone| observed_zone.zone())
                .collect();
            let declared = zone_ids(&ruleset);
            assert_eq!(listed, declared.iter().collect::<Vec<_>>());
        }

        // Own hand: five cards, each with its Card Definition and instance state.
        let hand = observed
            .zone(p(seat), &zone("hand"))
            .expect("the hand exists");
        let handles = hand.cards().expect("a Player sees their own hand");
        assert_eq!(handles.len(), 5);
        assert_eq!(hand.count(), 5);
        for &handle in handles {
            let visible = observed.card(handle).expect("a seen card is visible");
            let id = observer
                .card_instance(handle)
                .expect("the Observer holds the handle");
            let instance = state.card(id).expect("the card exists");
            assert_eq!(visible.card(), handle);
            assert_eq!(visible.owner(), instance.owner());
            assert_eq!(visible.owner(), p(seat));
            assert_eq!(visible.definition(), instance.definition());
            assert_eq!(visible.state(), instance.state());
            assert_eq!(state.zone_of(id), Some(&zone("hand")));
        }

        // Every card of the public zones of both Players, in stored order.
        for owner in [0, 1] {
            for public in ["leader_zone", "mana_zone", "field", "trash"] {
                let stored = state
                    .zone(p(owner), &zone(public))
                    .expect("the zone exists");
                let seen = observed
                    .zone(p(owner), &zone(public))
                    .expect("the zone is observed");
                assert_eq!(seen.count(), stored.len(), "{public} of seat {owner}");
                let handles = seen.cards().expect("a public zone is seen by everyone");
                let instances: Vec<CardInstanceId> = handles
                    .iter()
                    .map(|&handle| observer.card_instance(handle).expect("a held handle"))
                    .collect();
                assert_eq!(instances, stored, "{public} of seat {owner}");
            }
        }
        // A Leader is in each leader_zone and the opening has two cards of mana in play.
        assert_eq!(
            observed.zone(p(0), &zone("leader_zone")).map(|z| z.count()),
            Some(1)
        );
        assert_eq!(
            observed.zone(p(0), &zone("mana_zone")).map(|z| z.count()),
            Some(2)
        );

        // Only a count: both decks, both mana decks and the opponent's hand.
        let opponent = 1 - seat;
        let hidden = [
            (seat, "deck"),
            (opponent, "deck"),
            (seat, "mana_deck"),
            (opponent, "mana_deck"),
            (opponent, "hand"),
        ];
        for (owner, name) in hidden {
            let stored = state.zone(p(owner), &zone(name)).expect("the zone exists");
            let unseen = observed
                .zone(p(owner), &zone(name))
                .expect("the zone is observed");
            assert_eq!(unseen.cards(), None, "{name} of seat {owner}");
            assert_eq!(unseen.count(), stored.len(), "{name} of seat {owner}");
        }

        // The always-visible facts.
        assert_eq!(observed.turn_number(), Some(TurnNumber::FIRST));
        assert_eq!(observed.active_player(), p(0));
        assert_eq!(observed.phase(), Some(&PhaseId::new("main")));
        assert_eq!(observed.outcome(), None);

        // Every visible card is in a zone the Viewer sees, and nothing else is.
        let seen_total: usize = zone_ids(&ruleset)
            .iter()
            .flat_map(|zone_id| [(0, zone_id), (1, zone_id)])
            .filter(|(owner, zone_id)| sees(&ruleset, seat, *owner, zone_id))
            .map(|(owner, zone_id)| state.zone(p(owner), zone_id).map_or(0, <[_]>::len))
            .sum();
        assert_eq!(observed.cards().len(), seen_total);
    }
}

#[test]
fn the_two_players_observations_differ_only_where_visibility_differs() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let first = observe_opening(&ruleset, &creation, 0);
    let second = observe_opening(&ruleset, &creation, 1);

    // The always-visible facts are equal.
    assert_eq!(first.players(), second.players());
    assert_eq!(first.active_player(), second.active_player());
    assert_eq!(first.turn_number(), second.turn_number());
    assert_eq!(first.phase(), second.phase());
    assert_eq!(first.outcome(), second.outcome());
    assert_ne!(first.viewer(), second.viewer());

    for owner in [0, 1] {
        for zone_id in zone_ids(&ruleset) {
            let one = first.zone(p(owner), &zone_id).expect("observed");
            let other = second.zone(p(owner), &zone_id).expect("observed");
            // Every zone has the same count for both.
            assert_eq!(one.count(), other.count(), "{zone_id:?} of seat {owner}");
            match visibility_of(&ruleset, &zone_id) {
                Visibility::Public => {
                    // Same cards, same facts, in the same order; the handles may differ.
                    let facts = |observed: &ObservedGameState, handles: &[ObservedCard]| {
                        handles
                            .iter()
                            .map(|&handle| {
                                let visible = observed.card(handle).expect("visible");
                                (
                                    visible.owner(),
                                    visible.definition().clone(),
                                    visible.state().to_vec(),
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    let one_cards = one.cards().expect("public is seen");
                    let other_cards = other.cards().expect("public is seen");
                    assert_eq!(
                        facts(&first, one_cards),
                        facts(&second, other_cards),
                        "{zone_id:?} of seat {owner}"
                    );
                }
                Visibility::Owner => {
                    // Seen by its owner, a count for the other.
                    let (by_owner, by_other) = if owner == 0 {
                        (one, other)
                    } else {
                        (other, one)
                    };
                    assert!(by_owner.cards().is_some(), "{zone_id:?} of seat {owner}");
                    assert_eq!(by_other.cards(), None, "{zone_id:?} of seat {owner}");
                }
                Visibility::Hidden => {
                    assert_eq!(one.cards(), None);
                    assert_eq!(other.cards(), None);
                }
            }
        }
    }
}

#[test]
fn no_observation_exposes_a_card_instance_id_a_random_state_or_a_hidden_order() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let opening = &creation.opening.state;

    // No observed type names a Card Instance Id or a Random State.
    for seat in [0, 1] {
        let observed = observe_opening(&ruleset, &creation, seat);
        let text = format!("{observed:?}");
        assert!(!text.contains("CardInstanceId"), "seat {seat}: {text}");
        assert!(!text.contains("RandomState"), "seat {seat}: {text}");
    }

    // A shuffle of P0's deck to the reverse order and another Random State: the Observer that has
    // followed it, observing the shuffled GameState, shows the same as before.
    let deck = opening
        .zone(p(0), &zone("deck"))
        .expect("the deck exists")
        .to_vec();
    let reversed: Vec<CardInstanceId> = deck.iter().rev().copied().collect();
    assert_ne!(deck, reversed);
    let shuffle = Event::ZoneShuffled {
        player: p(0),
        zone: zone("deck"),
        order: reversed,
        random_state: RandomState::from_seed(Seed(SEED + 1000)),
    };
    assert_ne!(
        opening.random_state(),
        &RandomState::from_seed(Seed(SEED + 1000))
    );
    let shuffled = opening
        .apply_trace(std::slice::from_ref(&shuffle))
        .expect("the shuffle applies");
    assert_ne!(&shuffled, opening);

    let mut events = creation.opening.trace.events().to_vec();
    events.push(shuffle);
    for seat in [0, 1] {
        let before = observe_opening(&ruleset, &creation, seat);
        let observer = observer_over(&ruleset, &creation, seat, &events);
        let after = ruleset
            .observe(&observer, &shuffled)
            .expect("the shuffled GameState is observed");
        assert_eq!(after, before, "seat {seat}");
    }
}

#[test]
fn observed_cards_are_dense_from_zero_in_creation_trace_order() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let events = creation.opening.trace.events();

    for seat in [0, 1] {
        let observer = creation_observer(&ruleset, &creation, seat);
        let observed = ruleset
            .observe(&observer, &creation.opening.state)
            .expect("the Opening Position is observed");
        let expected = appearances(&ruleset, seat, events);

        // Handles are exactly 0..n.
        let handles: Vec<ObservedCard> = observed.cards().iter().map(|card| card.card()).collect();
        let dense: Vec<ObservedCard> = (0..expected.len())
            .map(|handle| ObservedCard(u32::try_from(handle).expect("few cards")))
            .collect();
        assert_eq!(handles, dense, "seat {seat}");

        // Handle i is the i-th card found visible in the creation trace.
        for (index, card) in expected.iter().enumerate() {
            let handle = ObservedCard(u32::try_from(index).expect("few cards"));
            assert_eq!(observer.card_instance(handle), Some(*card), "seat {seat}");
            assert_eq!(observer.observed_card(*card), Some(handle), "seat {seat}");
        }
        // One past the last handle is held by nobody.
        let next = ObservedCard(u32::try_from(expected.len()).expect("few cards"));
        assert_eq!(observer.card_instance(next), None);

        // The two Leaders come first, then the cards of the opening hand in draw order, and the
        // Event order is not the order of the ids: seat 1's Leader has the id 61.
        assert_eq!(expected[..2], [CardInstanceId(0), CardInstanceId(61)]);
        let hand = creation
            .opening
            .state
            .zone(p(seat), &zone("hand"))
            .expect("the hand exists");
        assert_eq!(&expected[2..7], hand, "seat {seat} draws five cards");
    }
}

#[test]
fn identical_games_give_identical_observations() {
    let ruleset = effects();
    let one = create_effects(&ruleset);
    let other = create_effects(&ruleset);
    assert_eq!(one, other);

    for seat in [0, 1] {
        assert_eq!(
            creation_observer(&ruleset, &one, seat),
            creation_observer(&ruleset, &other, seat)
        );
        assert_eq!(
            observe_opening(&ruleset, &one, seat),
            observe_opening(&ruleset, &other, seat)
        );
    }
}

#[test]
fn a_viewer_that_is_not_a_player_in_the_game_is_a_typed_error() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let stranger = Viewer::Player(p(2));

    let by_creation = ruleset.observer(stranger, &creation.empty, creation.opening.trace.events());
    assert!(matches!(
        by_creation,
        Err(ObservationError::UnknownViewer { viewer }) if viewer == stranger
    ));
    // With no Events at all the Viewer is still checked.
    let by_nothing = ruleset.observer(stranger, &creation.empty, &[]);
    assert!(matches!(
        by_nothing,
        Err(ObservationError::UnknownViewer { .. })
    ));
}

#[test]
fn an_observer_out_of_step_with_the_game_state_is_a_typed_error() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let events = creation.opening.trace.events();

    // The last card moved into a hand or the mana_zone is the last mana of the first Turn: both
    // Viewers see it arrive (the mana_zone is public).
    let missing = events
        .iter()
        .rposition(|event| {
            matches!(event, Event::CardMoved { to, .. }
                if *to == zone("hand") || *to == zone("mana_zone"))
        })
        .expect("the trace moves cards into hand and mana_zone");
    let mut short = events.to_vec();
    short.remove(missing);

    for seat in [0, 1] {
        let observer = observer_over(&ruleset, &creation, seat, &short);
        let result = ruleset.observe(&observer, &creation.opening.state);
        assert_eq!(
            result,
            Err(ObservationError::ObserverOutOfStep),
            "seat {seat}"
        );
    }

    // Events that do not fit what the Observer holds are out of step too, never a panic: a card
    // moved out of a zone the Viewer sees that it was never shown in, and a move to a zone the
    // ruleset does not declare.
    let unshown = Event::CardMoved {
        card: CardInstanceId(0),
        player: p(0),
        from: zone("hand"),
        to: zone("field"),
    };
    let result = ruleset.observer(viewer(0), &creation.empty, &[unshown]);
    assert!(matches!(result, Err(ObservationError::ObserverOutOfStep)));
    let undeclared = Event::CardMoved {
        card: CardInstanceId(0),
        player: p(0),
        from: zone("deck"),
        to: zone("no_such_zone"),
    };
    let result = ruleset.observer(viewer(0), &creation.empty, &[undeclared]);
    assert!(matches!(result, Err(ObservationError::ObserverOutOfStep)));
}

#[test]
fn a_derived_read_includes_self_and_aura_field_modifiers() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let opening = &creation.opening.state;
    assert_eq!(
        opening.card(VETERAN).map(|card| card.definition().as_str()),
        Some("battle_veteran")
    );
    assert_eq!(
        opening.card(BANNER).map(|card| card.definition().as_str()),
        Some("banner_bearer")
    );
    assert_eq!(opening.zone_of(VETERAN), Some(&zone("hand")));
    assert_eq!(opening.zone_of(BANNER), Some(&zone("hand")));
    let leader_1 = opening
        .zone(p(1), &zone("leader_zone"))
        .expect("the zone exists")[0];

    // Seat 0 plays the veteran, both Players pass, then seat 0 plays the banner bearer and attacks
    // with the veteran, which taps: the veteran reads 2 (base) + 1 (self, while tapped) + 1 (the
    // banner bearer's aura) = 4, and the banner bearer, which its own aura does not reach, reads 1.
    let script = [
        action(0, "play_creature", &[VETERAN]),
        action(0, "pass_turn", &[]),
        action(1, "pass_turn", &[]),
        action(0, "play_creature", &[BANNER]),
        action(0, "attack", &[VETERAN, leader_1]),
    ];
    let mut state: GameState = opening.clone();
    let mut events = creation.opening.trace.events().to_vec();
    for (index, step) in script.iter().enumerate() {
        let transition = ruleset
            .transition(&state, step)
            .unwrap_or_else(|rejection| panic!("action {} is rejected: {rejection:?}", index + 1));
        events.extend_from_slice(transition.trace.events());
        state = transition.state;
    }
    assert_eq!(state.zone_of(VETERAN), Some(&zone("field")));
    assert_eq!(state.zone_of(BANNER), Some(&zone("field")));

    let power = FieldName::new("power");
    for seat in [0, 1] {
        let observer = observer_over(&ruleset, &creation, seat, &events);
        let observed = ruleset
            .observe(&observer, &state)
            .expect("the GameState is observed");
        let veteran = observer
            .observed_card(VETERAN)
            .expect("the veteran is in the public field");
        let banner = observer
            .observed_card(BANNER)
            .expect("the banner bearer is in the public field");

        assert_eq!(
            ruleset.derived_read(&observer, &state, veteran, &power),
            Ok(4),
            "seat {seat}"
        );
        assert_eq!(
            ruleset.derived_read(&observer, &state, banner, &power),
            Ok(1),
            "seat {seat}"
        );
        // A field the modifiers do not touch reads its base value.
        assert_eq!(
            ruleset.derived_read(&observer, &state, veteran, &FieldName::new("cost")),
            Ok(1),
            "seat {seat}"
        );

        // The Observed GameState holds base facts only: the stored instance state, no power.
        let visible = observed.card(veteran).expect("the veteran is visible");
        let stored = state.card(VETERAN).expect("the veteran exists");
        assert_eq!(visible.state(), stored.state());
        assert!(visible
            .state()
            .iter()
            .any(|(name, value)| name.as_str() == "tapped" && *value == FieldValue::Bool(true)));
        assert!(!format!("{observed:?}").contains("power"));

        // A text field is not an int field, nor is a field the card does not have.
        let name = FieldName::new("name");
        let error = ruleset
            .derived_read(&observer, &state, veteran, &name)
            .expect_err("a text field has no Derived Read");
        assert_eq!(
            error,
            ObservationError::NotAnIntField {
                card: veteran,
                field: name
            }
        );
        let absent = FieldName::new("no_such_field");
        assert!(matches!(
            ruleset.derived_read(&observer, &state, veteran, &absent),
            Err(ObservationError::NotAnIntField { .. })
        ));

        // A handle the Viewer does not hold is a typed error, and no error names a Card Instance Id.
        let unheld = ObservedCard(100_000);
        let error = ruleset
            .derived_read(&observer, &state, unheld, &power)
            .expect_err("the Viewer holds no such handle");
        assert_eq!(
            error,
            ObservationError::UnknownObservedCard { card: unheld }
        );
        assert!(!format!("{error:?}").contains("CardInstanceId"));
        assert!(!format!("{error}").is_empty());
    }

    // The opponent's hand is not seen, so no handle of it is held: seat 1 holds no handle for a
    // card of seat 0's hand.
    let observer_1 = observer_over(&ruleset, &creation, 1, &events);
    let hidden_card = state.zone(p(0), &zone("hand")).expect("the hand exists")[0];
    assert_eq!(observer_1.observed_card(hidden_card), None);
}

#[test]
fn observations_work_on_the_small_ruleset() {
    // The Small Ruleset's files and golden snapshots are not edited by this ticket; the Game here is
    // created from the real files and only read.
    let ruleset = small();
    let creation = create(&ruleset, [SMALL_DECK_A, SMALL_DECK_B], 9);
    let state = &creation.opening.state;

    for seat in [0, 1] {
        let observer = creation_observer(&ruleset, &creation, seat);
        let observed = ruleset
            .observe(&observer, state)
            .expect("the Opening Position is observed");
        let opponent = 1 - seat;

        let hand = observed.zone(p(seat), &zone("hand")).expect("the hand");
        assert_eq!(hand.count(), 5);
        assert_eq!(hand.cards().map(<[_]>::len), Some(5));
        for (owner, name) in [
            (opponent, "hand"),
            (0, "deck"),
            (1, "deck"),
            (0, "mana_deck"),
            (1, "mana_deck"),
        ] {
            let unseen = observed.zone(p(owner), &zone(name)).expect("the zone");
            assert_eq!(unseen.cards(), None, "{name} of seat {owner}");
            assert_eq!(
                unseen.count(),
                state.zone(p(owner), &zone(name)).expect("stored").len()
            );
        }
        for owner in [0, 1] {
            for public in ["leader_zone", "mana_zone", "field", "trash"] {
                let seen = observed.zone(p(owner), &zone(public)).expect("the zone");
                assert!(seen.cards().is_some(), "{public} of seat {owner}");
            }
        }
        let expected = appearances(&ruleset, seat, creation.opening.trace.events());
        let by_handle: BTreeMap<ObservedCard, Option<CardInstanceId>> = observed
            .cards()
            .iter()
            .map(|card| (card.card(), observer.card_instance(card.card())))
            .collect();
        assert_eq!(
            by_handle.values().copied().collect::<Vec<_>>(),
            expected.into_iter().map(Some).collect::<Vec<_>>()
        );
    }
}
