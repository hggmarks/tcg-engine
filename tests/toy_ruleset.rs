//! Acceptance tests for the Toy Ruleset running through the generic Runtime.
//!
//! Player identities are deliberately unsorted (7 first, then 3) so the
//! caller's Player Order is visible in every assertion.

use tcg_engine::player::{PlayerId, PlayerOrder, PlayerOrderError};
use tcg_engine::ruleset::{EventTrace, Transition};
use tcg_engine::runtime::Runtime;
use tcg_engine::toy::{
    LifeTotal, PlayerState, ToyAction, ToyEvent, ToyGameState, ToyGameStateError, ToyRejection,
    ToyRuleset, STARTING_LIFE_TOTAL,
};
use tcg_engine::turn::TurnNumber;

type ToyTransition = Transition<ToyGameState, ToyEvent>;

fn p(id: u32) -> PlayerId {
    PlayerId(id)
}

fn order(ids: &[u32]) -> PlayerOrder {
    PlayerOrder::new(ids.iter().copied().map(PlayerId).collect()).expect("valid Player Order")
}

fn turn(value: u32) -> TurnNumber {
    TurnNumber::new(value).expect("non-zero Turn Number")
}

fn ps(id: u32, life: i64) -> PlayerState {
    PlayerState {
        player: p(id),
        life_total: LifeTotal(life),
    }
}

/// A valid two-Player state in the order [7, 3].
fn state(active: u32, turn_number: u32, life_7: i64, life_3: i64) -> ToyGameState {
    ToyGameState::new(
        order(&[7, 3]),
        p(active),
        turn(turn_number),
        vec![ps(7, life_7), ps(3, life_3)],
    )
    .expect("valid Toy GameState")
}

fn initial() -> ToyGameState {
    state(7, 1, 20, 20)
}

fn runtime() -> Runtime<ToyRuleset> {
    Runtime::new(ToyRuleset)
}

fn pass(player: u32) -> ToyAction {
    ToyAction::PassTurn { player: p(player) }
}

fn deal(source: u32, target: u32, amount: i64) -> ToyAction {
    ToyAction::DealDamage {
        source: p(source),
        target: p(target),
        amount,
    }
}

fn apply(state: &ToyGameState, action: &ToyAction) -> Result<ToyTransition, ToyRejection> {
    runtime().apply(state, action)
}

fn accept(state: &ToyGameState, action: &ToyAction) -> ToyTransition {
    apply(state, action).expect("Action is accepted")
}

fn assert_rejected(state: ToyGameState, action: ToyAction, expected: ToyRejection) {
    let before = state.clone();
    assert_eq!(apply(&state, &action), Err(expected));
    assert_eq!(state, before);
}

fn turn_passed(previous: u32, next: u32, turn_number: u32) -> ToyEvent {
    ToyEvent::TurnPassed {
        previous: p(previous),
        next: p(next),
        turn_number: turn(turn_number),
    }
}

fn damage_dealt(source: u32, target: u32, amount: i64, resulting: i64) -> ToyEvent {
    ToyEvent::DamageDealt {
        source: p(source),
        target: p(target),
        amount,
        resulting_life_total: LifeTotal(resulting),
    }
}

// ---------------------------------------------------------------- creation

#[test]
fn toy_game_creation_makes_first_player_active_on_turn_one_with_twenty_life() {
    let created = ToyRuleset.create_game(order(&[7, 3])).expect("two Players");

    let expected = ToyGameState::new(
        order(&[7, 3]),
        p(7),
        TurnNumber::FIRST,
        vec![ps(7, 20), ps(3, 20)],
    )
    .expect("valid Toy GameState");
    assert_eq!(created, expected);
    assert_eq!(STARTING_LIFE_TOTAL, LifeTotal(20));
}

#[test]
fn toy_game_creation_requires_exactly_two_players() {
    assert_eq!(
        ToyRuleset.create_game(order(&[7])),
        Err(ToyGameStateError::WrongPlayerCount { count: 1 })
    );
    assert_eq!(
        ToyRuleset.create_game(order(&[7, 3, 5])),
        Err(ToyGameStateError::WrongPlayerCount { count: 3 })
    );
}

