//! Fixtures shared by the self-play tests: the real Small Ruleset, catalog and sample decks.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{Action, Argument, DeclarativeRuleset, Event, GameState};
use tcg_engine::player::PlayerId;
use tcg_engine::ruleset::{EventTrace, Ruleset};
use tcg_engine::ruleset_definition::ActionId;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};
use tcg_selfplay::SelfPlayGame;

pub const SMALL_RULESET: &str =
    include_str!("../../../../rulesets/small-ruleset/small-ruleset.yaml");
pub const SMALL_CATALOG: &str =
    include_str!("../../../../rulesets/small-ruleset/small-catalog.yaml");
pub const DECK_A: &str = include_str!("../../../../rulesets/small-ruleset/decks/deck-a.yaml");
pub const DECK_B: &str = include_str!("../../../../rulesets/small-ruleset/decks/deck-b.yaml");

pub const EFFECTS_RULESET: &str =
    include_str!("../../../../rulesets/effects-ruleset/effects-ruleset.yaml");
pub const EFFECTS_CATALOG: &str =
    include_str!("../../../../rulesets/effects-ruleset/effects-catalog.yaml");
pub const EFFECTS_DECK_A: &str =
    include_str!("../../../../rulesets/effects-ruleset/decks/deck-a.yaml");
pub const EFFECTS_DECK_B: &str =
    include_str!("../../../../rulesets/effects-ruleset/decks/deck-b.yaml");

/// The Step Cap of the full Games of these tests. It is far above the longest Game of seeds
/// 0..1000 (see the test `self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding`).
pub const MAX_STEPS: usize = 1_000;

/// `text` with the one occurrence of `from` replaced by `to`.
pub fn edit(text: &str, from: &str, to: &str) -> String {
    assert_eq!(
        text.matches(from).count(),
        1,
        "`{from}` occurs once in the fixture"
    );
    text.replace(from, to)
}

pub fn ruleset_from(ruleset_yaml: &str, catalog_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog_yaml).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The real Small Ruleset and the real catalog.
pub fn small() -> DeclarativeRuleset {
    ruleset_from(SMALL_RULESET, SMALL_CATALOG)
}

/// The real Effects Ruleset and the real Effects catalog.
pub fn effects() -> DeclarativeRuleset {
    ruleset_from(EFFECTS_RULESET, EFFECTS_CATALOG)
}

/// The real Effects Ruleset with its text edited, and the real Effects catalog.
pub fn effects_with_ruleset(from: &str, to: &str) -> DeclarativeRuleset {
    ruleset_from(&edit(EFFECTS_RULESET, from, to), EFFECTS_CATALOG)
}

/// The real Small Ruleset with its text edited, and the real catalog.
pub fn small_with_ruleset(from: &str, to: &str) -> DeclarativeRuleset {
    ruleset_from(&edit(SMALL_RULESET, from, to), SMALL_CATALOG)
}

/// Version 2 of the Small Ruleset, with a catalog of version 2: both identities differ from the
/// real ones.
pub fn small_version_2() -> DeclarativeRuleset {
    ruleset_from(
        &edit(
            SMALL_RULESET,
            "meta: { id: small, version: 1 }",
            "meta: { id: small, version: 2 }",
        ),
        &edit(
            SMALL_CATALOG,
            "meta: { ruleset: small, ruleset_version: 1 }",
            "meta: { ruleset: small, ruleset_version: 2 }",
        ),
    )
}

fn deck(ruleset: &DeclarativeRuleset, yaml: &str) -> DeckList {
    load_deck_list(ruleset.definition(), ruleset.catalog(), yaml).expect("the sample deck is legal")
}

/// Deck A for seat 0 and deck B for seat 1.
pub fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![deck(ruleset, DECK_A), deck(ruleset, DECK_B)]
}

/// The Effects Deck List `yaml` loaded under `ruleset`.
pub fn effects_deck(ruleset: &DeclarativeRuleset, yaml: &str) -> DeckList {
    deck(ruleset, yaml)
}

/// The Effects sample Deck A for seat 0 and Deck B for seat 1.
pub fn effects_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![deck(ruleset, EFFECTS_DECK_A), deck(ruleset, EFFECTS_DECK_B)]
}

/// Deck B for seat 0 and deck A for seat 1.
pub fn swapped_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![deck(ruleset, DECK_B), deck(ruleset, DECK_A)]
}

pub fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

/// The player, action id and arguments of a Declared Action; `None` for an Answer.
pub fn declared(action: &Action) -> Option<(PlayerId, &ActionId, &[Argument])> {
    match action {
        Action::Declared {
            player,
            action,
            arguments,
        } => Some((*player, action, arguments.as_slice())),
        Action::Answer { .. } | Action::Pass { .. } => None,
    }
}

pub fn pass(player: u32) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new("pass_turn"),
        arguments: Vec::<Argument>::new(),
    }
}

