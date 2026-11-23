//! The correctness net over the Duel Ruleset v0 (ticket #96, ADR-0041): its sample decks are legal
//! through the engine's Deck List validation, and Games over Seeds 0..100 are played to a Terminal
//! Outcome with every Invariant checked (`Checks::On`: the twelve Invariants, the leak check
//! included), deterministically from the same Seed, and replay exactly. The 1,000-Seed net, its pin
//! and the Golden Game are ticket [43]'s.
//!
//! Everything runs on the real Duel Ruleset, catalog and sample decks.

use tcg_engine::random::Seed;
use tcg_selfplay::{self_play, Checks, FailureKind};

use crate::common::{duel, duel_decks, edit, DUEL_DECK_A, DUEL_DECK_B, MAX_STEPS};
use tcg_loader::load_deck_list;

/// The Seeds played with every Invariant: `0..SEEDS`.
const SEEDS: u64 = 100;

#[test]
fn the_duel_sample_decks_are_legal() {
    let ruleset = duel();
    for (name, yaml) in [("A", DUEL_DECK_A), ("B", DUEL_DECK_B)] {
        // The engine's validation accepts the deck: the input exists, every card is a monster, the
        // 40 cards have at most 3 copies each.
        let deck = load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
            .unwrap_or_else(|error| panic!("deck {name} is not legal: {error:?}"));
        assert_eq!(deck.inputs().len(), 1);
    }

    // The same validation refuses a deck that is not legal, so the check above means something: a
    // fourth copy of a card, and a deck of 39 cards.
    let four_copies = edit(
        DUEL_DECK_A,
        "{ card: lantern_moth, count: 3 }",
        "{ card: lantern_moth, count: 4 }",
    );
    assert!(load_deck_list(ruleset.definition(), ruleset.catalog(), &four_copies).is_err());
    let short = edit(
        DUEL_DECK_A,
        "{ card: lantern_moth, count: 3 }",
        "{ card: lantern_moth, count: 2 }",
    );
    assert!(load_deck_list(ruleset.definition(), ruleset.catalog(), &short).is_err());
}

#[test]
fn duel_self_play_ends_every_game_over_seeds_0_to_100_with_every_invariant_holding() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    let (mut life_out, mut deck_out, mut drawn) = (0usize, 0usize, 0usize);
    let mut longest = 0usize;
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).unwrap_or_else(
            |failure| {
                let reason = match &failure.kind {
                    FailureKind::InvariantViolated(violations) => format!("{violations:?}"),
                    other => format!("{other:?}"),
                };
                panic!(
                    "Seed {seed} failed after {:?} Actions: {reason}",
                    failure.action_index
                )
            },
        );
        let outcome = game
            .state
            .outcome()
            .unwrap_or_else(|| panic!("Seed {seed} has no Terminal Outcome"));
        longest = longest.max(game.record.actions().len());
        if outcome.is_drawn_game() {
            drawn += 1;
        }
        for loss in outcome.losses() {
            match loss.reason.as_str() {
                "life_out" => life_out += 1,
                "deck_out" => deck_out += 1,
                other => panic!("Seed {seed}: unknown reason `{other}`"),
            }
        }
    }
    // Reported, not asserted: the Losses by reason (a drawn Game counts both), the drawn Games and
    // the longest Game, in Actions.
    println!(
        "duel self-play, Seeds 0..{SEEDS}: {life_out} life_out losses, {deck_out} deck_out losses, {drawn} drawn Games, longest {longest} Actions"
    );
}

#[test]
fn duel_self_play_is_deterministic_from_the_same_seed() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    for seed in [0, 1, 7, 42] {
        let play = || {
            self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
                .expect("the Game is played")
        };
        let (first, second) = (play(), play());
        assert_eq!(first.record, second.record, "Seed {seed}: the record");
        assert_eq!(first.creation, second.creation, "Seed {seed}: the creation");
        assert_eq!(
            first.traces, second.traces,
            "Seed {seed}: every Event Trace"
        );
        assert_eq!(
            first.state, second.state,
            "Seed {seed}: the final GameState"
        );
    }

    // Another Seed plays another Game.
    let one = self_play(&ruleset, &decks, Seed(0), MAX_STEPS, Checks::On).expect("played");
    let other = self_play(&ruleset, &decks, Seed(1), MAX_STEPS, Checks::On).expect("played");
    assert_ne!(one.record.actions(), other.record.actions());
}

#[test]
fn replay_reproduces_a_duel_game() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    for seed in [0, 3, 11] {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        let replay = ruleset
            .replay(&game.record)
            .expect("the record of a self-played Game replays");
        assert_eq!(replay.creation, game.creation, "Seed {seed}: the creation");
        assert_eq!(replay.traces, game.traces, "Seed {seed}: every Event Trace");
        assert_eq!(replay.state, game.state, "Seed {seed}: the final GameState");
        assert!(
            replay.state.outcome().is_some(),
            "Seed {seed}: the Terminal Outcome is reproduced"
        );
        assert_eq!(replay.state.outcome(), game.state.outcome());
    }
}
