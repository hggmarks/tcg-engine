//! The Events of the declarative Ruleset. They are generic: they name cards, Players and zones, and
//! know nothing about any particular game (ADR-0010).

use super::decision::{Answer, DecisionKind, PausedResolution};
use super::stack::StackItem;
use super::state::{CardInstanceId, Loss};
use crate::card_catalog::CardId;
use crate::player::PlayerId;
use crate::random::RandomState;
use crate::ruleset_definition::{FieldValue, PhaseId, PlayerStateId, StateName, ZoneId};
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
    /// The Player state `state` of `player` changed to `value` (ADR-0041). The Player has that
    /// state, and `value` is of the type of its current value. The Ruleset emits it only for a
    /// value that actually changes. The initial values are in the Empty GameState: no Event
    /// creates them.
    PlayerStateChanged {
        player: PlayerId,
        state: PlayerStateId,
        value: FieldValue,
    },
    /// A draw found the zone empty. It changes nothing; win conditions read it (ADR-0014).
    DrawFromEmptyZone { player: PlayerId, zone: ZoneId },
    /// The Game ended: these Players lost, in seat order, each for one win condition. Applying it
    /// sets the Terminal Outcome (the winners are the other Players), and no Event applies after
    /// it. It is the last Event of the trace that emits it (ADR-0014).
    GameEnded { losses: Vec<Loss> },
    /// A Resolution paused for a decision: `decider` must answer. It is the last Event of the trace
    /// that emits it, before `GameEnded` if the same transition ends the Game. While it is applied,
    /// only `DecisionAnswered` and `GameEnded` apply (ADR-0033).
    DecisionPending {
        decider: PlayerId,
        kind: DecisionKind,
        paused: PausedResolution,
    },
    /// The Deciding Player answered the pending decision. It is the first Event of the trace of an
    /// Answer.
    DecisionAnswered { player: PlayerId, answer: Answer },
    /// An Action was pushed onto the Stack (ADR-0037): its cost was paid by the Events before it in
    /// the trace, and its effects wait as `item`. It comes after the cost Events and before the
    /// Trigger Resolution of the transition. Applying it puts `item` on top of the Stack.
    ItemPushed { item: StackItem },
    /// The top item of the Stack resolved: it is the first Event of the trace of a Pass, and the
    /// item's effect Events follow. Applying it pops the item, which must be the top one and must
    /// not Fizzle.
    ItemResolved { item: StackItem },
    /// The top item of the Stack Fizzled, a card it bound having left (ADR-0037): it is the first
    /// Event of the trace of a Pass, in the place of `ItemResolved`, and nothing of the item's
    /// effects happens. Applying it pops the item, which must be the top one and must Fizzle.
    ItemFizzled { item: StackItem },
}
