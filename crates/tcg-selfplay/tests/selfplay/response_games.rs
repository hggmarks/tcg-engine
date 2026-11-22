//! The Response Ruleset in self-play (ticket #85, ADR-0037): it loads, its sample decks hold the
//! fast cards, and seeded Games on it end with every Invariant checked, Passes, pushes and
//! responses included. The coverage counts and the pin of its Games are ticket [36]'s.
//!
//! Everything runs on the real Response Ruleset, catalog and sample decks.

use tcg_engine::declarative::{Action, Event, StackSource};
use tcg_engine::random::Seed;
use tcg_engine::ruleset_definition::ActionId;
use tcg_selfplay::{self_play, verify_record, Checks};

use crate::common::{
    decks, effects, effects_decks, response, response_decks, small, MAX_STEPS, RESPONSE_DECK_A,
    RESPONSE_DECK_B,
};

/// How many Seeds the Response Games are played for.
const SEEDS: u64 = 100;

#[test]
fn the_response_ruleset_loads_and_self_plays_with_checks_on() {
    let ruleset = response();
    let decks = response_decks(&ruleset);

    // The shipped decks hold the new fast cards, so that Games exercise the window.
    for deck in [RESPONSE_DECK_A, RESPONSE_DECK_B] {
        assert!(deck.contains("swift_sparrow") && deck.contains("flash_fox"));
    }

    let (mut passes, mut pushes, mut responses) = (0, 0, 0);
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .unwrap_or_else(|failure| panic!("seed {seed}: {:?}", failure.kind));
        assert!(game.state.outcome().is_some(), "seed {seed}: the Game ends");
        assert!(
            game.state.stack().is_empty(),
            "seed {seed}: the Stack is clear at the end"
        );
        passes += game
            .record
            .actions()
            .iter()
            .filter(|action| matches!(action, Action::Pass { .. }))
            .count();
        for event in game.traces.iter().flat_map(|trace| trace.events()) {
            match event {
                Event::ItemPushed { item } => {
                    pushes += 1;
                    if item.source == StackSource::Action(ActionId::new("fast_play")) {
                        responses += 1;
                    }
                }
                _ => {}
            }
        }
        // The record verifies, with every Invariant checked at each step.
        verify_record(&ruleset, &game.record)
            .unwrap_or_else(|failure| panic!("seed {seed}: {:?}", failure.kind));
    }
    assert!(passes >= 1, "no Game holds a Pass");
    assert!(pushes >= 1, "no Game pushes an item");
    assert!(
        responses >= 1,
        "no Game holds a response: the decks exercise no window"
    );
}

#[test]
fn self_play_with_checks_on_stays_green_on_all_three_rulesets() {
    let (small_ruleset, effects_ruleset, response_ruleset) = (small(), effects(), response());
    for seed in 0..5 {
        self_play(
            &small_ruleset,
            &decks(&small_ruleset),
            Seed(seed),
            MAX_STEPS,
            Checks::On,
        )
        .expect("the Small Ruleset plays");
        self_play(
            &effects_ruleset,
            &effects_decks(&effects_ruleset),
            Seed(seed),
            MAX_STEPS,
            Checks::On,
        )
        .expect("the Effects Ruleset plays");
        self_play(
            &response_ruleset,
            &response_decks(&response_ruleset),
            Seed(seed),
            MAX_STEPS,
            Checks::On,
        )
        .expect("the Response Ruleset plays");
    }
}

#[test]
fn the_response_ruleset_self_plays_the_same_game_from_the_same_seed() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    let first = self_play(&ruleset, &decks, Seed(3), MAX_STEPS, Checks::On).expect("played");
    let second = self_play(&ruleset, &decks, Seed(3), MAX_STEPS, Checks::On).expect("played");
    assert!(first == second, "the whole Game");
    // The checks only read: switching them off changes nothing.
    let unchecked = self_play(&ruleset, &decks, Seed(3), MAX_STEPS, Checks::Off).expect("played");
    assert!(first.record == unchecked.record && first.traces == unchecked.traces);
}
