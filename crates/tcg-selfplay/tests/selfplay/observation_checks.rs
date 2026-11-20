//! The Invariants over Observations (ADR-0031), on the real Effects Ruleset: the two checks that
//! take only public inputs, each shown catching a violation built through the public API, and the
//! order of the twelve Invariants. The leak check needs the Relabelling, which is crate-level, so
//! its test is a unit test (`src/observations/tests.rs`); the harness's wiring of the three checks
//! is pinned in `src/harness/observation_tests.rs`.

use tcg_engine::declarative::{
    Action, DeclarativeRuleset, Event, GameState, ObservedEvent, ObservedGameState, Observer,
    Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset};
use tcg_engine::ruleset_definition::ZoneId;
use tcg_selfplay::{
    check_observed_actions, check_observed_consistency, self_play, Checks, Invariant, SelfPlayGame,
};

use crate::common::{effects, effects_decks, steps, MAX_STEPS};

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(PlayerId(seat))
}

fn effects_game(seed: u64) -> (DeclarativeRuleset, SelfPlayGame) {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    // The Game is played without the checks under test: they are what these tests exercise.
    let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::Off).expect("played");
    (ruleset, game)
}

#[test]
fn the_three_observation_invariants_follow_the_nine_engine_ones_in_ticket_order() {
    let ordered = [
        Invariant::NoCardGainedOrLost,
        Invariant::EachInstanceInOneZone,
        Invariant::OrderedZonesKeepOrder,
        Invariant::InstanceStatesMatchType,
        Invariant::EventSourcedConsistency,
        Invariant::EndedGameHasNoLegalActions,
        Invariant::TurnAndActivePlayerConsistent,
        Invariant::DerivedReadsDoNotLeak,
        Invariant::RejectionLeavesStateIntact,
        Invariant::ObservationsStayConsistent,
        Invariant::ObservationsIgnoreHiddenCards,
        Invariant::ObservedActionsRoundTrip,
    ];
    assert_eq!(ordered.len(), 12);
    assert!(
        ordered.windows(2).all(|pair| pair[0] < pair[1]),
        "the Invariants are declared in ticket order"
    );
}

/// What one Viewer observed at one step, built with the public API: the previous Observed
/// GameState, the Observed Events and the next Observed GameState.
struct Observed {
    previous: ObservedGameState,
    events: Vec<ObservedEvent>,
    next: ObservedGameState,
}

/// The Opening Position first, then one entry per accepted Action.
fn observed_steps(
    ruleset: &DeclarativeRuleset,
    game: &SelfPlayGame,
    viewer: Viewer,
) -> Vec<Observed> {
    let empty = &game.creation.empty;
    let mut observer = ruleset.observer(viewer, empty, &[]).expect("the Observer");
    let previous = ruleset
        .observe(&observer, empty)
        .expect("the Empty GameState is observed");
    let events = ruleset
        .follow_events(&mut observer, empty, game.creation.opening.trace.events())
        .expect("the creation trace is followed");
    let next = ruleset
        .observe(&observer, &game.creation.opening.state)
        .expect("the Opening Position is observed");
    let mut observed = vec![Observed {
        previous,
        events,
        next,
    }];
    for (action, step) in game.record.actions().iter().zip(steps(ruleset, game)) {
        let transition = ruleset
            .transition(&step.prior, action)
            .expect("a recorded Action is accepted");
        let followed = ruleset
            .follow_transition(&mut observer, &step.prior, action, &transition)
            .expect("the transition is followed");
        let previous = observed
            .last()
            .map(|last| last.next.clone())
            .expect("the Opening Position is there");
        observed.push(Observed {
            previous,
            events: followed.events,
            next: followed.state,
        });
    }
    observed
}

#[test]
fn invariant_observations_stay_consistent_detects_a_violation() {
    let (ruleset, game) = effects_game(3);
    let mut dropped_events_caught = 0;
    for seat in 0..2 {
        let who = viewer(seat);
        let observed = observed_steps(&ruleset, &game, who);
        // The Opening Position, and the first transition with several Observed Events.
        let transition = observed
            .iter()
            .skip(1)
            .find(|step| step.events.len() >= 3)
            .expect("a transition with several Observed Events");
        for step in [&observed[0], transition] {
            // The real step is consistent.
            assert_eq!(
                check_observed_consistency(who, &step.previous, &step.events, &step.next),
                Vec::new(),
                "seat {seat}"
            );

            // (a) With one Observed Event dropped the Events no longer lead to `next`: one
            // violation of this Invariant for this Viewer, naming what differs, unless the Event
            // changed nothing.
            for index in 0..step.events.len() {
                let mut events = step.events.clone();
                events.remove(index);
                let violations =
                    check_observed_consistency(who, &step.previous, &events, &step.next);
                match step.previous.apply_events(&events) {
                    Ok(state) if state == step.next => {
                        assert_eq!(violations, Vec::new(), "Event {index} changes nothing");
                    }
                    other => {
                        dropped_events_caught += 1;
                        assert_eq!(violations.len(), 1, "seat {seat}, Event {index} dropped");
                        assert_eq!(
                            violations[0].invariant,
                            Invariant::ObservationsStayConsistent
                        );
                        assert_eq!(violations[0].viewer, Some(who));
                        assert_eq!(
                            violations[0].detail.contains("do not apply"),
                            other.is_err(),
                            "{}",
                            violations[0].detail
                        );
                    }
                }
            }

            // (b) A `next` altered in one zone (a card made out of nothing in the opponent's
            // deck): one violation, naming the zone.
            let mut altered = step.next.clone();
            altered
                .apply(&ObservedEvent::UnseenCardCreated {
                    owner: PlayerId(1 - seat),
                    zone: ZoneId::new("deck"),
                })
                .expect("a card is created in the opponent's deck");
            let violations =
                check_observed_consistency(who, &step.previous, &step.events, &altered);
            assert_eq!(violations.len(), 1, "seat {seat}: {violations:?}");
            assert_eq!(
                violations[0].invariant,
                Invariant::ObservationsStayConsistent
            );
            assert_eq!(violations[0].viewer, Some(who));
            assert!(
                violations[0].detail.contains("deck"),
                "the detail names the zone: {}",
                violations[0].detail
            );
        }
    }
    assert!(dropped_events_caught > 0);
}

