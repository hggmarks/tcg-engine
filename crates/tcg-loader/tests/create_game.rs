//! Acceptance tests for ticket #21: Game creation with a deterministic opening position, on the
//! real sample files of the Small Ruleset (its setup, catalog and two decks).
//!
//! The ruleset file is read up to its `turn:` line, because the behavior sections after it are not
//! loaded yet. Code-built creation, `apply` and the creation errors are tested in the engine crate,
//! in `tests/declarative_game.rs`.

use tcg_engine::card_catalog::CardId;
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState};
use tcg_engine::player::PlayerId;
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset_definition::{FieldName, FieldValue, RulesetDefinition, StateName, ZoneId};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

// ---- helpers

fn definition() -> RulesetDefinition {
    let (before_turn, _) = SMALL_RULESET
        .split_once("\nturn:")
        .expect("the Small Ruleset has a turn section");
    load_ruleset_definition(&format!("{before_turn}\n"))
        .expect("the Small Ruleset, without its turn section, loads")
}

fn ruleset() -> DeclarativeRuleset {
    let definition = definition();
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// Deck A for seat 0 and deck B for seat 1.
fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    [DECK_A, DECK_B]
        .into_iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn create(seed: u64) -> GameCreation {
    let ruleset = ruleset();
    ruleset
        .create_game(&decks(&ruleset), Seed(seed))
        .expect("the sample decks create a game")
}

fn seat(n: u32) -> PlayerId {
    PlayerId(n)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn cards_in(state: &GameState, player: PlayerId, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(player, &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

fn definition_of(state: &GameState, id: CardInstanceId) -> CardId {
    state
        .card(id)
        .expect("the card instance exists")
        .definition()
        .clone()
}

/// The ids of the `CardCreated` Events of a Player's zone, in trace order.
fn created_in(creation: &GameCreation, player: PlayerId, zone_id: &str) -> Vec<CardInstanceId> {
    creation
        .opening
        .trace
        .events()
        .iter()
        .filter_map(|event| match event {
            Event::CardCreated {
                card,
                owner,
                zone: placed,
                ..
            } if *owner == player && placed == &zone(zone_id) => Some(*card),
            _ => None,
        })
        .collect()
}

/// The `ZoneShuffled` Events of the trace, in order.
fn shuffles(creation: &GameCreation) -> Vec<&Event> {
    creation
        .opening
        .trace
        .events()
        .iter()
        .filter(|event| matches!(event, Event::ZoneShuffled { .. }))
        .collect()
}

/// The order a Player's deck was shuffled into.
fn shuffled_order(creation: &GameCreation, player: PlayerId, zone_id: &str) -> Vec<CardInstanceId> {
    creation
        .opening
        .trace
        .events()
        .iter()
        .find_map(|event| match event {
            Event::ZoneShuffled {
                player: shuffled,
                zone: shuffled_zone,
                order,
                ..
            } if *shuffled == player && shuffled_zone == &zone(zone_id) => Some(order.clone()),
            _ => None,
        })
        .expect("the zone was shuffled")
}

/// The cards of a Deck List, every entry repeated `count` times, sorted.
fn expanded_sorted(deck: &DeckList) -> Vec<CardId> {
    let mut cards: Vec<CardId> = deck
        .inputs()
        .iter()
        .flat_map(|input| input.entries())
        .flat_map(|entry| {
            let count = usize::try_from(entry.count().get()).expect("a count fits in usize");
            std::iter::repeat_n(entry.card().clone(), count)
        })
        .collect();
    cards.sort();
    cards
}

fn describe(event: &Event) -> String {
    match event {
        Event::CardCreated { owner, zone, .. } => {
            format!("created p{} {}", owner.0, zone.as_str())
        }
        Event::CardMoved {
            player, from, to, ..
        } => format!("moved p{} {} {}", player.0, from.as_str(), to.as_str()),
        Event::ZoneShuffled { player, zone, .. } => {
            format!("shuffled p{} {}", player.0, zone.as_str())
        }
    }
}

// ---- the opening position

#[test]
fn game_creation_places_each_leader_in_its_leader_zone() {
    let state = create(1).opening.state;
    for (player, leader) in [(0, "captain_ardel"), (1, "warden_ilse")] {
        let in_zone = cards_in(&state, seat(player), "leader_zone");
        assert_eq!(in_zone.len(), 1, "seat {player} has exactly one Leader");
        let id = in_zone[0];
        assert_eq!(definition_of(&state, id), CardId::new(leader));
        assert_eq!(state.card(id).map(|card| card.owner()), Some(seat(player)));
    }
}

#[test]
fn every_deck_card_is_one_card_instance_in_one_zone() {
    let ruleset = ruleset();
    let decks = decks(&ruleset);
    let state = ruleset
        .create_game(&decks, Seed(1))
        .expect("the sample decks create a game")
        .opening
        .state;

    // 2 Players x (1 Leader + 50 main deck + 10 mana deck) card instances, with dense ids.
    assert_eq!(state.cards().len(), 122);
    for (position, card) in state.cards().iter().enumerate() {
        let expected = u32::try_from(position).expect("the position fits in u32");
        assert_eq!(card.id(), CardInstanceId(expected));
    }

    // Each instance is in exactly one zone of exactly one Player, and that Player owns it.
    for card in state.cards() {
        let mut holders = Vec::new();
        for player in state.player_order().players() {
            for zone_cards in state.zones(*player).expect("the Player has zones") {
                let times = zone_cards
                    .cards()
                    .iter()
                    .filter(|&&held| held == card.id())
                    .count();
                holders.extend(std::iter::repeat_n(*player, times));
            }
        }
        assert_eq!(holders, vec![card.owner()], "card {:?}", card.id());
    }

    // Per seat, the definitions are exactly the expanded Deck List.
    for (player, deck) in [seat(0), seat(1)].into_iter().zip(&decks) {
        let mut definitions: Vec<CardId> = state
            .cards()
            .iter()
            .filter(|card| card.owner() == player)
            .map(|card| card.definition().clone())
            .collect();
        definitions.sort();
        assert_eq!(definitions, expanded_sorted(deck), "seat {}", player.0);
    }
}

#[test]
fn game_creation_shuffles_the_main_deck_with_the_random_state() {
    let seed = Seed(42);
    let creation = create(42);

    // Shuffle the pre-shuffle ids (the trace's `CardCreated` order) with the Seed's generator:
    // one stream, seat 0's deck first and then seat 1's.
    let mut generator = RandomState::from_seed(seed);
    let mut expected = Vec::new();
    for player in [seat(0), seat(1)] {
        let mut order = created_in(&creation, player, "deck");
        assert_eq!(order.len(), 50);
        generator.shuffle(&mut order);
        expected.push(Event::ZoneShuffled {
            player,
            zone: zone("deck"),
            order,
            random_state: generator.clone(),
        });
    }
    let found: Vec<Event> = shuffles(&creation).into_iter().cloned().collect();
    assert_eq!(found, expected);

    // The opening state keeps the Random State left by the last shuffle.
    assert_eq!(creation.opening.state.random_state(), &generator);
}

#[test]
fn game_creation_leaves_the_mana_deck_unshuffled() {
    let creation = create(42);
    for player in [seat(0), seat(1)] {
        let created = created_in(&creation, player, "mana_deck");
        assert_eq!(created.len(), 10);
        let ascending: Vec<CardInstanceId> = {
            let mut sorted = created.clone();
            sorted.sort();
            sorted
        };
        assert_eq!(created, ascending, "mana cards are created in id order");
        assert_eq!(
            cards_in(&creation.opening.state, player, "mana_deck"),
            created,
            "the mana deck keeps its creation order"
        );
    }
    for event in shuffles(&creation) {
        match event {
            Event::ZoneShuffled { zone: shuffled, .. } => {
                assert_ne!(shuffled, &zone("mana_deck"));
            }
            other => panic!("not a shuffle: {other:?}"),
        }
    }
}

#[test]
fn game_creation_draws_each_opening_hand_from_the_top_of_the_deck() {
    let creation = create(42);
    let state = &creation.opening.state;
    for player in [seat(0), seat(1)] {
        let order = shuffled_order(&creation, player, "deck");
        assert_eq!(order.len(), 50);

        // The top is the end of the list, and each draw appends to the hand.
        let top_five_first: Vec<CardInstanceId> = order.iter().rev().take(5).copied().collect();
        assert_eq!(cards_in(state, player, "hand"), top_five_first);
        assert_eq!(cards_in(state, player, "deck"), order[..45].to_vec());

        assert_eq!(cards_in(state, player, "deck").len(), 45);
        assert_eq!(cards_in(state, player, "hand").len(), 5);
        assert_eq!(cards_in(state, player, "mana_deck").len(), 10);
    }
}

// ---- instance state

#[test]
fn leader_current_life_equals_its_life() {
    let ruleset = ruleset();
    let decks = decks(&ruleset);
    let state = ruleset
        .create_game(&decks, Seed(1))
        .expect("the sample decks create a game")
        .opening
        .state;
    let current_life = StateName::new("current_life");
    for (player, leader, life) in [(0, "captain_ardel", 5), (1, "warden_ilse", 6)] {
        let id = cards_in(&state, seat(player), "leader_zone")[0];
        let instance = state.card(id).expect("the Leader instance exists");
        // The catalog's `life` is the source of truth, and the sample catalog pins the numbers.
        let defined = ruleset
            .catalog()
            .card(&CardId::new(leader))
            .and_then(|card| card.field(&FieldName::new("life")))
            .cloned();
        assert_eq!(defined, Some(FieldValue::Int(life)));
        assert_eq!(
            instance.state_value(&current_life),
            Some(&FieldValue::Int(life))
        );
    }
}

#[test]
fn card_instance_state_is_initialized_per_schema() {
    let ruleset = ruleset();
    let state = create(1).opening.state;
    let name = StateName::new;
    for card in state.cards() {
        let card_type = ruleset
            .catalog()
            .card(card.definition())
            .map(|definition| definition.card_type().as_str().to_string())
            .expect("the card is in the catalog");
        let names: Vec<&str> = card
            .state()
            .iter()
            .map(|(state_name, _)| state_name.as_str())
            .collect();
        // Schema order: `current_life` then `tapped` for a Leader, `tapped` then
        // `recently_played` for a creature, only `tapped` for mana.
        match card_type.as_str() {
            "leader" => assert_eq!(names, ["current_life", "tapped"]),
            "creature" => assert_eq!(names, ["tapped", "recently_played"]),
            "mana" => assert_eq!(names, ["tapped"]),
            other => panic!("unexpected card type {other}"),
        }
        // Every boolean entry starts false.
        for (state_name, value) in card.state() {
            if state_name.as_str() != "current_life" {
                assert_eq!(value, &FieldValue::Bool(false), "{}", state_name.as_str());
            }
        }
        assert_eq!(
            card.state_value(&name("tapped")),
            Some(&FieldValue::Bool(false))
        );
        assert_eq!(card.state_value(&name("no_such_state")), None);
    }
}

// ---- determinism

#[test]
fn same_seed_gives_identical_opening_and_event_trace() {
    assert_eq!(create(7), create(7));
}

#[test]
fn different_seeds_give_different_deck_orders() {
    let definitions = |seed: u64| -> Vec<Vec<CardId>> {
        let state = create(seed).opening.state;
        [seat(0), seat(1)]
            .into_iter()
            .map(|player| {
                cards_in(&state, player, "deck")
                    .into_iter()
                    .map(|id| definition_of(&state, id))
                    .collect()
            })
            .collect()
    };
    let first = definitions(1);
    let second = definitions(2);
    assert_eq!(first.len(), 2);
    for (a, b) in first.iter().zip(&second) {
        assert_eq!(a.len(), 45);
        assert_ne!(a, b, "the deck definition sequence differs between seeds");
    }
}

// ---- Event-sourced application

#[test]
fn setup_event_trace_applied_to_the_empty_state_gives_the_opening() {
    let creation = create(9);

    // The empty state has the Seed's Random State and nothing else in play.
    assert_eq!(
        creation.empty.random_state(),
        &RandomState::from_seed(Seed(9))
    );
    assert!(creation.empty.cards().is_empty());
    for player in [seat(0), seat(1)] {
        let zones = creation.empty.zones(player).expect("the Player has zones");
        assert_eq!(zones.len(), 7);
        assert!(zones.iter().all(|zone_cards| zone_cards.cards().is_empty()));
    }

    // Folding the setup trace over it gives the opening state, exactly.
    assert_eq!(
        creation.empty.apply_trace(creation.opening.trace.events()),
        Ok(creation.opening.state.clone())
    );
    assert_ne!(creation.empty, creation.opening.state);
}

#[test]
fn setup_event_trace_creates_then_shuffles_then_draws() {
    let creation = create(9);

    let mut expected: Vec<String> = Vec::new();
    for player in 0..2 {
        expected.push(format!("created p{player} leader_zone"));
        expected.extend((0..50).map(|_| format!("created p{player} deck")));
        expected.extend((0..10).map(|_| format!("created p{player} mana_deck")));
    }
    expected.push("shuffled p0 deck".to_string());
    expected.push("shuffled p1 deck".to_string());
    for player in 0..2 {
        expected.extend((0..5).map(|_| format!("moved p{player} deck hand")));
    }

    let found: Vec<String> = creation
        .opening
        .trace
        .events()
        .iter()
        .map(describe)
        .collect();
    assert_eq!(found.len(), 61 + 61 + 2 + 10);
    assert_eq!(found, expected);
}
