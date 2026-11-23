//! Acceptance tests for ticket #96, step 5: the Duel Ruleset v0 (`rulesets/duel-ruleset/`), loaded
//! from its real files with its real catalog and sample decks.
//!
//! The tests that need a monster of level 5 or an ATK of 4000 load the real ruleset with the real
//! catalog plus the test-only monsters of `fixtures/duel-test-cards.yaml`, and a test deck
//! (`fixtures/duel-test-deck.yaml`). A test finds the Seed whose opening hand holds the card it
//! needs, and asserts that it does.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState, Loss,
    Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, FieldName, FieldValue, ParamName, PlayerStateId, StateName, WinConditionId, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const RULESET: &str = include_str!("../../../rulesets/duel-ruleset/duel-ruleset.yaml");
const CATALOG: &str = include_str!("../../../rulesets/duel-ruleset/duel-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-b.yaml");

const TEST_CARDS: &str = include_str!("fixtures/duel-test-cards.yaml");
const TEST_DECK: &str = include_str!("fixtures/duel-test-deck.yaml");

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn declared(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn summon(player: u32, card: CardInstanceId) -> Action {
    declared(player, "normal_summon", &[card])
}

fn attack(player: u32, card: CardInstanceId) -> Action {
    declared(player, "direct_attack", &[card])
}

fn end_turn(player: u32) -> Action {
    declared(player, "end_turn", &[])
}

fn load(catalog: &str, decks: [&str; 2]) -> (DeclarativeRuleset, Vec<DeckList>) {
    let definition = load_ruleset_definition(RULESET).expect("the Duel Ruleset loads");
    let catalog = load_card_catalog(&definition, catalog).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks = decks
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect();
    (ruleset, decks)
}

/// The real Duel Ruleset with its two sample decks.
fn real() -> (DeclarativeRuleset, Vec<DeckList>) {
    load(CATALOG, [DECK_A, DECK_B])
}

/// The real Duel Ruleset with the test-only monsters and the test deck for both Players.
fn with_test_cards() -> (DeclarativeRuleset, Vec<DeckList>) {
    let catalog = format!("{CATALOG}\n{TEST_CARDS}");
    load(&catalog, [TEST_DECK, TEST_DECK])
}

fn create(ruleset: &DeclarativeRuleset, decks: &[DeckList], seed: u64) -> GameCreation {
    ruleset
        .create_game(decks, Seed(seed))
        .expect("the decks create a game")
}

/// The first Game, over Seeds 0.., whose Opening Position satisfies `wanted`.
fn find_game(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    wanted: impl Fn(&GameState) -> bool,
) -> GameCreation {
    (0..2_000)
        .map(|seed| create(ruleset, decks, seed))
        .find(|creation| wanted(&creation.opening.state))
        .expect("some Seed gives the opening hand the test needs")
}

fn take(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
) -> Transition<GameState, Event> {
    ruleset
        .transition(state, action)
        .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"))
}

/// Takes each Action in turn and returns the last state.
fn play(ruleset: &DeclarativeRuleset, state: &GameState, actions: &[Action]) -> GameState {
    let mut state = state.clone();
    for action in actions {
        state = take(ruleset, &state, action).state;
    }
    state
}

fn hand(state: &GameState, seat: u32) -> Vec<CardInstanceId> {
    state
        .zone(p(seat), &zone("hand"))
        .expect("the hand exists")
        .to_vec()
}

fn definition_id(state: &GameState, card: CardInstanceId) -> String {
    state
        .card(card)
        .expect("the card exists")
        .definition()
        .as_str()
        .to_string()
}

/// An int field of the Card Definition of `card`.
fn stat(ruleset: &DeclarativeRuleset, state: &GameState, card: CardInstanceId, field: &str) -> i64 {
    let instance = state.card(card).expect("the card exists");
    let definition = ruleset
        .catalog()
        .card(instance.definition())
        .expect("the definition is in the catalog");
    match definition.field(&FieldName::new(field)) {
        Some(FieldValue::Int(value)) => *value,
        other => panic!("`{field}` is not an int: {other:?}"),
    }
}

fn life_points(state: &GameState, seat: u32) -> i64 {
    match state.player_state_value(p(seat), &PlayerStateId::new("life_points")) {
        Some(FieldValue::Int(value)) => *value,
        other => panic!("life_points is {other:?}"),
    }
}

fn attacked(card: CardInstanceId, value: bool) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new("attacked"),
        value: FieldValue::Bool(value),
    }
}

