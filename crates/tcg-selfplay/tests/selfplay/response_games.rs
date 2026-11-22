//! The correctness net over the Response Ruleset in self-play (tickets #85 and #87, ADR-0037 and
//! ADR-0039): its sample decks hold the fast cards and the ability card, and the 1,000 pinned Seeds
//! end with every Invariant checked, Passes, pushes and responses included, counting what the
//! Games exercised and pinning their records and Event Traces (the Response Self-Play Pin).
//!
//! Everything runs on the real Response Ruleset, catalog and sample decks.

use tcg_engine::declarative::{Action, Event, StackItem, StackSource};
use tcg_engine::random::Seed;
use tcg_engine::ruleset_definition::ActionId;
use tcg_selfplay::{self_play, verify_record, Checks, SelfPlayGame, TriggerCapRejections};

use crate::common::{
    decks, digest_line, effects, effects_decks, pin_difference, response, response_decks, small,
    MAX_STEPS, RESPONSE_DECK_A, RESPONSE_DECK_B,
};

/// The Seeds of the Response Self-Play Pin: `0..PINNED_SEEDS`.
const PINNED_SEEDS: u64 = 1000;

/// The Seeds whose records are also verified (replayed, with every Invariant checked at each
/// step) in the same pass: the Seeds of the 100-Seed test this pass replaced.
const VERIFIED_SEEDS: u64 = 100;

/// The committed pin of the Game Records and Event Traces of the Seeds `0..PINNED_SEEDS` over the
/// Response Ruleset, one `seed actions digest` line per Seed, in the format of the Small and
/// Effects pins.
const DIGESTS: &str = include_str!("golden/response-self-play-seeds-0-1000.digest");

const DIGESTS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/selfplay/golden/response-self-play-seeds-0-1000.digest"
);

/// What the Response Games of the pinned Seeds exercised, counted from the Game Records and the
/// Event Traces only (ADR-0039), as ADR-0035 counts Choices. Each is asserted at least 1.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ResponseUse {
    /// Items pushed while the Stack was not empty: a response.
    responses_pushed: usize,
    /// Items pushed while the Stack already held two: a response to a response (depth 3).
    responses_to_responses: usize,
    /// `ItemFizzled` Events.
    fizzles: usize,
    /// `Action::Activate` in the records.
    abilities_used: usize,
    /// An attack Window that closed with the opponent passing and no response pushed.
    unanswered_windows: usize,
}

impl ResponseUse {
    fn add(&mut self, other: ResponseUse) {
        self.responses_pushed += other.responses_pushed;
        self.responses_to_responses += other.responses_to_responses;
        self.fizzles += other.fizzles;
        self.abilities_used += other.abilities_used;
        self.unanswered_windows += other.unanswered_windows;
    }
}

fn is_attack(item: &StackItem) -> bool {
    item.source == StackSource::Action(ActionId::new("attack"))
}

/// The number of items with an `Ability` source that `game` pushed.
fn ability_pushes(game: &SelfPlayGame) -> usize {
    game.creation
        .opening
        .trace
        .events()
        .iter()
        .chain(game.traces.iter().flat_map(|trace| trace.events()))
        .filter(|event| {
            matches!(
                event,
                Event::ItemPushed { item } if matches!(item.source, StackSource::Ability { .. })
            )
        })
        .count()
}

/// The coverage counts of one Game. The Stack depth is carried through the creation trace and
/// every Event Trace in order: `ItemPushed` adds one, `ItemResolved` and `ItemFizzled` take one.
///
/// An **Unanswered Window** is read from the traces alone, the harness exposing no listings: a
/// `Pass` whose trace resolves or fizzles an attack item that the Action just before it pushed, so
/// the opponent had the Priority and passed without responding.
fn response_use(game: &SelfPlayGame) -> ResponseUse {
    let mut used = ResponseUse::default();
    let mut depth = 0_usize;
    let mut count = |events: &[Event]| {
        for event in events {
            match event {
                Event::ItemPushed { .. } => {
                    if depth >= 1 {
                        used.responses_pushed += 1;
                    }
                    if depth >= 2 {
                        used.responses_to_responses += 1;
                    }
                    depth = depth.checked_add(1).expect("the Stack depth fits");
                }
                Event::ItemResolved { .. } => {
                    depth = depth
                        .checked_sub(1)
                        .expect("a resolved item was on the Stack");
                }
                Event::ItemFizzled { .. } => {
                    used.fizzles += 1;
                    depth = depth
                        .checked_sub(1)
                        .expect("a fizzled item was on the Stack");
                }
                _ => {}
            }
        }
    };
    count(game.creation.opening.trace.events());
    let mut attack_pushed_by_the_previous_action = false;
    for (action, trace) in game.record.actions().iter().zip(&game.traces) {
        let events = trace.events();
        count(events);
        if matches!(action, Action::Activate { .. }) {
            used.abilities_used += 1;
        }
        if matches!(action, Action::Pass { .. })
            && attack_pushed_by_the_previous_action
            && events.iter().any(|event| {
                matches!(
                    event,
                    Event::ItemResolved { item } | Event::ItemFizzled { item } if is_attack(item)
                )
            })
        {
            used.unanswered_windows += 1;
        }
        attack_pushed_by_the_previous_action = events
            .iter()
            .any(|event| matches!(event, Event::ItemPushed { item } if is_attack(item)));
    }
    assert_eq!(
        depth == 0,
        game.state.stack().is_empty(),
        "the Stack depth read from the Events is the Stack of the final GameState"
    );
    used
}

