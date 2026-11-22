//! Observing a declarative GameState (ADR-0028).
//!
//! A [`Viewer`] observes through an [`Observer`], its fold of the Events applied to the Empty
//! GameState, and sees an [`ObservedGameState`]: the zones its visibility allows, card by card
//! under [`ObservedCard`] handles, and every other zone as a count. The capability is the
//! [`Observations`](crate::ruleset::Observations) trait, which the Declarative Ruleset implements
//! and the Toy Ruleset does not. Following a transition yields its [`ObservedEvent`]s and an
//! [`ObservedTransition`] (ADR-0029), and an Observed GameState has an apply of its own. Each
//! concern has its own file.

mod acting;
mod derived_read;
mod error;
mod follow;
mod observed_event;
mod observed_state;
mod observer;
mod viewer;

pub use acting::{ObservedAction, ObservedArgument};
pub use error::ObservationError;
pub use observed_event::{
    ObservedAnswer, ObservedApplyError, ObservedEvent, ObservedTransition, TakenAction,
    TakenArgument,
};
pub use observed_state::{
    ObservedBoundCard, ObservedGameState, ObservedPendingDecision, ObservedStackItem,
    ObservedStackSource, ObservedZone, VisibleCard,
};
pub use observer::Observer;
pub use viewer::{ObservedCard, Viewer};