fn loss(seat: u32, reason: &str) -> Loss {
    Loss {
        player: p(seat),
        reason: WinConditionId::new(reason),
    }
}

/// Whether the Legal Actions of the state hold `action`.
fn is_legal(ruleset: &DeclarativeRuleset, state: &GameState, action: &Action) -> bool {
    ruleset.legal_actions(state).contains(action)
}

/// The Rejection of the card for `param`, by the `where` of the action.
fn where_not_met(param: &str, card: CardInstanceId) -> Rejection {
    Rejection::WhereConditionNotMet {
        param: ParamName::new(param),
        card,
    }
}

// ---- the files

#[test]
fn the_duel_ruleset_and_its_sample_decks_load() {
    let (ruleset, decks) = real();
    assert_eq!(ruleset.definition().meta().id().as_str(), "duel");
    assert_eq!(decks.len(), 2);
    for (seat, deck) in decks.iter().enumerate() {
        let entries = deck.inputs()[0].entries();
        let total: u32 = entries.iter().map(|entry| entry.count().get()).sum();
        assert_eq!(total, 40, "deck {seat} has 40 cards");
        assert!(
            entries.iter().all(|entry| entry.count().get() <= 3),
            "deck {seat} has at most 3 copies of a card"
        );
    }
    // The test-only cards load with the real ruleset too.
    let (_, test_decks) = with_test_cards();
    assert_eq!(test_decks.len(), 2);

    let creation = create(&ruleset, &decks, 1);
    assert!(creation.opening.state.outcome().is_none());
    for seat in 0..2 {
        assert_eq!(hand(&creation.opening.state, seat).len(), 5);
        assert_eq!(life_points(&creation.opening.state, seat), 4000);
    }
}

// ---- the rules of the Main phase

#[test]
fn a_direct_attack_costs_the_opponent_the_attackers_atk() {
    let (ruleset, decks) = real();
    let creation = create(&ruleset, &decks, 1);
    let monster = hand(&creation.opening.state, 0)[0];
    let atk = stat(&ruleset, &creation.opening.state, monster, "atk");

    let summoned = take(&ruleset, &creation.opening.state, &summon(0, monster)).state;
    let input = summoned.clone();
    let before = input.clone();
    let hit = take(&ruleset, &input, &attack(0, monster));
    assert_eq!(
        hit.trace.events().to_vec(),
        vec![
            attacked(monster, true),
            Event::PlayerStateChanged {
                player: p(1),
                state: PlayerStateId::new("life_points"),
                value: FieldValue::Int(4000 - atk),
            },
        ]
    );
    assert_eq!(life_points(&hit.state, 1), 4000 - atk);
    assert_eq!(life_points(&hit.state, 0), 4000);
    assert_eq!(input, before);
}

#[test]
fn a_second_normal_summon_in_a_turn_is_rejected() {
    let (ruleset, decks) = real();
    let creation = create(&ruleset, &decks, 1);
    let cards = hand(&creation.opening.state, 0);
    let (first, second) = (cards[0], cards[1]);
    assert!(is_legal(
        &ruleset,
        &creation.opening.state,
        &summon(0, second)
    ));

    let input = take(&ruleset, &creation.opening.state, &summon(0, first)).state;
    let before = input.clone();
    assert_eq!(
        ruleset.transition(&input, &summon(0, second)),
        Err(where_not_met("card", second))
    );
    assert_eq!(input, before);
    assert!(
        !ruleset.legal_actions(&input).iter().any(|action| matches!(
            action,
            Action::Declared { action, .. } if action.as_str() == "normal_summon"
        )),
        "no card can be summoned again this Turn"
    );
}

