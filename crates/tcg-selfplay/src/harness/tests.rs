//! Unit tests of the failure path of an Invariant Violation. The engine cannot be made to violate
//! an Invariant through its public API, so these tests inject the violation through a [`Checker`]
//! that reports a chosen one at a chosen Action, and check where the failure points: its Action
//! index, its record and the GameState it stopped in.

use std::cell::{Cell, RefCell};

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{DeclarativeRuleset, Event, GameRecord, GameState};
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{EventTrace, Ruleset};
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
        recomputation: &|_, _| Vec::new(),
        rejection: &|_, _, _, _| Vec::new(),
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
        recomputation: &|_, _| Vec::new(),
        rejection: &|_, _, _, _| Vec::new(),
    };
    let failure =
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(&checks)).expect_err("the opening violates");
    violated(&failure, SEED, None, 0);
    assert_eq!(failure.record, GameRecord::new(&ruleset, &decks, SEED));

    let verified = verify(&ruleset, &failure.record, &checks).expect_err("the record fails");
    assert_eq!(verified, failure);
}

/// Whether a card of `a` and the same card of `b` share one allocation of instance state. A
/// GameState and its clones share what an Event did not change (ADR-0021), so two GameStates
/// built from one another share, and two built apart from the Empty GameState share nothing.
fn shares_card_state(a: &GameState, b: &GameState) -> bool {
    a.cards().iter().zip(b.cards()).any(|(one, other)| {
        !one.state().is_empty() && std::ptr::eq(one.state().as_ptr(), other.state().as_ptr())
    })
}

/// Runs `body` with checks that record, for each call of the recomputation check, whether the
/// Recomputed GameState it was given shares memory with the engine's.
fn recording_sharing<R>(body: impl FnOnce(&Checker<'_>) -> R) -> (R, Vec<bool>) {
    let seen = RefCell::new(Vec::new());
    let recomputation = |recomputed: &GameState, next: &GameState| {
        seen.borrow_mut().push(shares_card_state(recomputed, next));
        Vec::new()
    };
    let checker = Checker {
        opening: &|_, _, _| Vec::new(),
        transition: &|_, _, _, _| Vec::new(),
        recomputation: &recomputation,
        rejection: &|_, _, _, _| Vec::new(),
    };
    let result = body(&checker);
    (result, seen.into_inner())
}

#[test]
fn the_recomputed_game_state_is_folded_from_the_empty_game_state_and_never_from_the_engines() {
    let (ruleset, decks) = small();
    let (game, during_play) = recording_sharing(|checker| {
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(checker)).expect("played")
    });
    let actions = game.record.actions().len();
    assert!(actions > 1);

    // The probe has teeth: a GameState folded from the engine's prior one shares card states with
    // the engine's next one, which is exactly what a Recomputed GameState must not do.
    let prior = &game.creation.opening.state;
    let engine_next = ruleset
        .transition(prior, &game.record.actions()[0])
        .expect("accepted")
        .state;
    let folded_from_the_engine = prior
        .apply_trace(game.traces[0].events())
        .expect("the trace applies");
    assert!(folded_from_the_engine == engine_next);
    assert!(shares_card_state(&folded_from_the_engine, &engine_next));

    // At the Opening Position and after every Action the harness gave the check a GameState that
    // it built apart, carrying the fold on from the Empty GameState, so it shares nothing with the
    // engine's GameState.
    assert_eq!(during_play.len(), actions + 1);
    assert!(
        during_play.iter().all(|shared| !shared),
        "the Recomputed GameState shares card states with the engine's at checks {:?}",
        during_play
            .iter()
            .enumerate()
            .filter(|(_, shared)| **shared)
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    );

    // Verification folds the same way.
    let (verified, during_verify) = recording_sharing(|checker| {
        verify(&ruleset, &game.record, checker).expect("the record verifies")
    });
    assert!(verified.state == game.state);
    assert_eq!(during_verify.len(), actions + 1);
    assert!(
        during_verify.iter().all(|shared| !shared),
        "verification folds from the engine's GameState"
    );
}
