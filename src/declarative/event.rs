//! The Events of the declarative Ruleset. They are generic: they name cards, Players and zones, and
//! know nothing about any particular game (ADR-0010).

use super::state::CardInstanceId;
use crate::card_catalog::CardId;
use crate::player::PlayerId;
use crate::random::RandomState;
use crate::ruleset_definition::{FieldValue, PhaseId, StateName, ZoneId};
use crate::turn::TurnNumber;

/// A fact that occurred. Applying Events through `GameState::apply` is the only way a declarative
/// GameState changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A card instance came into being on the top of `owner`'s `zone`. Its id must be the next
    /// unused one, so ids are dense from 0 in creation order. `state` is its instance state in
    /// schema order.
    CardCreated {
        card: CardInstanceId,
        owner: PlayerId,
        definition: CardId,
        zone: ZoneId,
        state: Vec<(StateName, FieldValue)>,
    },
    /// A card left `from`, wherever it was in that list, and went onto the top of `to`. Both zones
    /// belong to `player`.
    CardMoved {
        card: CardInstanceId,
        player: PlayerId,
        from: ZoneId,
        to: ZoneId,
    },
    /// A zone was reordered. `order` is the complete resulting order (bottom first) and
    /// `random_state` is the Random State after the shuffle, so applying the Event is exact and
    /// needs no generator. Future per-Player observations must never expose `random_state` or the
    /// order of a hidden zone (ADR-0010).
    ZoneShuffled {
        player: PlayerId,
        zone: ZoneId,
        order: Vec<CardInstanceId>,
        random_state: RandomState,
    },
    /// A Turn began: `player` is the Active Player and `turn` the Turn Number, and no phase has
    /// begun yet. Applying it checks only that the Player is in the Game; the Ruleset decides who
    /// is next and which Turn Number follows.
    TurnBegan { player: PlayerId, turn: TurnNumber },
    /// A phase of the Turn in progress began.
    PhaseBegan { phase: PhaseId },
    /// The instance state `state` of a card changed to `value`. The card has that state, and
    /// `value` is of the type of its current value. The Ruleset emits it only for a value that
    /// actually changes.
    CardStateChanged {
        card: CardInstanceId,
        state: StateName,
        value: FieldValue,
    },
    /// A draw found the zone empty. It changes nothing; win conditions read it ([09]).
    DrawFromEmptyZone { player: PlayerId, zone: ZoneId },
}
