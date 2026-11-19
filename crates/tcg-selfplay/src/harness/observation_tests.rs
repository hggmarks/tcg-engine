//! Unit tests of how the harness wires the three checks over Observations (ADR-0031): which inputs
//! each check receives, for which Viewer, at which step, and what a failure reports. The lesson of
//! PR #60's review: a checker can be correct and still be wired to the wrong input. Each wiring
//! test records what the harness handed to a check and compares it with an independent fold of the
//! Game (`test_support::independent_fold`), which uses the public API of `tcg-engine` only.

use std::cell::{Cell, RefCell};

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, CardInstanceId, DeclarativeRuleset, GameState, ObservedCard, ObservedEvent,
    ObservedGameState, Observer, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations};
use tcg_engine::ruleset_definition::{FieldName, FieldValue, Visibility, ZoneId};

use super::{play, verify, Checker, FailureKind, ENGINE_ONLY_CHECKS, REAL_CHECKS};
use crate::invariants::{Invariant, InvariantViolation};
use crate::observations::{ObservedStep, ReplayedStep};
use crate::test_support::{effects, expected_derived_reads, independent_fold, small, viewer};
use crate::SelfPlayGame;

const SEED: Seed = Seed(4);
const MAX_STEPS: usize = 1_000;

/// Checks that find nothing and run no check over Observations.
fn quiet() -> Checker<'static> {
    Checker {
        opening: &|_, _, _| Vec::new(),
        transition: &|_, _, _, _| Vec::new(),
        recomputation: &|_, _| Vec::new(),
        rejection: &|_, _, _, _| Vec::new(),
        consistency: None,
        hidden_cards: None,
        observed_actions: None,
    }
}

fn played(ruleset: &DeclarativeRuleset, decks: &[DeckList]) -> SelfPlayGame {
    played_with(ruleset, decks, SEED)
}

/// A Game played without any check: the checks are what these tests exercise.
fn played_with(ruleset: &DeclarativeRuleset, decks: &[DeckList], seed: Seed) -> SelfPlayGame {
    play(ruleset, decks, seed, MAX_STEPS, None).expect("the Game is played")
}

/// What the consistency check was handed, in call order.
type ConsistencyCall = (
    Viewer,
    ObservedGameState,
    Vec<ObservedEvent>,
    ObservedGameState,
);

fn consistency_calls(run: impl FnOnce(&Checker<'_>)) -> Vec<ConsistencyCall> {
    let calls = RefCell::new(Vec::new());
    let consistency = |viewer: Viewer,
                       previous: &ObservedGameState,
                       events: &[ObservedEvent],
                       next: &ObservedGameState| {
        calls
            .borrow_mut()
            .push((viewer, previous.clone(), events.to_vec(), next.clone()));
        Vec::new()
    };
    let checker = Checker {
        consistency: Some(&consistency),
        ..quiet()
    };
    run(&checker);
    calls.into_inner()
}

#[test]
fn the_observation_checks_receive_the_viewers_observed_steps() {
    let (ruleset, decks) = small();
    let game = played(&ruleset, &decks);
    let actions = game.record.actions().len();
    assert!(actions > 1);

    let during_play = consistency_calls(|checker| {
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(checker)).expect("played");
    });
    // One call per Viewer, in seat order, at the Opening Position and after every Action.
    assert_eq!(during_play.len(), 2 * (actions + 1));
    for seat in 0..2_usize {
        let folded = independent_fold(&ruleset, &game, viewer(seat as u32));
        assert_eq!(folded.len(), actions + 1);
        for (step, expected) in folded.iter().enumerate() {
            let (who, previous, events, next) = &during_play[2 * step + seat];
            assert_eq!(*who, viewer(seat as u32), "step {step}");
            assert_eq!(
                previous, &expected.previous,
                "seat {seat}, step {step}: previous"
            );
            assert_eq!(events, &expected.events, "seat {seat}, step {step}: events");
            assert_eq!(next, &expected.next, "seat {seat}, step {step}: next");
        }
    }

    // Verification observes the same way.
    let during_verify = consistency_calls(|checker| {
        verify(&ruleset, &game.record, checker).expect("the record verifies");
    });
    assert_eq!(during_verify, during_play);
}

