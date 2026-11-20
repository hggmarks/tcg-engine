//! Acceptance tests for ticket #75 over self-play (ADR-0033): Games that pause for an Option Choice
//! and are answered by the Uniform Random Policy, with every Invariant checked, including the three
//! over Observations, so the Relabelling of the Paused Resolution is exercised.
//!
//! The deck lists `curious_owl`, which the real catalog defines and no shipped deck lists: on
//! entering the field the controller may draw a card, which pauses the Resolution.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{Action, DeclarativeRuleset, Event};
use tcg_engine::random::Seed;
use tcg_selfplay::{self_play, verify_record, Checks, SelfPlayGame};

use crate::common::{effects, effects_deck, MAX_STEPS};

/// A Deck List for the Effects Ruleset that holds four `curious_owl`s.
const OWL_DECK: &str = "inputs:
  - input: leader
    cards:
      - { card: captain_ardel, count: 1 }
  - input: main_deck
    cards:
      - { card: curious_owl,    count: 4 }
      - { card: goblin_scout,   count: 4 }
      - { card: moss_sprite,    count: 4 }
      - { card: ember_hound,    count: 4 }
      - { card: river_otter,    count: 4 }
      - { card: stone_golem,    count: 4 }
      - { card: shadow_cat,     count: 4 }
      - { card: iron_guard,     count: 4 }
      - { card: wind_hawk,      count: 4 }
      - { card: thorn_boar,     count: 4 }
      - { card: frost_wolf,     count: 4 }
      - { card: ash_drake,      count: 4 }
      - { card: tide_serpent,   count: 2 }
  - input: mana_deck
    cards:
      - { card: mana, count: 10 }
";

/// The Seeds played: far more than the few that deal an owl early enough to be played.
const SEEDS: u64 = 40;

fn owl_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![
        effects_deck(ruleset, OWL_DECK),
        effects_deck(ruleset, OWL_DECK),
    ]
}

/// How many pauses the Event Traces of `game` hold.
fn pauses(game: &SelfPlayGame) -> usize {
    game.traces
        .iter()
        .flat_map(|trace| trace.events())
        .filter(|event| matches!(event, Event::DecisionPending { .. }))
        .count()
}

/// How many Answers the Game Record of `game` holds.
fn answers(game: &SelfPlayGame) -> usize {
    game.record
        .actions()
        .iter()
        .filter(|action| matches!(action, Action::Answer { .. }))
        .count()
}

#[test]
fn self_play_with_observation_checks_over_curious_owl() {
    let ruleset = effects();
    let decks = owl_decks(&ruleset);
    let mut total_pauses = 0;
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .unwrap_or_else(|failure| panic!("seed {seed} fails: {failure:?}"));
        // A Game ends with a Terminal Outcome, which clears any pending decision. Every pause is
        // answered by a later Action, except one whose own transition ended the Game.
        assert!(game.state.outcome().is_some(), "seed {seed}");
        assert!(game.state.pending_decision().is_none(), "seed {seed}");
        let ended_paused = game.traces.last().is_some_and(|trace| {
            trace
                .events()
                .iter()
                .any(|event| matches!(event, Event::DecisionPending { .. }))
        });
        assert_eq!(
            pauses(&game),
            answers(&game) + usize::from(ended_paused),
            "seed {seed}"
        );
        // The record, verified with every Invariant, replays the same Game.
        let replay = verify_record(&ruleset, &game.record)
            .unwrap_or_else(|failure| panic!("seed {seed} does not verify: {failure:?}"));
        assert_eq!(replay.state, game.state, "seed {seed}");
        assert_eq!(replay.traces, game.traces, "seed {seed}");
        total_pauses += pauses(&game);
    }
    // At least one Game paused, so the checks over Observations saw a Pending Decision.
    assert!(total_pauses > 0, "no Game of {SEEDS} Seeds paused");
}

#[test]
fn two_self_play_runs_over_curious_owl_from_the_same_seed_are_equal() {
    let ruleset = effects();
    let decks = owl_decks(&ruleset);
    for seed in 0..SEEDS {
        let play = || self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On);
        assert_eq!(play(), play(), "seed {seed}");
    }
}
