//! Listing the Legal Actions of the declarative Ruleset (ADR-0018).

use super::action::{current_action_phase, Action, Argument};
use super::arguments;
use super::creation::DeclarativeRuleset;
use super::state::GameState;
use crate::ruleset::{LegalActions, Ruleset};

impl LegalActions for DeclarativeRuleset {
    fn legal_actions(&self, state: &GameState) -> Vec<Action> {
        if state.outcome().is_some() {
            return Vec::new();
        }
        let Some(listed) = current_action_phase(self, state) else {
            return Vec::new();
        };
        let actor = state.active_player();
        let mut legal = Vec::new();
        for id in listed {
            let Some(declared) = self.definition().action(id) else {
                continue;
            };
            for filling in arguments::fillings(self, state, actor, declared) {
                let candidate = Action {
                    player: actor,
                    action: id.clone(),
                    arguments: filling.into_iter().map(Argument::Card).collect(),
                };
                if self.transition(state, &candidate).is_ok() {
                    legal.push(candidate);
                }
            }
        }
        legal
    }
}