#[test]
fn a_normal_summon_of_a_level_5_or_higher_monster_is_rejected() {
    let (ruleset, decks) = with_test_cards();
    let creation = find_game(&ruleset, &decks, |state| {
        hand(state, 0)
            .iter()
            .any(|&card| definition_id(state, card) == "test_colossus")
    });
    let input = creation.opening.state;
    let colossus = hand(&input, 0)
        .into_iter()
        .find(|&card| definition_id(&input, card) == "test_colossus")
        .expect("the hand holds a level 5 monster");
    assert_eq!(stat(&ruleset, &input, colossus, "level"), 5);
    let before = input.clone();

    assert_eq!(
        ruleset.transition(&input, &summon(0, colossus)),
        Err(where_not_met("card", colossus))
    );
    assert_eq!(input, before);
    assert!(!is_legal(&ruleset, &input, &summon(0, colossus)));
    // It is the level and not the Turn: another card of the hand can still be summoned.
    let other = hand(&input, 0)
        .into_iter()
        .find(|&card| stat(&ruleset, &input, card, "level") <= 4)
        .expect("the hand holds a monster of level 4 or less");
    assert!(is_legal(&ruleset, &input, &summon(0, other)));
}

#[test]
fn a_second_attack_by_the_same_monster_in_a_turn_is_rejected() {
    let (ruleset, decks) = real();
    let creation = create(&ruleset, &decks, 1);
    let opening = creation.opening.state;
    let first = hand(&opening, 0)[0];

    // Turn 1: seat 0 summons its first monster. Turn 2: seat 1 passes.
    let turn_3 = play(
        &ruleset,
        &opening,
        &[summon(0, first), end_turn(0), end_turn(1)],
    );
    assert_eq!(turn_3.turn_number(), TurnNumber::new(3));
    let second = hand(&turn_3, 0)
        .into_iter()
        .find(|&card| card != first)
        .expect("another monster in the hand");

    // Turn 3: a second monster, then the first one attacks.
    let both = play(&ruleset, &turn_3, &[summon(0, second), attack(0, first)]);
    let before = both.clone();
    assert_eq!(
        ruleset.transition(&both, &attack(0, first)),
        Err(where_not_met("attacker", first))
    );
    assert_eq!(both, before);
    assert!(!is_legal(&ruleset, &both, &attack(0, first)));

    // The monster that has not attacked still can.
    assert!(is_legal(&ruleset, &both, &attack(0, second)));
    let hit = take(&ruleset, &both, &attack(0, second));
    assert_eq!(hit.trace.events().len(), 2);
}

#[test]
fn the_summon_flag_and_the_attacks_reset_at_the_end_of_the_turn() {
    let (ruleset, decks) = real();
    let creation = create(&ruleset, &decks, 1);
    let opening = creation.opening.state;
    let first = hand(&opening, 0)[0];

    // Turn 1: summon and attack. Both are used up for the rest of the Turn.
    let used = play(&ruleset, &opening, &[summon(0, first), attack(0, first)]);
    let other = hand(&used, 0)[0];
    assert!(!is_legal(&ruleset, &used, &summon(0, other)));
    assert!(!is_legal(&ruleset, &used, &attack(0, first)));

    // Turn 3, the same Player's next Turn: the End phase of Turn 1 reset both.
    let turn_3 = play(&ruleset, &used, &[end_turn(0), end_turn(1)]);
    let card = hand(&turn_3, 0)[0];
    assert!(is_legal(&ruleset, &turn_3, &summon(0, card)));
    assert!(is_legal(&ruleset, &turn_3, &attack(0, first)));
    let hit = take(&ruleset, &turn_3, &attack(0, first));
    assert_eq!(hit.trace.events().len(), 2);
    assert!(is_legal(&ruleset, &turn_3, &summon(0, card)));
    assert!(ruleset.transition(&turn_3, &summon(0, card)).is_ok());
}

