//! Acceptance tests for ticket #22: the Small Ruleset's turn, read from the real ruleset file,
//! run over several rounds with the sample catalog and decks.
//!
//! The file is read up to its top-level `actions:` section, with Main waiting for `pass_turn` only,
//! followed by a `pass_turn` definition. Playing creatures is tested in `play_creature.rs`, and
//! `attack` waits for ticket [11]. Code-built Turn cycles, the rejections and the
//! definition errors are tested in the engine crate (`tests/turn_cycle.rs`).

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{Action, CardInstanceId, DeclarativeRuleset, Event, GameState};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, FieldValue, PhaseId, RulesetDefinition, StateName, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

// ---- helpers

fn definition() -> RulesetDefinition {
    let (turn_and_setup, _) = SMALL_RULESET
        .split_once("\nactions:")
        .expect("the Small Ruleset has an actions section");
    let all_actions = "actions: [play_creature, attack, pass_turn]";
    assert_eq!(
        turn_and_setup.matches(all_actions).count(),
        1,
        "Main lists play_creature, attack and pass_turn exactly once"
    );
    let main_passes_only = turn_and_setup.replace(all_actions, "actions: [pass_turn]");
    load_ruleset_definition(&format!(
        "{main_passes_only}\nactions:\n  - id: pass_turn\n    effects: [ {{ end_turn: {{}} }} ]\n"
    ))
    .expect("the Small Ruleset with only pass_turn loads")
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

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn pass(player: u32) -> Action {
    Action {
        player: p(player),
        action: ActionId::new("pass_turn"),
        arguments: vec![],
    }
}

/// Passes the Turn as the Active Player.
fn pass_turn(ruleset: &DeclarativeRuleset, state: &GameState) -> Transition<GameState, Event> {
    ruleset
        .transition(state, &pass(state.active_player().0))
        .expect("PassTurn is accepted")
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

/// The `n`-th card from the Top of a zone, 0 being the Top.
fn from_top(state: &GameState, player: u32, zone_id: &str, n: usize) -> CardInstanceId {
    *cards_in(state, player, zone_id)
        .iter()
        .rev()
        .nth(n)
        .expect("the zone has that many cards")
}

/// The sizes of a Player's hand, deck, mana deck and Mana Zone.
fn counts(state: &GameState, player: u32) -> [usize; 4] {
    ["hand", "deck", "mana_deck", "mana_zone"].map(|zone_id| cards_in(state, player, zone_id).len())
}

fn state_value(state: &GameState, card: CardInstanceId, name: &str) -> Option<FieldValue> {
    state
        .card(card)
        .and_then(|instance| instance.state_value(&StateName::new(name)))
        .cloned()
}

fn turn_began(player: u32, turn: u32) -> Event {
    Event::TurnBegan {
        player: p(player),
        turn: TurnNumber::new(turn).expect("a Turn Number is at least 1"),
    }
}

fn phase_began(id: &str) -> Event {
    Event::PhaseBegan {
        phase: PhaseId::new(id),
    }
}

fn moved(card: CardInstanceId, player: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(player),
        from: zone(from),
        to: zone(to),
    }
}

fn changed(card: CardInstanceId, state: &str, value: bool) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new(state),
        value: FieldValue::Bool(value),
    }
}

/// Applies Events that set a scenario up; each must be accepted.
fn apply_all(state: &mut GameState, events: &[Event]) {
    for event in events {
        state.apply(event).expect("the scenario Event is accepted");
    }
}

// ---- scenarios

