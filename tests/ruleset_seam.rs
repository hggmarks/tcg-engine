//! The Runtime has no Toy-specific knowledge: a Ruleset with a different
//! GameState shape runs through the same Runtime.

use tcg_engine::player::{PlayerId, PlayerOrder};
use tcg_engine::ruleset::{EventTrace, LegalActions, Ruleset, Transition};
use tcg_engine::runtime::Runtime;
use tcg_engine::toy::{ToyAction, ToyRuleset};

/// A test-local Ruleset whose GameState is a bare `u8` tally: no Players.
struct TallyRuleset;

#[derive(Debug, Clone, PartialEq, Eq)]
enum TallyAction {
    Add(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TallyEvent {
    Added { amount: u8, total: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TallyRejection {
    Overflow { total: u8, amount: u8 },
}

impl Ruleset for TallyRuleset {
    type State = u8;
    type Action = TallyAction;
    type Event = TallyEvent;
    type Rejection = TallyRejection;

    fn transition(
        &self,
        state: &u8,
        action: &TallyAction,
    ) -> Result<Transition<u8, TallyEvent>, TallyRejection> {
        let TallyAction::Add(amount) = action;
        let total = state.checked_add(*amount).ok_or(TallyRejection::Overflow {
            total: *state,
            amount: *amount,
        })?;
        Ok(Transition {
            state: total,
            trace: EventTrace::new(vec![TallyEvent::Added {
                amount: *amount,
                total,
            }]),
        })
    }
}

#[test]
fn alternate_ruleset_with_a_different_state_runs_through_the_same_runtime() {
    let runtime = Runtime::new(TallyRuleset);

    let before = 250_u8;
    let accepted = runtime.apply(&before, &TallyAction::Add(5));
    assert_eq!(
        accepted,
        Ok(Transition {
            state: 255,
            trace: EventTrace::new(vec![TallyEvent::Added {
                amount: 5,
                total: 255
            }]),
        })
    );
    assert_eq!(before, 250);

    let full = 255_u8;
    let rejected = runtime.apply(&full, &TallyAction::Add(1));
    assert_eq!(
        rejected,
        Err(TallyRejection::Overflow {
            total: 255,
            amount: 1
        })
    );
    assert_eq!(full, 255);
}

/// The tally can list its Legal Actions: every amount that does not overflow, ascending.
impl LegalActions for TallyRuleset {
    fn legal_actions(&self, state: &u8) -> Vec<TallyAction> {
        (0..=u8::MAX)
            .filter(|amount| state.checked_add(*amount).is_some())
            .map(TallyAction::Add)
            .collect()
    }
}

#[test]
fn legal_action_listing_is_a_separate_capability_of_the_seam() {
    // A Ruleset that implements the capability lists through the Runtime, and what it lists is
    // exactly what its transition accepts.
    let runtime = Runtime::new(TallyRuleset);
    let state = 253_u8;
    let listed = runtime.legal_actions(&state);
    assert_eq!(
        listed,
        vec![
            TallyAction::Add(0),
            TallyAction::Add(1),
            TallyAction::Add(2)
        ]
    );
    for action in &listed {
        assert!(runtime.apply(&state, action).is_ok());
    }
    assert_eq!(
        runtime.apply(&state, &TallyAction::Add(3)),
        Err(TallyRejection::Overflow {
            total: 253,
            amount: 3
        })
    );
    assert_eq!(runtime.legal_actions(&255_u8), vec![TallyAction::Add(0)]);
    assert_eq!(state, 253, "listing does not change the state");

    // The Toy Ruleset does not implement the capability and is still a Ruleset: it transitions
    // through the Runtime. Its DealDamage amount is unbounded, so it could not list. (That it has
    // no `LegalActions` is the `compile_fail` doctest of the trait.)
    let toy = Runtime::new(ToyRuleset);
    let order = PlayerOrder::new(vec![PlayerId(7), PlayerId(3)]).expect("valid Player Order");
    let created = ToyRuleset.create_game(order).expect("two Players");
    let passed = toy
        .apply(
            &created,
            &ToyAction::PassTurn {
                player: PlayerId(7),
            },
        )
        .expect("the Active Player passes");
    assert_eq!(passed.state.active_player(), PlayerId(3));
}
