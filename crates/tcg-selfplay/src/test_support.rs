//! What the unit tests of the harness and of the observation checks share: the real Small and
//! Effects Rulesets with their sample decks, and an independent fold of a Game into what each Viewer
//! observes at each step. The fold uses the public API of `tcg-engine` only and none of the code
//! under test, so that a test can compare what the harness did with what it should have done.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    DeclarativeRuleset, GameState, ObservedCard, ObservedEvent, ObservedGameState, Observer,
    TakenAction, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::ruleset::{Observations, Ruleset};
use tcg_engine::ruleset_definition::{FieldName, FieldValue};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

use crate::SelfPlayGame;

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

fn loaded(ruleset: &str, catalog: &str, decks: [&str; 2]) -> (DeclarativeRuleset, Vec<DeckList>) {
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
    (ruleset, decks)
}

/// The real Small Ruleset, its catalog and its sample decks.
pub(crate) fn small() -> (DeclarativeRuleset, Vec<DeckList>) {
    loaded(SMALL_RULESET, SMALL_CATALOG, [SMALL_DECK_A, SMALL_DECK_B])
}

/// The real Effects Ruleset, its catalog and its sample decks.
pub(crate) fn effects() -> (DeclarativeRuleset, Vec<DeckList>) {
    loaded(
        EFFECTS_RULESET,
        EFFECTS_CATALOG,
        [EFFECTS_DECK_A, EFFECTS_DECK_B],
    )
}

/// The Viewer of seat `seat`.
pub(crate) fn viewer(seat: u32) -> Viewer {
    Viewer::Player(PlayerId(seat))
}

/// What one Viewer observes at one step of a Game, folded independently of the harness: a fresh
/// Observer from the Empty GameState over the Events of the Game, one step at a time.
#[derive(Debug, Clone)]
pub(crate) struct Folded {
    /// The Observed GameState before the step (of the Empty GameState, for the Opening Position).
    pub(crate) previous: ObservedGameState,
    /// The Observed Events of the creation trace, or of the transition.
    pub(crate) events: Vec<ObservedEvent>,
    /// The Taken Action; `None` at the Opening Position.
    pub(crate) taken: Option<TakenAction>,
    /// The Observed GameState after the step: `observe` of the engine's GameState.
    pub(crate) next: ObservedGameState,
    /// The Observer after the step.
    pub(crate) observer: Observer,
    /// The engine's GameState after the step.
    pub(crate) state: GameState,
}

/// The fold of `game` for `viewer`: the Opening Position first, then one entry per accepted Action.
pub(crate) fn independent_fold(
    ruleset: &DeclarativeRuleset,
    game: &SelfPlayGame,
    viewer: Viewer,
) -> Vec<Folded> {
    let empty = &game.creation.empty;
    let mut observer = ruleset
        .observer(viewer, empty, &[])
        .expect("the Observer is built");
    let previous = ruleset
        .observe(&observer, empty)
        .expect("the Empty GameState is observed");
    let events = ruleset
        .follow_events(&mut observer, empty, game.creation.opening.trace.events())
        .expect("the creation trace is followed");
    let mut state = game.creation.opening.state.clone();
    let next = ruleset
        .observe(&observer, &state)
        .expect("the Opening Position is observed");
    let mut folded = vec![Folded {
        previous,
        events,
        taken: None,
        next,
        observer: observer.clone(),
        state: state.clone(),
    }];
    for action in game.record.actions() {
        let transition = ruleset
            .transition(&state, action)
            .expect("a recorded Action is accepted");
        let previous = folded
            .last()
            .map(|step| step.next.clone())
            .expect("the Opening Position is there");
        let observed = ruleset
            .follow_transition(&mut observer, &state, action, &transition)
            .expect("the transition is followed");
        state = transition.state;
        folded.push(Folded {
            previous,
            events: observed.events,
            taken: Some(observed.action),
            next: observed.state,
            observer: observer.clone(),
            state: state.clone(),
        });
    }
    folded
}

/// The Derived Reads of an Observed Step: for every Visible Card in handle order, every int field
/// of its Card Definition in catalog order, read through the Ruleset.
pub(crate) fn expected_derived_reads(
    ruleset: &DeclarativeRuleset,
    observer: &Observer,
    state: &GameState,
    observed: &ObservedGameState,
) -> Vec<(ObservedCard, FieldName, i64)> {
    let mut reads = Vec::new();
    for card in observed.cards() {
        let definition = ruleset
            .catalog()
            .card(card.definition())
            .expect("a Visible Card has a Card Definition");
        for (field, value) in definition.fields() {
            if matches!(value, FieldValue::Int(_)) {
                let read = ruleset
                    .derived_read(observer, state, card.card(), field)
                    .expect("an int field of a Visible Card is read");
                reads.push((card.card(), field.clone(), read));
            }
        }
    }
    reads
}
