//! The correctness net over Observations (ADR-0031): the three Invariants that hold for every
//! Viewer at every step, and the Relabelling the leak check is made of.
//!
//! Checkpoint skeleton: the signatures the acceptance tests call. The bodies come next.

use std::collections::BTreeSet;

use tcg_engine::declarative::{
    Action, CardInstanceId, DeclarativeRuleset, Event, GameState, ObservationError, ObservedCard,
    ObservedEvent, ObservedGameState, Observer, TakenAction, Viewer,
};
use tcg_engine::random::RandomState;
use tcg_engine::ruleset_definition::FieldName;

use crate::invariants::InvariantViolation;

/// The constant the Relabelling's generator is derived with: one
/// `RandomState::from_seed(Seed(seed.0 ^ RELABEL_STREAM))` per Game, separate from the GameState's
/// Random State and from the policy's ([`POLICY_STREAM`](crate::POLICY_STREAM)).
pub const RELABEL_STREAM: u64 = 0x7265_6c61_6265_6c31;

/// What one Viewer observes at one step: the Opening Position (the creation trace) or one accepted
/// transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedStep {
    pub viewer: Viewer,
    /// The Observed Events of the creation trace, or of the transition.
    pub events: Vec<ObservedEvent>,
    /// The Taken Action; `None` at the Opening Position.
    pub taken: Option<TakenAction>,
    pub state: ObservedGameState,
    /// For every Visible Card in handle order, every `FieldValue::Int` field of its Card Definition
    /// in catalog order, through `derived_read`.
    pub derived_reads: Vec<(ObservedCard, FieldName, i64)>,
}

/// The Events of a Game so far, in order: the creation trace, then the trace of each accepted
/// transition. The last trace is the current step's, or the creation is when there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameEvents {
    pub(crate) creation: Vec<Event>,
    pub(crate) traces: Vec<Vec<Event>>,
}

/// A Game replayed from the Empty GameState to the end of one step, and the Observed Step of a
/// Viewer there: the Relabelled Game when the Events were relabelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayedStep {
    /// The GameState at the end of the step, folded from the Empty GameState.
    pub(crate) state: GameState,
    pub(crate) step: ObservedStep,
}

/// The Events and the Action of a step after the Relabelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Relabelled {
    pub(crate) events: GameEvents,
    pub(crate) action: Option<Action>,
}

/// The Hidden Cards of `viewer` after `events`: the cards that were never in a zone the Viewer
/// sees (a `CardCreated` into a seen zone, or a `CardMoved` with a seen `from` or `to`).
pub(crate) fn hidden_cards(
    _ruleset: &DeclarativeRuleset,
    _viewer: Viewer,
    _events: &GameEvents,
) -> BTreeSet<CardInstanceId> {
    todo!("ticket 66: the Hidden Cards of a Viewer")
}

/// Relabels the cards of `to_relabel` in `events` and `action`: definitions, then ids within
/// classes, then the order of the zones `viewer` cannot see; drawing from `rng` in that fixed
/// order.
pub(crate) fn relabel(
    _ruleset: &DeclarativeRuleset,
    _viewer: Viewer,
    _to_relabel: &BTreeSet<CardInstanceId>,
    _events: &GameEvents,
    _action: Option<&Action>,
    _rng: &mut RandomState,
) -> Relabelled {
    todo!("ticket 66: the Relabelling")
}

/// The Observed Step of `viewer` at the end of the last step of `events`, built from scratch: the
/// Events folded from `empty`, a fresh Observer from `empty` over every Event before the step,
/// then the step itself with `action` (`None` at the Opening Position).
pub(crate) fn replay_observed_step(
    _ruleset: &DeclarativeRuleset,
    _viewer: Viewer,
    _empty: &GameState,
    _events: &GameEvents,
    _action: Option<&Action>,
) -> Result<ReplayedStep, ObservationError> {
    todo!("ticket 66: the Relabelled Game")
}

/// Observations stay consistent: the Observed Events applied to `previous` give `next`.
pub fn check_observed_consistency(
    _viewer: Viewer,
    _previous: &ObservedGameState,
    _events: &[ObservedEvent],
    _next: &ObservedGameState,
) -> Vec<InvariantViolation> {
    todo!("ticket 66: observations stay consistent")
}

/// Observations ignore hidden cards: `observed` and `relabelled` are equal.
pub fn check_hidden_cards_ignored(
    _observed: &ObservedStep,
    _relabelled: &ObservedStep,
) -> Vec<InvariantViolation> {
    todo!("ticket 66: observations ignore hidden cards")
}

/// Observed Actions round-trip: the Observer's Observed Legal Actions, translated back, are the
/// Viewer's Actions of `legal`, in the same order.
pub fn check_observed_actions(
    _ruleset: &DeclarativeRuleset,
    _observer: &Observer,
    _state: &GameState,
    _legal: &[Action],
) -> Vec<InvariantViolation> {
    todo!("ticket 66: observed actions round-trip")
}

#[cfg(test)]
mod tests;
