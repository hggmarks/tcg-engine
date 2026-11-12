//! Where the time of self-play goes (ticket #46): a benchmark of self-play over a fixed range of
//! Seeds of the Small Ruleset, and a profile that separates listing the Legal Actions from running
//! the chosen Action's transition, and the clone of the GameState from the rest of each.
//!
//! The constants are fixed, so a run is reproducible: the Seeds are `0..GAMES`, the Step Cap is
//! `MAX_STEPS`. Run it in both profiles:
//!
//! ```text
//! cargo run --release -p tcg-selfplay --example listing_cost
//! cargo run -p tcg-selfplay --example listing_cost
//! ```
//!
//! Every figure is integer arithmetic on nanoseconds: nothing here is floating point.

use std::hint::black_box;
use std::num::NonZeroU64;
use std::time::Instant;

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::DeclarativeRuleset;
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset::{LegalActions, Ruleset};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};
use tcg_selfplay::{self_play, Checks, POLICY_STREAM};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// How many Games each measurement plays: the Seeds `0..GAMES`.
const GAMES: u64 = 1_000;
/// The Step Cap of every Game.
const MAX_STEPS: usize = 1_000;
/// How many times the profile clones the GameState at each step, so that the mean of a clone
/// is stable; the total is divided back.
const CLONES_PER_STEP: u32 = 20;

const NANOS_PER_SECOND: u128 = 1_000_000_000;

fn main() {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks: Vec<DeckList> = [DECK_A, DECK_B]
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect();

    println!(
        "profile: {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!("Small Ruleset, Seeds 0..{GAMES}, Step Cap {MAX_STEPS}");
    println!();
    println!("A. Benchmark: self_play");
    let mut benchmark_actions = 0;
    for (label, checks) in [("checks off", Checks::Off), ("checks on ", Checks::On)] {
        let (actions, nanos) = benchmark(&ruleset, &decks, checks);
        benchmark_actions = actions;
        println!(
            "  {label}: {GAMES} Games, {actions} Actions, {} s, {} Games/s, {} Actions/s",
            seconds(nanos),
            rate(u128::from(GAMES), nanos),
            rate(u128::from(actions), nanos),
        );
    }
    println!();
    profile(&ruleset, &decks, benchmark_actions);
}

/// Plays the Seeds `0..GAMES` and returns the number of Actions and the wall time in nanoseconds.
/// Every Game must end well.
#[expect(
    clippy::disallowed_methods,
    reason = "wall-clock time is the point of a benchmark; it is not engine code and never enters a GameState"
)]
fn benchmark(ruleset: &DeclarativeRuleset, decks: &[DeckList], checks: Checks) -> (u64, u128) {
    let started = Instant::now();
    let mut actions = 0_u64;
    for seed in 0..GAMES {
        match self_play(ruleset, decks, Seed(seed), MAX_STEPS, checks) {
            Ok(game) => actions += u64::try_from(game.record.actions().len()).unwrap_or(u64::MAX),
            Err(failure) => panic!(
                "seed {} failed after Action index {:?}: {:?}",
                failure.seed.0, failure.action_index, failure.kind
            ),
        }
    }
    (actions, started.elapsed().as_nanos())
}

/// The time of `work`, and what it returned.
#[expect(
    clippy::disallowed_methods,
    reason = "wall-clock time is the point of a profile; it is not engine code and never enters a GameState"
)]
fn timed<T>(work: impl FnOnce() -> T) -> (T, u128) {
    let started = Instant::now();
    let result = work();
    (result, started.elapsed().as_nanos())
}

/// What the profile accumulates over every step of every Game.
#[derive(Default)]
struct Profile {
    steps: u64,
    listed: u64,
    listing_nanos: u128,
    transition_nanos: u128,
    clone_nanos: u128,
    clones: u128,
}

/// Plays the same Games as part A with the same policy as the harness, by hand through the public
/// API, timing at every step: listing, the chosen Action's transition, and the clone alone.
fn profile(ruleset: &DeclarativeRuleset, decks: &[DeckList], expected_actions: u64) {
    let mut total = Profile::default();
    for seed in 0..GAMES {
        let created = ruleset
            .create_game(decks, Seed(seed))
            .expect("the Game is created");
        let mut policy = RandomState::from_seed(Seed(seed ^ POLICY_STREAM));
        let mut state = created.opening.state;
        while state.outcome().is_none() {
            assert!(
                total.steps <= u64::try_from(MAX_STEPS).unwrap_or(u64::MAX) * GAMES,
                "the Step Cap"
            );
            let (legal, listing) = timed(|| black_box(ruleset.legal_actions(black_box(&state))));
            let bound = NonZeroU64::new(u64::try_from(legal.len()).expect("a small listing"))
                .expect("an unfinished Game has a Legal Action");
            let index = usize::try_from(policy.next_below(bound)).expect("a small index");
            let chosen = legal.get(index).expect("the index is below the length");

            let (transition, execution) =
                timed(|| ruleset.transition(black_box(&state), black_box(chosen)));
            let transition = transition.expect("a Legal Action is accepted");

            let ((), cloning) = timed(|| {
                for _ in 0..CLONES_PER_STEP {
                    black_box(black_box(&state).clone());
                }
            });

            total.steps += 1;
            total.listed += u64::try_from(legal.len()).unwrap_or(u64::MAX);
            total.listing_nanos += listing;
            total.transition_nanos += execution;
            total.clone_nanos += cloning;
            total.clones += u128::from(CLONES_PER_STEP);
            state = transition.state;
        }
    }
    assert_eq!(
        total.steps, expected_actions,
        "the profile plays the same Games as part A"
    );
    print_profile(&total);
}