/// The Relabelled Game of each call of the leak check: the real Observed Step the harness
/// observed, the GameState the relabelled Events were folded to, and the relabelled Observed Step.
fn hidden_cards_calls(
    run: impl FnOnce(&Checker<'_>),
) -> Vec<(ObservedStep, GameState, ObservedStep)> {
    let calls = RefCell::new(Vec::new());
    let hidden = |observed: &ObservedStep, relabelled: &ReplayedStep| {
        calls.borrow_mut().push((
            observed.clone(),
            relabelled.state.clone(),
            relabelled.step.clone(),
        ));
        Vec::new()
    };
    let checker = Checker {
        hidden_cards: Some(&hidden),
        ..quiet()
    };
    run(&checker);
    calls.into_inner()
}

#[test]
fn the_hidden_cards_check_compares_the_real_observed_step_with_a_relabelled_game() {
    // On the Small Ruleset, which has no Active Zone, and on the Effects Ruleset, whose `field`
    // is its Active Zone: on a Game of the Effects Ruleset where a continuous card modifies a
    // visible card, so that the Derived Reads the check compares carry a Field Modifier.
    let (small_ruleset, small_decks) = small();
    let modified = assert_hidden_cards_wiring(&small_ruleset, &small_decks, SEED);
    assert!(!modified, "the Small Ruleset has no Field Modifier");

    let (ruleset, decks) = effects();
    let seed = (0..200)
        .map(Seed)
        .find(|seed| {
            let game = played_with(&ruleset, &decks, *seed);
            (0..2).any(|seat| {
                independent_fold(&ruleset, &game, viewer(seat))
                    .iter()
                    .any(|step| {
                        has_a_modified_read(
                            &ruleset,
                            &expected_derived_reads(
                                &ruleset,
                                &step.observer,
                                &step.state,
                                &step.next,
                            ),
                            &step.next,
                        )
                    })
            })
        })
        .expect("some Seed puts a continuous card in the field next to a card it modifies");
    assert!(
        assert_hidden_cards_wiring(&ruleset, &decks, seed),
        "seed {seed:?}: a Derived Read the check compared carries a Field Modifier"
    );
}

/// Whether one of `reads` of the Visible Cards of `observed` differs from the base value in the
/// catalog: a Field Modifier is active on it.
fn has_a_modified_read(
    ruleset: &DeclarativeRuleset,
    reads: &[(ObservedCard, FieldName, i64)],
    observed: &ObservedGameState,
) -> bool {
    reads.iter().any(|(card, field, read)| {
        observed
            .card(*card)
            .and_then(|visible| ruleset.catalog().card(visible.definition()))
            .and_then(|definition| {
                definition
                    .fields()
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, value)| value.clone())
            })
            .is_some_and(|base| base != FieldValue::Int(*read))
    })
}

