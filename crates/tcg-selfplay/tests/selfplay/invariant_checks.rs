//! The engine invariants (ADR-0020). Each test that expects a violation builds its input through
//! the public API only: a tampered Event Trace, GameStates of different Games, or a Ruleset that
//! declares something else than the Game was made with.
//!
//! Invariants 2 (each instance in exactly one zone) and 6 (an ended Game has no Legal Actions)
//! cannot be violated through the public API, so they have no "detects" test (ADR-0020).

use tcg_engine::declarative::{Event, GameCreation, GameState};
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{EventTrace, Transition};
use tcg_engine::ruleset_definition::ZoneId;
use tcg_selfplay::{
    check_opening, check_transition, self_play, Checks, Invariant, InvariantViolation,
};

use crate::common::{decks, small, small_with_ruleset, steps, swapped_decks, Step, MAX_STEPS};

/// The Invariants of `violations`, in the order reported.
fn invariants(violations: &[InvariantViolation]) -> Vec<Invariant> {
    violations
        .iter()
        .map(|violation| violation.invariant)
        .collect()
}

fn empty_trace() -> EventTrace<Event> {
    EventTrace::new(Vec::new())
}

#[test]
fn the_opening_position_satisfies_every_invariant() {
    let ruleset = small();
    let decks = decks(&ruleset);
    for seed in 0..5 {
        let creation = ruleset
            .create_game(&decks, Seed(seed))
            .expect("the sample decks create a game");
        assert_eq!(
            check_opening(&ruleset, &decks, &creation),
            Vec::new(),
            "seed {seed}"
        );
    }
}

#[test]
fn every_transition_of_a_played_game_satisfies_every_invariant() {
    let ruleset = small();
    let game = self_play(&ruleset, &decks(&ruleset), Seed(1), MAX_STEPS, Checks::On)
        .expect("the Game is played");
    for (index, step) in steps(&ruleset, &game).iter().enumerate() {
        assert_eq!(
            check_transition(&ruleset, &step.prior, &step.trace, &step.next),
            Vec::new(),
            "Action index {index}"
        );
    }
}

#[test]
fn invariant_no_card_gained_or_lost_detects_a_violation() {
    let ruleset = small();
    let decks_ab = decks(&ruleset);
    let creation_ab = ruleset.create_game(&decks_ab, Seed(1)).expect("creates");
    let creation_ba = ruleset
        .create_game(&swapped_decks(&ruleset), Seed(1))
        .expect("creates");

    // Across a transition: the next GameState holds other cards than the prior one.
    let across = check_transition(
        &ruleset,
        &creation_ab.opening.state,
        &empty_trace(),
        &creation_ba.opening.state,
    );
    assert!(
        invariants(&across).contains(&Invariant::NoCardGainedOrLost),
        "{across:?}"
    );

    // At the opening: the cards are not those of the Deck Lists.
    let opening = check_opening(&ruleset, &decks_ab, &creation_ba);
    assert!(
        invariants(&opening).contains(&Invariant::NoCardGainedOrLost),
        "{opening:?}"
    );
}

#[test]
fn invariant_ordered_zones_keep_their_order_rules_detects_a_violation() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let creation = ruleset.create_game(&decks, Seed(1)).expect("creates");

    // The creation trace without its shuffles: the decks stay in creation order, but the
    // Opening Position holds them shuffled.
    let unshuffled: Vec<Event> = creation
        .opening
        .trace
        .events()
        .iter()
        .filter(|event| !matches!(event, Event::ZoneShuffled { .. }))
        .cloned()
        .collect();
    assert!(unshuffled.len() < creation.opening.trace.events().len());
    let tampered = GameCreation {
        empty: creation.empty.clone(),
        opening: Transition {
            state: creation.opening.state.clone(),
            trace: EventTrace::new(unshuffled),
        },
    };
    let violations = check_opening(&ruleset, &decks, &tampered);
    assert!(
        invariants(&violations).contains(&Invariant::OrderedZonesKeepOrder),
        "{violations:?}"
    );

    // A shuffle of an unordered zone is a violation too (ADR-0013), though it applies cleanly.
    let state = &creation.opening.state;
    let hand = ZoneId::new("hand");
    let player = state.player_order().players()[0];
    let shuffle = Event::ZoneShuffled {
        player,
        zone: hand.clone(),
        order: state.zone(player, &hand).expect("a hand").to_vec(),
        random_state: state.random_state().clone(),
    };
    let trace = EventTrace::new(vec![shuffle]);
    let next = state
        .apply_trace(trace.events())
        .expect("the shuffle applies");
    let violations = check_transition(&ruleset, state, &trace, &next);
    assert!(
        invariants(&violations).contains(&Invariant::OrderedZonesKeepOrder),
        "{violations:?}"
    );
}

