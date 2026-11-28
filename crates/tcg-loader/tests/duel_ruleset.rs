//! Acceptance tests for ticket #96, step 5, and for ticket #97 (ADR-0042): the Duel Ruleset
//! (`rulesets/duel-ruleset/`), loaded from its real files with its real catalog and sample decks.
//! Since [40] a Turn has a Main and a Battle phase: monsters are summoned in Main, `to_battle`
//! moves to Battle, and attacks are taken there, never on Turn 1 and, as a direct attack, only
//! while the opponent has no monster.
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
    ActionId, FieldName, FieldValue, ParamName, PhaseId, PlayerStateId, StateName, WinConditionId,
    ZoneId,
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

fn to_battle(player: u32) -> Action {
    declared(player, "to_battle", &[])
}

fn phase_began(id: &str) -> Event {
    Event::PhaseBegan {
        phase: PhaseId::new(id),
    }
}

/// Each Active Player ends their Turn from Main, in turn, until Turn `turn` begins.
fn pass_to_turn(ruleset: &DeclarativeRuleset, state: &GameState, turn: u32) -> GameState {
    let mut state = state.clone();
    while state.turn_number().map(TurnNumber::get) < Some(turn) {
        let player = state.active_player();
        state = take(ruleset, &state, &end_turn(player.0)).state;
    }
    state
}

/// The cards of the hand of `seat` that can be normal summoned: monsters of level 4 or less (the
/// hand may hold spells and monsters that need tributes since ticket #99).
fn summonables(ruleset: &DeclarativeRuleset, state: &GameState, seat: u32) -> Vec<CardInstanceId> {
    hand(state, seat)
        .into_iter()
        .filter(|&card| {
            let definition = state.card(card).expect("the card exists").definition();
            ruleset
                .catalog()
                .card(definition)
                .is_some_and(|definition| definition.card_type().as_str() == "monster")
        })
        .filter(|&card| stat(ruleset, state, card, "level") <= 4)
        .collect()
}

