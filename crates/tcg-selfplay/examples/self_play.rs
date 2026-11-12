//! Plays one seeded Game of the Small Ruleset, prints a readable Game log, then reports the
//! throughput of self-play in Games per second, with the invariant checks on and off.
//!
//! Run it in release mode for the throughput figures:
//!
//! ```text
//! cargo run --release -p tcg-selfplay --example self_play
//! ```

use std::time::Instant;

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{Argument, DeclarativeRuleset, GameState};
use tcg_engine::random::Seed;
use tcg_engine::ruleset::Ruleset;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};
use tcg_selfplay::{self_play, Checks};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// The Seed of the Game that is printed.
const LOG_SEED: u64 = 2026;
/// The Step Cap of every Game of the example.
const MAX_STEPS: usize = 1_000;
/// How many Games each throughput measurement plays: the Seeds `0..THROUGHPUT_GAMES`.
const THROUGHPUT_GAMES: u64 = 1_000;

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

    print_game_log(&ruleset, &decks);

    for (label, checks) in [
        ("invariant checks on ", Checks::On),
        ("invariant checks off", Checks::Off),
    ] {
        let (games, actions, longest, seconds) = measure(&ruleset, &decks, checks);
        println!(
            "{label}: {games} Games, {actions} Actions (longest {longest}) in {seconds:.3} s = {:.0} Games/s, {:.0} Actions/s",
            rate(games, seconds),
            rate(actions, seconds),
        );
    }
}

/// Plays `LOG_SEED` with the checks on and prints it: a header per Turn, one line per Action with
/// the Player, the action id and the Card Id of each argument, then the Terminal Outcome.
fn print_game_log(ruleset: &DeclarativeRuleset, decks: &[DeckList]) {
    let game = match self_play(ruleset, decks, Seed(LOG_SEED), MAX_STEPS, Checks::On) {
        Ok(game) => game,
        Err(failure) => {
            println!(
                "seed {} failed after Action index {:?}: {:?}",
                failure.seed.0, failure.action_index, failure.kind
            );
            return;
        }
    };
    println!(
        "Small Ruleset, seed {LOG_SEED}, {} Actions",
        game.record.actions().len()
    );
    let mut state = game.creation.opening.state.clone();
    print_turn_header(&state);
    for (index, action) in game.record.actions().iter().enumerate() {
        let cards: Vec<String> = action
            .arguments
            .iter()
            .map(|argument| {
                let Argument::Card(id) = argument;
                match state.card(*id) {
                    Some(card) => format!("{} (#{})", card.definition().as_str(), id.0),
                    None => format!("#{}", id.0),
                }
            })
            .collect();
        println!(
            "  {index:>3}. Player {} {}{}",
            action.player.0,
            action.action.as_str(),
            if cards.is_empty() {
                String::new()
            } else {
                format!(" {}", cards.join(", "))
            }
        );
        let turn_before = state.turn_number();
        state = match ruleset.transition(&state, action) {
            Ok(transition) => transition.state,
            Err(rejection) => {
                println!("  the recorded Action was rejected: {rejection}");
                return;
            }
        };
        if state.turn_number() != turn_before {
            print_turn_header(&state);
        }
    }
    match state.outcome() {
        Some(outcome) => {
            for loss in outcome.losses() {
                println!(
                    "Player {} lost by `{}`",
                    loss.player.0,
                    loss.reason.as_str()
                );
            }
            let winners: Vec<String> = outcome
                .winners()
                .iter()
                .map(|player| player.0.to_string())
                .collect();
            if winners.is_empty() {
                println!("Terminal Outcome: a drawn game");
            } else {
                println!("Terminal Outcome: Player {} won", winners.join(", "));
            }
        }
        None => println!("The Game has no Terminal Outcome"),
    }
    println!();
}

fn print_turn_header(state: &GameState) {
    if let Some(turn) = state.turn_number() {
        println!("Turn {} (Player {})", turn.get(), state.active_player().0);
    }
}

/// Plays Seeds `0..THROUGHPUT_GAMES` and returns the number of Games, the number of Actions, the
/// length of the longest Game and the wall time in seconds. Every Game must end well.
#[expect(
    clippy::disallowed_methods,
    reason = "wall-clock time is the point of a throughput measurement; it is not engine code and never enters a GameState"
)]
fn measure(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    checks: Checks,
) -> (u64, u64, usize, f64) {
    let started = Instant::now();
    let mut actions = 0_u64;
    let mut longest = 0;
    for seed in 0..THROUGHPUT_GAMES {
        match self_play(ruleset, decks, Seed(seed), MAX_STEPS, checks) {
            Ok(game) => {
                let length = game.record.actions().len();
                longest = longest.max(length);
                actions += u64::try_from(length).unwrap_or(u64::MAX);
            }
            Err(failure) => panic!(
                "seed {} failed after Action index {:?}: {:?}",
                failure.seed.0, failure.action_index, failure.kind
            ),
        }
    }
    (
        THROUGHPUT_GAMES,
        actions,
        longest,
        started.elapsed().as_secs_f64(),
    )
}

/// `count` per second.
#[expect(
    clippy::float_arithmetic,
    reason = "a throughput figure is a ratio printed for people; it is not engine code and never enters a GameState"
)]
fn rate(count: u64, seconds: f64) -> f64 {
    count as f64 / seconds
}
