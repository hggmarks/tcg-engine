//! The declarative GameState and Event Application.
//!
//! The fields are private and [`GameState::apply`] is the only function that changes a GameState
//! (ADR-0010). A zone is a list; its top is the end of the list.

use std::fmt;

use super::event::Event;
use crate::card_catalog::CardId;
use crate::player::{PlayerId, PlayerOrder};
use crate::random::RandomState;
use crate::ruleset_definition::{
    FieldType, FieldValue, PhaseId, StateName, WinConditionId, ZoneId,
};
use crate::turn::TurnNumber;

/// The identity of a card instance, unique within one Game. Ids are dense from 0, in creation
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CardInstanceId(pub u32);

/// A card in a running Game: its owner, the Card Definition it was made from, and its mutable
/// instance state. Where it is lives in the GameState's zones, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardInstance {
    id: CardInstanceId,
    owner: PlayerId,
    definition: CardId,
    state: Vec<(StateName, FieldValue)>,
}

impl CardInstance {
    /// The id of this instance.
    pub fn id(&self) -> CardInstanceId {
        self.id
    }

    /// The Player who owns this card for the whole Game.
    pub fn owner(&self) -> PlayerId {
        self.owner
    }

    /// The immutable Card Definition this instance was made from.
    pub fn definition(&self) -> &CardId {
        &self.definition
    }

    /// The instance state in the order of the card type's schema.
    pub fn state(&self) -> &[(StateName, FieldValue)] {
        &self.state
    }

    /// The value of the instance-state entry `name`, if the card type has one.
    pub fn state_value(&self, name: &StateName) -> Option<&FieldValue> {
        self.state
            .iter()
            .find(|(entry, _)| entry == name)
            .map(|(_, value)| value)
    }
}

/// The cards of one zone of one Player, bottom first: the last card is the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneCards {
    zone: ZoneId,
    cards: Vec<CardInstanceId>,
}

impl ZoneCards {
    /// The zone these cards are in.
    pub fn zone(&self) -> &ZoneId {
        &self.zone
    }

    /// The cards, bottom first.
    pub fn cards(&self) -> &[CardInstanceId] {
        &self.cards
    }
}

/// A Player who lost, and the id of the win condition that made them lose (ADR-0014).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loss {
    pub player: PlayerId,
    pub reason: WinConditionId,
}

/// How a Game ended: who lost and why, and who won. It is built only by applying `GameEnded`
/// (ADR-0014), so its winners are always the Players who did not lose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalOutcome {
    winners: Vec<PlayerId>,
    losses: Vec<Loss>,
}

impl TerminalOutcome {
    /// The Players who did not lose, in seat order. Empty for a drawn game.
    pub fn winners(&self) -> &[PlayerId] {
        &self.winners
    }

    /// The losses, one per Player who lost, in seat order.
    pub fn losses(&self) -> &[Loss] {
        &self.losses
    }

    /// Whether every Player lost at the same time, so nobody won.
    pub fn is_drawn_game(&self) -> bool {
        self.winners.is_empty()
    }
}

/// The complete state of one declarative Game: Players by seat, the Active Player, the Turn Number
/// and phase, each Player's zones, every card instance, the Random State and, once the Game has
/// ended, its Terminal Outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    player_order: PlayerOrder,
    active_player: PlayerId,
    turn_number: Option<TurnNumber>,
    phase: Option<PhaseId>,
    zones: Vec<(PlayerId, Vec<ZoneCards>)>,
    cards: Vec<CardInstance>,
    random_state: RandomState,
    outcome: Option<TerminalOutcome>,
}