/// A card of the hand of `seat` that can be normal summoned (level 4 or less).
fn summonable(ruleset: &DeclarativeRuleset, state: &GameState, seat: u32) -> CardInstanceId {
    summonables(ruleset, state, seat)
        .into_iter()
        .next()
        .expect("the hand holds a monster of level 4 or less")
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
    let monster = summonable(&ruleset, &creation.opening.state, 0);
    let atk = stat(&ruleset, &creation.opening.state, monster, "atk");

    // Summoned on Turn 1; on Turn 3 the monster attacks from Battle, and the opponent has none.
    let summoned = take(&ruleset, &creation.opening.state, &summon(0, monster)).state;
    let turn_3 = pass_to_turn(&ruleset, &summoned, 3);
    let input = take(&ruleset, &turn_3, &to_battle(0)).state;
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
    let cards = summonables(&ruleset, &creation.opening.state, 0);
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
    let first = summonable(&ruleset, &opening, 0);

    // Turn 1: seat 0 summons its first monster. Turn 2: seat 1 passes.
    let turn_3 = play(
        &ruleset,
        &opening,
        &[summon(0, first), end_turn(0), end_turn(1)],
    );
    assert_eq!(turn_3.turn_number(), TurnNumber::new(3));
    let second = summonables(&ruleset, &turn_3, 0)
        .into_iter()
        .find(|&card| card != first)
        .expect("another monster in the hand");

    // Turn 3: a second monster, then the first one attacks from Battle.
    let both = play(
        &ruleset,
        &turn_3,
        &[summon(0, second), to_battle(0), attack(0, first)],
    );
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
    let first = summonable(&ruleset, &opening, 0);

    // Turn 1: the summon is used up for the rest of the Turn.
    let summoned = take(&ruleset, &opening, &summon(0, first)).state;
    let other = summonable(&ruleset, &summoned, 0);
    assert!(!is_legal(&ruleset, &summoned, &summon(0, other)));

    // Turn 3, the same Player's next Turn: the End phase of Turn 1 reset the summon flag. The
    // monster attacks in Battle, and has used its attack for the rest of the Turn.
    let turn_3 = pass_to_turn(&ruleset, &summoned, 3);
    let card = summonable(&ruleset, &turn_3, 0);
    assert!(is_legal(&ruleset, &turn_3, &summon(0, card)));
    let attacked_once = play(&ruleset, &turn_3, &[to_battle(0), attack(0, first)]);
    assert!(!is_legal(&ruleset, &attacked_once, &attack(0, first)));

    // Turn 5: the End phase of Turn 3 reset the attack.
    let turn_5_main = pass_to_turn(&ruleset, &attacked_once, 5);
    // Turn 3 was ended from Battle, and every Turn begins in Main.
    assert_eq!(turn_5_main.phase(), Some(&PhaseId::new("main")));
    let turn_5 = take(&ruleset, &turn_5_main, &to_battle(0)).state;
    assert!(is_legal(&ruleset, &turn_5, &attack(0, first)));
    let hit = take(&ruleset, &turn_5, &attack(0, first));
    assert_eq!(hit.trace.events().len(), 2);
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

    // Summoned on Turn 1, it attacks from Battle on Turn 3 (no attack on Turn 1, and Turn 2 is
    // the opponent's).
    let summoned = take(&ruleset, &opening, &summon(0, sniper)).state;
    let turn_3 = pass_to_turn(&ruleset, &summoned, 3);
    let in_battle = take(&ruleset, &turn_3, &to_battle(0)).state;
    let hit = take(&ruleset, &in_battle, &attack(0, sniper));
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

// ---- Main and Battle (ticket #97, ADR-0042)

/// A Game in which seat 0 has three monsters in its Monster Zone at the start of Turn 7: one
/// summoned on each of Turns 1, 3 and 5, with seat 1 passing.
fn three_monsters() -> (DeclarativeRuleset, GameState) {
    let (ruleset, decks) = real();
    let mut state = create(&ruleset, &decks, 1).opening.state;
    for turn in [1, 3, 5] {
        state = pass_to_turn(&ruleset, &state, turn);
        let card = summonable(&ruleset, &state, 0);
        state = take(&ruleset, &state, &summon(0, card)).state;
    }
    state = pass_to_turn(&ruleset, &state, 7);
    assert_eq!(
        state.zone(p(0), &zone("monster_zone")).map(<[_]>::len),
        Some(3)
    );
    (ruleset, state)
}

#[test]
fn to_battle_moves_the_duel_to_the_battle_phase() {
    let (ruleset, decks) = real();
    let input = create(&ruleset, &decks, 1).opening.state;
    assert_eq!(input.phase(), Some(&PhaseId::new("main")));
    let before = input.clone();

    let moved = take(&ruleset, &input, &to_battle(0));
    // No automatic phase lies between Main and Battle.
    assert_eq!(moved.trace.events().to_vec(), vec![phase_began("battle")]);
    assert_eq!(moved.state.phase(), Some(&PhaseId::new("battle")));
    assert_eq!(moved.state.active_player(), p(0));
    assert_eq!(moved.state.turn_number(), TurnNumber::new(1));
    assert_eq!(input, before);
    // Battle lists the attacks and `end_turn`: with no monster, only `end_turn` is legal.
    assert_eq!(ruleset.legal_actions(&moved.state), vec![end_turn(0)]);
}

#[test]
fn a_summon_into_a_full_monster_zone_is_rejected() {
    let (ruleset, input) = three_monsters();
    let card = summonable(&ruleset, &input, 0);
    // It is the full Monster Zone and nothing else: the summon flag is down and the level is fine.
    assert_eq!(
        input.player_state_value(p(0), &PlayerStateId::new("normal_summoned")),
        Some(&FieldValue::Bool(false))
    );
    assert!(stat(&ruleset, &input, card, "level") <= 4);
    let before = input.clone();

    assert_eq!(
        ruleset.transition(&input, &summon(0, card)),
        Err(where_not_met("card", card))
    );
    assert_eq!(input, before);
    assert!(!is_legal(&ruleset, &input, &summon(0, card)));
}

#[test]
fn an_attack_in_the_main_phase_is_rejected() {
    let (ruleset, decks) = real();
    let creation = create(&ruleset, &decks, 1);
    let monster = summonable(&ruleset, &creation.opening.state, 0);
    let summoned = take(&ruleset, &creation.opening.state, &summon(0, monster)).state;
    // Turn 3: the monster could attack, and nothing but the phase forbids it.
    let input = pass_to_turn(&ruleset, &summoned, 3);
    assert_eq!(input.phase(), Some(&PhaseId::new("main")));
    let before = input.clone();

    assert_eq!(
        ruleset.transition(&input, &attack(0, monster)),
        Err(Rejection::ActionNotInCurrentPhase {
            action: ActionId::new("direct_attack")
        })
    );
    assert_eq!(input, before);
    assert!(!is_legal(&ruleset, &input, &attack(0, monster)));
    assert!(is_legal(&ruleset, &input, &to_battle(0)));
    // In Battle the same attack is legal.
    let in_battle = take(&ruleset, &input, &to_battle(0)).state;
    assert!(is_legal(&ruleset, &in_battle, &attack(0, monster)));
}

#[test]
fn a_summon_in_the_battle_phase_is_rejected() {
    let (ruleset, decks) = real();
    let input = create(&ruleset, &decks, 1).opening.state;
    let card = summonable(&ruleset, &input, 0);
    assert!(is_legal(&ruleset, &input, &summon(0, card)));
    let in_battle = take(&ruleset, &input, &to_battle(0)).state;
    let before = in_battle.clone();

    assert_eq!(
        ruleset.transition(&in_battle, &summon(0, card)),
        Err(Rejection::ActionNotInCurrentPhase {
            action: ActionId::new("normal_summon")
        })
    );
    assert_eq!(in_battle, before);
    assert!(!is_legal(&ruleset, &in_battle, &summon(0, card)));
    // There is no way back to Main, and no way forward from the last action phase.
    assert_eq!(
        ruleset.transition(&in_battle, &to_battle(0)),
        Err(Rejection::ActionNotInCurrentPhase {
            action: ActionId::new("to_battle")
        })
    );
}

#[test]
fn a_direct_attack_while_the_opponent_has_a_monster_is_rejected() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let attacker = summonable(&ruleset, &opening, 0);
    // Turn 1: seat 0 summons. Turn 2: seat 1 summons. Turn 3: seat 0 is in Battle, with a monster
    // that has not attacked and the Game past Turn 1; the opponent's Monster Zone is not empty.
    let turn_2 = play(&ruleset, &opening, &[summon(0, attacker), end_turn(0)]);
    let defender = summonable(&ruleset, &turn_2, 1);
    let turn_3 = play(&ruleset, &turn_2, &[summon(1, defender), end_turn(1)]);
    let input = take(&ruleset, &turn_3, &to_battle(0)).state;
    assert_eq!(input.turn_number(), TurnNumber::new(3));
    let before = input.clone();

    assert_eq!(
        ruleset.transition(&input, &attack(0, attacker)),
        Err(where_not_met("attacker", attacker))
    );
    assert_eq!(input, before);
    assert!(!is_legal(&ruleset, &input, &attack(0, attacker)));
}

