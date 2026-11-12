//! Seeded self-play (ADR-0020): the Uniform Random Policy, the Step Cap and failure records.

use std::num::NonZeroU64;

use tcg_engine::declarative::{GameRecord, Rejection, ReplayError};
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset::{LegalActions, Ruleset};
use tcg_selfplay::{self_play, verify_record, Checks, FailureKind, SelfPlayFailure, POLICY_STREAM};

use crate::common::{decks, p, pass, small, small_version_2, small_with_ruleset, MAX_STEPS};

#[test]
fn self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let mut failures = Vec::new();
    let mut longest = 0;
    for seed in 0..1000 {
        match self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On) {
            Ok(game) => {
                if game.state.outcome().is_none() {
                    failures.push(format!("seed {seed}: the Game has no Terminal Outcome"));
                }
                longest = longest.max(game.record.actions().len());
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
}

#[test]
fn self_play_picks_the_legal_action_the_seeded_policy_draws() {
    let ruleset = small();
    let decks = decks(&ruleset);
    for seed in 0..5 {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .expect("the Game is played");

        // The policy has its own generator, derived from the Seed with the documented constant.
        let mut policy = RandomState::from_seed(Seed(seed ^ POLICY_STREAM));
        let mut state = game.creation.opening.state.clone();
        for (index, action) in game.record.actions().iter().enumerate() {
            let legal = ruleset.legal_actions(&state);
            let bound = NonZeroU64::new(u64::try_from(legal.len()).expect("a small listing"))
                .expect("an unfinished Game has a Legal Action");
            let drawn = usize::try_from(policy.next_below(bound)).expect("a small index");
            assert_eq!(
                action, &legal[drawn],
                "seed {seed}, Action index {index}: not the Action the policy draws"
            );
            state = ruleset
                .transition(&state, action)
                .expect("the Action is accepted")
                .state;
        }
        assert!(state.outcome().is_some());
    }
}

#[test]
fn self_play_with_the_same_seed_is_deterministic() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let first = self_play(&ruleset, &decks, Seed(7), MAX_STEPS, Checks::On).expect("played");
    let second = self_play(&ruleset, &decks, Seed(7), MAX_STEPS, Checks::On).expect("played");
    assert!(first.record == second.record, "records");
    assert!(first.creation == second.creation, "creation");
    assert!(first.traces == second.traces, "Event Traces");
    assert!(first.state == second.state, "final GameState");

    // The invariant checks only read: switching them off changes nothing.
    let unchecked = self_play(&ruleset, &decks, Seed(7), MAX_STEPS, Checks::Off).expect("played");
    assert!(first == unchecked, "the same Game with checks off");

    let other = self_play(&ruleset, &decks, Seed(8), MAX_STEPS, Checks::On).expect("played");
    assert_ne!(first.record.actions(), other.record.actions());
}

/// A Game of seed 4 stopped by a Step Cap of 5.
fn capped() -> Box<SelfPlayFailure> {
    let ruleset = small();
    self_play(&ruleset, &decks(&ruleset), Seed(4), 5, Checks::On)
        .expect_err("5 Actions do not end a Game")
}

#[test]
fn reaching_the_step_cap_is_a_failure() {
    let failure = capped();
    assert_eq!(failure.kind, FailureKind::StepCapReached { max_steps: 5 });
    assert_eq!(failure.seed, Seed(4));
    assert_eq!(failure.action_index, Some(4));
    assert_eq!(failure.record.actions().len(), 5);
    assert_eq!(failure.record.seed(), Seed(4));
}

#[test]
fn a_game_that_ends_on_its_last_allowed_step_is_not_a_failure() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let full = self_play(&ruleset, &decks, Seed(4), MAX_STEPS, Checks::On).expect("played");
    let n = full.record.actions().len();
    assert!(n > 2, "the Game of seed 4 is longer than 2 Actions");

    // The cap fails only a Game that has not ended after `max_steps` accepted Actions.
    let exact = self_play(&ruleset, &decks, Seed(4), n, Checks::On).expect("ends on the last step");
    assert!(exact == full, "a cap of n gives the full Game");

    let short =
        self_play(&ruleset, &decks, Seed(4), n - 1, Checks::On).expect_err("one step short");
    assert_eq!(short.kind, FailureKind::StepCapReached { max_steps: n - 1 });
    assert_eq!(short.action_index, Some(n - 2));
    assert_eq!(short.record.actions().len(), n - 1);

    let none = self_play(&ruleset, &decks, Seed(4), 0, Checks::On).expect_err("no step is allowed");
    assert_eq!(none.kind, FailureKind::StepCapReached { max_steps: 0 });
    assert_eq!(none.action_index, None);
    assert!(none.record.actions().is_empty());
}