// ---------------------------------------------------------------- validity

#[test]
fn player_order_rejects_empty_and_duplicate_players() {
    assert_eq!(PlayerOrder::new(vec![]), Err(PlayerOrderError::Empty));
    assert_eq!(
        PlayerOrder::new(vec![p(1), p(2), p(1)]),
        Err(PlayerOrderError::DuplicatePlayer { player: p(1) })
    );
    // The first repeat in list order, not the first value that repeats.
    assert_eq!(
        PlayerOrder::new(vec![p(1), p(2), p(2), p(1)]),
        Err(PlayerOrderError::DuplicatePlayer { player: p(2) })
    );

    let kept = PlayerOrder::new(vec![p(7), p(3), p(5)]).expect("distinct Players");
    assert_eq!(kept.players(), &[p(7), p(3), p(5)]);
    assert!(kept.contains(p(5)));
    assert!(!kept.contains(p(99)));
    // Cyclic succession follows the caller's order, not sorted ids.
    assert_eq!(kept.next_after(p(7)), Some(p(3)));
    assert_eq!(kept.next_after(p(3)), Some(p(5)));
    assert_eq!(kept.next_after(p(5)), Some(p(7)));
    assert_eq!(kept.next_after(p(99)), None);
}

#[test]
fn turn_number_is_one_based_and_checked() {
    assert_eq!(TurnNumber::new(0), None);
    assert_eq!(TurnNumber::FIRST.get(), 1);
    assert_eq!(TurnNumber::new(1), Some(TurnNumber::FIRST));
    assert_eq!(TurnNumber::FIRST.checked_next(), TurnNumber::new(2));
    assert_eq!(
        TurnNumber::new(u32::MAX).and_then(TurnNumber::checked_next),
        None
    );
}

#[test]
fn game_state_rejects_invalid_combinations() {
    let two = vec![ps(7, 20), ps(3, 20)];

    assert_eq!(
        ToyGameState::new(
            order(&[7, 3, 5]),
            p(7),
            TurnNumber::FIRST,
            vec![ps(7, 20), ps(3, 20), ps(5, 20)]
        ),
        Err(ToyGameStateError::WrongPlayerCount { count: 3 })
    );
    assert_eq!(
        ToyGameState::new(order(&[7, 3]), p(9), TurnNumber::FIRST, two.clone()),
        Err(ToyGameStateError::ActivePlayerNotInPlayerOrder { player: p(9) })
    );
    assert_eq!(
        ToyGameState::new(order(&[7, 3]), p(7), TurnNumber::FIRST, vec![ps(7, 20)]),
        Err(ToyGameStateError::PlayerStateCountMismatch {
            players: 2,
            player_states: 1
        })
    );
    assert_eq!(
        ToyGameState::new(
            order(&[7, 3]),
            p(7),
            TurnNumber::FIRST,
            vec![ps(7, 20), ps(3, 20), ps(9, 20)]
        ),
        Err(ToyGameStateError::PlayerStateCountMismatch {
            players: 2,
            player_states: 3
        })
    );
    assert_eq!(
        ToyGameState::new(
            order(&[7, 3]),
            p(7),
            TurnNumber::FIRST,
            vec![ps(3, 20), ps(7, 20)]
        ),
        Err(ToyGameStateError::PlayerStateMisaligned {
            position: 0,
            expected: p(7),
            found: p(3)
        })
    );
    assert_eq!(
        ToyGameState::new(
            order(&[7, 3]),
            p(7),
            TurnNumber::FIRST,
            vec![ps(7, 20), ps(9, 20)]
        ),
        Err(ToyGameStateError::PlayerStateMisaligned {
            position: 1,
            expected: p(3),
            found: p(9)
        })
    );
}