#[test]
fn an_attack_on_turn_one_is_rejected() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let monster = summonable(&ruleset, &opening, 0);
    // Turn 1, in Battle, the opponent's Monster Zone is empty and the monster has not attacked:
    // only the Turn forbids the attack.
    let input = play(&ruleset, &opening, &[summon(0, monster), to_battle(0)]);
    assert_eq!(input.turn_number(), TurnNumber::new(1));
    let before = input.clone();

    assert_eq!(
        ruleset.transition(&input, &attack(0, monster)),
        Err(where_not_met("attacker", monster))
    );
    assert_eq!(input, before);
    assert!(!is_legal(&ruleset, &input, &attack(0, monster)));

    // The same attack on Turn 2, by the other Player, is legal.
    let turn_2 = take(&ruleset, &opening, &end_turn(0)).state;
    let other = summonable(&ruleset, &turn_2, 1);
    let battle_2 = play(&ruleset, &turn_2, &[summon(1, other), to_battle(1)]);
    assert!(is_legal(&ruleset, &battle_2, &attack(1, other)));
}

// ---- positions and combat (ticket #98, ADR-0043)
//
// A monster is summoned in attack position or set in defence position, face-up this phase. It
// changes position once per Turn in Main, and not on the Turn it arrived. In Battle an
// attack-position monster attacks a monster of the opponent: the lower ATK is destroyed and its
// controller loses the difference; against a defence-position monster ATK above DEF destroys it,
// ATK below DEF costs the attacker's controller the difference.
//
// The six combat cases use the real catalog: seat 0 summons the attacker on Turn 1, seat 1 plays the
// target on Turn 2 (summoned, or set), and seat 0 goes to Battle on Turn 3. A test finds a Seed
// whose opening hands hold the two monsters, and asserts that they do.

fn set_monster(player: u32, card: CardInstanceId) -> Action {
    declared(player, "set_monster", &[card])
}

fn change_position(player: u32, card: CardInstanceId) -> Action {
    declared(player, "change_position", &[card])
}

fn fight(player: u32, attacker: CardInstanceId, target: CardInstanceId) -> Action {
    declared(player, "attack", &[attacker, target])
}

/// The text instance state `position` of `card`.
fn position(state: &GameState, card: CardInstanceId) -> String {
    match state
        .card(card)
        .expect("the card exists")
        .state_value(&StateName::new("position"))
    {
        Some(FieldValue::Text(value)) => value.clone(),
        other => panic!("position is {other:?}"),
    }
}

/// The bool instance state `position_fixed` of `card`.
fn position_fixed(state: &GameState, card: CardInstanceId) -> bool {
    match state
        .card(card)
        .expect("the card exists")
        .state_value(&StateName::new("position_fixed"))
    {
        Some(FieldValue::Bool(value)) => *value,
        other => panic!("position_fixed is {other:?}"),
    }
}

fn card_state(card: CardInstanceId, state: &str, value: FieldValue) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new(state),
        value,
    }
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

fn moved(card: CardInstanceId, seat: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(seat),
        from: zone(from),
        to: zone(to),
    }
}

fn life_changed(seat: u32, value: i64) -> Event {
    Event::PlayerStateChanged {
        player: p(seat),
        state: PlayerStateId::new("life_points"),
        value: FieldValue::Int(value),
    }
}