/// The Observer of `who` after the first `actions` Actions of `game`, and the GameState there.
fn observer_after(
    ruleset: &DeclarativeRuleset,
    game: &SelfPlayGame,
    actions: usize,
    who: Viewer,
) -> (Observer, GameState) {
    let mut events: Vec<Event> = game.creation.opening.trace.events().to_vec();
    for trace in &game.traces[..actions] {
        events.extend_from_slice(trace.events());
    }
    let state = if actions == 0 {
        game.creation.opening.state.clone()
    } else {
        steps(ruleset, game)[actions - 1].next.clone()
    };
    let observer = ruleset
        .observer(who, &game.creation.empty, &events)
        .expect("the Observer");
    (observer, state)
}

#[test]
fn invariant_observed_actions_round_trip_detects_a_violation() {
    let (ruleset, game) = effects_game(3);
    // A GameState whose active Player has at least three different Legal Actions.
    let (actions, legal) = (0..=game.record.actions().len())
        .map(|actions| {
            let (_, state) = observer_after(&ruleset, &game, actions, viewer(0));
            (actions, ruleset.legal_actions(&state))
        })
        .find(|(_, legal)| legal.len() >= 3 && legal[1] != legal[2])
        .expect("a GameState with several Legal Actions");
    let (observer, state) = {
        let acting = legal[0].player();
        observer_after(&ruleset, &game, actions, Viewer::Player(acting))
    };
    let who = observer.viewer();
    let n = legal.len();

    // The real listing round-trips.
    assert_eq!(
        check_observed_actions(&ruleset, &observer, &state, &legal),
        Vec::new()
    );
    // So does the other Player's: it has no Legal Action, and the check filters to the Viewer's.
    let (other_observer, _) = observer_after(
        &ruleset,
        &game,
        actions,
        Viewer::Player(PlayerId(1 - legal[0].player().0)),
    );
    assert_eq!(
        check_observed_actions(&ruleset, &other_observer, &state, &legal),
        Vec::new()
    );

    let one_violation = |violations: Vec<tcg_selfplay::InvariantViolation>, index: usize| {
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].invariant, Invariant::ObservedActionsRoundTrip);
        assert_eq!(violations[0].viewer, Some(who));
        assert!(
            violations[0].detail.contains(&format!("index {index}")),
            "the detail names index {index}: {}",
            violations[0].detail
        );
    };

    // Reordered: the Legal Actions at index 1 and 2 swap places.
    let mut reordered: Vec<Action> = legal.clone();
    reordered.swap(1, 2);
    one_violation(
        check_observed_actions(&ruleset, &observer, &state, &reordered),
        1,
    );

    // Short by one: the last Observed Legal Action has no Action to match.
    one_violation(
        check_observed_actions(&ruleset, &observer, &state, &legal[..n - 1]),
        n - 1,
    );

    // With an extra Action at the end: an Action the Observed Legal Actions do not have.
    let mut extended = legal.clone();
    extended.push(legal[0].clone());
    one_violation(
        check_observed_actions(&ruleset, &observer, &state, &extended),
        n,
    );

    // An error of the Observer is the violation: an Observer that stopped following at the
    // Opening Position cannot list the Legal Actions of a later GameState whose cards it never
    // saw (a card drawn since, for one).
    let (stale, _) = observer_after(&ruleset, &game, 0, viewer(0));
    let (later, legal) = (1..game.record.actions().len())
        .map(|actions| {
            let (_, state) = observer_after(&ruleset, &game, actions, viewer(0));
            let legal = ruleset.legal_actions(&state);
            (state, legal)
        })
        .find(|(state, _)| ruleset.observed_legal_actions(&stale, state).is_err())
        .expect("a later GameState the stale Observer cannot list");
    let violations = check_observed_actions(&ruleset, &stale, &later, &legal);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(violations[0].invariant, Invariant::ObservedActionsRoundTrip);
    assert_eq!(violations[0].viewer, Some(viewer(0)));
    assert!(
        violations[0].detail.contains("ObserverOutOfStep"),
        "{}",
        violations[0].detail
    );
}