fn print_profile(total: &Profile) {
    let steps = u128::from(total.steps);
    let listed = u128::from(total.listed);
    // The mean cost of one clone, in picoseconds so that integer division keeps three digits.
    let clone_ps = total.clone_nanos * 1_000 / total.clones.max(1);
    let clone_part = |clones: u128| clones * clone_ps / 1_000;

    let measured = total.listing_nanos + total.transition_nanos;
    println!("B. Profile: {steps} steps (one per accepted Action), {listed} listed Actions");
    println!(
        "  listed Actions per step: {} (mean)",
        tenths(listed * 10 / steps.max(1))
    );
    println!(
        "  listing vs the chosen transition (clone timing and the timers' own overhead excluded from both):"
    );
    println!(
        "    listing:             {:>10} ms ({} % of the two)",
        total.listing_nanos / 1_000_000,
        tenths(total.listing_nanos * 1_000 / measured.max(1))
    );
    println!(
        "    chosen transition:   {:>10} ms ({} % of the two)",
        total.transition_nanos / 1_000_000,
        tenths(total.transition_nanos * 1_000 / measured.max(1))
    );
    println!(
        "    mean per step:       listing {} us, chosen transition {} us",
        tenths(total.listing_nanos * 10 / steps.max(1) / 1_000),
        tenths(total.transition_nanos * 10 / steps.max(1) / 1_000)
    );
    println!(
        "  mean cost of one clone of the GameState: {clone_ps} ps = {} us",
        { tenths(clone_ps / 100_000) }
    );

    let chosen_clone = clone_part(steps);
    println!("  chosen transition = 1 clone + the rest:");
    println!(
        "    clone:               {:>10} ms ({} % of the transition)",
        chosen_clone / 1_000_000,
        tenths(chosen_clone * 1_000 / total.transition_nanos.max(1))
    );
    println!(
        "    execution:           {:>10} ms ({} % of the transition)",
        total.transition_nanos.saturating_sub(chosen_clone) / 1_000_000,
        tenths(
            total.transition_nanos.saturating_sub(chosen_clone) * 1_000
                / total.transition_nanos.max(1)
        )
    );

    // The number of clones in a listing is the number of candidates that pass argument binding,
    // which the public API does not show; the number of listed Actions is a lower bound.
    let listing_clone = clone_part(listed);
    println!("  listing = (at least) listed Actions x clone + the rest (executions, candidate building):");
    println!(
        "    clones (lower bound = listed Actions): {listed}, {} ms ({} % of the listing)",
        listing_clone / 1_000_000,
        tenths(listing_clone * 1_000 / total.listing_nanos.max(1))
    );
    println!(
        "    the rest:            {:>10} ms ({} % of the listing)",
        total.listing_nanos.saturating_sub(listing_clone) / 1_000_000,
        tenths(
            total.listing_nanos.saturating_sub(listing_clone) * 1_000 / total.listing_nanos.max(1)
        )
    );
    println!(
        "  clones of the GameState per step: about {} in the listing (bound), 1 in the chosen transition",
        tenths(listed * 10 / steps.max(1))
    );
    println!(
        "  timer overhead: two clock reads around each of the three measured calls per step; \
         the clone is timed apart from the transition, never inside it; each clone is dropped inside the timed loop"
    );
}

/// `nanos` as seconds with three decimals.
fn seconds(nanos: u128) -> String {
    let millis = nanos / 1_000_000;
    format!("{}.{:03}", millis / 1_000, millis % 1_000)
}

/// `count` per second, with one decimal.
fn rate(count: u128, nanos: u128) -> String {
    tenths(count * NANOS_PER_SECOND * 10 / nanos.max(1))
}

/// A value in tenths, written with one decimal.
fn tenths(value: u128) -> String {
    format!("{}.{}", value / 10, value % 10)
}
