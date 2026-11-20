//! Decisions: a Resolution pauses for an Option Choice until the Deciding Player answers
//! (ADR-0033).
//!
//! A Choice in a Triggered Card Effect asks a Player to pick one of several effect lists. Reaching
//! it ends the transition: the GameState holds a [`PendingDecision`], which holds the
//! [`PausedResolution`], everything the engine needs to go on once the Answer comes. The types here
//! are plain data, and they are the contract later Choices build on, so they stay minimal.

use super::event::Event;
use super::state::CardInstanceId;
use crate::player::PlayerId;

/// What a Player answers to a pending decision. Its only kind is the index of an option of an
/// Option Choice (`Card` comes with the Card Choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The option at this index, counted from 0, in the order the card declares them.
    Option(u32),
}

/// What kind of decision is pending. It stores no option count: that is read from the catalog
/// through the path of the Paused Resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    /// Pick one of the options of an Option Choice.
    Option,
}

/// A trigger waiting to fire, or the one that is paused: the carrying card and the index of the
/// Card Effect in the `effects` of its Card Definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueuedTrigger {
    pub card: CardInstanceId,
    /// Index in the `effects` of the card's Card Definition.
    pub effect: u32,
}

/// Which list of an effect a [`Branch`] enters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchKind {
    /// The `then` list of an `if`.
    Then,
    /// The `else` list of an `if`.
    Else,
    /// The option at this index of an Option Choice.
    Option(u32),
}

/// One step into a nested effect list: the effect at index `effect` of the current list, and which
/// of its lists is entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub effect: u32,
    pub into: BranchKind,
}

/// The position of a Choice in the `do` of a Triggered Card Effect: the branches entered from the
/// `do`, outermost first, then the index of the Choice in the innermost list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectPath {
    pub branches: Vec<Branch>,
    pub choice: u32,
}

/// A Resolution stopped at a Choice: where, what is still queued, what was not scanned yet and how
/// many triggers it has taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PausedResolution {
    /// The trigger whose `do` is paused; `self` is `trigger.card`.
    pub trigger: QueuedTrigger,
    /// The Choice it is paused at.
    pub at: EffectPath,
    /// The triggers still queued, first in first out, front first.
    pub queue: Vec<QueuedTrigger>,
    /// The Events the paused `do` emitted that no trigger has scanned yet, in order. They are
    /// scanned after the `do` completes, together with the Events after the pause, so a pause
    /// changes no order. Never a Decision Event.
    pub unscanned: Vec<Event>,
    /// The Trigger Cap count of this Resolution so far, the paused trigger included.
    pub resolutions: u32,
}

/// The decision a Player is asked for while the Game is paused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDecision {
    decider: PlayerId,
    kind: DecisionKind,
    paused: PausedResolution,
}

impl PendingDecision {
    pub(super) fn new(decider: PlayerId, kind: DecisionKind, paused: PausedResolution) -> Self {
        Self {
            decider,
            kind,
            paused,
        }
    }

    /// The Player asked: the only one who may answer.
    pub fn decider(&self) -> PlayerId {
        self.decider
    }

    /// The kind of decision.
    pub fn kind(&self) -> DecisionKind {
        self.kind
    }

    /// The Resolution that goes on once the decision is answered.
    pub fn paused(&self) -> &PausedResolution {
        &self.paused
    }
}
