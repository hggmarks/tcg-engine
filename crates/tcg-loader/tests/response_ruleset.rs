//! Acceptance tests for ticket #87: the Response Golden Game (ADR-0039), a scripted Game on the real
//! Response Ruleset, its real catalog and its real sample decks that holds a Fizzle and a response
//! to a response and ends in a Terminal Outcome. Its Event Trace snapshot (ADR-0017) and the
//! snapshot as each Player observed it (ADR-0031) change only by the ignored test
//! `regenerate_response_golden_snapshots`, never by a normal run.

mod common;

use common::golden::{
    action, assert_matches_snapshot, assert_replaying_the_record_reproduces, attack, decks, pass,
    play_game, render, render_observed, GoldenGame, Played,
};
use tcg_engine::declarative::{
    Action, CardInstanceId, DeclarativeRuleset, Event, StackItem, StackSource,
};
use tcg_engine::player::PlayerId;
use tcg_engine::ruleset_definition::ActionId;
use tcg_loader::{load_card_catalog, load_ruleset_definition};

const RESPONSE_RULESET: &str =
    include_str!("../../../rulesets/response-ruleset/response-ruleset.yaml");
const RESPONSE_CATALOG: &str =
    include_str!("../../../rulesets/response-ruleset/response-catalog.yaml");
const RESPONSE_DECK_A: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-a.yaml");
const RESPONSE_DECK_B: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-b.yaml");

const FIZZLE_SNAPSHOT: &str = include_str!("golden/response-fizzle.trace");
const FIZZLE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/response-fizzle.trace"
);
const FIZZLE_OBSERVED_SNAPSHOTS: [&str; 2] = [
    include_str!("golden/response-fizzle.observed-by-player-0.trace"),
    include_str!("golden/response-fizzle.observed-by-player-1.trace"),
];
const FIZZLE_OBSERVED_PATHS: [&str; 2] = [
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/response-fizzle.observed-by-player-0.trace"
    ),
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/response-fizzle.observed-by-player-1.trace"
    ),
];
const REGENERATE_FIZZLE_COMMAND: &str =
    "cargo test -p tcg-loader --test response_ruleset -- --ignored regenerate_response_golden_snapshots";

/// The Seed of the Response Golden Game, found by a throwaway search: a scripted bot played the
/// shipped Response decks over the Seeds 0..1000, in four variants of its choices, and the shortest
/// Game that holds a Fizzle of an attack and a Stack of depth 3 and ends was kept. The search is
/// not committed: the script is the Game.
const FIZZLE_SEED: u64 = 294;

/// The cards of the script, in the Opening Position of `FIZZLE_SEED` with the Response sample
/// decks (Deck A numbers its main deck from 1 in file order, Deck B from 62).
const FIZZLE_LEADER_0: CardInstanceId = CardInstanceId(0);
const FIZZLE_LEADER_1: CardInstanceId = CardInstanceId(61);
/// Seat 0's attackers, and the fast creature it answers with.
const FIZZLE_GOBLIN_0: CardInstanceId = CardInstanceId(2);
const FIZZLE_SPARROW_0: CardInstanceId = CardInstanceId(3);
const FIZZLE_MOSS_0: CardInstanceId = CardInstanceId(8);
const FIZZLE_FOX_0: CardInstanceId = CardInstanceId(10);
const FIZZLE_LAMP_0: CardInstanceId = CardInstanceId(30);
const FIZZLE_LAMP_0_SECOND: CardInstanceId = CardInstanceId(31);
const FIZZLE_IMP_0: CardInstanceId = CardInstanceId(46);
/// Seat 1's creatures: the Skittish Hare is the target of the attack that Fizzles.
const FIZZLE_MOSS_1: CardInstanceId = CardInstanceId(62);
const FIZZLE_SPARROW_1: CardInstanceId = CardInstanceId(65);
const FIZZLE_OTTER_1: CardInstanceId = CardInstanceId(71);
const FIZZLE_HARE_1: CardInstanceId = CardInstanceId(77);

/// The Ability Index of the Skittish Hare's one effect: the activated ability.
const HARE_ABILITY: u32 = 0;

