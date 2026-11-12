//! Acceptance tests for ticket #26: the canonical Small Ruleset, its catalog and its two sample
//! decks, played end to end from declarative data.
//!
//! A **Golden Game** is a Seed, the two sample decks (deck A for seat 0 and deck B for seat 1) and
//! a literal list of Actions. Its **Golden Snapshot** is a text file (`tests/golden/*.trace`) with
//! one `Debug` line per Event, compared byte for byte (ADR-0017). A snapshot changes only by the
//! ignored test `regenerate_golden_snapshots`, never by a normal run.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameState, Loss,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::Ruleset;
use tcg_engine::ruleset_definition::{
    ActionId, PhaseId, PhaseKind, StateName, WinConditionId, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

const DECK_OUT_SNAPSHOT: &str = include_str!("golden/small-deck-out.trace");
const LEADER_KILL_SNAPSHOT: &str = include_str!("golden/small-leader-kill.trace");

const DECK_OUT_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/small-deck-out.trace"
);
const LEADER_KILL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/small-leader-kill.trace"
);
const REGENERATE_COMMAND: &str =
    "cargo test -p tcg-loader --test small_ruleset_end_to_end -- --ignored regenerate_golden_snapshots";

/// The Seed of the deck-out Golden Game: nothing but passes, so any Seed would do.
const DECK_OUT_SEED: u64 = 1;

/// The Seed of the Leader-kill Golden Game. With it seat 0 opens with `goblin_scout` (1, cost 1,
/// power 1), `river_otter` (13, cost 2, power 3), `moss_sprite` (7, cost 1, power 2),
/// `thorn_boar` (35, cost 4, power 4) and `ash_drake`; it draws `thorn_boar` (34) on Turn 3,
/// `wind_hawk` (31) on Turn 5 and `shadow_cat` on Turn 7. Seat 1 opens with `moss_sprite` (63),
/// `sun_knight`, `stone_golem` (75), `river_otter` (72) and `ash_drake`; it draws `iron_guard`
/// (85) on Turn 2, `wind_hawk` (88) on Turn 4 and `shadow_cat` (80) on Turn 6.
const LEADER_KILL_SEED: u64 = 9;

const LEADER_0: CardInstanceId = CardInstanceId(0);
const LEADER_1: CardInstanceId = CardInstanceId(61);
const GOBLIN_0: CardInstanceId = CardInstanceId(1);
const MOSS_0: CardInstanceId = CardInstanceId(7);
const OTTER_0: CardInstanceId = CardInstanceId(13);
const HAWK_0: CardInstanceId = CardInstanceId(31);
const BOAR_0: CardInstanceId = CardInstanceId(35);
const MOSS_1: CardInstanceId = CardInstanceId(63);
const OTTER_1: CardInstanceId = CardInstanceId(72);
const GOLEM_1: CardInstanceId = CardInstanceId(75);
const CAT_1: CardInstanceId = CardInstanceId(80);
const GUARD_1: CardInstanceId = CardInstanceId(85);

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

