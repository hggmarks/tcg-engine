//! Unit tests of the failure path of an Invariant Violation. The engine cannot be made to violate
//! an Invariant through its public API, so these tests inject the violation through a [`Checker`]
//! that reports a chosen one at a chosen Action, and check where the failure points: its Action
//! index, its record and the GameState it stopped in.

use std::cell::{Cell, RefCell};

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{DeclarativeRuleset, Event, GameRecord, GameState};
use tcg_engine::random::Seed;
use tcg_engine::ruleset::EventTrace;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

use super::{play, verify, Checker, FailureKind, SelfPlayFailure, REAL_CHECKS};
use crate::invariants::{Invariant, InvariantViolation};

const SMALL_RULESET: &str = include_str!("../../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../../rulesets/small-ruleset/decks/deck-b.yaml");

const SEED: Seed = Seed(4);
const MAX_STEPS: usize = 1_000;

fn small() -> (DeclarativeRuleset, Vec<DeckList>) {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks = [DECK_A, DECK_B]
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect();
    (ruleset, decks)
}

fn injected() -> InvariantViolation {
    InvariantViolation {
        invariant: Invariant::EndedGameHasNoLegalActions,
        detail: "injected".to_string(),
    }
}

/// Runs `body` with checks that find nothing at the Opening Position and report [`injected`] at
/// the transition of Action index `at`, and with the GameState they flagged. Each call starts
/// counting transitions again.
fn with_violation_at<R>(
    at: usize,
    body: impl FnOnce(&Checker<'_>, &RefCell<Option<GameState>>) -> R,
) -> R {
    let calls = Cell::new(0);
    let flagged = RefCell::new(None);
    let transition =
        |_: &DeclarativeRuleset, _: &GameState, _: &EventTrace<Event>, next: &GameState| {
            let index = calls.get();
            calls.set(index + 1);
            if index == at {
                *flagged.borrow_mut() = Some(next.clone());
                vec![injected()]
            } else {
                Vec::new()
            }
        };
    let checker = Checker {
        opening: &|_, _, _| Vec::new(),
        transition: &transition,
    };
    body(&checker, &flagged)
}

fn violated(failure: &SelfPlayFailure, seed: Seed, index: Option<usize>, actions: usize) {
    assert_eq!(failure.seed, seed);
    assert_eq!(failure.action_index, index);
    assert_eq!(
        failure.kind,
        FailureKind::InvariantViolated(vec![injected()])
    );
    assert_eq!(failure.record.actions().len(), actions);
}

#[test]
fn a_transition_invariant_violation_fails_self_play_with_a_record_cut_after_that_action() {
    let (ruleset, decks) = small();
    let full = play(&ruleset, &decks, SEED, MAX_STEPS, Some(&REAL_CHECKS)).expect("played");
    let k = 3;
    assert!(
        full.record.actions().len() > k + 1,
        "the Game outlasts Action {k}"
    );

    let (failure, flagged) = with_violation_at(k, |checker, flagged| {
        let failure = play(&ruleset, &decks, SEED, MAX_STEPS, Some(checker))
            .expect_err("the injected violation fails the Game");
        (failure, flagged.borrow().clone())
    });
    violated(&failure, SEED, Some(k), k + 1);

    // The record is the first k + 1 Actions of the Game, and replaying it ends in the GameState
    // the checker flagged.
    assert_eq!(failure.record.actions(), &full.record.actions()[..=k]);
    let replay = ruleset.replay(&failure.record).expect("the record replays");
    assert_eq!(Some(replay.state), flagged);
}

#[test]
fn verification_fails_at_the_action_whose_transition_violates_an_invariant() {
    let (ruleset, decks) = small();
    let k = 3;
    let failure = with_violation_at(k, |checker, _| {
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(checker))
            .expect_err("the injected violation fails the Game")
    });

    // The record of the failure, verified with the same checks, fails the same way at the same
    // index; with the real checks it verifies.
    let verified = with_violation_at(k, |checker, _| {
        verify(&ruleset, &failure.record, checker).expect_err("the record fails verification")
    });
    assert_eq!(verified, failure);
    verify(&ruleset, &failure.record, &REAL_CHECKS).expect("the real checks find nothing");

    // A longer record is cut after the Action that violates.
    let full = play(&ruleset, &decks, SEED, MAX_STEPS, Some(&REAL_CHECKS)).expect("played");
    let cut = with_violation_at(k, |checker, _| {
        verify(&ruleset, &full.record, checker).expect_err("the record fails verification")
    });
    violated(&cut, SEED, Some(k), k + 1);
    assert_eq!(cut.record, failure.record);
}

#[test]
fn an_opening_invariant_violation_has_no_action_index_and_an_empty_record() {
    let (ruleset, decks) = small();
    let checks = Checker {
        opening: &|_, _, _| vec![injected()],
        transition: &|_, _, _, _| Vec::new(),
    };
    let failure =
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(&checks)).expect_err("the opening violates");
    violated(&failure, SEED, None, 0);
    assert_eq!(failure.record, GameRecord::new(&ruleset, &decks, SEED));

    let verified = verify(&ruleset, &failure.record, &checks).expect_err("the record fails");
    assert_eq!(verified, failure);
}