#[test]
fn the_response_sample_decks_hold_the_fast_cards_and_the_ability_card() {
    for deck in [RESPONSE_DECK_A, RESPONSE_DECK_B] {
        for card in ["swift_sparrow", "flash_fox", "skittish_hare"] {
            assert!(deck.contains(card), "a Response sample deck lacks {card}");
        }
    }
}

#[test]
fn response_self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    let mut failures = Vec::new();
    let mut longest = 0;
    let mut digests = Vec::new();
    let mut used = ResponseUse::default();
    for seed in 0..PINNED_SEEDS {
        match self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On) {
            Ok(game) => {
                let game_use = response_use(&game);
                used.add(game_use);
                let pushed = ability_pushes(&game);
                if pushed != game_use.abilities_used {
                    failures.push(format!(
                        "seed {seed}: {} Activate Actions, but {pushed} Ability items pushed",
                        game_use.abilities_used
                    ));
                }
                if game.state.outcome().is_none() {
                    failures.push(format!("seed {seed}: the Game has no Terminal Outcome"));
                }
                if !game.state.stack().is_empty() {
                    failures.push(format!("seed {seed}: the Stack is not empty at the end"));
                }
                if game.trigger_cap_rejections != TriggerCapRejections::Counted(0) {
                    failures.push(format!(
                        "seed {seed}: {}, but no shipped deck lists a card that reaches the cap",
                        game.trigger_cap_rejections
                    ));
                }
                if seed < VERIFIED_SEEDS {
                    if let Err(failure) = verify_record(&ruleset, &game.record) {
                        failures.push(format!(
                            "seed {seed}: the record does not verify: {:?}",
                            failure.kind
                        ));
                    }
                }
                longest = longest.max(game.record.actions().len());
                digests.push(digest_line(seed, &game));
            }
            Err(failure) => failures.push(format!(
                "seed {}: failed after Action index {:?}: {:?}",
                failure.seed.0, failure.action_index, failure.kind
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        longest * 2 < MAX_STEPS,
        "the Step Cap of {MAX_STEPS} has no headroom over the longest Game ({longest} Actions)"
    );

    // The Uniform Random Policy reaches every count over the 1,000 Games (ADR-0039). A count of 0
    // means the sample decks stopped reaching a card; change the decks, not the policy.
    assert!(
        used.responses_pushed >= 1,
        "coverage count responses_pushed is 0 over Seeds 0..1000: {used:?}"
    );
    assert!(
        used.responses_to_responses >= 1,
        "coverage count responses_to_responses is 0 over Seeds 0..1000: {used:?}"
    );
    assert!(
        used.fizzles >= 1,
        "coverage count fizzles is 0 over Seeds 0..1000: {used:?}"
    );
    assert!(
        used.abilities_used >= 1,
        "coverage count abilities_used is 0 over Seeds 0..1000: {used:?}"
    );
    assert!(
        used.unanswered_windows >= 1,
        "coverage count unanswered_windows is 0 over Seeds 0..1000: {used:?}"
    );

    // The same Games, one pass: their Game Records and Event Traces are the pinned ones.
    if let Some(difference) = pin_difference(DIGESTS, &digests) {
        panic!(
            "{difference}\nIf the change is deliberate, run: cargo test -p tcg-selfplay --test selfplay -- --ignored regenerate_response_self_play_digests\n(coverage so far: {used:?}, longest Game {longest} Actions)"
        );
    }
}

/// Rewrites the pin of the 1,000 Response Games. Run it by name when a rule, an Event's shape or a
/// shipped Response deck changes on purpose, and read the diff before committing it. It writes
/// that one file.
#[test]
#[ignore = "deliberate update: rewrites the Response self-play pin"]
fn regenerate_response_self_play_digests() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    let mut text = String::new();
    for seed in 0..PINNED_SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .expect("the Game is played");
        text.push_str(&digest_line(seed, &game));
    }
    std::fs::write(DIGESTS_PATH, text).expect("the Response self-play pin is written");
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