fn normal_summoned(seat: u32, value: bool) -> Event {
    Event::PlayerStateChanged {
        player: p(seat),
        state: PlayerStateId::new("normal_summoned"),
        value: FieldValue::Bool(value),
    }
}

fn monster_zone(state: &GameState, seat: u32) -> Vec<CardInstanceId> {
    state
        .zone(p(seat), &zone("monster_zone"))
        .expect("the Monster Zone exists")
        .to_vec()
}

fn graveyard(state: &GameState, seat: u32) -> Vec<CardInstanceId> {
    state
        .zone(p(seat), &zone("graveyard"))
        .expect("the Graveyard exists")
        .to_vec()
}

/// A card of the hand of `seat` made from the Card Definition `definition`.
fn in_hand(state: &GameState, seat: u32, definition: &str) -> Option<CardInstanceId> {
    hand(state, seat)
        .into_iter()
        .find(|&card| definition_id(state, card) == definition)
}

/// The Opening Position of a Game where seat 0 holds `mine` and seat 1 holds `theirs`, and the two
/// cards.
fn matchup(
    mine: &str,
    theirs: &str,
) -> (
    DeclarativeRuleset,
    GameState,
    CardInstanceId,
    CardInstanceId,
) {
    let (ruleset, decks) = real();
    let creation = find_game(&ruleset, &decks, |state| {
        in_hand(state, 0, mine).is_some() && in_hand(state, 1, theirs).is_some()
    });
    let opening = creation.opening.state;
    let attacker = in_hand(&opening, 0, mine).expect("seat 0 holds the attacker");
    let target = in_hand(&opening, 1, theirs).expect("seat 1 holds the target");
    (ruleset, opening, attacker, target)
}

/// Turn 1: seat 0 summons `attacker`. Turn 2: seat 1 plays `second`. Turn 3: seat 0 goes to Battle.
fn at_battle(
    ruleset: &DeclarativeRuleset,
    opening: &GameState,
    attacker: CardInstanceId,
    second: &Action,
) -> GameState {
    let in_battle = play(
        ruleset,
        opening,
        &[
            summon(0, attacker),
            end_turn(0),
            second.clone(),
            end_turn(1),
            to_battle(0),
        ],
    );
    assert_eq!(in_battle.turn_number(), TurnNumber::new(3));
    assert_eq!(in_battle.phase(), Some(&PhaseId::new("battle")));
    in_battle
}

/// Whether some Legal Action is the declared action `id`.
fn offers(ruleset: &DeclarativeRuleset, state: &GameState, id: &str) -> bool {
    ruleset
        .legal_actions(state)
        .iter()
        .any(|action| matches!(action, Action::Declared { action, .. } if action.as_str() == id))
}

// ---- the six combat cases

#[test]
fn a_higher_atk_destroys_the_attack_position_target_and_costs_its_controller_the_difference() {
    // Gale Hawk (ATK 1400) against Pebble Golem (ATK 700), in attack position.
    let (ruleset, opening, attacker, target) = matchup("gale_hawk", "pebble_golem");
    let input = at_battle(&ruleset, &opening, attacker, &summon(1, target));
    assert_eq!(position(&input, target), "attack");
    assert!(is_legal(&ruleset, &input, &fight(0, attacker, target)));
    let before = input.clone();

    let hit = take(&ruleset, &input, &fight(0, attacker, target));
    assert_eq!(
        hit.trace.events().to_vec(),
        vec![
            attacked(attacker, true),
            life_changed(1, 4000 - (1400 - 700)),
            moved(target, 1, "monster_zone", "graveyard"),
        ]
    );
    assert_eq!(graveyard(&hit.state, 1), vec![target]);
    assert_eq!(monster_zone(&hit.state, 1), Vec::<CardInstanceId>::new());
    assert_eq!(monster_zone(&hit.state, 0), vec![attacker]);
    assert_eq!(graveyard(&hit.state, 0), Vec::<CardInstanceId>::new());
    assert_eq!(life_points(&hit.state, 0), 4000);
    assert_eq!(life_points(&hit.state, 1), 3300);
    assert_eq!(input, before);
}

#[test]
fn a_lower_atk_destroys_the_attacker_and_costs_its_controller_the_difference() {
    // Moss Toad (ATK 300) against Ember Fox (ATK 1200), in attack position.
    let (ruleset, opening, attacker, target) = matchup("moss_toad", "ember_fox");
    let input = at_battle(&ruleset, &opening, attacker, &summon(1, target));
    let before = input.clone();

    let hit = take(&ruleset, &input, &fight(0, attacker, target));
    assert_eq!(
        hit.trace.events().to_vec(),
        vec![
            attacked(attacker, true),
            life_changed(0, 4000 - (1200 - 300)),
            moved(attacker, 0, "monster_zone", "graveyard"),
        ]
    );
    assert_eq!(graveyard(&hit.state, 0), vec![attacker]);
    assert_eq!(monster_zone(&hit.state, 0), Vec::<CardInstanceId>::new());
    assert_eq!(monster_zone(&hit.state, 1), vec![target]);
    assert_eq!(graveyard(&hit.state, 1), Vec::<CardInstanceId>::new());
    assert_eq!(life_points(&hit.state, 0), 3100);
    assert_eq!(life_points(&hit.state, 1), 4000);
    assert_eq!(input, before);
}