/// Plays `seed` while recording the calls of the leak check and compares them with an independent
/// fold of the Game, for both Viewers at every step; verifying the record gives the very same
/// calls (the Relabelling is seeded from the Game's Seed and drawn in the same order). Returns
/// whether a Derived Read the harness handed to the check differs from its base value.
fn assert_hidden_cards_wiring(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    seed: Seed,
) -> bool {
    let game = played_with(ruleset, decks, seed);
    let actions = game.record.actions().len();
    let calls = hidden_cards_calls(|checker| {
        play(ruleset, decks, seed, MAX_STEPS, Some(checker)).expect("played");
    });
    assert_eq!(calls.len(), 2 * (actions + 1));

    // Verification reproduces the same Relabelling: the same real Observed Step, the same
    // relabelled GameState and the same relabelled Observed Step at every call.
    let during_verify = hidden_cards_calls(|checker| {
        verify(ruleset, &game.record, checker).expect("the record verifies");
    });
    assert!(
        during_verify == calls,
        "verification did not reproduce the Relabelling of self-play"
    );

    let mut definition_changed = false;
    let mut unseen_zone_changed = false;
    let mut modified_read = false;
    for seat in 0..2_usize {
        let who = viewer(seat as u32);
        let folded = independent_fold(ruleset, &game, who);

        // The cards this Viewer has seen at a step boundary: the ones in a zone it sees, then or
        // before. A card in none of them is hidden, or was seen in the middle of a trace only.
        let mut ever_seen: Vec<CardInstanceId> = Vec::new();
        for (step, expected) in folded.iter().enumerate() {
            ever_seen.extend(seen_cards(ruleset, who, &expected.state));
            let (observed, relabelled_state, relabelled_step) = &calls[2 * step + seat];

            // The observed side is the harness's real incremental Observed Step, as the
            // independent fold has it.
            assert_eq!(observed.viewer, who);
            assert_eq!(observed.events, expected.events, "seat {seat}, step {step}");
            assert_eq!(observed.taken, expected.taken, "seat {seat}, step {step}");
            assert_eq!(observed.state, expected.next, "seat {seat}, step {step}");
            let reads = expected_derived_reads(
                ruleset,
                &expected.observer,
                &expected.state,
                &expected.next,
            );
            assert_eq!(observed.derived_reads, reads, "seat {seat}, step {step}");
            modified_read |= has_a_modified_read(ruleset, &observed.derived_reads, &observed.state);
            // The real check found nothing, so the relabelled Observed Step is the same one.
            assert_eq!(relabelled_step, observed, "seat {seat}, step {step}");

            // The Relabelled Game is a Game of the same cards: every card the Viewer has seen is
            // unchanged and where it was; the Hidden Cards are not.
            assert_eq!(relabelled_state.cards().len(), expected.state.cards().len());
            for card in &ever_seen {
                assert_eq!(
                    relabelled_state.card(*card),
                    expected.state.card(*card),
                    "seat {seat}, step {step}: a seen card changed"
                );
            }
            for card in expected.state.cards() {
                if !ever_seen.contains(&card.id()) {
                    definition_changed |= relabelled_state
                        .card(card.id())
                        .is_some_and(|other| other.definition() != card.definition());
                }
            }
            for (other, own) in unseen_zones(ruleset, who, &expected.state) {
                unseen_zone_changed |=
                    relabelled_state.zone(other, &own) != expected.state.zone(other, &own);
            }
        }
    }
    // Over the Game the Relabelling changed what the Viewer cannot see: some Hidden Card has
    // another definition than the engine's, and some zone the Viewer cannot see holds other ids
    // in another order (the ids and the order of an unseen zone change together).
    assert!(
        definition_changed,
        "no Hidden Card was given another definition"
    );
    assert!(
        unseen_zone_changed,
        "no unseen zone was reordered or given other ids"
    );
    modified_read
}

/// Whether `who` sees the cards of `owner`'s zone `zone` (ADR-0004): `public` everyone, `owner`
/// its owner only, `hidden` nobody.
fn sees(ruleset: &DeclarativeRuleset, who: Viewer, owner: PlayerId, zone: &ZoneId) -> bool {
    match ruleset
        .definition()
        .zone(zone)
        .map(|declared| declared.visibility())
    {
        Some(Visibility::Public) => true,
        Some(Visibility::Owner) => who == Viewer::Player(owner),
        Some(Visibility::Hidden) | None => false,
    }
}

/// The cards in the zones `who` sees of `state`.
fn seen_cards(ruleset: &DeclarativeRuleset, who: Viewer, state: &GameState) -> Vec<CardInstanceId> {
    let mut cards = Vec::new();
    for &owner in state.player_order().players() {
        for zone in state.zones(owner).unwrap_or_default() {
            if sees(ruleset, who, owner, zone.zone()) {
                cards.extend_from_slice(zone.cards());
            }
        }
    }
    cards
}

