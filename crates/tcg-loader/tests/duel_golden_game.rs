//! Acceptance tests for ticket #100: the Duel Golden Game (ADR-0045), a scripted Game on the real
//! Duel Ruleset, its real catalog and its real sample decks that holds a two-tribute summon, a tie,
//! damage to an attacker's owner from a higher DEF and a kill by direct attack. Its Event Trace
//! snapshot (ADR-0017) and the snapshot as each Player observed it (ADR-0031) change only by the
//! ignored test `regenerate_duel_golden_snapshots`, never by a normal run.

mod common;

use common::golden::{
    action, assert_matches_snapshot, assert_replaying_the_record_reproduces, attack, decks,
    play_game, render, render_observed, GoldenGame, Played,
};
use tcg_engine::declarative::{Action, CardInstanceId, DeclarativeRuleset, Event};
use tcg_engine::player::PlayerId;
use tcg_engine::ruleset_definition::{FieldValue, PlayerStateId, StateName, ZoneId};
use tcg_loader::{load_card_catalog, load_ruleset_definition};

const DUEL_RULESET: &str = include_str!("../../../rulesets/duel-ruleset/duel-ruleset.yaml");
const DUEL_CATALOG: &str = include_str!("../../../rulesets/duel-ruleset/duel-catalog.yaml");
const DUEL_DECK_A: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-a.yaml");
const DUEL_DECK_B: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-b.yaml");

const DIRECT_KILL_SNAPSHOT: &str = include_str!("golden/duel-direct-kill.trace");
const DIRECT_KILL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/duel-direct-kill.trace"
);
const DIRECT_KILL_OBSERVED_SNAPSHOTS: [&str; 2] = [
    include_str!("golden/duel-direct-kill.observed-by-player-0.trace"),
    include_str!("golden/duel-direct-kill.observed-by-player-1.trace"),
];
const DIRECT_KILL_OBSERVED_PATHS: [&str; 2] = [
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/duel-direct-kill.observed-by-player-0.trace"
    ),
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/duel-direct-kill.observed-by-player-1.trace"
    ),
];
const REGENERATE_DIRECT_KILL_COMMAND: &str =
    "cargo test -p tcg-loader --test duel_golden_game -- --ignored regenerate_duel_golden_snapshots";

/// The Seed of the Duel Golden Game, found by a throwaway search: a goal-driven bot played the
/// shipped Duel decks over the Seeds 0..3000, in forty variants of its choices, each Game asking
/// for a two-tribute summon, then a tie, then damage from a higher DEF, then a kill by direct
/// attack, and the shortest Game that holds all four and ends by that kill was kept. The search is
/// not committed: the script is the Game.
const DIRECT_KILL_SEED: u64 = 226;

/// The cards of the script, in the Opening Position of `DIRECT_KILL_SEED` with the Duel sample
/// decks (no Leader: Deck A numbers its 40 cards from 0 in file order, Deck B from 40).
/// Seat 0's Quarry Bear (set, then a tribute), Pebble Golem (a tribute), Abyss Leviathan (the
/// two-tribute summon, level 7, ATK 2600) and Cinder Ogre (ATK 1700, the tie).
const QUARRY_BEAR_0: CardInstanceId = CardInstanceId(23);
const PEBBLE_GOLEM_0: CardInstanceId = CardInstanceId(9);
const ABYSS_LEVIATHAN_0: CardInstanceId = CardInstanceId(32);
const CINDER_OGRE_0: CardInstanceId = CardInstanceId(24);
/// Seat 1's Reed Stalker (ATK 900, the attacker of the higher DEF) and Cinder Ogre (ATK 1700, the
/// tie).
const REED_STALKER_1: CardInstanceId = CardInstanceId(45);
const CINDER_OGRE_1: CardInstanceId = CardInstanceId(63);

