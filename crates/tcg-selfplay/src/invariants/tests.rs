//! Unit tests of `state_difference`, the part of the Recomputed GameState check that names where
//! two GameStates differ. It is private, so its tests live in the crate.

use tcg_engine::declarative::{Event, GameState};
use tcg_engine::random::Seed;
use tcg_engine::ruleset_definition::PhaseId;

use super::state_difference;
use crate::test_support::effects;

/// The Opening Position of an Effects Game: its first Turn has begun and the Game waits in `main`.
fn opening() -> GameState {
    let (ruleset, decks) = effects();
    ruleset
        .create_game(&decks, Seed(1))
        .expect("the sample decks create a game")
        .opening
        .state
}

fn after(state: &GameState, event: &Event) -> GameState {
    let mut next = state.clone();
    next.apply(event).expect("the Event applies");
    next
}

#[test]
fn state_difference_names_a_differing_phase() {
    let opening = opening();
    assert_eq!(opening.phase(), Some(&PhaseId::new("main")));
    let started = after(
        &opening,
        &Event::PhaseBegan {
            phase: PhaseId::new("start"),
        },
    );
    // Everything but the phase is equal, so nothing else can be named.
    assert_eq!(opening.turn_number(), started.turn_number());
    assert_eq!(opening.active_player(), started.active_player());
    assert_eq!(opening.cards(), started.cards());

    assert_eq!(
        state_difference(&opening, &started).as_deref(),
        Some(r#"the phase is "main" in the Recomputed GameState and "start" in the engine's"#)
    );
    assert_eq!(
        state_difference(&started, &opening).as_deref(),
        Some(r#"the phase is "start" in the Recomputed GameState and "main" in the engine's"#)
    );

    // A Turn that has begun and has no phase yet differs from one in a phase.
    let turn = opening.turn_number().expect("the first Turn has begun");
    let turn_began = after(
        &opening,
        &Event::TurnBegan {
            player: opening.active_player(),
            turn,
        },
    );
    assert_eq!(turn_began.phase(), None);
    assert_eq!(
        state_difference(&opening, &turn_began).as_deref(),
        Some(r#"the phase is "main" in the Recomputed GameState and none in the engine's"#)
    );
    assert_eq!(
        state_difference(&turn_began, &opening).as_deref(),
        Some(r#"the phase is none in the Recomputed GameState and "main" in the engine's"#)
    );

    // Equal GameStates have no difference.
    assert_eq!(state_difference(&opening, &opening), None);
}

// ---- Player state (ticket #96, step 4, ADR-0041)

/// The Opening Position of a Game of the Player-state fixture, and the transition of seat 0
/// draining its opponent, which changes only Player state.
fn drained() -> (
    tcg_engine::declarative::DeclarativeRuleset,
    GameState,
    tcg_engine::ruleset::Transition<GameState, Event>,
) {
    use tcg_engine::declarative::Action;
    use tcg_engine::player::PlayerId;
    use tcg_engine::ruleset::Ruleset;
    use tcg_engine::ruleset_definition::ActionId;

    let (ruleset, decks) = crate::test_support::player_state();
    let opening = ruleset
        .create_game(&decks, Seed(1))
        .expect("the decks create a game")
        .opening
        .state;
    let transition = ruleset
        .transition(
            &opening,
            &Action::Declared {
                player: PlayerId(0),
                action: ActionId::new("drain"),
                arguments: Vec::new(),
            },
        )
        .expect("the Action is accepted");
    (ruleset, opening, transition)
}

#[test]
fn state_difference_names_a_differing_player_state() {
    let (_, opening, transition) = drained();
    // Everything but one Player's `life_points` is equal.
    assert_eq!(opening.cards(), transition.state.cards());
    assert_eq!(opening.phase(), transition.state.phase());

    assert_eq!(
        state_difference(&opening, &transition.state).as_deref(),
        Some(
            "the Player state `life_points` of Player 1 is Int(4000) in the Recomputed GameState and Int(3700) in the engine's"
        )
    );
    assert_eq!(state_difference(&transition.state, &transition.state), None);
}

#[test]
fn event_sourced_consistency_detects_a_player_state_not_explained_by_its_events() {
    use tcg_engine::ruleset::EventTrace;

    let (ruleset, opening, transition) = drained();
    // The trace of the transition explains the next GameState.
    assert_eq!(
        crate::check_transition(&ruleset, &opening, &transition.trace, &transition.state),
        Vec::new()
    );

    // A trace that does not hold the `PlayerStateChanged` leaves the change unexplained.
    let violations = crate::check_transition(
        &ruleset,
        &opening,
        &EventTrace::new(Vec::new()),
        &transition.state,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.invariant == crate::Invariant::EventSourcedConsistency),
        "{violations:?}"
    );
}
