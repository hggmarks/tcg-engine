//! Players and the Player Order.

use std::fmt;

/// A strongly typed Player identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerId(pub u32);

/// The canonical, ordered membership of Players in a GameState.
///
/// Immutable once built, non-empty and duplicate-free. The order is exactly
/// the caller's order; it is never sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerOrder {
    players: Vec<PlayerId>,
}

/// Why a [`PlayerOrder`] cannot be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerOrderError {
    /// No Players were given.
    Empty,
    /// A Player appears more than once; this is the first repeat in list
    /// order.
    DuplicatePlayer { player: PlayerId },
}

impl fmt::Display for PlayerOrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "a Player Order must contain at least one Player"),
            Self::DuplicatePlayer { player } => {
                write!(f, "Player {} appears more than once", player.0)
            }
        }
    }
}

impl std::error::Error for PlayerOrderError {}

impl PlayerOrder {
    /// Builds a Player Order from `players`, keeping their order.
    ///
    /// Checks in order: empty, then the first repeated Player.
    pub fn new(players: Vec<PlayerId>) -> Result<Self, PlayerOrderError> {
        if players.is_empty() {
            return Err(PlayerOrderError::Empty);
        }
        let mut seen: Vec<PlayerId> = Vec::with_capacity(players.len());
        for &player in &players {
            if seen.contains(&player) {
                return Err(PlayerOrderError::DuplicatePlayer { player });
            }
            seen.push(player);
        }
        Ok(Self { players })
    }

    /// The Players in the caller's order.
    pub fn players(&self) -> &[PlayerId] {
        &self.players
    }

    /// Whether `player` is a member of this order.
    pub fn contains(&self, player: PlayerId) -> bool {
        self.players.contains(&player)
    }

    /// The Player after `player`, wrapping from the last to the first.
    ///
    /// `None` if `player` is not a member.
    pub fn next_after(&self, player: PlayerId) -> Option<PlayerId> {
        let position = self.players.iter().position(|&member| member == player)?;
        self.players
            .iter()
            .skip(position)
            .nth(1)
            .or_else(|| self.players.first())
            .copied()
    }
}