fn response() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(RESPONSE_RULESET).expect("the Response Ruleset loads");
    let catalog = load_card_catalog(&definition, RESPONSE_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn play(player: u32, card: CardInstanceId) -> Action {
    action(player, "play_creature", &[card])
}

fn fast_play(player: u32, card: CardInstanceId) -> Action {
    action(player, "fast_play", &[card])
}

fn window_pass(player: u32) -> Action {
    Action::Pass {
        player: PlayerId(player),
    }
}

fn activate(player: u32, card: CardInstanceId, ability: u32) -> Action {
    Action::Activate {
        player: PlayerId(player),
        card,
        ability,
    }
}

/// Seat 0 builds a board and attacks seat 1's Leader, seat 1 builds its own and attacks with its
/// Skittish Hare, which taps it. Then seat 0 attacks the tapped hare: seat 1 answers by activating
/// the hare, which sends it to its hand once it resolves (the Stack holds two items); seat 0
/// answers that with a Flash Fox (three items). Seat 1 passes and the fox resolves, seat 0 passes
/// and the hare returns to its hand, seat 1 passes and the attack, whose target has left,
/// Fizzles. Seat 0 then attacks seat 1's Leader until it is dead; seat 1 passes every window.
fn response_fizzle_script() -> Vec<Action> {
    vec![
        play(0, FIZZLE_SPARROW_0),
        play(0, FIZZLE_LAMP_0),
        pass(0),
        play(1, FIZZLE_MOSS_1),
        play(1, FIZZLE_HARE_1),
        pass(1),
        attack(0, FIZZLE_SPARROW_0, FIZZLE_LEADER_1),
        window_pass(1),
        attack(0, FIZZLE_LAMP_0, FIZZLE_LEADER_1),
        window_pass(1),
        play(0, FIZZLE_GOBLIN_0),
        play(0, FIZZLE_MOSS_0),
        play(0, FIZZLE_LAMP_0_SECOND),
        play(0, FIZZLE_IMP_0),
        pass(0),
        attack(1, FIZZLE_MOSS_1, FIZZLE_LEADER_0),
        window_pass(0),
        attack(1, FIZZLE_HARE_1, FIZZLE_LEADER_0),
        window_pass(0),
        play(1, FIZZLE_SPARROW_1),
        play(1, FIZZLE_OTTER_1),
        pass(1),
        attack(0, FIZZLE_GOBLIN_0, FIZZLE_HARE_1),
        activate(1, FIZZLE_HARE_1, HARE_ABILITY),
        fast_play(0, FIZZLE_FOX_0),
        window_pass(1),
        window_pass(0),
        window_pass(1),
        attack(0, FIZZLE_SPARROW_0, FIZZLE_LEADER_1),
        window_pass(1),
        attack(0, FIZZLE_MOSS_0, FIZZLE_LEADER_1),
        window_pass(1),
        attack(0, FIZZLE_LAMP_0, FIZZLE_LEADER_1),
        window_pass(1),
        attack(0, FIZZLE_LAMP_0_SECOND, FIZZLE_LEADER_1),
        window_pass(1),
    ]
}

/// The Response Golden Game: the Response sample decks, `FIZZLE_SEED`.
fn response_fizzle() -> GoldenGame {
    GoldenGame {
        name: "response-fizzle",
        seed: FIZZLE_SEED,
        script: response_fizzle_script(),
    }
}

fn response_decks(ruleset: &DeclarativeRuleset) -> Vec<tcg_engine::deck_list::DeckList> {
    decks(ruleset, [RESPONSE_DECK_A, RESPONSE_DECK_B])
}

fn play_response_fizzle() -> Played {
    let ruleset = response();
    play_game(&ruleset, &response_decks(&ruleset), &response_fizzle())
}

fn render_response_observed(game: &GoldenGame, seat: u32) -> String {
    let ruleset = response();
    render_observed(&ruleset, &response_decks(&ruleset), game, seat)
}

fn is_attack(item: &StackItem) -> bool {
    item.source == StackSource::Action(ActionId::new("attack"))
}

#[test]
fn the_response_fizzle_golden_game_matches_its_snapshot() {
    let played = play_response_fizzle();
    assert_eq!(played.steps.len(), response_fizzle_script().len());
    assert_matches_snapshot(
        "response-fizzle",
        FIZZLE_SNAPSHOT,
        &render(&played),
        REGENERATE_FIZZLE_COMMAND,
    );
}

#[test]
fn the_response_fizzle_golden_game_as_observed_by_each_player_matches_its_snapshot() {
    let game = response_fizzle();
    let observed: Vec<String> = (0..2)
        .map(|seat| render_response_observed(&game, seat))
        .collect();
    for (seat, (actual, snapshot)) in observed.iter().zip(FIZZLE_OBSERVED_SNAPSHOTS).enumerate() {
        assert_matches_snapshot(
            &format!("response-fizzle as observed by player {seat}"),
            snapshot,
            actual,
            REGENERATE_FIZZLE_COMMAND,
        );
    }
    // The two Players do not see the same Game: what each observed differs.
    assert_ne!(FIZZLE_OBSERVED_SNAPSHOTS[0], FIZZLE_OBSERVED_SNAPSHOTS[1]);
}

/// The key Events of the Game, asserted outside the snapshot, so a script edit that stops short
/// of the Fizzle, of the response to a response or of the end is caught here and not only by a
/// diff.
#[test]
fn the_response_fizzle_golden_game_holds_a_fizzle_and_a_response_to_a_response() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    let played = play_game(&ruleset, &decks, &response_fizzle());
    let events: Vec<&Event> = played.steps.iter().flat_map(|(_, events)| events).collect();

    // A Fizzle: an attack item whose bound target is the hare that left.
    let fizzled_attacks: Vec<&StackItem> = events
        .iter()
        .filter_map(|event| match event {
            Event::ItemFizzled { item } if is_attack(item) => Some(item),
            _ => None,
        })
        .collect();
    assert_eq!(fizzled_attacks.len(), 1, "one attack Fizzles");
    assert!(
        fizzled_attacks[0]
            .bound
            .iter()
            .any(|bound| bound.card == FIZZLE_HARE_1
                && bound.param.as_str() == "target"
                && bound.left),
        "the attack that Fizzles targeted the hare, which had left: {:?}",
        fizzled_attacks[0]
    );

    // The Stack is as deep as three items, read from the Events...
    let mut depth = 0_usize;
    let mut deepest = 0_usize;
    for event in &events {
        match event {
            Event::ItemPushed { .. } => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            Event::ItemResolved { .. } | Event::ItemFizzled { .. } => depth -= 1,
            _ => {}
        }
    }
    assert_eq!(deepest, 3, "the deepest Stack of the Game");
    assert_eq!(depth, 0, "every item left the Stack");

    // ...and in the GameState after the Action that pushes the third item: the attack, the hare's
    // ability and the Flash Fox.
    let fox_push = response_fizzle_script()
        .iter()
        .position(|action| *action == fast_play(0, FIZZLE_FOX_0))
        .expect("the script plays the Flash Fox as a response");
    let up_to_the_fox = GoldenGame {
        name: "response-fizzle-up-to-the-fox",
        seed: FIZZLE_SEED,
        script: response_fizzle_script()[..=fox_push].to_vec(),
    };
    let at_depth_3 = play_game(&ruleset, &decks, &up_to_the_fox);
    let stack = at_depth_3.final_state.stack();
    assert_eq!(stack.len(), 3);
    // Bottom to top: seat 0's attack, seat 1's hare ability, seat 0's `fast_play` of the fox.
    assert!(is_attack(&stack[0]) && stack[0].controller == PlayerId(0));
    assert!(
        matches!(
            stack[1].source,
            StackSource::Ability { card, ability } if card == FIZZLE_HARE_1 && ability == HARE_ABILITY
        ) && stack[1].controller == PlayerId(1)
    );
    assert_eq!(
        stack[2].source,
        StackSource::Action(ActionId::new("fast_play"))
    );
    assert_eq!(stack[2].controller, PlayerId(0));
    assert!(stack[2]
        .bound
        .iter()
        .any(|bound| bound.card == FIZZLE_FOX_0));

    // An ability was used and a fast creature was pushed as a response.
    assert!(
        played
            .steps
            .iter()
            .any(|(action, _)| *action == activate(1, FIZZLE_HARE_1, HARE_ABILITY)),
        "the script activates the hare"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ItemPushed { item }
            if item.source == StackSource::Action(ActionId::new("fast_play"))
                && item.controller == PlayerId(0)
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ItemPushed { item }
            if matches!(item.source, StackSource::Ability { card, ability }
                if card == FIZZLE_HARE_1 && ability == HARE_ABILITY)
    )));

    // The Game ends in a Terminal Outcome: seat 0 wins by killing seat 1's Leader.
    let outcome = played
        .final_state
        .outcome()
        .expect("the Response Golden Game ends the Game");
    assert_eq!(outcome.winners(), [PlayerId(0)]);
    assert!(played.final_state.stack().is_empty());
}

#[test]
fn replaying_the_record_of_response_fizzle_reproduces_its_game() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    assert_replaying_the_record_reproduces(&ruleset, &decks, &response_fizzle());
}

/// Rewrites the three snapshots of the Response Golden Game: the Event Trace and the two observed
/// ones. Run it on purpose, and read the diff before committing it. It does not touch the Small or
/// Effects files.
#[test]
#[ignore = "deliberate update: rewrites the Response golden snapshots"]
fn regenerate_response_golden_snapshots() {
    std::fs::write(FIZZLE_PATH, render(&play_response_fizzle()))
        .expect("the Response snapshot is written");
    for (seat, path) in FIZZLE_OBSERVED_PATHS.iter().enumerate() {
        let seat = u32::try_from(seat).expect("a seat");
        std::fs::write(path, render_response_observed(&response_fizzle(), seat))
            .expect("the observed Response snapshot is written");
    }
}
