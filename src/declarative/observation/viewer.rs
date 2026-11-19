//! The Viewer: who observes a Game (ADR-0028).

use crate::player::PlayerId;

/// Who observes. Its only kind is a Player of the Game; a Spectator, who sees only what every
/// Player sees, is a later kind, so the enum is non-exhaustive and adding one is additive
/// (ADR-0028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Viewer {
    /// A Player of the Game: sees their own `owner` zones and every `public` zone.
    Player(PlayerId),
}

/// A handle a Viewer holds for a card they have been shown, in place of its Card Instance Id
/// (ADR-0028). Handles are dense from 0 per Viewer, in the order the cards become visible to that
/// Viewer, in Event order. Only the Observer maps a handle back to a Card Instance, and it is never
/// shown to the Viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObservedCard(pub u32);