#[test]
fn tied_atk_destroys_both_monsters_without_damage() {
    // Stone Sentry and Ember Fox both have ATK 1200.
    let (ruleset, opening, attacker, target) = matchup("stone_sentry", "ember_fox");
    assert_eq!(stat(&ruleset, &opening, attacker, "atk"), 1200);
    assert_eq!(stat(&ruleset, &opening, target, "atk"), 1200);
    let input = at_battle(&ruleset, &opening, attacker, &summon(1, target));
    let before = input.clone();

    let hit = take(&ruleset, &input, &fight(0, attacker, target));
    assert_eq!(
        hit.trace.events().to_vec(),
        vec![
            attacked(attacker, true),
            moved(target, 1, "monster_zone", "graveyard"),
            moved(attacker, 0, "monster_zone", "graveyard"),
        ]
    );
    assert_eq!(graveyard(&hit.state, 0), vec![attacker]);
    assert_eq!(graveyard(&hit.state, 1), vec![target]);
    assert_eq!(monster_zone(&hit.state, 0), Vec::<CardInstanceId>::new());
    assert_eq!(monster_zone(&hit.state, 1), Vec::<CardInstanceId>::new());
    assert_eq!(life_points(&hit.state, 0), 4000);
    assert_eq!(life_points(&hit.state, 1), 4000);
    assert_eq!(input, before);
}

#[test]
fn atk_above_def_destroys_the_defence_position_target_without_damage() {
    // Iron Beetle (ATK 1300) against Ash Sprite (DEF 200), set.
    let (ruleset, opening, attacker, target) = matchup("iron_beetle", "ash_sprite");
    let input = at_battle(&ruleset, &opening, attacker, &set_monster(1, target));
    assert_eq!(position(&input, target), "defence");
    let before = input.clone();

    let hit = take(&ruleset, &input, &fight(0, attacker, target));
    assert_eq!(
        hit.trace.events().to_vec(),
        vec![
            attacked(attacker, true),
            moved(target, 1, "monster_zone", "graveyard"),
        ]
    );
    assert_eq!(graveyard(&hit.state, 1), vec![target]);
    assert_eq!(monster_zone(&hit.state, 0), vec![attacker]);
    assert_eq!(monster_zone(&hit.state, 1), Vec::<CardInstanceId>::new());
    assert_eq!(graveyard(&hit.state, 0), Vec::<CardInstanceId>::new());
    assert_eq!(life_points(&hit.state, 0), 4000);
    assert_eq!(life_points(&hit.state, 1), 4000);
    assert_eq!(input, before);
}

#[test]
fn atk_below_def_costs_the_attackers_controller_the_difference() {
    // Moss Toad (ATK 300) against Stone Sentry (DEF 1900), set: nothing is destroyed.
    let (ruleset, opening, attacker, target) = matchup("moss_toad", "stone_sentry");
    let input = at_battle(&ruleset, &opening, attacker, &set_monster(1, target));
    let before = input.clone();

    let hit = take(&ruleset, &input, &fight(0, attacker, target));
    assert_eq!(
        hit.trace.events().to_vec(),
        vec![
            attacked(attacker, true),
            life_changed(0, 4000 - (1900 - 300)),
        ]
    );
    assert_eq!(monster_zone(&hit.state, 0), vec![attacker]);
    assert_eq!(monster_zone(&hit.state, 1), vec![target]);
    assert_eq!(graveyard(&hit.state, 0), Vec::<CardInstanceId>::new());
    assert_eq!(graveyard(&hit.state, 1), Vec::<CardInstanceId>::new());
    assert_eq!(life_points(&hit.state, 0), 2400);
    assert_eq!(life_points(&hit.state, 1), 4000);
    assert_eq!(input, before);
}

#[test]
fn atk_equal_to_def_changes_nothing() {
    // Tide Scribe (ATK 1000) against Iron Beetle (DEF 1000), set: the attack is spent, and that is
    // all.
    let (ruleset, opening, attacker, target) = matchup("tide_scribe", "iron_beetle");
    assert_eq!(stat(&ruleset, &opening, attacker, "atk"), 1000);
    assert_eq!(stat(&ruleset, &opening, target, "def"), 1000);
    let input = at_battle(&ruleset, &opening, attacker, &set_monster(1, target));
    let before = input.clone();

    let hit = take(&ruleset, &input, &fight(0, attacker, target));
    assert_eq!(hit.trace.events().to_vec(), vec![attacked(attacker, true)]);
    assert_eq!(monster_zone(&hit.state, 0), vec![attacker]);
    assert_eq!(monster_zone(&hit.state, 1), vec![target]);
    assert_eq!(graveyard(&hit.state, 0), Vec::<CardInstanceId>::new());
    assert_eq!(graveyard(&hit.state, 1), Vec::<CardInstanceId>::new());
    assert_eq!(life_points(&hit.state, 0), 4000);
    assert_eq!(life_points(&hit.state, 1), 4000);
    assert_eq!(input, before);
}

