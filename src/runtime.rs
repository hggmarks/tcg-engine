//! The game-agnostic execution boundary.

use crate::ruleset::{LegalActions, Ruleset, Transition};

/// Applies Actions through the one [`Ruleset`] it owns.
///
/// The Runtime has no knowledge of any particular game: Legality and the next
/// GameState are the Ruleset's (ADR-0001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime<R> {
    ruleset: R,
}

impl<R: Ruleset> Runtime<R> {
    /// Builds a Runtime around `ruleset`.
    pub fn new(ruleset: R) -> Self {
        Self { ruleset }
    }

    /// The Ruleset this Runtime applies.
    pub fn ruleset(&self) -> &R {
        &self.ruleset
    }

    /// Applies `action` to `state` as one State Transition.
    ///
    /// Returns the next GameState with its Event Trace, or the Ruleset's typed
    /// Rejection. `state` is taken by shared reference and never changed
    /// (ADR-0002).
    pub fn apply(
        &self,
        state: &R::State,
        action: &R::Action,
    ) -> Result<Transition<R::State, R::Event>, R::Rejection> {
        self.ruleset.transition(state, action)
    }
}

impl<R: LegalActions> Runtime<R> {
    /// Lists the legal Actions of `state` through the Ruleset's optional capability.
    pub fn legal_actions(&self, state: &R::State) -> Vec<R::Action> {
        self.ruleset.legal_actions(state)
    }
}
