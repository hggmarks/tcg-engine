//! Seeded self-play and invariant checks for the declarative engine (ADR-0020).
//!
//! This crate uses only the public API of `tcg-engine`. It plays whole Games with the Uniform
//! Random Policy, checks the engine's invariants after every transition, and verifies Game
//! Records. The engine core has no knowledge of it.

mod harness;
mod invariants;
mod observations;

pub use harness::{
    self_play, verify_record, Checks, FailureKind, SelfPlayFailure, SelfPlayGame, POLICY_STREAM,
};
pub use invariants::{
    check_opening, check_recomputation, check_rejection, check_transition, Invariant,
    InvariantViolation,
};
pub use observations::{
    check_hidden_cards_ignored, check_observed_actions, check_observed_consistency, ObservedStep,
    RELABEL_STREAM,
};

#[cfg(test)]
mod test_support;