#[test]
fn an_attack_must_target_a_monster_in_the_opponents_monster_zone() {
    let (ruleset, opening, attacker, target) = matchup("gale_hawk", "pebble_golem");
    let input = at_battle(&ruleset, &opening, attacker, &summon(1, target));
    let before = input.clone();

    // The attacker itself is not a target: since ticket 99 the card params of an Action are
    // distinct (ADR-0044), so naming it twice is `CardBoundTwice`, checked before the owner. A
    // card of the summoner's own hand is not the opponent's card, and a card of the opponent's hand
    // is not in its Monster Zone.
    assert_eq!(
        ruleset.transition(&input, &fight(0, attacker, attacker)),
        Err(Rejection::CardBoundTwice {
            param: ParamName::new("target"),
            card: attacker,
            first: ParamName::new("attacker"),
        })
    );
    let own_in_hand = hand(&input, 0)[0];
    assert_eq!(
        ruleset.transition(&input, &fight(0, attacker, own_in_hand)),
        Err(Rejection::NotOpponentCard {
            param: ParamName::new("target"),
            card: own_in_hand,
        })
    );
    let in_their_hand = hand(&input, 1)[0];
    assert_eq!(
        ruleset.transition(&input, &fight(0, attacker, in_their_hand)),
        Err(Rejection::CardNotInZone {
            param: ParamName::new("target"),
            card: in_their_hand,
            zone: zone("monster_zone"),
        })
    );
    assert_eq!(input, before);

    // While the opponent has a monster the attack is legal and the direct attack is not; with no
    // monster of the opponent's there is no target, so the attack is not legal and the direct one is.
    assert!(offers(&ruleset, &input, "attack"));
    assert!(!offers(&ruleset, &input, "direct_attack"));
    let alone = play(
        &ruleset,
        &opening,
        &[summon(0, attacker), end_turn(0), end_turn(1), to_battle(0)],
    );
    assert!(!offers(&ruleset, &alone, "attack"));
    assert!(offers(&ruleset, &alone, "direct_attack"));
}

// ---- the rejections

#[test]
fn a_position_change_on_the_turn_the_monster_arrived_is_rejected() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let arrived = summonable(&ruleset, &opening, 0);

    // A normal summoned monster, then a set one, on the Turn they arrive.
    let summoned = take(&ruleset, &opening, &summon(0, arrived)).state;
    let before = summoned.clone();
    assert_eq!(
        ruleset.transition(&summoned, &change_position(0, arrived)),
        Err(where_not_met("monster", arrived))
    );
    assert_eq!(summoned, before);
    assert!(!is_legal(&ruleset, &summoned, &change_position(0, arrived)));
    assert!(!offers(&ruleset, &summoned, "change_position"));

    let set = take(&ruleset, &opening, &set_monster(0, arrived)).state;
    assert_eq!(
        ruleset.transition(&set, &change_position(0, arrived)),
        Err(where_not_met("monster", arrived))
    );
    assert!(!offers(&ruleset, &set, "change_position"));

    // On the owner's next Turn the older monster may change position and the new one may not.
    let turn_3 = pass_to_turn(&ruleset, &summoned, 3);
    let newer = summonable(&ruleset, &turn_3, 0);
    let with_both = take(&ruleset, &turn_3, &summon(0, newer)).state;
    assert!(is_legal(&ruleset, &with_both, &change_position(0, arrived)));
    assert!(!is_legal(&ruleset, &with_both, &change_position(0, newer)));
    assert_eq!(
        ruleset.transition(&with_both, &change_position(0, newer)),
        Err(where_not_met("monster", newer))
    );
}

#[test]
fn a_second_position_change_in_a_turn_is_rejected() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let monster = summonable(&ruleset, &opening, 0);
    let turn_3 = pass_to_turn(
        &ruleset,
        &take(&ruleset, &opening, &summon(0, monster)).state,
        3,
    );

    let changed_once = take(&ruleset, &turn_3, &change_position(0, monster)).state;
    let before = changed_once.clone();
    assert_eq!(
        ruleset.transition(&changed_once, &change_position(0, monster)),
        Err(where_not_met("monster", monster))
    );
    assert_eq!(changed_once, before);
    assert!(!is_legal(
        &ruleset,
        &changed_once,
        &change_position(0, monster)
    ));
}