#[test]
fn a_game_without_a_legal_action_is_a_failure() {
    // The Action Phase lists no action: the Game is on and nothing can be done.
    let ruleset = small_with_ruleset("actions: [play_creature, attack, pass_turn]", "actions: []");
    let decks = decks(&ruleset);
    let failure = self_play(&ruleset, &decks, Seed(5), MAX_STEPS, Checks::On)
        .expect_err("there is no Legal Action");
    assert_eq!(failure.kind, FailureKind::NoLegalAction);
    assert_eq!(failure.seed, Seed(5));
    assert_eq!(failure.action_index, None);
    assert!(failure.record == GameRecord::new(&ruleset, &decks, Seed(5)));
}

#[test]
fn a_failure_record_reproduces_the_failure() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let failure = capped();

    // The record of the failure is the first 5 Actions of the full Game, because the policy is
    // seeded; replaying it gives the GameState the failure stopped in.
    let full = self_play(&ruleset, &decks, Seed(4), MAX_STEPS, Checks::On).expect("played");
    let mut prefix = GameRecord::new(&ruleset, &decks, Seed(4));
    for action in &full.record.actions()[..5] {
        prefix.push_accepted(action.clone());
    }
    assert!(failure.record == prefix, "the failure record is the prefix");

    let replay = ruleset.replay(&failure.record).expect("the record replays");
    assert!(replay.state.outcome().is_none());
    assert_eq!(replay.traces.len(), 5);
    assert!(replay.traces == full.traces[..5], "Event Traces");

    // `verify_record` replays with the invariant checks on: a record that fails nothing is Ok and
    // gives exactly what the self-play of the same Game gave.
    let verified = verify_record(&ruleset, &full.record).expect("a good record verifies");
    assert!(verified.creation == full.creation, "creation");
    assert!(verified.traces == full.traces, "Event Traces");
    assert!(verified.state == full.state, "final GameState");
    let verified_prefix = verify_record(&ruleset, &failure.record).expect("a good record verifies");
    assert!(
        verified_prefix.state == replay.state,
        "the stopped GameState"
    );
}

#[test]
fn verify_record_reports_a_record_of_another_ruleset() {
    let failure = verify_record(&small_version_2(), &capped().record)
        .expect_err("the record is of version 1");
    assert_eq!(failure.seed, Seed(4));
    assert_eq!(failure.action_index, None);
    match failure.kind {
        FailureKind::Replay(ReplayError::RulesetMismatch {
            record_version,
            ruleset_version,
            ..
        }) => {
            assert_eq!(record_version.get(), 1);
            assert_eq!(ruleset_version.get(), 2);
        }
        other => panic!("expected a RulesetMismatch, got {other:?}"),
    }
}

#[test]
fn verify_record_reports_the_last_accepted_action_of_a_rejected_record() {
    let ruleset = small();
    let decks = decks(&ruleset);

    // The full Game of seed 2 plus one Action the ended Game rejects: the last accepted Action
    // is the last one of the Game.
    let full = self_play(&ruleset, &decks, Seed(2), MAX_STEPS, Checks::On).expect("played");
    let n = full.record.actions().len();
    let mut longer = full.record.clone();
    longer.push_accepted(pass(0));
    let failure = verify_record(&ruleset, &longer).expect_err("the extra Action is rejected");
    assert_eq!(
        failure.kind,
        FailureKind::Replay(ReplayError::ActionRejected {
            index: n,
            rejection: Rejection::GameOver,
        })
    );
    assert_eq!(failure.action_index, Some(n - 1));
    assert_eq!(failure.seed, Seed(2));

    // A record whose first Action is rejected has no accepted Action.
    let mut first = GameRecord::new(&ruleset, &decks, Seed(2));
    first.push_accepted(pass(1));
    let failure = verify_record(&ruleset, &first).expect_err("the first Action is rejected");
    assert_eq!(
        failure.kind,
        FailureKind::Replay(ReplayError::ActionRejected {
            index: 0,
            rejection: Rejection::NotActivePlayer {
                player: p(1),
                active: p(0),
            },
        })
    );
    assert_eq!(failure.action_index, None);
}
