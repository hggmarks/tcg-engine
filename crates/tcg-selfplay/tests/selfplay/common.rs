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

/// The Step Cap of the full Games of these tests. It is far above the longest Game of seeds
/// 0..1000 (see the test `self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding`).
pub const MAX_STEPS: usize = 5_000;

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

/// Deck B for seat 0 and deck A for seat 1.
pub fn swapped_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![deck(ruleset, DECK_B), deck(ruleset, DECK_A)]
}

pub fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

pub fn pass(player: u32) -> Action {
    Action {
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