#[test]
fn game_state_accepts_explicit_valid_states() {
    let built = ToyGameState::new(
        order(&[7, 3]),
        p(3),
        turn(u32::MAX),
        vec![ps(7, -5), ps(3, i64::MIN)],
    )
    .expect("explicit valid state");

    assert_eq!(built.player_order().players(), &[p(7), p(3)]);
    assert_eq!(built.active_player(), p(3));
    assert_eq!(built.turn_number(), turn(u32::MAX));
    assert_eq!(built.player_states(), &[ps(7, -5), ps(3, i64::MIN)]);
    assert_eq!(built.life_total(p(7)), Some(LifeTotal(-5)));
    assert_eq!(built.life_total(p(3)), Some(LifeTotal(i64::MIN)));
    assert_eq!(built.life_total(p(99)), None);
}

// ---------------------------------------------------------------- PassTurn

#[test]
fn pass_turn_makes_next_player_active_and_increments_turn_number() {
    let transition = accept(&initial(), &pass(7));

    assert_eq!(
        transition.trace,
        EventTrace::new(vec![turn_passed(7, 3, 2)])
    );
    assert_eq!(transition.state, state(3, 2, 20, 20));
}

#[test]
fn pass_turn_wraps_around_the_player_order() {
    let transition = accept(&state(3, 5, 20, 20), &pass(3));

    assert_eq!(
        transition.trace,
        EventTrace::new(vec![turn_passed(3, 7, 6)])
    );
    assert_eq!(transition.state, state(7, 6, 20, 20));
}

#[test]
fn pass_turn_reaches_the_maximum_turn_number() {
    let transition = accept(&state(7, u32::MAX - 1, 20, 20), &pass(7));

    assert_eq!(
        transition.trace,
        EventTrace::new(vec![turn_passed(7, 3, u32::MAX)])
    );
    assert_eq!(transition.state, state(3, u32::MAX, 20, 20));
}

// -------------------------------------------------------------- DealDamage

#[test]
fn deal_damage_lowers_opposing_life_total() {
    let transition = accept(&initial(), &deal(7, 3, 3));

    assert_eq!(
        transition.trace,
        EventTrace::new(vec![damage_dealt(7, 3, 3, 17)])
    );
    // Active Player, Turn Number and the source's LifeTotal are unchanged.
    assert_eq!(transition.state, state(7, 1, 20, 17));
}

#[test]
fn deal_damage_may_make_life_total_negative() {
    let transition = accept(&initial(), &deal(7, 3, 25));

    assert_eq!(
        transition.trace,
        EventTrace::new(vec![damage_dealt(7, 3, 25, -5)])
    );
    assert_eq!(transition.state, state(7, 1, 20, -5));
}

#[test]
fn deal_damage_may_repeat_within_a_turn() {
    let first = accept(&initial(), &deal(7, 3, 3));
    assert_eq!(
        first.trace,
        EventTrace::new(vec![damage_dealt(7, 3, 3, 17)])
    );
    assert_eq!(first.state, state(7, 1, 20, 17));

    let second = accept(&first.state, &deal(7, 3, 5));
    assert_eq!(
        second.trace,
        EventTrace::new(vec![damage_dealt(7, 3, 5, 12)])
    );
    assert_eq!(second.state, state(7, 1, 20, 12));
}

#[test]
fn deal_damage_reaches_the_minimum_life_total() {
    let transition = accept(&state(7, 1, 20, i64::MIN + 1), &deal(7, 3, 1));

    assert_eq!(
        transition.trace,
        EventTrace::new(vec![damage_dealt(7, 3, 1, i64::MIN)])
    );
    assert_eq!(transition.state, state(7, 1, 20, i64::MIN));
}

// -------------------------------------------------------------- rejections

#[test]
fn pass_turn_by_unknown_player_is_rejected() {
    assert_rejected(
        initial(),
        pass(99),
        ToyRejection::UnknownPlayer { player: p(99) },
    );
}

#[test]
fn pass_turn_by_non_active_player_is_rejected() {
    assert_rejected(
        initial(),
        pass(3),
        ToyRejection::NotActivePlayer { player: p(3) },
    );
}

#[test]
fn pass_turn_past_the_maximum_turn_number_is_rejected() {
    assert_rejected(
        state(7, u32::MAX, 20, 20),
        pass(7),
        ToyRejection::TurnNumberOverflow {
            turn_number: turn(u32::MAX),
        },
    );
}

