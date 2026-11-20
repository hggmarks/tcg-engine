//! Listing the Legal Actions of the declarative Ruleset (ADR-0018, ADR-0033).

use super::action::{current_action_phase, Action, Argument, Rejection};
use super::arguments;
use super::creation::DeclarativeRuleset;
use super::decision::Answer;
use super::state::GameState;
use super::triggers;
use crate::ruleset::{LegalActions, Ruleset};

impl LegalActions for DeclarativeRuleset {
    fn legal_actions(&self, state: &GameState) -> Vec<Action> {
        self.walk(state, false)
            .into_iter()
            .map(|(action, _)| action)
            .collect()
    }
}

impl DeclarativeRuleset {
    /// Every candidate Action of `state` in Listing Order, each with the Rejection that refused
    /// it, or `None` when `transition` accepts it. Filtering to the `None`s and dropping the
    /// rejections gives exactly `legal_actions(state)`, in the same order: both walk the same
    /// candidates through the same `transition` (ADR-0018, ADR-0026).
    pub fn listing_with_rejections(&self, state: &GameState) -> Vec<(Action, Option<Rejection>)> {
        self.walk(state, true)
    }

    /// The one walk over the candidates, in Listing Order. With `keep_rejected` false a refused
    /// candidate is dropped on the spot, so `legal_actions` allocates nothing for it. While a
    /// decision is pending the candidates are the Answers of its Deciding Player, in index order
    /// (ADR-0033); otherwise they are the Declared Actions of the Active Player.
    fn walk(&self, state: &GameState, keep_rejected: bool) -> Vec<(Action, Option<Rejection>)> {
        if state.outcome().is_some() {
            return Vec::new();
        }
        let mut walked = Vec::new();
        let mut consider = |candidate: Action| match self.transition(state, &candidate) {
            Ok(_) => walked.push((candidate, None)),
            Err(rejection) if keep_rejected => walked.push((candidate, Some(rejection))),
            Err(_) => {}
        };
        if let Some(pending) = state.pending_decision() {
            // A path that does not lead to a Choice lists nothing: `transition` would refuse every
            // Answer as a data mismatch.
            let options = triggers::option_count(self, state, pending.paused()).unwrap_or(0);
            for index in 0..options {
                consider(Action::Answer {
                    player: pending.decider(),
                    answer: Answer::Option(index),
                });
            }
            return walked;
        }
        let Some(listed) = current_action_phase(self, state) else {
            return Vec::new();
        };
        let actor = state.active_player();
        for id in listed {
            let Some(declared) = self.definition().action(id) else {
                continue;
            };
            for filling in arguments::fillings(self, state, actor, declared) {
                consider(Action::Declared {
                    player: actor,
                    action: id.clone(),
                    arguments: filling.into_iter().map(Argument::Card).collect(),
                });
            }
        }
        walked
    }
}
