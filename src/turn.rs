//! Turn Numbers.

use std::num::NonZeroU32;

/// The 1-based count of Turns since the Game began.
///
/// Zero is unrepresentable. Every accepted `PassTurn` increments it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TurnNumber(NonZeroU32);

impl TurnNumber {
    /// The first Turn of a Game.
    pub const FIRST: TurnNumber = TurnNumber(NonZeroU32::MIN);

    /// `None` for 0, since Turn Numbers are 1-based.
    pub fn new(value: u32) -> Option<TurnNumber> {
        NonZeroU32::new(value).map(TurnNumber)
    }

    /// The numeric value, at least 1.
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// The following Turn Number, or `None` at the maximum.
    pub fn checked_next(self) -> Option<TurnNumber> {
        self.0.checked_add(1).map(TurnNumber)
    }
}