#[test]
fn deal_damage_by_unknown_source_is_rejected() {
    assert_rejected(
        initial(),
        deal(99, 3, 1),
        ToyRejection::UnknownPlayer { player: p(99) },
    );
}

#[test]
fn deal_damage_by_non_active_player_is_rejected() {
    assert_rejected(
        initial(),
        deal(3, 7, 1),
        ToyRejection::NotActivePlayer { player: p(3) },
    );
}

#[test]
fn deal_damage_to_unknown_target_is_rejected() {
    assert_rejected(
        initial(),
        deal(7, 99, 1),
        ToyRejection::UnknownPlayer { player: p(99) },
    );
}

#[test]
fn deal_damage_to_self_is_rejected() {
    assert_rejected(
        initial(),
        deal(7, 7, 1),
        ToyRejection::SelfTarget { player: p(7) },
    );
}

#[test]
fn deal_damage_with_non_positive_amount_is_rejected() {
    for amount in [0, -1, i64::MIN] {
        assert_rejected(
            initial(),
            deal(7, 3, amount),
            ToyRejection::NonPositiveAmount { amount },
        );
    }
}

#[test]
fn deal_damage_life_total_underflow_is_rejected() {
    assert_rejected(
        state(7, 1, 20, i64::MIN),
        deal(7, 3, 1),
        ToyRejection::LifeTotalUnderflow {
            target: p(3),
            life_total: LifeTotal(i64::MIN),
            amount: 1,
        },
    );
    assert_rejected(
        state(7, 1, 20, i64::MIN + 1),
        deal(7, 3, 2),
        ToyRejection::LifeTotalUnderflow {
            target: p(3),
            life_total: LifeTotal(i64::MIN + 1),
            amount: 2,
        },
    );
}

// --------------------------------------------------------------- no panics

#[test]
fn no_action_panics_on_extreme_inputs() {
    let players = [7, 3, 99];
    let amounts = [i64::MIN, -1, 0, 1, i64::MAX];
    let lives = [i64::MIN, 0, i64::MAX];
    let turns = [1, u32::MAX];

    let mut actions = Vec::new();
    for &source in &players {
        actions.push(pass(source));
        for &target in &players {
            for &amount in &amounts {
                actions.push(deal(source, target, amount));
            }
        }
    }

    for &active in &[7, 3] {
        for &turn_number in &turns {
            for &life_7 in &lives {
                for &life_3 in &lives {
                    let input = state(active, turn_number, life_7, life_3);
                    let before = input.clone();
                    for action in &actions {
                        let _ = apply(&input, action);
                        assert_eq!(input, before);
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------- determinism

#[test]
fn same_initial_state_and_actions_produce_identical_runs() {
    let actions = [
        deal(7, 3, 5),
        deal(7, 3, 4),
        pass(7),
        deal(3, 7, 6),
        pass(3),
        deal(7, 3, 11),
        pass(7),
    ];

    let run = |initial: ToyGameState| {
        let mut current = initial;
        let mut events = Vec::new();
        for action in &actions {
            let transition = accept(&current, action);
            events.extend(transition.trace.into_events());
            current = transition.state;
        }
        (current, EventTrace::new(events))
    };

    let first = run(ToyRuleset.create_game(order(&[7, 3])).expect("two Players"));
    let second = run(ToyRuleset.create_game(order(&[7, 3])).expect("two Players"));

    assert_eq!(first, second);
    assert_eq!(first.0, state(3, 4, 14, 0));
    assert_eq!(
        first.1,
        EventTrace::new(vec![
            damage_dealt(7, 3, 5, 15),
            damage_dealt(7, 3, 4, 11),
            turn_passed(7, 3, 2),
            damage_dealt(3, 7, 6, 14),
            turn_passed(3, 7, 3),
            damage_dealt(7, 3, 11, 0),
            turn_passed(7, 3, 4),
        ])
    );
}

// ------------------------------------------------------------- prior state

#[test]
fn accepted_actions_leave_the_input_state_unchanged() {
    for action in [pass(7), deal(7, 3, 3)] {
        let input = initial();
        let before = input.clone();

        let transition = accept(&input, &action);

        assert_eq!(input, before);
        assert_ne!(transition.state, input);
    }
}