#[test]
fn an_attack_by_a_defence_position_monster_is_rejected() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let monster = summonable(&ruleset, &opening, 0);

    // A direct attack: seat 1 has no monster, the Game is past Turn 1 and the monster has not
    // attacked. Set, it may not attack; summoned, it may.
    let set_battle = play(
        &ruleset,
        &opening,
        &[
            set_monster(0, monster),
            end_turn(0),
            end_turn(1),
            to_battle(0),
        ],
    );
    let before = set_battle.clone();
    assert_eq!(position(&set_battle, monster), "defence");
    assert_eq!(
        ruleset.transition(&set_battle, &attack(0, monster)),
        Err(where_not_met("attacker", monster))
    );
    assert_eq!(set_battle, before);
    assert!(!is_legal(&ruleset, &set_battle, &attack(0, monster)));
    let summoned_battle = play(
        &ruleset,
        &opening,
        &[summon(0, monster), end_turn(0), end_turn(1), to_battle(0)],
    );
    assert!(is_legal(&ruleset, &summoned_battle, &attack(0, monster)));

    // An attack on a monster: the same monster, set, against a monster of seat 1.
    let turn_2 = take(&ruleset, &opening, &end_turn(0)).state;
    let target = summonable(&ruleset, &turn_2, 1);
    let fight_battle = play(
        &ruleset,
        &opening,
        &[
            set_monster(0, monster),
            end_turn(0),
            summon(1, target),
            end_turn(1),
            to_battle(0),
        ],
    );
    let before = fight_battle.clone();
    assert_eq!(
        ruleset.transition(&fight_battle, &fight(0, monster, target)),
        Err(where_not_met("attacker", monster))
    );
    assert_eq!(fight_battle, before);
    assert!(!is_legal(
        &ruleset,
        &fight_battle,
        &fight(0, monster, target)
    ));
    assert!(!offers(&ruleset, &fight_battle, "attack"));
}

#[test]
fn a_second_attack_by_one_monster_in_a_turn_is_rejected() {
    // Tide Scribe (ATK 1000) against a set Iron Beetle (DEF 1000): the attacker survives its first
    // attack, so only the once-per-Turn rule can refuse the second.
    let (ruleset, opening, attacker, target) = matchup("tide_scribe", "iron_beetle");
    let input = at_battle(&ruleset, &opening, attacker, &set_monster(1, target));
    let first = take(&ruleset, &input, &fight(0, attacker, target)).state;
    assert_eq!(monster_zone(&first, 0), vec![attacker]);
    assert_eq!(monster_zone(&first, 1), vec![target]);
    let before = first.clone();

    assert_eq!(
        ruleset.transition(&first, &fight(0, attacker, target)),
        Err(where_not_met("attacker", attacker))
    );
    assert_eq!(first, before);
    assert!(!is_legal(&ruleset, &first, &fight(0, attacker, target)));
    assert!(!offers(&ruleset, &first, "attack"));
}

// ---- positions

#[test]
fn a_summoned_monster_is_in_attack_position_and_a_set_one_in_defence_position() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let monster = summonable(&ruleset, &opening, 0);
    // In hand it has not arrived: attack position, free to change.
    assert_eq!(position(&opening, monster), "attack");
    assert!(!position_fixed(&opening, monster));

    let summoned = take(&ruleset, &opening, &summon(0, monster));
    assert_eq!(
        summoned.trace.events().to_vec(),
        vec![
            moved(monster, 0, "hand", "monster_zone"),
            normal_summoned(0, true),
            card_state(monster, "position_fixed", FieldValue::Bool(true)),
        ]
    );
    assert_eq!(position(&summoned.state, monster), "attack");
    assert!(position_fixed(&summoned.state, monster));

    // A set: the monster is face-up and public until Phase H, in defence position.
    let set = take(&ruleset, &opening, &set_monster(0, monster));
    assert_eq!(
        set.trace.events().to_vec(),
        vec![
            moved(monster, 0, "hand", "monster_zone"),
            normal_summoned(0, true),
            card_state(monster, "position", text("defence")),
            card_state(monster, "position_fixed", FieldValue::Bool(true)),
        ]
    );
    assert_eq!(position(&set.state, monster), "defence");
    assert!(position_fixed(&set.state, monster));
    assert_eq!(monster_zone(&set.state, 0), vec![monster]);
}

