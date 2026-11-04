//! The Ruleset seam between the generic Runtime and game-specific rules.
//!
//! A Ruleset owns Legality and State Transitions (ADR-0001). The seam is
//! generic over each Ruleset's own GameState, Action, Event and Rejection
//! types (ADR-0005), so Rulesets with completely different state shapes run
//! through the same Runtime. A transition takes the GameState by shared
//! reference and returns a new value, leaving the input untouched (ADR-0002).

/// A game-specific definition of legal Actions and State Transitions.
pub trait Ruleset {
    /// The complete state of one running Game under this Ruleset.
    type State;
    /// A description of player or system intent under this Ruleset.
    type Action;
    /// A fact produced by an accepted Action under this Ruleset.
    type Event;
    /// The typed reason this Ruleset refuses an Action.
    type Rejection;

    /// Validates `action` against `state`; if it is legal, returns the next
    /// GameState and its Event Trace.
    ///
    /// A rejected Action returns a typed Rejection; `state` is never changed.
    fn transition(
        &self,
        state: &Self::State,
        action: &Self::Action,
    ) -> Result<Transition<Self::State, Self::Event>, Self::Rejection>;
}

/// The outcome of an accepted State Transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition<S, E> {
    /// The next GameState, distinct from the one the Action was applied to.
    pub state: S,
    /// The ordered facts produced by the accepted Action.
    pub trace: EventTrace<E>,
}

/// The ordered record of Events produced by one accepted Action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTrace<E> {
    events: Vec<E>,
}

impl<E> EventTrace<E> {
    /// Builds a trace holding `events` in the given order.
    pub fn new(events: Vec<E>) -> Self {
        Self { events }
    }

    /// The Events in the order they occurred.
    pub fn events(&self) -> &[E] {
        &self.events
    }

    /// Consumes the trace, returning its Events in order.
    pub fn into_events(self) -> Vec<E> {
        self.events
    }
}