/// Why an Event cannot be applied. The GameState is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// The Event names a Player who is not in the Game.
    UnknownPlayer { player: PlayerId },
    /// The Event names a zone the Player does not have.
    UnknownZone { player: PlayerId, zone: ZoneId },
    /// A created card's id is not the next unused id.
    CardIdOutOfSequence { found: CardInstanceId },
    /// The card is not in the named zone of the named Player.
    CardNotInZone {
        card: CardInstanceId,
        player: PlayerId,
        zone: ZoneId,
    },
    /// A shuffle's order is not a permutation of the cards in the zone.
    NotAPermutation { player: PlayerId, zone: ZoneId },
    /// The Event names a card instance that does not exist.
    UnknownCard { card: CardInstanceId },
    /// The card has no instance-state entry named `state`.
    UnknownState {
        card: CardInstanceId,
        state: StateName,
    },
    /// The new value of the state is not of the type of its current value.
    StateTypeMismatch {
        card: CardInstanceId,
        state: StateName,
        expected: FieldType,
        found: FieldType,
    },
    /// A draw-from-empty fact names a zone that holds cards.
    ZoneNotEmpty { player: PlayerId, zone: ZoneId },
    /// A phase began, but no Turn has begun.
    NoTurnInProgress,
    /// The Game has ended: no Event applies after `GameEnded`.
    GameAlreadyEnded,
    /// `GameEnded` names no loser.
    NoLoser,
    /// `GameEnded` lists `player` at or before the position of the loser before them in seat
    /// order: a Player listed twice, or the losses not in seat order.
    LosersOutOfSeatOrder { player: PlayerId },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPlayer { player } => write!(f, "Player {} is not in the Game", player.0),
            Self::UnknownZone { player, zone } => {
                write!(f, "Player {} has no zone `{}`", player.0, zone.as_str())
            }
            Self::CardIdOutOfSequence { found } => {
                write!(f, "card instance id {} is not the next unused id", found.0)
            }
            Self::CardNotInZone { card, player, zone } => write!(
                f,
                "card instance {} is not in zone `{}` of Player {}",
                card.0,
                zone.as_str(),
                player.0
            ),
            Self::NotAPermutation { player, zone } => write!(
                f,
                "the new order is not a permutation of zone `{}` of Player {}",
                zone.as_str(),
                player.0
            ),
            Self::UnknownCard { card } => write!(f, "card instance {} does not exist", card.0),
            Self::UnknownState { card, state } => write!(
                f,
                "card instance {} has no state `{}`",
                card.0,
                state.as_str()
            ),
            Self::StateTypeMismatch {
                card,
                state,
                expected,
                found,
            } => write!(
                f,
                "the state `{}` of card instance {} is {expected:?}, not {found:?}",
                state.as_str(),
                card.0
            ),
            Self::ZoneNotEmpty { player, zone } => write!(
                f,
                "zone `{}` of Player {} is not empty",
                zone.as_str(),
                player.0
            ),
            Self::NoTurnInProgress => write!(f, "no Turn is in progress"),
            Self::GameAlreadyEnded => write!(f, "the Game has already ended"),
            Self::NoLoser => write!(f, "a Game cannot end without a loser"),
            Self::LosersOutOfSeatOrder { player } => write!(
                f,
                "Player {} is not listed after the previous loser in seat order",
                player.0
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

impl GameState {
    /// The Empty GameState: every Player has every zone, all empty, and there are no card
    /// instances.
    pub(super) fn empty(
        player_order: PlayerOrder,
        active_player: PlayerId,
        zones: &[ZoneId],
        random_state: RandomState,
    ) -> GameState {
        let zones = player_order
            .players()
            .iter()
            .map(|&player| {
                let held = zones
                    .iter()
                    .map(|zone| ZoneCards {
                        zone: zone.clone(),
                        cards: Vec::new(),
                    })
                    .collect();
                (player, held)
            })
            .collect();
        GameState {
            player_order,
            active_player,
            turn_number: None,
            phase: None,
            zones,
            cards: Vec::new(),
            random_state,
            outcome: None,
        }
    }

    /// The Players in seat order.
    pub fn player_order(&self) -> &PlayerOrder {
        &self.player_order
    }

    /// The Player whose Turn it is.
    pub fn active_player(&self) -> PlayerId {
        self.active_player
    }

    /// The Turn Number of the Turn in progress; `None` before the first Turn has begun (in the
    /// Empty GameState, or in a Game whose ruleset has no turn).
    pub fn turn_number(&self) -> Option<TurnNumber> {
        self.turn_number
    }

    /// The phase of the Turn in progress that began last; `None` when no phase has begun.
    pub fn phase(&self) -> Option<&PhaseId> {
        self.phase.as_ref()
    }

    /// The zones of `player` in declared order; `None` for a Player not in the Game.
    pub fn zones(&self, player: PlayerId) -> Option<&[ZoneCards]> {
        self.zones
            .iter()
            .find(|(owner, _)| *owner == player)
            .map(|(_, zones)| zones.as_slice())
    }

    /// The cards of one zone of `player`, bottom first (the top is the last card); `None` for an
    /// unknown Player or zone.
    pub fn zone(&self, player: PlayerId, zone: &ZoneId) -> Option<&[CardInstanceId]> {
        self.zones(player)?
            .iter()
            .find(|held| &held.zone == zone)
            .map(|held| held.cards.as_slice())
    }

    /// Every card instance, in id order.
    pub fn cards(&self) -> &[CardInstance] {
        &self.cards
    }

    /// The card instance with `id`.
    pub fn card(&self, id: CardInstanceId) -> Option<&CardInstance> {
        let position = usize::try_from(id.0).ok()?;
        self.cards.get(position)
    }

    /// The Random State, advanced by every shuffle.
    pub fn random_state(&self) -> &RandomState {
        &self.random_state
    }

    /// The Terminal Outcome; `None` while the Game is on.
    pub fn outcome(&self) -> Option<&TerminalOutcome> {
        self.outcome.as_ref()
    }

    /// The single function through which a GameState changes.
    ///
    /// Atomic: the Event is fully checked first, so on `Err` the state is unchanged. Once the Game
    /// has ended no Event applies. `GameEnded` only records what the Ruleset decided, which
    /// Players lost; it holds no win-condition logic.
    pub fn apply(&mut self, event: &Event) -> Result<(), ApplyError> {
        if self.outcome.is_some() {
            return Err(ApplyError::GameAlreadyEnded);
        }
        match event {
            Event::CardCreated {
                card,
                owner,
                definition,
                zone,
                state,
            } => {
                self.require_zone(*owner, zone)?;
                if usize::try_from(card.0).ok() != Some(self.cards.len()) {
                    return Err(ApplyError::CardIdOutOfSequence { found: *card });
                }
                self.cards.push(CardInstance {
                    id: *card,
                    owner: *owner,
                    definition: definition.clone(),
                    state: state.clone(),
                });
                if let Some(held) = self.zone_mut(*owner, zone) {
                    held.cards.push(*card);
                }
                Ok(())
            }
            Event::CardMoved {
                card,
                player,
                from,
                to,
            } => {
                let in_from = self.require_zone(*player, from)?;
                self.require_zone(*player, to)?;
                if !in_from.contains(card) {
                    return Err(ApplyError::CardNotInZone {
                        card: *card,
                        player: *player,
                        zone: from.clone(),
                    });
                }
                if let Some(held) = self.zone_mut(*player, from) {
                    held.cards.retain(|member| member != card);
                }
                if let Some(held) = self.zone_mut(*player, to) {
                    held.cards.push(*card);
                }
                Ok(())
            }
            Event::ZoneShuffled {
                player,
                zone,
                order,
                random_state,
            } => {
                let current = self.require_zone(*player, zone)?;
                let mut before = current.to_vec();
                let mut after = order.clone();
                before.sort();
                after.sort();
                if before != after {
                    return Err(ApplyError::NotAPermutation {
                        player: *player,
                        zone: zone.clone(),
                    });
                }
                if let Some(held) = self.zone_mut(*player, zone) {
                    held.cards.clone_from(order);
                }
                self.random_state = random_state.clone();
                Ok(())
            }
            Event::TurnBegan { player, turn } => {
                if !self.player_order.contains(*player) {
                    return Err(ApplyError::UnknownPlayer { player: *player });
                }
                self.active_player = *player;
                self.turn_number = Some(*turn);
                self.phase = None;
                Ok(())
            }
            Event::PhaseBegan { phase } => {
                if self.turn_number.is_none() {
                    return Err(ApplyError::NoTurnInProgress);
                }
                self.phase = Some(phase.clone());
                Ok(())
            }
            Event::CardStateChanged { card, state, value } => {
                let instance = usize::try_from(card.0)
                    .ok()
                    .and_then(|position| self.cards.get_mut(position))
                    .ok_or(ApplyError::UnknownCard { card: *card })?;
                let Some((_, current)) = instance.state.iter_mut().find(|(name, _)| name == state)
                else {
                    return Err(ApplyError::UnknownState {
                        card: *card,
                        state: state.clone(),
                    });
                };
                if current.field_type() != value.field_type() {
                    return Err(ApplyError::StateTypeMismatch {
                        card: *card,
                        state: state.clone(),
                        expected: current.field_type(),
                        found: value.field_type(),
                    });
                }
                current.clone_from(value);
                Ok(())
            }
            Event::DrawFromEmptyZone { player, zone } => {
                if !self.require_zone(*player, zone)?.is_empty() {
                    return Err(ApplyError::ZoneNotEmpty {
                        player: *player,
                        zone: zone.clone(),
                    });
                }
                Ok(())
            }
            Event::GameEnded { losses } => {
                self.outcome = Some(self.outcome_of(losses)?);
                Ok(())
            }
        }
    }

    /// The Terminal Outcome of a Game that ends with `losses`: the winners are the Players who did
    /// not lose.
    fn outcome_of(&self, losses: &[Loss]) -> Result<TerminalOutcome, ApplyError> {
        if losses.is_empty() {
            return Err(ApplyError::NoLoser);
        }
        let players = self.player_order.players();
        let mut previous: Option<usize> = None;
        for loss in losses {
            let Some(seat) = players.iter().position(|&member| member == loss.player) else {
                return Err(ApplyError::UnknownPlayer {
                    player: loss.player,
                });
            };
            if previous.is_some_and(|before| seat <= before) {
                return Err(ApplyError::LosersOutOfSeatOrder {
                    player: loss.player,
                });
            }
            previous = Some(seat);
        }
        let winners = players
            .iter()
            .copied()
            .filter(|player| !losses.iter().any(|loss| loss.player == *player))
            .collect();
        Ok(TerminalOutcome {
            winners,
            losses: losses.to_vec(),
        })
    }

    /// Applies `events` in order to a clone of this state and returns the result; this state is
    /// untouched. Stops at the first Event that is rejected.
    pub fn apply_trace(&self, events: &[Event]) -> Result<GameState, ApplyError> {
        let mut next = self.clone();
        for event in events {
            next.apply(event)?;
        }
        Ok(next)
    }

    /// The cards of a zone of a Player, or the error that says which of the two is unknown.
    fn require_zone(
        &self,
        player: PlayerId,
        zone: &ZoneId,
    ) -> Result<&[CardInstanceId], ApplyError> {
        let Some(zones) = self.zones(player) else {
            return Err(ApplyError::UnknownPlayer { player });
        };
        zones
            .iter()
            .find(|held| &held.zone == zone)
            .map(|held| held.cards.as_slice())
            .ok_or_else(|| ApplyError::UnknownZone {
                player,
                zone: zone.clone(),
            })
    }

    fn zone_mut(&mut self, player: PlayerId, zone: &ZoneId) -> Option<&mut ZoneCards> {
        self.zones
            .iter_mut()
            .find(|(owner, _)| *owner == player)?
            .1
            .iter_mut()
            .find(|held| &held.zone == zone)
    }
}