#[test]
fn a_set_and_a_normal_summon_share_the_once_per_turn_flag() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let cards = summonables(&ruleset, &opening, 0);
    let (first, second) = (cards[0], cards[1]);
    assert!(stat(&ruleset, &opening, first, "level") <= 4);
    assert!(stat(&ruleset, &opening, second, "level") <= 4);

    // A summon uses the Turn's one, so a set is refused, and the other way round.
    let summoned = take(&ruleset, &opening, &summon(0, first)).state;
    let before = summoned.clone();
    assert_eq!(
        ruleset.transition(&summoned, &set_monster(0, second)),
        Err(where_not_met("card", second))
    );
    assert_eq!(summoned, before);
    assert!(!offers(&ruleset, &summoned, "set_monster"));

    let set = take(&ruleset, &opening, &set_monster(0, first)).state;
    let before = set.clone();
    assert_eq!(
        ruleset.transition(&set, &summon(0, second)),
        Err(where_not_met("card", second))
    );
    assert_eq!(set, before);
    assert!(!offers(&ruleset, &set, "normal_summon"));
    assert!(!offers(&ruleset, &set, "set_monster"));

    // A set follows the other rules of a summon: the Monster Zone's room.
    let (full_ruleset, full) = three_monsters();
    let card = summonable(&full_ruleset, &full, 0);
    assert_eq!(
        full_ruleset.transition(&full, &set_monster(0, card)),
        Err(where_not_met("card", card))
    );
    assert!(!is_legal(&full_ruleset, &full, &set_monster(0, card)));
}

#[test]
fn a_position_change_toggles_the_position() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let monster = summonable(&ruleset, &opening, 0);

    // Attack to defence: the position is fixed and then changes.
    let turn_3 = pass_to_turn(
        &ruleset,
        &take(&ruleset, &opening, &summon(0, monster)).state,
        3,
    );
    assert!(!position_fixed(&turn_3, monster));
    let to_defence = take(&ruleset, &turn_3, &change_position(0, monster));
    assert_eq!(
        to_defence.trace.events().to_vec(),
        vec![
            card_state(monster, "position_fixed", FieldValue::Bool(true)),
            card_state(monster, "position", text("defence")),
        ]
    );
    assert_eq!(position(&to_defence.state, monster), "defence");

    // Defence to attack: a monster that was set on Turn 1 attacks on Turn 3 after the change.
    let set_turn_3 = pass_to_turn(
        &ruleset,
        &take(&ruleset, &opening, &set_monster(0, monster)).state,
        3,
    );
    assert_eq!(position(&set_turn_3, monster), "defence");
    let to_attack = take(&ruleset, &set_turn_3, &change_position(0, monster));
    assert_eq!(
        to_attack.trace.events().to_vec(),
        vec![
            card_state(monster, "position_fixed", FieldValue::Bool(true)),
            card_state(monster, "position", text("attack")),
        ]
    );
    assert_eq!(position(&to_attack.state, monster), "attack");
    let in_battle = take(&ruleset, &to_attack.state, &to_battle(0)).state;
    assert!(is_legal(&ruleset, &in_battle, &attack(0, monster)));
    // A change is a Main action: Battle does not list it.
    assert!(!offers(&ruleset, &in_battle, "change_position"));
}

#[test]
fn position_changes_are_allowed_again_on_a_later_turn() {
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let monster = summonable(&ruleset, &opening, 0);

    // Turn 1: summoned, so fixed. The End phase of the Turn lifts it.
    let summoned = take(&ruleset, &opening, &summon(0, monster)).state;
    assert!(position_fixed(&summoned, monster));
    let ended = take(&ruleset, &summoned, &end_turn(0));
    assert!(ended.trace.events().contains(&card_state(
        monster,
        "position_fixed",
        FieldValue::Bool(false)
    )));
    assert!(!position_fixed(&ended.state, monster));

    // Turn 3: one change, then none until the End phase lifts it again; Turn 5: one more.
    let turn_3 = take(&ruleset, &ended.state, &end_turn(1)).state;
    let changed = take(&ruleset, &turn_3, &change_position(0, monster)).state;
    assert_eq!(position(&changed, monster), "defence");
    assert!(!is_legal(&ruleset, &changed, &change_position(0, monster)));
    let turn_5 = pass_to_turn(&ruleset, &changed, 5);
    assert!(!position_fixed(&turn_5, monster));
    let back = take(&ruleset, &turn_5, &change_position(0, monster));
    assert_eq!(
        back.trace.events().to_vec(),
        vec![
            card_state(monster, "position_fixed", FieldValue::Bool(true)),
            card_state(monster, "position", text("attack")),
        ]
    );
}

#[test]
fn each_players_end_phase_lifts_only_their_own_monsters_position_lock() {
    // The reset is the Active Player's: the lock of a monster that arrived on Turn 2 is still on
    // after Turn 1 and lifted at the end of Turn 2.
    let (ruleset, decks) = real();
    let opening = create(&ruleset, &decks, 1).opening.state;
    let mine = summonable(&ruleset, &opening, 0);
    let turn_2 = play(&ruleset, &opening, &[summon(0, mine), end_turn(0)]);
    assert!(!position_fixed(&turn_2, mine));
    let theirs = summonable(&ruleset, &turn_2, 1);
    let theirs_summoned = take(&ruleset, &turn_2, &summon(1, theirs)).state;
    assert!(position_fixed(&theirs_summoned, theirs));
    let turn_3 = take(&ruleset, &theirs_summoned, &end_turn(1)).state;
    assert!(!position_fixed(&turn_3, theirs));
}
