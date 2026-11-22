//! The Stack of a GameState (ADR-0037): the Stack Items that wait to resolve while a Response
//! Window is open.
//!
//! A pushed Action pays its cost at once and its effects wait here as a [`StackItem`], bottom
//! first. Whether a window is open and who has Priority are derived from the Stack, never stored.

use super::state::CardInstanceId;
use crate::player::PlayerId;
use crate::ruleset_definition::{ActionId, ParamName, ZoneId};

/// What put a Stack Item on the Stack: the Action whose effects it holds, or the activated ability
/// whose effects it holds (ADR-0038). The enum is deliberately not `#[non_exhaustive]`, so every
/// `match` on it shows where a new source must be handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackSource {
    /// A pushed Action of the ruleset.
    Action(ActionId),
    /// The activated ability of `card` at Ability Index `ability`: the position of its Card Effect
    /// among all the effects of the card's definition, whatever their shapes. The card is also
    /// bound as `self` in the item's bound cards.
    Ability { card: CardInstanceId, ability: u32 },
}

/// One card a pushed Action bound: its param, the card, and the zone it was in once the cost was
/// paid (the zone it was bound from, unless the cost moved it). `left` is state, not history: it is
/// set when the card moves, even if it comes back, and a Stack Item that binds a card that left
/// Fizzles when it resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundCard {
    pub param: ParamName,
    pub card: CardInstanceId,
    pub zone: ZoneId,
    pub left: bool,
}

/// A pushed Action waiting to resolve: who controls it, what it is, and the cards it bound, in
/// param order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackItem {
    /// The Player who took the Action.
    pub controller: PlayerId,
    pub source: StackSource,
    pub bound: Vec<BoundCard>,
}

impl StackItem {
    /// Whether the item Fizzles when it resolves: some card it bound has left (ADR-0037). Nothing
    /// else is compared, and `where` conditions are not read again.
    pub fn fizzles(&self) -> bool {
        self.bound.iter().any(|bound| bound.left)
    }
}