fn duel() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(DUEL_RULESET).expect("the Duel Ruleset loads");
    let catalog = load_card_catalog(&definition, DUEL_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn duel_decks(ruleset: &DeclarativeRuleset) -> Vec<tcg_engine::deck_list::DeckList> {
    decks(ruleset, [DUEL_DECK_A, DUEL_DECK_B])
}

fn set(player: u32, card: CardInstanceId) -> Action {
    action(player, "set_monster", &[card])
}

fn summon(player: u32, card: CardInstanceId) -> Action {
    action(player, "normal_summon", &[card])
}

fn to_battle(player: u32) -> Action {
    action(player, "to_battle", &[])
}

fn end_turn(player: u32) -> Action {
    action(player, "end_turn", &[])
}

fn tribute_summon_two(
    player: u32,
    card: CardInstanceId,
    tribute_1: CardInstanceId,
    tribute_2: CardInstanceId,
) -> Action {
    action(player, "tribute_summon_two", &[card, tribute_1, tribute_2])
}

fn direct_attack(player: u32, attacker: CardInstanceId) -> Action {
    action(player, "direct_attack", &[attacker])
}

/// Seat 0 sets its Quarry Bear and ends its Turn (it may not attack on the Game's first Turn),
/// seat 1 only ends its own. Seat 0 sets its Pebble Golem; seat 1 summons its Reed Stalker and
/// attacks the set Quarry Bear with it: ATK 900 against DEF 1100, so no monster is destroyed and
/// seat 1 loses the 200 difference. Seat 0 then summons its Abyss Leviathan with the Pebble Golem
/// and the Quarry Bear as tributes, attacks the Reed Stalker with it (seat 1 loses 1,700) and ends
/// its Turn. Seat 1 summons its Cinder Ogre; seat 0 summons its own and attacks the other with it:
/// ATK 1700 against ATK 1700, a tie that destroys both. Seat 1 has no monster left, so the Abyss
/// Leviathan attacks it directly for 2,600 and seat 1, at 2,100 Life Points, loses.
fn duel_direct_kill_script() -> Vec<Action> {
    vec![
        set(0, QUARRY_BEAR_0),
        to_battle(0),
        end_turn(0),
        to_battle(1),
        end_turn(1),
        set(0, PEBBLE_GOLEM_0),
        to_battle(0),
        end_turn(0),
        summon(1, REED_STALKER_1),
        to_battle(1),
        attack(1, REED_STALKER_1, QUARRY_BEAR_0),
        end_turn(1),
        tribute_summon_two(0, ABYSS_LEVIATHAN_0, PEBBLE_GOLEM_0, QUARRY_BEAR_0),
        to_battle(0),
        attack(0, ABYSS_LEVIATHAN_0, REED_STALKER_1),
        end_turn(0),
        summon(1, CINDER_OGRE_1),
        end_turn(1),
        summon(0, CINDER_OGRE_0),
        to_battle(0),
        attack(0, CINDER_OGRE_0, CINDER_OGRE_1),
        direct_attack(0, ABYSS_LEVIATHAN_0),
    ]
}

/// The Duel Golden Game: the Duel sample decks, `DIRECT_KILL_SEED`.
fn duel_direct_kill() -> GoldenGame {
    GoldenGame {
        name: "duel-direct-kill",
        seed: DIRECT_KILL_SEED,
        script: duel_direct_kill_script(),
    }
}

fn play_duel_direct_kill() -> Played {
    let ruleset = duel();
    play_game(&ruleset, &duel_decks(&ruleset), &duel_direct_kill())
}

fn render_duel_observed(game: &GoldenGame, seat: u32) -> String {
    let ruleset = duel();
    render_observed(&ruleset, &duel_decks(&ruleset), game, seat)
}

#[test]
fn the_duel_direct_kill_golden_game_matches_its_snapshot() {
    let played = play_duel_direct_kill();
    assert_eq!(played.steps.len(), duel_direct_kill_script().len());
    assert_matches_snapshot(
        "duel-direct-kill",
        DIRECT_KILL_SNAPSHOT,
        &render(&played),
        REGENERATE_DIRECT_KILL_COMMAND,
    );
}

#[test]
fn the_duel_direct_kill_golden_game_as_observed_by_each_player_matches_its_snapshot() {
    let game = duel_direct_kill();
    let observed: Vec<String> = (0..2)
        .map(|seat| render_duel_observed(&game, seat))
        .collect();
    for (seat, (actual, snapshot)) in observed
        .iter()
        .zip(DIRECT_KILL_OBSERVED_SNAPSHOTS)
        .enumerate()
    {
        assert_matches_snapshot(
            &format!("duel-direct-kill as observed by player {seat}"),
            snapshot,
            actual,
            REGENERATE_DIRECT_KILL_COMMAND,
        );
    }
    // The two Players do not see the same Game: what each observed differs (their hands).
    assert_ne!(
        DIRECT_KILL_OBSERVED_SNAPSHOTS[0],
        DIRECT_KILL_OBSERVED_SNAPSHOTS[1]
    );
}

/// The Events of the script's Action `action`, which must be in the script exactly once.
fn events_of<'a>(played: &'a Played, wanted: &Action) -> &'a [Event] {
    let mut found = played
        .steps
        .iter()
        .filter(|(taken, _)| taken == wanted)
        .map(|(_, events)| events.as_slice());
    let events = found.next().expect("the script holds the Action");
    assert!(found.next().is_none(), "the script holds the Action once");
    events
}

fn moved(card: CardInstanceId, player: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: PlayerId(player),
        from: ZoneId::new(from),
        to: ZoneId::new(to),
    }
}

fn life_points(player: u32, value: i64) -> Event {
    Event::PlayerStateChanged {
        player: PlayerId(player),
        state: PlayerStateId::new("life_points"),
        value: FieldValue::Int(value),
    }
}

fn attacked(card: CardInstanceId) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new("attacked"),
        value: FieldValue::Bool(true),
    }
}

