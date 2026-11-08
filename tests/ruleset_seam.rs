//! The Runtime has no Toy-specific knowledge: a Ruleset with a different
//! GameState shape runs through the same Runtime.

use tcg_engine::ruleset::{EventTrace, Ruleset, Transition};
use tcg_engine::runtime::Runtime;

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
