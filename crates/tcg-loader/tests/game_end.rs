//! Acceptance tests for ticket #23: the Small Ruleset's `win_conditions`, read from the real
//! ruleset file, and a Game that ends by deck-out over many Turns with the sample catalog and
//! decks.
//!
//! The file is read as in `turn_cycle.rs`: up to its top-level `actions:` section with Main waiting
//! for `pass_turn` only, then a `pass_turn` definition, then the file's own `win_conditions`
//! section. `leader_dead` is an expression, which ticket [11] adds, so most tests cut it. Code-built
//! win conditions, the Terminal Outcome and `apply` are tested in the engine crate
//! (`tests/win_conditions.rs`).

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, CardInstanceId, DeclarativeRuleset, Event, GameState, Loss, Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, DefinitionError, DefinitionErrorKind, Loser, PhaseId, RulesetDefinition,
    WinConditionId, WinConditionWhen, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, LoadError, YamlError, YamlErrorKind,
};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");
const STRUCTURE: &str = include_str!("fixtures/small-ruleset-structure.yaml");

// ---- helpers

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

/// The file up to its `actions:` section, with Main waiting for `pass_turn` only, followed by a
/// `pass_turn` definition.
fn turn_and_actions() -> String {
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
    format!(
        "{main_passes_only}\nactions:\n  - id: pass_turn\n    effects: [ {{ end_turn: {{}} }} ]\n"
    )
}

/// The file's own `win_conditions` section, from its key to the end of the file.
fn own_win_conditions() -> &'static str {
    let (_, section) = SMALL_RULESET
        .split_once("\nwin_conditions:")
        .expect("the Small Ruleset has a win_conditions section");
    section
}

/// The same section without `leader_dead`: only `deck_out`, as the file writes it.
fn deck_out_only() -> String {
    let (_, deck_out) = own_win_conditions()
        .split_once("  - id: deck_out")
        .expect("the section has deck_out");
    format!("\nwin_conditions:\n  - id: deck_out{deck_out}")
}

fn definition() -> RulesetDefinition {
    load_ruleset_definition(&format!("{}{}", turn_and_actions(), deck_out_only()))
        .expect("the Small Ruleset without leader_dead loads")
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

fn deck_out_loss(seat: u32) -> Loss {
    Loss {
        player: p(seat),
        reason: WinConditionId::new("deck_out"),
    }
}

/// The only place this file builds an Action.
fn action(player: u32, id: &str) -> Action {
    Action {
        player: p(player),
        action: ActionId::new(id),
    }
}

fn pass(player: u32) -> Action {
    action(player, "pass_turn")
}

fn pass_turn(ruleset: &DeclarativeRuleset, state: &GameState) -> Transition<GameState, Event> {
    ruleset
        .transition(state, &pass(state.active_player().0))
        .expect("PassTurn is accepted")
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &ZoneId::new(zone_id))
        .expect("the zone exists")
        .to_vec()
}

fn yaml_error(yaml: &str) -> YamlError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Yaml(error)) => error,
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

fn definition_error(yaml: &str) -> DefinitionError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Definition(error)) => error,
        other => panic!("expected a definition error, got {other:?}"),
    }
}

/// Plays a Game of the Small Ruleset in which both Players only pass, up to and including the
/// PassTurn that ends it. Returns the Game as created, then every state and Event Trace after it.
fn play_to_the_end() -> (
    DeclarativeRuleset,
    Transition<GameState, Event>,
    Vec<Transition<GameState, Event>>,
) {
    let ruleset = ruleset();
    let opening = ruleset
        .create_game(&decks(&ruleset), Seed(1))
        .expect("the sample decks create a game")
        .opening;
    let mut transitions = Vec::new();
    let mut state = opening.state.clone();
    // 91 PassTurns: see `a_game_where_both_players_only_pass_ends_by_deck_out_of_player_1_on_turn_92`.
    for _ in 0..91 {
        let transition = pass_turn(&ruleset, &state);
        state = transition.state.clone();
        transitions.push(transition);
    }
    (ruleset, opening, transitions)
}

// ---- the Small Ruleset's win conditions

#[test]
fn deck_out_win_condition_loads_from_the_small_ruleset() {
    let definition = definition();
    let conditions = definition.win_conditions();
    assert_eq!(conditions.len(), 1);
    assert_eq!(conditions[0].id(), &WinConditionId::new("deck_out"));
    assert_eq!(
        conditions[0].when(),
        &WinConditionWhen::DrawFromEmpty(ZoneId::new("deck"))
    );
    assert_eq!(conditions[0].loser(), Loser::Owner);
}

#[test]
fn leader_dead_win_condition_is_unsupported() {
    // The file's own section: `leader_dead` is first, and it is an expression ([11]).
    let error = definition_error(&format!(
        "{}\nwin_conditions:{}",
        turn_and_actions(),
        own_win_conditions()
    ));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("win_conditions").id("leader_dead").key("when"),
            kind: DefinitionErrorKind::UnsupportedWinCondition {
                operator: "lte".to_string()
            },
        }
    );
}

#[test]
fn unknown_win_condition_key_is_an_unknown_key() {
    // An unknown key of a win condition.
    let error = yaml_error(&format!(
        "{STRUCTURE}win_conditions:\n  - {{ id: x, when: {{ draw_from_empty: deck }}, loser: owner, winner: other }}\n"
    ));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "winner".to_string()
        }
    );
    assert_eq!(error.path, path().key("win_conditions").index(0));

    // An unknown condition primitive is an unknown key of the `when` map, like an unknown step
    // primitive is of its list element.
    let error = yaml_error(&format!(
        "{STRUCTURE}win_conditions:\n  - {{ id: x, when: {{ hand_is_full: deck }}, loser: owner }}\n"
    ));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "hand_is_full".to_string()
        }
    );
    assert_eq!(
        error.path,
        path().key("win_conditions").index(0).key("when")
    );
}