/// The key Events of the Game, asserted outside the snapshot, so a script edit that stops short
/// of the two-tribute summon, of the tie, of the damage from a higher DEF or of the kill by direct
/// attack is caught here and not only by a diff.
#[test]
fn the_duel_direct_kill_golden_game_holds_a_two_tribute_summon_a_tie_damage_from_a_higher_def_and_a_kill_by_direct_attack(
) {
    let played = play_duel_direct_kill();

    // A summon with two tributes: both tributes go to the Graveyard as the cost, in parameter
    // order, then the Abyss Leviathan goes from the hand to the Monster Zone.
    let events = events_of(
        &played,
        &tribute_summon_two(0, ABYSS_LEVIATHAN_0, PEBBLE_GOLEM_0, QUARRY_BEAR_0),
    );
    let position = |wanted: &Event| {
        events
            .iter()
            .position(|event| event == wanted)
            .unwrap_or_else(|| panic!("the summon's trace holds {wanted:?}: {events:?}"))
    };
    let first_tribute = position(&moved(PEBBLE_GOLEM_0, 0, "monster_zone", "graveyard"));
    let second_tribute = position(&moved(QUARRY_BEAR_0, 0, "monster_zone", "graveyard"));
    let summoned = position(&moved(ABYSS_LEVIATHAN_0, 0, "hand", "monster_zone"));
    assert!(
        first_tribute < second_tribute && second_tribute < summoned,
        "the tributes are the cost, paid before the summon: {events:?}"
    );

    // Damage to the attacker's owner from a higher DEF: the Reed Stalker (ATK 900) attacks the
    // set Quarry Bear (DEF 1100), no card moves, and seat 1 alone loses the 200 difference.
    let events = events_of(&played, &attack(1, REED_STALKER_1, QUARRY_BEAR_0));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::CardMoved { .. })),
        "no card moves when the attacker's ATK is below the target's DEF: {events:?}"
    );
    assert_eq!(
        events,
        [attacked(REED_STALKER_1), life_points(1, 3800)],
        "seat 1 goes from 4000 to 3800 Life Points"
    );

    // A tie: ATK 1700 against ATK 1700 sends the target and then the attacker to the Graveyard,
    // and no Player loses Life Points.
    let events = events_of(&played, &attack(0, CINDER_OGRE_0, CINDER_OGRE_1));
    assert_eq!(
        events,
        [
            attacked(CINDER_OGRE_0),
            moved(CINDER_OGRE_1, 1, "monster_zone", "graveyard"),
            moved(CINDER_OGRE_0, 0, "monster_zone", "graveyard"),
        ],
        "a tie destroys the target and then the attacker, and costs neither Player Life Points"
    );

    // A kill by direct attack: the last Action takes seat 1 from 2,100 Life Points to -500 and
    // ends the Game with one Loss, seat 1's, for `life_out`.
    let (last_action, events) = played.steps.last().expect("the script has Actions");
    assert_eq!(*last_action, direct_attack(0, ABYSS_LEVIATHAN_0));
    assert_eq!(
        events[..events.len() - 1],
        [attacked(ABYSS_LEVIATHAN_0), life_points(1, -500)],
        "seat 1 is at 2,100 Life Points before the direct attack, so its 2,600 ATK kills"
    );
    let Some(Event::GameEnded { losses }) = events.last() else {
        panic!("the last Action ends the Game: {events:?}");
    };
    assert_eq!(losses.len(), 1, "one Loss: {losses:?}");
    assert_eq!(losses[0].player, PlayerId(1));
    assert_eq!(losses[0].reason.as_str(), "life_out");
    let outcome = played
        .final_state
        .outcome()
        .expect("the Duel Golden Game ends the Game");
    assert_eq!(outcome.winners(), [PlayerId(0)]);

    // It is the only Action that ends the Game, and the only direct attack.
    assert_eq!(
        played
            .steps
            .iter()
            .filter(|(_, events)| events
                .iter()
                .any(|event| matches!(event, Event::GameEnded { .. })))
            .count(),
        1
    );
    assert_eq!(
        played
            .steps
            .iter()
            .filter(|(taken, _)| matches!(taken, Action::Declared { action, .. } if action.as_str() == "direct_attack"))
            .count(),
        1
    );
}

#[test]
fn replaying_the_duel_direct_kill_golden_game_reproduces_it() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    assert_replaying_the_record_reproduces(&ruleset, &decks, &duel_direct_kill());
}

/// Rewrites the three snapshots of the Duel Golden Game: the Event Trace and the two observed
/// ones. Run it on purpose, and read the diff before committing it. It does not touch the Small,
/// Effects or Response files.
#[test]
#[ignore = "deliberate update: rewrites the Duel golden snapshots"]
fn regenerate_duel_golden_snapshots() {
    std::fs::write(DIRECT_KILL_PATH, render(&play_duel_direct_kill()))
        .expect("the Duel snapshot is written");
    for (seat, path) in DIRECT_KILL_OBSERVED_PATHS.iter().enumerate() {
        let seat = u32::try_from(seat).expect("a seat");
        std::fs::write(path, render_duel_observed(&duel_direct_kill(), seat))
            .expect("the observed Duel snapshot is written");
    }
}
