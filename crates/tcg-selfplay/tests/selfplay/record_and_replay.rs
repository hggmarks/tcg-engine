//! Game Records and exact Replay (ADR-0019).

use tcg_engine::declarative::{DeclarativeRuleset, GameRecord, Rejection, ReplayError};
use tcg_engine::random::Seed;
use tcg_engine::ruleset_definition::RulesetId;
use tcg_selfplay::{self_play, Checks};

use crate::common::{decks, p, pass, small, small_version_2, MAX_STEPS};

#[test]
fn a_game_record_holds_the_identities_the_seed_and_the_accepted_actions() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let mut record = GameRecord::new(&ruleset, &decks, Seed(77));
    record.push_accepted(pass(0));
    record.push_accepted(pass(1));

    assert_eq!(record.ruleset_id(), &RulesetId::new("small"));
    assert_eq!(
        record.ruleset_version(),
        ruleset.definition().meta().version()
    );
    assert_eq!(record.ruleset_version().get(), 1);
    assert_eq!(record.catalog(), ruleset.catalog().meta());
    assert_eq!(record.catalog().ruleset().as_str(), "small");
    assert_eq!(record.catalog().ruleset_version().get(), 1);
    assert_eq!(record.decks(), decks.as_slice());
    assert_eq!(record.seed(), Seed(77));
    assert_eq!(record.actions(), [pass(0), pass(1)]);
}

#[test]
fn replay_reproduces_the_event_traces_and_the_final_game_state() {
    let ruleset = small();
    let decks = decks(&ruleset);
    for seed in [3, 4, 5] {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .expect("the Game is played");

        // A second, separately loaded Ruleset: replay must not depend on the first value.
        let replay = small().replay(&game.record).expect("the record replays");

        assert!(replay.creation == game.creation, "seed {seed}: creation");
        assert_eq!(replay.traces.len(), game.record.actions().len());
        assert!(replay.traces == game.traces, "seed {seed}: Event Traces");
        assert!(replay.state == game.state, "seed {seed}: final GameState");
    }
}

#[test]
fn replay_rejects_a_record_of_another_ruleset() {
    let ruleset = small();
    let record = self_play(&ruleset, &decks(&ruleset), Seed(1), 10, Checks::On)
        .expect_err("a cap of 10 stops the Game")
        .record;

    // Version 2 of the ruleset, with a catalog of version 2: both identities differ, and the
    // ruleset identity is the one reported, because it is checked first.
    match small_version_2().replay(&record) {
        Err(ReplayError::RulesetMismatch {
            record_id,
            record_version,
            ruleset_id,
            ruleset_version,
        }) => {
            assert_eq!(record_id.as_str(), "small");
            assert_eq!(record_version.get(), 1);
            assert_eq!(ruleset_id.as_str(), "small");
            assert_eq!(ruleset_version.get(), 2);
        }
        other => panic!("expected RulesetMismatch, got {other:?}"),
    }
}

#[test]
fn replay_rejects_a_record_of_another_catalog() {
    let ruleset = small();
    let record = self_play(&ruleset, &decks(&ruleset), Seed(1), 10, Checks::On)
        .expect_err("a cap of 10 stops the Game")
        .record;

    // The definition of version 1 with the catalog of version 2: `DeclarativeRuleset::new` accepts
    // the pair, so the ruleset identity matches the record and only the catalog identity differs.
    let mismatched = DeclarativeRuleset::new(
        ruleset.definition().clone(),
        small_version_2().catalog().clone(),
    );
    match mismatched.replay(&record) {
        Err(ReplayError::CatalogMismatch { record, ruleset }) => {
            assert_eq!(record.ruleset_version().get(), 1);
            assert_eq!(ruleset.ruleset_version().get(), 2);
        }
        other => panic!("expected CatalogMismatch, got {other:?}"),
    }
}

#[test]
fn replay_reports_the_index_of_a_rejected_action() {
    let ruleset = small();
    let decks = decks(&ruleset);

    // The first Action of the record is by the wrong Player.
    let mut record = GameRecord::new(&ruleset, &decks, Seed(2));
    record.push_accepted(pass(1));
    assert_eq!(
        ruleset.replay(&record).err(),
        Some(ReplayError::ActionRejected {
            index: 0,
            rejection: Rejection::NotActivePlayer {
                player: p(1),
                active: p(0),
            },
        })
    );

    // A whole Game followed by one more Action: the index is the one after the last accepted.
    let game = self_play(&ruleset, &decks, Seed(2), MAX_STEPS, Checks::On).expect("the Game ends");
    let accepted = game.record.actions().len();
    let mut longer = game.record.clone();
    longer.push_accepted(pass(0));
    assert_eq!(
        ruleset.replay(&longer).err(),
        Some(ReplayError::ActionRejected {
            index: accepted,
            rejection: Rejection::GameOver,
        })
    );
}