/// Every (owner, zone) of `state` that `who` cannot see.
fn unseen_zones(
    ruleset: &DeclarativeRuleset,
    who: Viewer,
    state: &GameState,
) -> Vec<(PlayerId, ZoneId)> {
    let mut zones = Vec::new();
    for &owner in state.player_order().players() {
        for zone in state.zones(owner).unwrap_or_default() {
            if !sees(ruleset, who, owner, zone.zone()) {
                zones.push((owner, zone.zone().clone()));
            }
        }
    }
    zones
}

/// What the round-trip check was handed, in call order: the Observer, the GameState and the
/// listing.
type RoundTripCall = (Observer, GameState, Vec<Action>);

fn round_trip_calls(run: impl FnOnce(&Checker<'_>)) -> Vec<RoundTripCall> {
    let calls = RefCell::new(Vec::new());
    let observed_actions =
        |_: &DeclarativeRuleset, observer: &Observer, state: &GameState, legal: &[Action]| {
            calls
                .borrow_mut()
                .push((observer.clone(), state.clone(), legal.to_vec()));
            Vec::new()
        };
    let checker = Checker {
        observed_actions: Some(&observed_actions),
        ..quiet()
    };
    run(&checker);
    calls.into_inner()
}

#[test]
fn the_round_trip_check_receives_the_listing_of_the_checkpoint_game_state() {
    let (ruleset, decks) = small();
    let game = played(&ruleset, &decks);
    let actions = game.record.actions().len();
    let during_play = round_trip_calls(|checker| {
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(checker)).expect("played");
    });
    // One call per Viewer, in seat order, for the GameState at the Opening Position and after
    // every Action, the ended one included.
    assert_eq!(during_play.len(), 2 * (actions + 1));
    for seat in 0..2_usize {
        let who = viewer(seat as u32);
        let folded = independent_fold(&ruleset, &game, who);
        for (step, expected) in folded.iter().enumerate() {
            let (observer, state, legal) = &during_play[2 * step + seat];
            assert_eq!(observer.viewer(), who, "step {step}");
            assert!(
                state == &expected.state,
                "seat {seat}, step {step}: the GameState"
            );
            assert_eq!(
                legal,
                &ruleset.legal_actions(&expected.state),
                "seat {seat}, step {step}"
            );
            assert_eq!(
                ruleset.observe(observer, state),
                Ok(expected.next.clone()),
                "seat {seat}, step {step}: the Observer has followed the GameState"
            );
        }
    }
    // The ended GameState has no Legal Action.
    assert!(during_play[during_play.len() - 1].2.is_empty());

    let during_verify = round_trip_calls(|checker| {
        verify(&ruleset, &game.record, checker).expect("the record verifies");
    });
    assert_eq!(during_verify.len(), during_play.len());
    for (one, other) in during_verify.iter().zip(&during_play) {
        assert!(one.0 == other.0 && one.1 == other.1 && one.2 == other.2);
    }
}

fn violation(invariant: Invariant, seat: u32) -> InvariantViolation {
    InvariantViolation {
        invariant,
        viewer: Some(viewer(seat)),
        detail: "injected".to_string(),
    }
}