/// One accepted transition of a Game: the GameState before, the Event Trace and the GameState
/// after.
pub struct Step {
    pub prior: GameState,
    pub trace: EventTrace<Event>,
    pub next: GameState,
}

/// The transitions of `game`, rebuilt by running each recorded Action through `ruleset`.
pub fn steps(ruleset: &DeclarativeRuleset, game: &SelfPlayGame) -> Vec<Step> {
    let mut state = game.creation.opening.state.clone();
    let mut steps = Vec::new();
    for action in game.record.actions() {
        let transition = ruleset
            .transition(&state, action)
            .expect("a recorded Action is accepted");
        steps.push(Step {
            prior: state,
            trace: transition.trace,
            next: transition.state.clone(),
        });
        state = transition.state;
    }
    steps
}

/// 64-bit FNV-1a: a fixed, dependency-free hash, so the digests never depend on a library version.
pub fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The FNV-1a offset basis, the start of every digest.
pub const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// The pin line of one Game: `seed actions digest`, with `actions` the number of accepted Actions
/// and `digest` the FNV-1a, in hex, of one `Debug` line per Event of the creation trace, then, for
/// every accepted Action, its `Debug` line followed by one `Debug` line per Event of its trace,
/// then the `Debug` line of the Terminal Outcome. It is the `Debug`-line convention of the Golden
/// Snapshots (ADR-0017), so it covers both the Game Record and the whole Event Trace.
pub fn digest_line(seed: u64, game: &SelfPlayGame) -> String {
    let mut hash = FNV_START;
    let mut line = |text: String| {
        hash = fnv1a(hash, text.as_bytes());
        hash = fnv1a(hash, b"\n");
    };
    for event in game.creation.opening.trace.events() {
        line(format!("{event:?}"));
    }
    for (action, trace) in game.record.actions().iter().zip(&game.traces) {
        line(action_line(action));
        for event in trace.events() {
            line(format!("{event:?}"));
        }
    }
    line(format!("{:?}", game.state.outcome()));
    format!("{seed} {} {hash:016x}\n", game.record.actions().len())
}

/// The line of an Action in the pin (ADR-0033). A Declared Action keeps the `Debug` line of the struct
/// it was before `Action` became an enum, `Action { player: .., action: .., arguments: [..] }`, so the
/// committed pins stay byte for byte what they were; an Answer, which no committed Game holds, is
/// its own `Debug`.
fn action_line(action: &Action) -> String {
    match action {
        Action::Declared {
            player,
            action,
            arguments,
        } => {
            format!("Action {{ player: {player:?}, action: {action:?}, arguments: {arguments:?} }}")
        }
        Action::Answer { .. } | Action::Pass { .. } => format!("{action:?}"),
    }
}

/// Compares `lines` (one per Seed, in order) with the `committed` pin, byte for byte; the answer
/// names the first Seed that differs.
pub fn pin_difference(committed: &str, lines: &[String]) -> Option<String> {
    let committed: Vec<&str> = committed.split_inclusive('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        if committed.get(index) != Some(&line.as_str()) {
            return Some(format!(
                "seed {index}: the Game Record or its Event Trace changed\n  pinned: {:?}\n  played: {line:?}",
                committed.get(index)
            ));
        }
    }
    (committed.len() != lines.len()).then(|| {
        format!(
            "the pin has {} lines, {} Seeds were played",
            committed.len(),
            lines.len()
        )
    })
}

pub const RESPONSE_RULESET: &str =
    include_str!("../../../../rulesets/response-ruleset/response-ruleset.yaml");
pub const RESPONSE_CATALOG: &str =
    include_str!("../../../../rulesets/response-ruleset/response-catalog.yaml");
pub const RESPONSE_DECK_A: &str =
    include_str!("../../../../rulesets/response-ruleset/decks/deck-a.yaml");
pub const RESPONSE_DECK_B: &str =
    include_str!("../../../../rulesets/response-ruleset/decks/deck-b.yaml");

/// The real Response Ruleset and the real Response catalog.
pub fn response() -> DeclarativeRuleset {
    ruleset_from(RESPONSE_RULESET, RESPONSE_CATALOG)
}

/// The Response sample Deck A for seat 0 and Deck B for seat 1.
pub fn response_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![
        deck(ruleset, RESPONSE_DECK_A),
        deck(ruleset, RESPONSE_DECK_B),
    ]
}

/// Asserts that `one` and `other` are the same Game: the same record, creation, Event Traces and
/// final GameState. Their Trigger Cap Rejections are the one field left out, because a Game run
/// with `Checks::Off` does not measure them and one run with checks on does (ADR-0036); a test
/// asserts the two values itself.
pub fn assert_same_game_but_for_cap_rejections(one: &SelfPlayGame, other: &SelfPlayGame, at: &str) {
    assert!(one.record == other.record, "{at}: records");
    assert!(one.creation == other.creation, "{at}: creation");
    assert!(one.traces == other.traces, "{at}: Event Traces");
    assert!(one.state == other.state, "{at}: final GameState");
}