#[test]
fn an_absent_or_empty_win_conditions_section_means_the_game_never_ends() {
    let absent = load_ruleset_definition(&turn_and_actions()).expect("the file loads");
    assert!(absent.win_conditions().is_empty());
    let empty = load_ruleset_definition(&format!("{}\nwin_conditions: []\n", turn_and_actions()))
        .expect("the file loads");
    assert!(empty.win_conditions().is_empty());
}

// ---- scenarios

#[test]
fn a_game_where_both_players_only_pass_ends_by_deck_out_of_player_1_on_turn_92() {
    // Each Player starts with 50 cards in the deck and draws 5 in setup: 45 are left.
    // Seat 0 skips the draw on Turn 1 and then draws on its Turns, the odd ones from 3 on:
    // 3, 5, ..., 91 are 45 draws, so its deck is empty after Turn 91 but it never has to draw from
    // an empty deck before Turn 93.
    // Seat 1 draws on every one of its Turns, the even ones from 2 on: 2, 4, ..., 90 are 45 draws,
    // so its deck is empty after Turn 90 and the draw of Turn 92 finds it empty.
    // Turn n starts after n - 1 PassTurns, so Turn 92 starts with the 91st.
    let ruleset = ruleset();
    let creation = ruleset
        .create_game(&decks(&ruleset), Seed(1))
        .expect("the sample decks create a game");
    let mut state = creation.opening.state;
    assert_eq!(state.outcome(), None);
    assert_eq!(cards_in(&state, 0, "deck").len(), 45);
    assert_eq!(cards_in(&state, 1, "deck").len(), 45);

    for turn in 2..=91 {
        let transition = pass_turn(&ruleset, &state);
        state = transition.state;
        assert_eq!(state.turn_number(), TurnNumber::new(turn));
        assert_eq!(state.outcome(), None, "the Game is on at Turn {turn}");
    }
    // 90 PassTurns have been accepted. Seat 0's mana deck has been empty since its fifth Turn
    // (Turn 9; nothing moved from it from Turn 11 on), and that does not end the Game; seat 0's
    // deck is empty too, but seat 0 has not had to draw from it.
    assert!(cards_in(&state, 0, "mana_deck").is_empty());
    assert!(cards_in(&state, 0, "deck").is_empty());
    assert_eq!(cards_in(&state, 1, "deck").len(), 0);
    assert_eq!(state.active_player(), p(0));

    // The 91st PassTurn begins Turn 92 for seat 1, whose Start phase draws from an empty deck.
    let before = state.clone();
    let transition = pass_turn(&ruleset, &state);
    let events = transition.trace.events();
    assert!(events.contains(&Event::DrawFromEmptyZone {
        player: p(1),
        zone: ZoneId::new("deck"),
    }));
    // Everything the transition runs still runs: `main` began before the Game ended, and
    // `GameEnded` is the last Event.
    assert_eq!(
        events.last(),
        Some(&Event::GameEnded {
            losses: vec![deck_out_loss(1)]
        })
    );
    assert_eq!(
        events.get(events.len().saturating_sub(2)),
        Some(&Event::PhaseBegan {
            phase: PhaseId::new("main")
        })
    );
    let ended = events
        .iter()
        .filter(|event| matches!(event, Event::GameEnded { .. }))
        .count();
    assert_eq!(ended, 1);

    let next = transition.state;
    assert_eq!(next.turn_number(), TurnNumber::new(92));
    assert_eq!(next.active_player(), p(1));
    let outcome = next.outcome().expect("the Game ended");
    assert_eq!(outcome.winners(), [p(0)]);
    assert_eq!(outcome.losses(), [deck_out_loss(1)]);
    assert!(!outcome.is_drawn_game());

    // Applying the trace to the previous state reproduces the next one.
    assert_eq!(before.apply_trace(events), Ok(next));
}

#[test]
fn every_action_after_the_game_ended_is_rejected_with_game_over() {
    let (ruleset, _, transitions) = play_to_the_end();
    let ended = transitions
        .last()
        .map(|transition| transition.state.clone())
        .expect("the Game was played");
    assert!(ended.outcome().is_some());

    for (what, rejected) in [
        ("PassTurn by seat 1, the Active Player", pass(1)),
        ("PassTurn by seat 0", pass(0)),
        ("PassTurn by a Player who is not in the Game", pass(9)),
        (
            "an action the ruleset does not declare",
            action(1, "concede"),
        ),
    ] {
        let before = ended.clone();
        assert_eq!(
            ruleset.transition(&ended, &rejected),
            Err(Rejection::GameOver),
            "{what}"
        );
        assert_eq!(ended, before, "{what} left the state unchanged");
    }
}

#[test]
fn a_game_that_ends_by_deck_out_is_deterministic() {
    let run = || {
        let (_, opening, transitions) = play_to_the_end();
        let mut all = vec![opening];
        all.extend(transitions);
        all
    };
    let first = run();
    let second = run();
    assert_eq!(first.len(), 92);
    // Every GameState and every Event Trace of the two runs is equal.
    assert_eq!(first, second);
}
