//! The declarative Ruleset: a [`Ruleset`](crate::ruleset::Ruleset) that interprets a
//! [`RulesetDefinition`](crate::ruleset_definition::RulesetDefinition) and a
//! [`CardCatalog`](crate::card_catalog::CardCatalog) (ADR-0004, ADR-0005).
//!
//! Its [`GameState`] changes only by applying [`Event`]s (ADR-0010), and Game creation returns the
//! Event Trace of its setup (ADR-0011). Names here are unprefixed, unlike the Toy Ruleset's.

mod creation;
mod event;
mod state;

pub use creation::{Action, DeclarativeRuleset, GameCreation, GameCreationError, Rejection};
pub use event::Event;
pub use state::{ApplyError, CardInstance, CardInstanceId, GameState, ZoneCards};
