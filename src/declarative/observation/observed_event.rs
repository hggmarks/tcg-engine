//! Observed Events: what one Viewer observes of each Event of a transition (ADR-0029).
//!
//! No variant carries a Card Instance Id or a Random State. A card is named by the Observed Card
//! the Viewer holds for it, and only while the Viewer sees it; what the Viewer cannot see is
//! anonymous (an owner and zones, never a card, never an order).

use std::fmt;

use super::observed_state::{ObservedGameState, ObservedStackItem};
use super::viewer::ObservedCard;
use crate::card_catalog::CardId;
use crate::declarative::decision::DecisionKind;
use crate::declarative::state::Loss;
use crate::player::PlayerId;
use crate::ruleset_definition::{ActionId, FieldValue, PhaseId, StateName, ZoneId};
use crate::turn::TurnNumber;

/// One Event as a Viewer observes it (ADR-0029). No variant carries a Card Instance Id or a Random
/// State.
///
/// Every move variant keeps the order the Viewer sees: a zone's cards bottom first, the moved card
/// pushed onto the end of `to`, as `CardMoved` does to the GameState.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedEvent {
    /// A card was created in a zone the Viewer sees: its new Observed Card, owner, Card Definition
    /// and instance state.
    CardCreated {
        card: ObservedCard,
        owner: PlayerId,
        definition: CardId,
        zone: ZoneId,
        state: Vec<(StateName, FieldValue)>,
    },
    /// A card was created in a zone the Viewer cannot see: only that one was, in whose zone.
    UnseenCardCreated { owner: PlayerId, zone: ZoneId },
    /// A card the Viewer sees, before and after, moved; it keeps its Observed Card.
    CardMoved {
        card: ObservedCard,
        player: PlayerId,
        from: ZoneId,
        to: ZoneId,
    },
    /// A card came into the Viewer's sight: a new Observed Card, with its Card Definition and its
    /// instance state at that point of the transition (the real `CardMoved` carries neither).
    CardMovedIntoSight {
        card: ObservedCard,
        player: PlayerId,
        from: ZoneId,
        to: ZoneId,
        definition: CardId,
        state: Vec<(StateName, FieldValue)>,
    },
    /// A card left the Viewer's sight: its Observed Card is retired and never named again.
    CardMovedOutOfSight {
        card: ObservedCard,
        player: PlayerId,
        from: ZoneId,
        to: ZoneId,
    },
    /// A card the Viewer sees neither before nor after moved: only the owner and the two zones.
    UnseenCardMoved {
        player: PlayerId,
        from: ZoneId,
        to: ZoneId,
    },
    /// A zone the Viewer sees was reordered: its new order, bottom first. Never the Random State.
    ZoneShuffled {
        player: PlayerId,
        zone: ZoneId,
        order: Vec<ObservedCard>,
    },
    /// A zone the Viewer cannot see was reordered: no order, no Random State.
    UnseenZoneShuffled { player: PlayerId, zone: ZoneId },
    /// An instance state of a card the Viewer sees changed. A card the Viewer cannot see produces
    /// no Observed Event.
    CardStateChanged {
        card: ObservedCard,
        state: StateName,
        value: FieldValue,
    },
    /// A Turn began.
    TurnBegan { player: PlayerId, turn: TurnNumber },
    /// A phase began.
    PhaseBegan { phase: PhaseId },
    /// A draw found the zone empty.
    DrawFromEmptyZone { player: PlayerId, zone: ZoneId },
    /// The Game ended: who lost and why.
    GameEnded { losses: Vec<Loss> },
    /// A decision became pending: who is asked and what kind. Never the Paused Resolution.
    DecisionPending {
        decider: PlayerId,
        kind: DecisionKind,
    },
    /// The Deciding Player answered the pending decision.
    DecisionAnswered {
        player: PlayerId,
        answer: ObservedAnswer,
    },
    /// An Action was pushed onto the Stack, which is fully public (ADR-0037): the whole item.
    ItemPushed { item: ObservedStackItem },
    /// The top item resolved: the item as it was.
    ItemResolved { item: ObservedStackItem },
    /// The top item Fizzled, a card it bound having left: the item as it was.
    ItemFizzled { item: ObservedStackItem },
}

/// The Action as a Viewer observes it taken (ADR-0029): the acting Player and, for a Declared
/// Action, the declared action and each card argument as an Observed Card or hidden; for an
/// Answer, its option index. It is the observed form of the `Action` of a transition, not an
/// Action the Viewer can take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakenAction {
    Declared {
        player: PlayerId,
        action: ActionId,
        /// One per argument of the Action, in order.
        arguments: Vec<TakenArgument>,
    },
    Answer {
        player: PlayerId,
        answer: ObservedAnswer,
    },
    /// The Player with Priority passed.
    Pass { player: PlayerId },
}

/// An Answer as a Viewer observes it (ADR-0033, ADR-0034): the index of the option picked, or the
/// card picked as the Taken Action rule names a card. Every Viewer sees every Answer, and a card
/// only as far as it sees the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedAnswer {
    /// The option at this index.
    Option(u32),
    /// The card picked: the Observed Card the Viewer holds after the transition, else the one it
    /// held before, else hidden. The handle may be one issued later in the same trace.
    Card(TakenArgument),
}

/// One card argument of a Taken Action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakenArgument {
    /// The card the Viewer holds after the transition, else the one it held before (now retired).
    Card(ObservedCard),
    /// The Viewer sees the card neither before nor after the transition.
    HiddenCard,
}

/// A transition as a Viewer observes it (ADR-0029), mirroring `Transition`: the Taken Action, the
/// Observed Events in trace order and the next Observed GameState.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedTransition {
    pub action: TakenAction,
    pub events: Vec<ObservedEvent>,
    pub state: ObservedGameState,
}

/// Why an Observed Event cannot be applied to an Observed GameState. It is coarse: the fold needs
/// no Ruleset and no Observer. The Observed GameState is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedApplyError {
    /// The Game has ended: no Observed Event applies after `GameEnded`.
    GameAlreadyEnded,
    /// The Observed Event does not fit the Observed GameState: an unknown Player, zone, card or
    /// state, a zone seen or unseen the other way, a count that would go below zero, an order that
    /// is not a permutation, a value of the wrong type, a phase with no Turn, or bad losses.
    EventDoesNotFit,
}

impl fmt::Display for ObservedApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameAlreadyEnded => write!(f, "the Game has already ended"),
            Self::EventDoesNotFit => {
                write!(f, "the Observed Event does not fit the Observed GameState")
            }
        }
    }
}

impl std::error::Error for ObservedApplyError {}