/// The real Small Ruleset and the real catalog, loaded from their text.
fn small() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// Deck A for seat 0 and deck B for seat 1.
fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    [DECK_A, DECK_B]
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn opening(ruleset: &DeclarativeRuleset, seed: u64) -> GameState {
    ruleset
        .create_game(&decks(ruleset), Seed(seed))
        .expect("the sample decks create a game")
        .opening
        .state
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn play(player: u32, card: CardInstanceId) -> Action {
    action(player, "play_creature", &[card])
}

fn pass(player: u32) -> Action {
    action(player, "pass_turn", &[])
}

fn attack(player: u32, attacker: CardInstanceId, target: CardInstanceId) -> Action {
    action(player, "attack", &[attacker, target])
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

/// The card `depth` places below the top of a zone (0 is the top, the last card).
fn from_top(state: &GameState, player: u32, zone_id: &str, depth: usize) -> CardInstanceId {
    let cards = cards_in(state, player, zone_id);
    *cards
        .iter()
        .rev()
        .nth(depth)
        .expect("the zone has that many cards")
}

fn phase_began(phase: &str) -> Event {
    Event::PhaseBegan {
        phase: PhaseId::new(phase),
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

fn deck_out_loss(seat: u32) -> Loss {
    Loss {
        player: p(seat),
        reason: WinConditionId::new("deck_out"),
    }
}

fn leader_dead_loss(seat: u32) -> Loss {
    Loss {
        player: p(seat),
        reason: WinConditionId::new("leader_dead"),
    }
}

// ---- Golden Games

/// A Golden Game as it was played: the Event Trace of Game creation, each accepted Action with its
/// Event Trace, and the final GameState.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Played {
    name: &'static str,
    seed: u64,
    creation: Vec<Event>,
    steps: Vec<(Action, Vec<Event>)>,
    final_state: GameState,
}

/// Plays a literal script on a ruleset loaded from the real files. Every Action must be accepted.
fn play_game(
    ruleset: &DeclarativeRuleset,
    name: &'static str,
    seed: u64,
    script: &[Action],
) -> Played {
    let created = ruleset
        .create_game(&decks(ruleset), Seed(seed))
        .expect("the sample decks create a game");
    let creation = created.opening.trace.events().to_vec();
    let mut state = created.opening.state;
    let mut steps = Vec::new();
    for (index, action) in script.iter().enumerate() {
        let transition = ruleset
            .transition(&state, action)
            .unwrap_or_else(|rejection| {
                panic!(
                    "action {} of {name} ({action:?}) is rejected: {rejection:?}",
                    index + 1
                )
            });
        steps.push((action.clone(), transition.trace.events().to_vec()));
        state = transition.state;
    }
    Played {
        name,
        seed,
        creation,
        steps,
        final_state: state,
    }
}

/// Both players pass, 91 times: seat 1 draws from an empty deck when Turn 92 begins.
fn deck_out_script() -> Vec<Action> {
    (0..91).map(|pass_number| pass(pass_number % 2)).collect()
}

fn play_deck_out(ruleset: &DeclarativeRuleset, seed: u64) -> Played {
    play_game(ruleset, "small-deck-out", seed, &deck_out_script())
}

/// Seat 0 races seat 1's Leader (life 6) and seat 1 answers; the script is read turn by turn.
fn leader_kill_script() -> Vec<Action> {
    vec![
        // Turn 1, seat 0 (2 mana): two creatures.
        play(0, MOSS_0),
        play(0, GOBLIN_0),
        pass(0),
        // Turn 2, seat 1 (2 mana): `river_otter`.
        play(1, OTTER_1),
        pass(1),
        // Turn 3, seat 0 (4 mana): both creatures hit the Leader of seat 1 (6 to 4); then another
        // creature.
        attack(0, MOSS_0, LEADER_1),
        attack(0, GOBLIN_0, LEADER_1),
        play(0, OTTER_0),
        pass(0),
        // Turn 4, seat 1 (4 mana): its otter (power 3) kills the tapped moss sprite (power 2); then
        // two creatures.
        attack(1, OTTER_1, MOSS_0),
        play(1, GOLEM_1),
        play(1, MOSS_1),
        pass(1),
        // Turn 5, seat 0 (6 mana): the goblin (power 1) attacks the tapped otter (power 3) and
        // only taps; the otter hits the Leader (4 to 3); then two creatures.
        attack(0, GOBLIN_0, OTTER_1),
        attack(0, OTTER_0, LEADER_1),
        play(0, BOAR_0),
        play(0, HAWK_0),
        pass(0),
        // Turn 6, seat 1 (6 mana): the otter kills the tapped otter of seat 0, and two creatures
        // hit the Leader of seat 0 (5 to 3); then two creatures.
        attack(1, OTTER_1, OTTER_0),
        attack(1, GOLEM_1, LEADER_0),
        attack(1, MOSS_1, LEADER_0),
        play(1, CAT_1),
        play(1, GUARD_1),
        pass(1),
        // Turn 7, seat 0: three ready creatures take the Leader of seat 1 from 3 to 0.
        attack(0, GOBLIN_0, LEADER_1),
        attack(0, BOAR_0, LEADER_1),
        attack(0, HAWK_0, LEADER_1),
    ]
}

fn play_leader_kill(ruleset: &DeclarativeRuleset, seed: u64) -> Played {
    play_game(ruleset, "small-leader-kill", seed, &leader_kill_script())
}

/// The text of a Golden Snapshot (ADR-0017). It ends with exactly one newline.
fn render(played: &Played) -> String {
    let mut text = format!(
        "# {} | seed {} | seat 0 deck-a, seat 1 deck-b\n== creation\n",
        played.name, played.seed
    );
    for event in &played.creation {
        text.push_str(&format!("{event:?}\n"));
    }
    for (index, (action, events)) in played.steps.iter().enumerate() {
        let cards: Vec<String> = action
            .arguments
            .iter()
            .map(|argument| match argument {
                Argument::Card(card) => card.0.to_string(),
            })
            .collect();
        text.push_str(&format!(
            "== action {}: player {} {} [{}]\n",
            index + 1,
            action.player.0,
            action.action.as_str(),
            cards.join(", ")
        ));
        for event in events {
            text.push_str(&format!("{event:?}\n"));
        }
    }
    let outcome = played
        .final_state
        .outcome()
        .expect("a Golden Game ends the Game");
    text.push_str(&format!("== outcome\n{outcome:?}\n"));
    text
}

/// Fails with the first differing line, the two lines and the way to update the snapshot.
fn assert_matches_snapshot(name: &str, snapshot: &str, actual: &str) {
    if snapshot == actual {
        return;
    }
    let mut expected_lines = snapshot.lines();
    let mut actual_lines = actual.lines();
    for line_number in 1.. {
        let (expected, found) = (expected_lines.next(), actual_lines.next());
        if expected != found {
            panic!(
                "the Golden Game {name} differs from its snapshot at line {line_number}\n  \
                 expected: {}\n  actual:   {}\nIf the change is deliberate, rewrite the snapshots \
                 with:\n  {REGENERATE_COMMAND}",
                expected.unwrap_or("<end of the snapshot>"),
                found.unwrap_or("<end of the trace>"),
            );
        }
        if expected.is_none() {
            break;
        }
    }
    panic!(
        "the Golden Game {name} has the lines of its snapshot but differs in its line endings; \
         rewrite the snapshots with:\n  {REGENERATE_COMMAND}"
    );
}

// ---- loading

#[test]
fn the_canonical_small_ruleset_catalog_and_sample_decks_load() {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks = decks(&ruleset);
    assert_eq!(decks.len(), 2);

    let definition = ruleset.definition();
    let zones: Vec<&str> = definition.zones().iter().map(|z| z.id().as_str()).collect();
    assert_eq!(
        zones,
        [
            "deck",
            "hand",
            "field",
            "trash",
            "mana_deck",
            "mana_zone",
            "leader_zone"
        ]
    );
    let card_types: Vec<&str> = definition
        .card_types()
        .iter()
        .map(|t| t.id().as_str())
        .collect();
    assert_eq!(card_types, ["leader", "creature", "mana"]);
    let inputs: Vec<&str> = definition
        .deck_inputs()
        .iter()
        .map(|input| input.id().as_str())
        .collect();
    assert_eq!(inputs, ["leader", "main_deck", "mana_deck"]);
    assert_eq!(definition.setup().len(), 2);

    let turn = definition.turn().expect("the ruleset has a turn");
    let phase_ids: Vec<&str> = turn.phases().iter().map(|ph| ph.id().as_str()).collect();
    assert_eq!(phase_ids, ["start", "main", "end"]);
    let main = turn
        .phases()
        .iter()
        .find(|phase| phase.id() == &PhaseId::new("main"))
        .expect("the turn has a main phase");
    let PhaseKind::Actions(listed) = main.kind() else {
        panic!("main waits for actions, got {:?}", main.kind());
    };
    let listed: Vec<&str> = listed.iter().map(ActionId::as_str).collect();
    assert_eq!(listed, ["play_creature", "attack", "pass_turn"]);

    let actions: Vec<&str> = definition
        .actions()
        .iter()
        .map(|a| a.id().as_str())
        .collect();
    assert_eq!(actions, ["play_creature", "attack", "pass_turn"]);
    let win_conditions: Vec<&str> = definition
        .win_conditions()
        .iter()
        .map(|w| w.id().as_str())
        .collect();
    assert_eq!(win_conditions, ["leader_dead", "deck_out"]);

    assert!(ruleset.create_game(&decks, Seed(1)).is_ok());
}

// ---- roadmap decisions that the existing tests state only in passing

#[test]
fn the_starting_player_skips_the_draw_on_turn_1_only() {
    let ruleset = small();
    let turn_1 = opening(&ruleset, 1);
    assert_eq!(turn_1.turn_number(), Some(TurnNumber::FIRST));
    assert_eq!(turn_1.active_player(), p(0));
    // Turn 1: the opening hands are intact, and nothing was drawn.
    assert_eq!(cards_in(&turn_1, 0, "hand").len(), 5);
    assert_eq!(cards_in(&turn_1, 0, "deck").len(), 45);
    assert_eq!(cards_in(&turn_1, 1, "hand").len(), 5);

    // Turn 2: seat 1 draws.
    let turn_2 = ruleset
        .transition(&turn_1, &pass(0))
        .expect("seat 0 passes")
        .state;
    assert_eq!(turn_2.turn_number(), TurnNumber::new(2));
    assert_eq!(cards_in(&turn_2, 0, "hand").len(), 5);
    assert_eq!(cards_in(&turn_2, 1, "hand").len(), 6);

    // Turn 3: seat 0 draws, for the first time.
    let turn_3 = ruleset
        .transition(&turn_2, &pass(1))
        .expect("seat 1 passes")
        .state;
    assert_eq!(turn_3.turn_number(), TurnNumber::new(3));
    assert_eq!(cards_in(&turn_3, 0, "hand").len(), 6);
    assert_eq!(cards_in(&turn_3, 0, "deck").len(), 44);
    assert_eq!(cards_in(&turn_3, 1, "hand").len(), 6);
}

#[test]
fn pass_turn_is_taken_in_main_and_resolves_end() {
    let ruleset = small();
    let turn_1 = opening(&ruleset, 1);
    assert_eq!(turn_1.phase(), Some(&PhaseId::new("main")));

    // Seat 1's Start phase of Turn 2 draws the top of its deck and moves the top 2 of its mana
    // deck. Nothing was tapped, so nothing untaps.
    let drawn = from_top(&turn_1, 1, "deck", 0);
    let first_mana = from_top(&turn_1, 1, "mana_deck", 0);
    let second_mana = from_top(&turn_1, 1, "mana_deck", 1);
    let transition = ruleset
        .transition(&turn_1, &pass(0))
        .expect("PassTurn is taken in Main");
    assert_eq!(
        transition.trace.events(),
        [
            // The End phase of Turn 1 runs (it has no steps), then Turn 2 begins at once.
            phase_began("end"),
            Event::TurnBegan {
                player: p(1),
                turn: TurnNumber::new(2).expect("Turn 2 exists"),
            },
            phase_began("start"),
            moved(drawn, 1, "deck", "hand"),
            moved(first_mana, 1, "mana_deck", "mana_zone"),
            moved(second_mana, 1, "mana_deck", "mana_zone"),
            phase_began("main"),
        ]
    );
    // The Game waits in the next Active Player's Main.
    assert_eq!(transition.state.active_player(), p(1));
    assert_eq!(transition.state.phase(), Some(&PhaseId::new("main")));
}

// ---- the Golden Games

#[test]
fn the_deck_out_golden_game_matches_its_snapshot() {
    let ruleset = small();
    let played = play_deck_out(&ruleset, DECK_OUT_SEED);
    assert_matches_snapshot("small-deck-out", DECK_OUT_SNAPSHOT, &render(&played));

    let outcome = played.final_state.outcome().expect("the Game ended");
    assert_eq!(outcome.losses(), [deck_out_loss(1)]);
    assert_eq!(outcome.winners(), [p(0)]);
    assert_eq!(played.final_state.turn_number(), TurnNumber::new(92));
}

#[test]
fn the_leader_kill_golden_game_matches_its_snapshot() {
    let ruleset = small();
    // Every scripted Action is accepted: `play_game` panics on the first one that is not.
    let played = play_leader_kill(&ruleset, LEADER_KILL_SEED);
    assert_eq!(played.steps.len(), leader_kill_script().len());
    assert_matches_snapshot("small-leader-kill", LEADER_KILL_SNAPSHOT, &render(&played));

    let outcome = played.final_state.outcome().expect("the Game ended");
    assert_eq!(outcome.losses(), [leader_dead_loss(1)]);
    assert_eq!(outcome.winners(), [p(0)]);
    // The creatures that died went to the trash, keeping their state: `tapped` is still true.
    assert_eq!(cards_in(&played.final_state, 0, "trash"), [MOSS_0, OTTER_0]);
    for dead in [MOSS_0, OTTER_0] {
        assert_eq!(
            played
                .final_state
                .card(dead)
                .and_then(|card| card.state_value(&StateName::new("tapped"))),
            Some(&tcg_engine::ruleset_definition::FieldValue::Bool(true))
        );
    }
}

#[test]
fn a_golden_snapshot_pins_the_whole_event_trace() {
    // The same script with another Seed gives another trace, and the comparison sees it: the
    // snapshot is not compared in a way that every trace passes.
    let ruleset = small();
    let other = render(&play_deck_out(&ruleset, DECK_OUT_SEED + 1));
    assert_ne!(other, DECK_OUT_SNAPSHOT);
    let same = render(&play_deck_out(&ruleset, DECK_OUT_SEED));
    assert_eq!(same, DECK_OUT_SNAPSHOT);

    // A snapshot ends with exactly one newline, so a trace with one more or one less differs.
    assert!(DECK_OUT_SNAPSHOT.ends_with('\n') && !DECK_OUT_SNAPSHOT.ends_with("\n\n"));
    assert!(LEADER_KILL_SNAPSHOT.ends_with('\n') && !LEADER_KILL_SNAPSHOT.ends_with("\n\n"));
}

#[test]
fn the_same_seed_and_actions_give_identical_event_traces_and_final_gamestate() {
    // Two rulesets loaded independently from the text, not one cloned.
    let (first, second) = (small(), small());
    let games = [
        (
            "deck-out",
            play_deck_out(&first, DECK_OUT_SEED),
            play_deck_out(&second, DECK_OUT_SEED),
        ),
        (
            "leader-kill",
            play_leader_kill(&first, LEADER_KILL_SEED),
            play_leader_kill(&second, LEADER_KILL_SEED),
        ),
    ];
    for (name, one, two) in games {
        assert!(!one.creation.is_empty(), "{name} has a creation trace");
        assert_eq!(one.creation, two.creation, "{name}: creation traces");
        assert_eq!(one.steps, two.steps, "{name}: Action traces");
        assert_eq!(one.final_state, two.final_state, "{name}: final GameState");
        assert_eq!(one, two, "{name}: the whole Game");
    }
}

#[test]
fn every_event_kind_appears_in_a_golden_snapshot() {
    let text = format!("{DECK_OUT_SNAPSHOT}{LEADER_KILL_SNAPSHOT}");
    for variant in [
        "CardCreated {",
        "CardMoved {",
        "ZoneShuffled {",
        "TurnBegan {",
        "PhaseBegan {",
        "CardStateChanged {",
        "DrawFromEmptyZone {",
        "GameEnded {",
    ] {
        assert!(
            text.lines().any(|line| line.starts_with(variant)),
            "no snapshot line starts with `{variant}`"
        );
    }
}

#[test]
fn the_golden_snapshots_end_with_the_game_end() {
    for (name, snapshot, loss) in [
        ("deck-out", DECK_OUT_SNAPSHOT, deck_out_loss(1)),
        ("leader-kill", LEADER_KILL_SNAPSHOT, leader_dead_loss(1)),
    ] {
        let lines: Vec<&str> = snapshot.lines().collect();
        let ended = format!("{:?}", Event::GameEnded { losses: vec![loss] });
        let [.., last_event, outcome_header, _outcome] = lines.as_slice() else {
            panic!("{name} has an outcome");
        };
        assert_eq!(*last_event, ended, "{name}: GameEnded is the last Event");
        assert_eq!(*outcome_header, "== outcome");
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.starts_with("GameEnded"))
                .count(),
            1,
            "{name}: one GameEnded"
        );
    }
}

/// Rewrites the Golden Snapshots. Run it by name when an Event's shape or a rule changes on
/// purpose, and read the diff before committing it.
#[test]
#[ignore = "deliberate update: rewrites the golden snapshots"]
fn regenerate_golden_snapshots() {
    let ruleset = small();
    let deck_out = render(&play_deck_out(&ruleset, DECK_OUT_SEED));
    let leader_kill = render(&play_leader_kill(&ruleset, LEADER_KILL_SEED));
    std::fs::write(DECK_OUT_PATH, deck_out).expect("the deck-out snapshot is written");
    std::fs::write(LEADER_KILL_PATH, leader_kill).expect("the leader-kill snapshot is written");
}
