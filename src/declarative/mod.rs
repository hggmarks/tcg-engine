//! The declarative Ruleset: a [`Ruleset`](crate::ruleset::Ruleset) that interprets a
//! [`RulesetDefinition`](crate::ruleset_definition::RulesetDefinition) and a
//! [`CardCatalog`](crate::card_catalog::CardCatalog) (ADR-0004, ADR-0005).
//!
//! Its [`GameState`] changes only by applying [`Event`]s (ADR-0010), and Game creation returns the
//! Event Trace of its setup and of the first Turn's start (ADR-0011, ADR-0012). Names here are
//! unprefixed, unlike the Toy Ruleset's.

mod action;
mod action_effects;
mod arguments;
mod creation;
mod event;
mod execution;
mod state;
mod turn_cycle;

pub use action::{Action, Argument, Rejection};
pub use creation::{DeclarativeRuleset, GameCreation, GameCreationError};
pub use event::Event;
pub use state::{ApplyError, CardInstance, CardInstanceId, GameState, ZoneCards};
