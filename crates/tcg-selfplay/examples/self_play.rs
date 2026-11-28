//! Plays one seeded Game of the Small, Effects, Response and Duel Rulesets, prints a readable
//! Game log of each, then reports the throughput of self-play in Games per second on all four
//! rulesets, with the checks off, with the nine engine Invariants only, and with all twelve (the
//! three over Observations too): the difference is the cost of observing (ADR-0031).
//!
//! The log has one line per Action and, under it, one line per Event the Action caused. A trigger
//! resolution is visible there: the Events of a triggered effect (a draw, a move) come after the
//! Events of the Action that woke it. The comparison of the throughput of the two rulesets is the
//! cost of the derived reads of Card Effects (ADR-0024).
//!
//! Run it in release mode for the throughput figures:
//!
//! ```text
//! cargo run --release -p tcg-selfplay --example self_play
//! ```

use std::time::Instant;

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{Action, Answer, Argument, DeclarativeRuleset, Event, GameState};
use tcg_engine::random::Seed;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};
use tcg_selfplay::{self_play, Checks, SelfPlayGame};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const SMALL_DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const SMALL_DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");
const EFFECTS_DECK_A: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-a.yaml");
const EFFECTS_DECK_B: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-b.yaml");

const RESPONSE_RULESET: &str =
    include_str!("../../../rulesets/response-ruleset/response-ruleset.yaml");
const RESPONSE_CATALOG: &str =
    include_str!("../../../rulesets/response-ruleset/response-catalog.yaml");
const RESPONSE_DECK_A: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-a.yaml");
const RESPONSE_DECK_B: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-b.yaml");

const DUEL_RULESET: &str = include_str!("../../../rulesets/duel-ruleset/duel-ruleset.yaml");
const DUEL_CATALOG: &str = include_str!("../../../rulesets/duel-ruleset/duel-catalog.yaml");
const DUEL_DECK_A: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-a.yaml");
const DUEL_DECK_B: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-b.yaml");

/// The Seed of the Games that are printed.
const LOG_SEED: u64 = 2026;
/// The Step Cap of every Game of the example.
const MAX_STEPS: usize = 1_000;
/// How many Games each throughput measurement plays: the Seeds `0..THROUGHPUT_GAMES`.
const THROUGHPUT_GAMES: u64 = 1_000;

/// A ruleset of the example, loaded, with its sample decks.
struct Loaded {
    name: &'static str,
    ruleset: DeclarativeRuleset,
    decks: Vec<DeckList>,
}

fn load(name: &'static str, ruleset: &str, catalog: &str, decks: [&str; 2]) -> Loaded {
    let definition = load_ruleset_definition(ruleset).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks = decks
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect();
    Loaded {
        name,
        ruleset,
        decks,
    }
}

fn main() {
    let rulesets = [
        load(
            "Small Ruleset",
            SMALL_RULESET,
            SMALL_CATALOG,
            [SMALL_DECK_A, SMALL_DECK_B],
        ),
        load(
            "Effects Ruleset",
            EFFECTS_RULESET,
            EFFECTS_CATALOG,
            [EFFECTS_DECK_A, EFFECTS_DECK_B],
        ),
        load(
            "Response Ruleset",
            RESPONSE_RULESET,
            RESPONSE_CATALOG,
            [RESPONSE_DECK_A, RESPONSE_DECK_B],
        ),
        load(
            "Duel Ruleset",
            DUEL_RULESET,
            DUEL_CATALOG,
            [DUEL_DECK_A, DUEL_DECK_B],
        ),
    ];

    for loaded in &rulesets {
        print_game_log(loaded);
    }

    for loaded in &rulesets {
        for (label, checks) in [
            ("checks off", Checks::Off),
            ("engine checks only", Checks::EngineOnly),
            ("all checks", Checks::On),
        ] {
            let measured = measure(loaded, checks);
            println!(
                "{:<15} {label:<18}: {} Games, {} Actions (longest {}) in {:.3} s = {:.0} Games/s, {:.0} Actions/s",
                loaded.name,
                measured.games,
                measured.actions,
                measured.longest,
                measured.seconds,
                rate(measured.games, measured.seconds),
                rate(measured.actions, measured.seconds),
            );
        }
    }
}