/// The first accepted transition of a Game whose Event Trace is not empty and changed the state.
fn a_step_that_changes_the_state() -> (tcg_engine::declarative::DeclarativeRuleset, Step) {
    let ruleset = small();
    let game = self_play(&ruleset, &decks(&ruleset), Seed(1), MAX_STEPS, Checks::On)
        .expect("the Game is played");
    let step = steps(&ruleset, &game)
        .into_iter()
        .find(|step| !step.trace.events().is_empty() && step.prior != step.next)
        .expect("a Game changes its state");
    (ruleset, step)
}

#[test]
fn invariant_event_sourced_consistency_detects_a_violation() {
    let (ruleset, step) = a_step_that_changes_the_state();
    assert_eq!(
        check_transition(&ruleset, &step.prior, &step.trace, &step.next),
        Vec::new()
    );

    // The trace says nothing happened, but the next GameState differs from the prior one.
    let violations = check_transition(&ruleset, &step.prior, &empty_trace(), &step.next);
    assert!(
        invariants(&violations).contains(&Invariant::EventSourcedConsistency),
        "{violations:?}"
    );

    // The same trace and the right next GameState, but from another prior GameState.
    let other_prior: &GameState = &step.next;
    let violations = check_transition(&ruleset, other_prior, &step.trace, &step.next);
    assert!(
        invariants(&violations).contains(&Invariant::EventSourcedConsistency),
        "{violations:?}"
    );
}

#[test]
fn invariant_instances_carry_only_the_states_their_type_declares_detects_a_violation() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let creation = ruleset.create_game(&decks, Seed(1)).expect("creates");

    // A Ruleset whose creature declares its two instance states in the other order: the
    // creatures of the Game, made from the real Ruleset, no longer match their type's schema.
    let reordered = small_with_ruleset(
        "      - { name: tapped,           type: bool, init: false }\n      - { name: recently_played,  type: bool, init: false }",
        "      - { name: recently_played,  type: bool, init: false }\n      - { name: tapped,           type: bool, init: false }",
    );
    let violations = check_opening(&reordered, &decks, &creation);
    assert!(
        invariants(&violations).contains(&Invariant::InstanceStatesMatchType),
        "{violations:?}"
    );
    assert_eq!(check_opening(&ruleset, &decks, &creation), Vec::new());
}

#[test]
fn invariant_turn_and_active_player_consistent_detects_a_violation() {
    let ruleset = small();
    let decks = decks(&ruleset);
    let creation = ruleset.create_game(&decks, Seed(1)).expect("creates");

    // A Ruleset whose starting Player is seat 1: Turn 1 of the Game, which seat 0 has, no
    // longer follows from the Player Order.
    let other_start = small_with_ruleset("starting_player: 0", "starting_player: 1");
    let violations = check_opening(&other_start, &decks, &creation);
    assert!(
        invariants(&violations).contains(&Invariant::TurnAndActivePlayerConsistent),
        "{violations:?}"
    );

    // The Turn Number never decreases across a transition.
    let game = self_play(&ruleset, &decks, Seed(1), MAX_STEPS, Checks::On).expect("played");
    let step = a_pass_turn_step(&ruleset, &game);
    let violations = check_transition(&ruleset, &step.next, &empty_trace(), &step.prior);
    assert!(
        invariants(&violations).contains(&Invariant::TurnAndActivePlayerConsistent),
        "{violations:?}"
    );
}

#[test]
fn check_transition_returns_every_violation_in_ticket_order() {
    let ruleset = small();
    let game = self_play(&ruleset, &decks(&ruleset), Seed(1), MAX_STEPS, Checks::On)
        .expect("the Game is played");
    let step = a_pass_turn_step(&ruleset, &game);

    // Backwards: from the GameState after a pass to the one before it, with no Event. The decks
    // and the mana decks are not what the trace gives (3), applying the trace does not give the
    // next GameState (5) and the Turn Number goes down (7). Nothing else is wrong.
    let violations = check_transition(&ruleset, &step.next, &empty_trace(), &step.prior);
    assert_eq!(
        invariants(&violations),
        vec![
            Invariant::OrderedZonesKeepOrder,
            Invariant::EventSourcedConsistency,
            Invariant::TurnAndActivePlayerConsistent,
        ],
        "{violations:?}"
    );
}

/// The first transition of `game` that began another Turn.
fn a_pass_turn_step(
    ruleset: &tcg_engine::declarative::DeclarativeRuleset,
    game: &tcg_selfplay::SelfPlayGame,
) -> Step {
    steps(ruleset, game)
        .into_iter()
        .find(|step| step.prior.turn_number() != step.next.turn_number())
        .expect("a Game passes the turn")
}