#[test]
fn two_full_rounds_follow_the_small_ruleset_turn() {
    let ruleset = ruleset();
    let creation = ruleset
        .create_game(&decks(&ruleset), Seed(1))
        .expect("the sample decks create a game");
    let mut state = creation.opening.state;

    // Turn 1, Player 0: no draw on the first Turn, and the first 2 mana.
    assert_eq!(state.turn_number(), Some(TurnNumber::FIRST));
    assert_eq!(state.phase(), Some(&PhaseId::new("main")));
    assert_eq!(counts(&state, 0), [5, 45, 8, 2]);
    assert_eq!(counts(&state, 1), [5, 45, 10, 0]);

    // Turn 2, Player 1: they draw, and take their first 2 mana.
    state = pass_turn(&ruleset, &state).state;
    assert_eq!(state.turn_number(), TurnNumber::new(2));
    assert_eq!(state.active_player(), p(1));
    assert_eq!(counts(&state, 0), [5, 45, 8, 2]);
    assert_eq!(counts(&state, 1), [6, 44, 8, 2]);

    // During Turn 2: Player 0's mana is tapped, and a creature of theirs is on the field, tapped
    // and recently played; Player 1's mana is tapped too.
    let mana = cards_in(&state, 0, "mana_zone");
    let creature = from_top(&state, 0, "hand", 0);
    let opponent_mana = from_top(&state, 1, "mana_zone", 0);
    apply_all(
        &mut state,
        &[
            changed(mana[0], "tapped", true),
            changed(mana[1], "tapped", true),
            moved(creature, 0, "hand", "field"),
            changed(creature, "tapped", true),
            changed(creature, "recently_played", true),
            changed(opponent_mana, "tapped", true),
        ],
    );

    // Turn 3, Player 0: their Start phase untaps the Mana Zone and the field, clears
    // `recently_played`, draws and moves 2 more mana. Player 1's tapped mana stays tapped.
    let drawn = from_top(&state, 0, "deck", 0);
    let (first_mana, second_mana) = (
        from_top(&state, 0, "mana_deck", 0),
        from_top(&state, 0, "mana_deck", 1),
    );
    let transition = pass_turn(&ruleset, &state);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            turn_began(0, 3),
            phase_began("start"),
            changed(mana[0], "tapped", false),
            changed(mana[1], "tapped", false),
            changed(creature, "tapped", false),
            changed(creature, "recently_played", false),
            moved(drawn, 0, "deck", "hand"),
            moved(first_mana, 0, "mana_deck", "mana_zone"),
            moved(second_mana, 0, "mana_deck", "mana_zone"),
            phase_began("main"),
        ]
    );
    state = transition.state;
    assert_eq!(state.turn_number(), TurnNumber::new(3));
    assert_eq!(state.active_player(), p(0));
    assert_eq!(counts(&state, 0), [5, 44, 6, 4]);
    assert_eq!(cards_in(&state, 0, "field"), [creature]);
    assert_eq!(counts(&state, 1), [6, 44, 8, 2]);
    assert_eq!(
        state_value(&state, opponent_mana, "tapped"),
        Some(FieldValue::Bool(true))
    );
    assert_eq!(
        state_value(&state, creature, "tapped"),
        Some(FieldValue::Bool(false))
    );
    assert_eq!(
        state_value(&state, creature, "recently_played"),
        Some(FieldValue::Bool(false))
    );

    // Turn 4, Player 1: now Player 1's Start phase untaps their mana.
    let transition = pass_turn(&ruleset, &state);
    assert!(
        transition
            .trace
            .events()
            .contains(&changed(opponent_mana, "tapped", false)),
        "Player 1's Start phase untaps the mana they tapped"
    );
    state = transition.state;
    assert_eq!(state.turn_number(), TurnNumber::new(4));
    assert_eq!(counts(&state, 0), [5, 44, 6, 4]);
    assert_eq!(counts(&state, 1), [7, 43, 6, 4]);
}

#[test]
fn the_mana_deck_runs_out_after_five_turns() {
    let ruleset = ruleset();
    let creation = ruleset
        .create_game(&decks(&ruleset), Seed(1))
        .expect("the sample decks create a game");

    // For each Turn: the Mana Zone of its Active Player after the Start phase, and how many cards
    // that Start phase took from the mana deck.
    let mana_moves = |events: &[Event]| {
        events
            .iter()
            .filter(|event| matches!(event, Event::CardMoved { from, .. } if from == &zone("mana_deck")))
            .count()
    };
    let mut state = creation.opening.state;
    let mut turns = vec![(
        1,
        cards_in(&state, 0, "mana_zone").len(),
        // The Opening Position's trace holds the first Turn's moves and none from setup.
        mana_moves(creation.opening.trace.events()),
    )];
    for turn in 2..=11 {
        let transition = pass_turn(&ruleset, &state);
        state = transition.state;
        let active = state.active_player().0;
        turns.push((
            turn,
            cards_in(&state, active, "mana_zone").len(),
            mana_moves(transition.trace.events()),
        ));
    }
    assert_eq!(
        turns,
        [
            (1, 2, 2),
            (2, 2, 2),
            (3, 4, 2),
            (4, 4, 2),
            (5, 6, 2),
            (6, 6, 2),
            (7, 8, 2),
            (8, 8, 2),
            (9, 10, 2),
            (10, 10, 2),
            // The mana deck is empty: Player 0's sixth Turn moves nothing and the Mana Zone stays
            // at 10.
            (11, 10, 0),
        ]
    );
    assert_eq!(state.turn_number(), TurnNumber::new(11));
    assert_eq!(counts(&state, 0), [10, 40, 0, 10]);
}