/// Plays `LOG_SEED` with the checks on and prints it: a header per Turn, one line per Action with
/// the Player, the action id and the Card Id of each argument, one indented line per Event the
/// Action caused, then the Terminal Outcome.
fn print_game_log(loaded: &Loaded) {
    let game = match self_play(
        &loaded.ruleset,
        &loaded.decks,
        Seed(LOG_SEED),
        MAX_STEPS,
        Checks::On,
    ) {
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
        "{}, seed {LOG_SEED}, {} Actions, {}",
        loaded.name,
        game.record.actions().len(),
        game.trigger_cap_rejections
    );
    print_turn_header(&game.creation.opening.state);
    let mut state = game.creation.opening.state.clone();
    for (index, (action, trace)) in game.record.actions().iter().zip(&game.traces).enumerate() {
        let (name, cards): (String, Vec<String>) = match action {
            Action::Declared {
                action, arguments, ..
            } => (
                action.as_str().to_string(),
                arguments
                    .iter()
                    .map(|argument| {
                        let Argument::Card(id) = argument;
                        card_name(&game, *id)
                    })
                    .collect(),
            ),
            Action::Answer {
                answer: Answer::Option(option),
                ..
            } => (format!("answers option {option}"), Vec::new()),
            Action::Answer {
                answer: Answer::Card(card),
                ..
            } => ("answers card".to_string(), vec![card_name(&game, *card)]),
            Action::Pass { .. } => ("passes".to_string(), Vec::new()),
            Action::Activate { card, ability, .. } => (
                format!("activates ability {ability} of"),
                vec![card_name(&game, *card)],
            ),
        };
        println!(
            "  {index:>3}. Player {} {name}{}",
            action.player().0,
            if cards.is_empty() {
                String::new()
            } else {
                format!(" {}", cards.join(", "))
            }
        );
        for event in trace.events() {
            println!("         {}", describe(&game, event));
        }
        state = match state.apply_trace(trace.events()) {
            Ok(next) => next,
            Err(error) => {
                println!("  the recorded Events do not apply: {error}");
                return;
            }
        };
        if trace
            .events()
            .iter()
            .any(|event| matches!(event, Event::TurnBegan { .. }))
        {
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

/// `definition (#id)` for a card instance of `game`.
fn card_name(game: &SelfPlayGame, id: tcg_engine::declarative::CardInstanceId) -> String {
    match game.state.card(id) {
        Some(card) => format!("{} (#{})", card.definition().as_str(), id.0),
        None => format!("#{}", id.0),
    }
}

/// One readable line for an Event.
fn describe(game: &SelfPlayGame, event: &Event) -> String {
    match event {
        Event::CardMoved {
            card,
            player,
            from,
            to,
        } => format!(
            "Player {} moves {} from {} to {}",
            player.0,
            card_name(game, *card),
            from.as_str(),
            to.as_str()
        ),
        Event::CardStateChanged { card, state, value } => format!(
            "{} has {} = {value:?}",
            card_name(game, *card),
            state.as_str()
        ),
        Event::ZoneShuffled { player, zone, .. } => {
            format!("Player {}'s {} is shuffled", player.0, zone.as_str())
        }
        Event::PlayerStateChanged {
            player,
            state,
            value,
        } => format!("Player {} has {} = {value:?}", player.0, state.as_str()),
        Event::TurnBegan { player, turn } => {
            format!("Turn {} begins for Player {}", turn.get(), player.0)
        }
        Event::PhaseBegan { phase } => format!("phase {} begins", phase.as_str()),
        Event::DrawFromEmptyZone { player, zone } => {
            format!("Player {} draws from the empty {}", player.0, zone.as_str())
        }
        Event::GameEnded { losses } => format!("the Game ends with {} loss(es)", losses.len()),
        Event::CardCreated { card, .. } => format!("{} is created", card_name(game, *card)),
        Event::DecisionPending { decider, .. } => format!("Player {} is asked", decider.0),
        Event::DecisionAnswered { player, .. } => format!("Player {} answers", player.0),
        Event::ItemPushed { item } => format!("Player {} pushes an item", item.controller.0),
        Event::ItemResolved { item } => format!("Player {}'s item resolves", item.controller.0),
        Event::ItemFizzled { item } => format!("Player {}'s item fizzles", item.controller.0),
    }
}

fn print_turn_header(state: &GameState) {
    if let Some(turn) = state.turn_number() {
        println!("Turn {} (Player {})", turn.get(), state.active_player().0);
    }
}

/// What one throughput measurement played.
struct Measured {
    games: u64,
    actions: u64,
    longest: usize,
    seconds: f64,
}

/// Plays Seeds `0..THROUGHPUT_GAMES`. Every Game must end well.
#[expect(
    clippy::disallowed_methods,
    reason = "wall-clock time is the point of a throughput measurement; it is not engine code and never enters a GameState"
)]
fn measure(loaded: &Loaded, checks: Checks) -> Measured {
    let started = Instant::now();
    let mut actions = 0_u64;
    let mut longest = 0;
    for seed in 0..THROUGHPUT_GAMES {
        match self_play(
            &loaded.ruleset,
            &loaded.decks,
            Seed(seed),
            MAX_STEPS,
            checks,
        ) {
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
    Measured {
        games: THROUGHPUT_GAMES,
        actions,
        longest,
        seconds: started.elapsed().as_secs_f64(),
    }
}

/// `count` per second.
#[expect(
    clippy::float_arithmetic,
    reason = "a throughput figure is a ratio printed for people; it is not engine code and never enters a GameState"
)]
fn rate(count: u64, seconds: f64) -> f64 {
    count as f64 / seconds
}