#[test]
fn the_starting_player_skips_the_draw_of_turn_one() {
    let (ruleset, decks) = real();
    let creation = create(&ruleset, &decks, 1);
    let turn_1 = creation.opening.state;
    assert_eq!(turn_1.turn_number(), TurnNumber::new(1));
    assert_eq!(hand(&turn_1, 0).len(), 5);
    assert_eq!(hand(&turn_1, 1).len(), 5);

    let turn_2 = take(&ruleset, &turn_1, &end_turn(0)).state;
    assert_eq!(turn_2.turn_number(), TurnNumber::new(2));
    assert_eq!(hand(&turn_2, 1).len(), 6);
    assert_eq!(hand(&turn_2, 0).len(), 5);

    let turn_3 = take(&ruleset, &turn_2, &end_turn(1)).state;
    assert_eq!(hand(&turn_3, 0).len(), 6);
}

// ---- the end of the Game

#[test]
fn reaching_zero_life_points_ends_the_game() {
    let (ruleset, decks) = with_test_cards();
    let creation = find_game(&ruleset, &decks, |state| {
        hand(state, 0)
            .iter()
            .any(|&card| definition_id(state, card) == "test_sniper")
    });
    let opening = creation.opening.state;
    let sniper = hand(&opening, 0)
        .into_iter()
        .find(|&card| definition_id(&opening, card) == "test_sniper")
        .expect("the hand holds the sniper");
    assert_eq!(stat(&ruleset, &opening, sniper, "atk"), 4000);

    let summoned = take(&ruleset, &opening, &summon(0, sniper)).state;
    let hit = take(&ruleset, &summoned, &attack(0, sniper));
    assert_eq!(
        hit.trace.events().to_vec(),
        vec![
            attacked(sniper, true),
            Event::PlayerStateChanged {
                player: p(1),
                state: PlayerStateId::new("life_points"),
                value: FieldValue::Int(0),
            },
            Event::GameEnded {
                losses: vec![loss(1, "life_out")],
            },
        ]
    );
    let outcome = hit.state.outcome().expect("the Game has ended");
    assert_eq!(outcome.winners(), [p(0)].as_slice());
    assert!(ruleset.legal_actions(&hit.state).is_empty());
}

#[test]
fn drawing_from_an_empty_deck_ends_the_game() {
    // Each Player draws 5 cards in setup, so 35 are left. Seat 0 skips the draw of Turn 1 and then
    // draws on Turns 3, 5, ..., 71 (35 draws); seat 1 draws on Turns 2, 4, ..., 70 (35 draws). On
    // Turn 72 seat 1 must draw from an empty deck: seat 1 loses for `deck_out`, when seat 0 ends
    // Turn 71.
    let (ruleset, decks) = real();
    let creation = create(&ruleset, &decks, 1);
    let mut state = creation.opening.state;
    for turn in 1..=70 {
        assert!(state.outcome().is_none(), "the Game is on at Turn {turn}");
        let player = state.active_player();
        assert_eq!(player, p(if turn % 2 == 1 { 0 } else { 1 }));
        state = take(&ruleset, &state, &end_turn(player.0)).state;
    }
    assert_eq!(state.turn_number(), TurnNumber::new(71));
    assert!(state.outcome().is_none());
    assert_eq!(state.zone(p(1), &zone("deck")), Some([].as_slice()));
    // Seat 0 has just drawn its 35th card, at the start of Turn 71.
    assert_eq!(state.zone(p(0), &zone("deck")), Some([].as_slice()));

    let last = take(&ruleset, &state, &end_turn(0));
    let events = last.trace.events();
    assert_eq!(
        events.last(),
        Some(&Event::GameEnded {
            losses: vec![loss(1, "deck_out")],
        })
    );
    // The failed draw is a fact of the transition, and the Turn goes on to Main before the Game
    // ends.
    assert!(events.contains(&Event::DrawFromEmptyZone {
        player: p(1),
        zone: zone("deck"),
    }));
    assert_eq!(last.state.turn_number(), TurnNumber::new(72));
    let outcome = last.state.outcome().expect("the Game has ended");
    assert_eq!(outcome.winners(), [p(0)].as_slice());
}
