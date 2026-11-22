//! The correctness net over the Response Ruleset in self-play (tickets #85 and #87, ADR-0037 and
//! ADR-0039): its sample decks hold the fast cards and the ability card, and the 1,000 pinned Seeds
//! end with every Invariant checked, Passes, pushes and responses included, counting what the
//! Games exercised and pinning their records and Event Traces (the Response Self-Play Pin).
//!
//! Everything runs on the real Response Ruleset, catalog and sample decks.

use std::sync::OnceLock;

use tcg_engine::card_catalog::CardId;
use tcg_engine::declarative::{Action, Event, StackItem, StackSource};
use tcg_engine::random::Seed;
use tcg_engine::ruleset_definition::{ActionId, DeckInputId};
use tcg_selfplay::{self_play, verify_record, Checks, SelfPlayGame, TriggerCapRejections};

use crate::common::{
    decks, digest_line, effects, effects_decks, pin_difference, response, response_decks, small,
    MAX_STEPS,
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
                // A Terminal Outcome clears the Stack (ADR-0037).
                Event::GameEnded { .. } => depth = 0,
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
    let ruleset = response();
    let main_deck = DeckInputId::new("main_deck");
    for (seat, deck) in response_decks(&ruleset).iter().enumerate() {
        let entries = deck
            .input(&main_deck)
            .expect("a Response deck has a main deck")
            .entries();
        for card in ["swift_sparrow", "flash_fox", "skittish_hare"] {
            let card = CardId::new(card);
            assert!(
                entries
                    .iter()
                    .any(|entry| *entry.card() == card && entry.count().get() >= 1),
                "the main deck of seat {seat} lacks {card:?}"
            );
        }
    }
}

/// The number of Seed Ranges the pinned Seeds are split into (ADR-0040): one test each, run on
/// parallel threads by the test harness. Chosen by measurement.
const PARTS: usize = 4;

const _: () = assert!(
    PINNED_SEEDS.is_multiple_of(PARTS as u64),
    "the Seed Ranges partition the pinned Seeds evenly"
);

/// The Seeds of Seed Range `part`: `part * LEN .. (part + 1) * LEN`, contiguous and in order.
fn seeds_of_range(part: usize) -> std::ops::Range<u64> {
    let len = PINNED_SEEDS / (PARTS as u64);
    let part = part as u64;
    part * len..(part + 1) * len
}

/// What one Seed Range's Games gave, computed once per test process by `seed_range` (ADR-0040).
struct SeedRangePlay {
    /// One line per failed check, as the single 1,000-Seed test collected them.
    failures: Vec<String>,
    /// The most Actions of any Game of the range.
    longest: usize,
    /// The `digest_line` of every Seed of the range, in Seed order.
    digests: Vec<String>,
    /// The coverage counts of the range's Games, summed.
    used: ResponseUse,
}

/// Plays every Seed of Seed Range `part` under `Checks::On` and checks each Game. Failures are
/// collected as data, never raised, so the part test reports its own lines.
fn play_seed_range(part: usize) -> SeedRangePlay {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    let mut failures = Vec::new();
    let mut longest = 0;
    let mut digests = Vec::new();
    let mut used = ResponseUse::default();
    for seed in seeds_of_range(part) {
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
    SeedRangePlay {
        failures,
        longest,
        digests,
        used,
    }
}

/// Each Seed Range is a pure function of its Seeds, played at most once per test process: the part
/// test and the joining test both read it here, and whichever reaches a range first plays it while
/// any other caller waits. Test scaffolding only; no engine state (ADR-0040).
static SEED_RANGES: [OnceLock<SeedRangePlay>; PARTS] = [const { OnceLock::new() }; PARTS];

fn seed_range(part: usize) -> &'static SeedRangePlay {
    SEED_RANGES[part].get_or_init(|| play_seed_range(part))
}

/// The part tests assert, for every Game of their Seed Range, what the single 1,000-Seed test
/// asserted: a Terminal Outcome, an empty Stack, `TriggerCapRejections::Counted(0)`, the Activate
/// and Ability-push counts equal, `verify_record` on Seeds 0..100 and no `FailureKind`.
fn assert_seed_range_holds(part: usize) {
    let play = seed_range(part);
    assert!(
        play.failures.is_empty(),
        "Seeds {:?}:\n{}",
        seeds_of_range(part),
        play.failures.join("\n")
    );
}

#[test]
fn response_self_play_ends_every_game_over_seeds_0_to_250_with_every_invariant_holding() {
    assert_seed_range_holds(0);
}

#[test]
fn response_self_play_ends_every_game_over_seeds_250_to_500_with_every_invariant_holding() {
    assert_seed_range_holds(1);
}

#[test]
fn response_self_play_ends_every_game_over_seeds_500_to_750_with_every_invariant_holding() {
    assert_seed_range_holds(2);
}

#[test]
fn response_self_play_ends_every_game_over_seeds_750_to_1000_with_every_invariant_holding() {
    assert_seed_range_holds(3);
}

#[test]
fn the_response_seed_ranges_cover_seeds_0_to_1000_once() {
    let mut next = 0;
    for part in 0..PARTS {
        let seeds = seeds_of_range(part);
        assert_eq!(
            seeds.start, next,
            "the Seed Range {part} starts where the last one ended"
        );
        assert!(
            seeds.start < seeds.end,
            "the Seed Range {part} is not empty"
        );
        next = seeds.end;
    }
    assert_eq!(
        next, PINNED_SEEDS,
        "the Seed Ranges end at the last pinned Seed"
    );
}

/// The last step: the five coverage counts summed over every Seed Range, and the Response
/// Self-Play Pin compared whole over the digest lines of all 1,000 Seeds in Seed order. It plays
/// no Game itself: it reads the Seed Ranges the part tests play (each is played once per process,
/// by whichever test reaches it first).
#[test]
fn response_self_play_covers_every_response_use_and_matches_the_pin_over_1000_seeds() {
    let plays: Vec<&SeedRangePlay> = (0..PARTS).map(seed_range).collect();
    for (part, play) in plays.iter().enumerate() {
        assert!(
            play.failures.is_empty(),
            "the Seeds {:?} have failures, listed by the part test for that Seed Range",
            seeds_of_range(part)
        );
    }
    let longest = plays.iter().map(|play| play.longest).max().unwrap_or(0);
    let mut used = ResponseUse::default();
    let mut digests = Vec::new();
    for play in &plays {
        used.add(play.used);
        digests.extend(play.digests.iter().cloned());
    }
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

    // The same Games, played once: their Game Records and Event Traces are the pinned ones.
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