/// Runs `body` with a consistency check that reports a violation for seat 1 at its call number
/// `at` (calls count from 0: two per step, seat 0 then seat 1, the Opening Position first). Each
/// call of this function starts counting again.
fn with_consistency_violation_at<R>(at: usize, body: impl FnOnce(&Checker<'_>) -> R) -> R {
    let calls = Cell::new(0);
    let consistency =
        |_: Viewer, _: &ObservedGameState, _: &[ObservedEvent], _: &ObservedGameState| {
            let call = calls.get();
            calls.set(call + 1);
            if call == at {
                vec![violation(Invariant::ObservationsStayConsistent, 1)]
            } else {
                Vec::new()
            }
        };
    let checker = Checker {
        consistency: Some(&consistency),
        ..quiet()
    };
    body(&checker)
}

#[test]
fn an_observation_violation_reports_seed_action_index_viewer_and_a_reproducing_record() {
    let (ruleset, decks) = small();
    let full = played(&ruleset, &decks);
    let k = 3;
    assert!(full.record.actions().len() > k + 1);

    // Seat 1's check at the step after Action index `k` is call 2 * (k + 1) + 1.
    let at = 2 * (k + 1) + 1;
    let failure = with_consistency_violation_at(at, |checker| {
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(checker))
            .expect_err("the injected violation fails the Game")
    });
    assert_eq!(failure.seed, SEED);
    assert_eq!(failure.action_index, Some(k));
    assert_eq!(
        failure.kind,
        FailureKind::InvariantViolated(vec![violation(Invariant::ObservationsStayConsistent, 1)])
    );
    assert_eq!(
        failure.kind,
        FailureKind::InvariantViolated(vec![InvariantViolation {
            invariant: Invariant::ObservationsStayConsistent,
            viewer: Some(Viewer::Player(PlayerId(1))),
            detail: "injected".to_string(),
        }])
    );
    // The record is the Actions up to and including Action `k`.
    assert_eq!(failure.record.actions(), &full.record.actions()[..=k]);

    // Verifying that record fails with the same violation at the same index; with the real
    // checks it verifies.
    let verified = with_consistency_violation_at(at, |checker| {
        verify(&ruleset, &failure.record, checker).expect_err("the record fails verification")
    });
    assert_eq!(verified, failure);
    verify(&ruleset, &failure.record, &REAL_CHECKS).expect("the real checks find nothing");

    // A longer record is cut after the Action that caused it.
    let cut = with_consistency_violation_at(at, |checker| {
        verify(&ruleset, &full.record, checker).expect_err("the record fails verification")
    });
    assert_eq!(cut, failure);

    // At the Opening Position there is no Action index and the record is empty.
    let opening = with_consistency_violation_at(1, |checker| {
        play(&ruleset, &decks, SEED, MAX_STEPS, Some(checker))
            .expect_err("the Opening Position violates")
    });
    assert_eq!(opening.action_index, None);
    assert!(opening.record.actions().is_empty());
}

#[test]
fn a_step_reports_its_observation_violations_ordered_by_invariant_and_then_by_seat() {
    let (ruleset, decks) = small();
    // At the Opening Position both Viewers' consistency checks and seat 0's leak check report.
    let consistency =
        |who: Viewer, _: &ObservedGameState, _: &[ObservedEvent], _: &ObservedGameState| {
            let seat = u32::from(who == viewer(1));
            vec![violation(Invariant::ObservationsStayConsistent, seat)]
        };
    let hidden = |observed: &ObservedStep, _: &ReplayedStep| {
        let seat = u32::from(observed.viewer == viewer(1));
        if seat == 0 {
            vec![violation(Invariant::ObservationsIgnoreHiddenCards, seat)]
        } else {
            Vec::new()
        }
    };
    let checker = Checker {
        consistency: Some(&consistency),
        hidden_cards: Some(&hidden),
        ..quiet()
    };
    let failure = play(&ruleset, &decks, SEED, MAX_STEPS, Some(&checker))
        .expect_err("the Opening Position violates");
    assert_eq!(failure.action_index, None);
    assert_eq!(
        failure.kind,
        FailureKind::InvariantViolated(vec![
            violation(Invariant::ObservationsStayConsistent, 0),
            violation(Invariant::ObservationsStayConsistent, 1),
            violation(Invariant::ObservationsIgnoreHiddenCards, 0),
        ])
    );
}

#[test]
fn engine_only_checks_observe_nothing_and_the_real_checks_run_all_three_observation_checks() {
    assert!(REAL_CHECKS.consistency.is_some());
    assert!(REAL_CHECKS.hidden_cards.is_some());
    assert!(REAL_CHECKS.observed_actions.is_some());
    assert!(ENGINE_ONLY_CHECKS.consistency.is_none());
    assert!(ENGINE_ONLY_CHECKS.hidden_cards.is_none());
    assert!(ENGINE_ONLY_CHECKS.observed_actions.is_none());
}
