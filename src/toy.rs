//! The Toy Ruleset: the intentionally small proving scenario for the first
//! Runtime increment.
//!
//! Two Players, `PassTurn` and `DealDamage`. No phases, cards, randomness or
//! Terminal Outcome.
//!
//! This is the Toy Ruleset described in `CONTEXT.md`, a proving scenario
//! written in code. It is not a game Ruleset: game Rulesets are declarative
//! data (ADR-0004).

use std::fmt;

use crate::player::{PlayerId, PlayerOrder};
use crate::ruleset::{EventTrace, Ruleset, Transition};
use crate::turn::TurnNumber;

/// The LifeTotal every Player starts a Toy Game with.
pub const STARTING_LIFE_TOTAL: LifeTotal = LifeTotal(20);

/// A signed measure of a Player's remaining life; it may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LifeTotal(pub i64);

/// The domain facts owned by one Player: identity and LifeTotal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerState {
    pub player: PlayerId,
    pub life_total: LifeTotal,
}

/// The complete state of one Toy Game.
///
/// Always valid: exactly two Players, the Active Player belongs to the Player
/// Order, and there is one [`PlayerState`] per Player, aligned 1:1 and in
/// order with the Player Order (ADR-0003). Fields are private so no invalid
/// combination can be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToyGameState {
    player_order: PlayerOrder,
    active_player: PlayerId,
    turn_number: TurnNumber,
    player_states: Vec<PlayerState>,
}

/// Why a [`ToyGameState`] cannot be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToyGameStateError {
    /// A Toy Game has exactly two Players.
    WrongPlayerCount { count: usize },
    /// The Active Player is not in the Player Order.
    ActivePlayerNotInPlayerOrder { player: PlayerId },
    /// There is not exactly one PlayerState per Player.
    PlayerStateCountMismatch {
        players: usize,
        player_states: usize,
    },
    /// The PlayerState at `position` belongs to a different Player than the
    /// Player Order has at that position.
    PlayerStateMisaligned {
        position: usize,
        expected: PlayerId,
        found: PlayerId,
    },
}

impl fmt::Display for ToyGameStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPlayerCount { count } => {
                write!(f, "a Toy Game has exactly two Players, got {count}")
            }
            Self::ActivePlayerNotInPlayerOrder { player } => {
                write!(f, "Active Player {} is not in the Player Order", player.0)
            }
            Self::PlayerStateCountMismatch {
                players,
                player_states,
            } => write!(
                f,
                "{players} Players need {players} PlayerStates, got {player_states}"
            ),
            Self::PlayerStateMisaligned {
                position,
                expected,
                found,
            } => write!(
                f,
                "PlayerState at position {position} belongs to Player {}, expected Player {}",
                found.0, expected.0
            ),
        }
    }
}

impl std::error::Error for ToyGameStateError {}

impl ToyGameState {
    /// Builds an explicit, valid Toy GameState.
    ///
    /// The first failure wins, checked in this order: the Player count, the
    /// Active Player belonging to the Player Order, the PlayerState count, then
    /// the first misaligned position.
    pub fn new(
        player_order: PlayerOrder,
        active_player: PlayerId,
        turn_number: TurnNumber,
        player_states: Vec<PlayerState>,
    ) -> Result<Self, ToyGameStateError> {
        let count = player_order.players().len();
        if count != 2 {
            return Err(ToyGameStateError::WrongPlayerCount { count });
        }
        if !player_order.contains(active_player) {
            return Err(ToyGameStateError::ActivePlayerNotInPlayerOrder {
                player: active_player,
            });
        }
        if player_states.len() != count {
            return Err(ToyGameStateError::PlayerStateCountMismatch {
                players: count,
                player_states: player_states.len(),
            });
        }
        for (position, (&expected, state)) in player_order
            .players()
            .iter()
            .zip(&player_states)
            .enumerate()
        {
            if state.player != expected {
                return Err(ToyGameStateError::PlayerStateMisaligned {
                    position,
                    expected,
                    found: state.player,
                });
            }
        }
        Ok(Self {
            player_order,
            active_player,
            turn_number,
            player_states,
        })
    }

    /// The immutable Player Order.
    pub fn player_order(&self) -> &PlayerOrder {
        &self.player_order
    }

    /// The Player who owns the current Turn.
    pub fn active_player(&self) -> PlayerId {
        self.active_player
    }

    /// The 1-based count of Turns since the Game began.
    pub fn turn_number(&self) -> TurnNumber {
        self.turn_number
    }

    /// One PlayerState per Player, in Player Order.
    pub fn player_states(&self) -> &[PlayerState] {
        &self.player_states
    }

    /// The LifeTotal of `player`, or `None` if they are not in this Game.
    pub fn life_total(&self, player: PlayerId) -> Option<LifeTotal> {
        self.player_states
            .iter()
            .find(|state| state.player == player)
            .map(|state| state.life_total)
    }
}

/// The Toy Ruleset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToyRuleset;

impl ToyRuleset {
    /// Toy Game creation: first Player active, Turn 1, every LifeTotal 20.
    ///
    /// Fails with `WrongPlayerCount` unless `player_order` has exactly two
    /// Players.
    pub fn create_game(
        &self,
        player_order: PlayerOrder,
    ) -> Result<ToyGameState, ToyGameStateError> {
        let first = match player_order.players() {
            [first, _] => *first,
            players => {
                return Err(ToyGameStateError::WrongPlayerCount {
                    count: players.len(),
                })
            }
        };
        let player_states = player_order
            .players()
            .iter()
            .map(|&player| PlayerState {
                player,
                life_total: STARTING_LIFE_TOTAL,
            })
            .collect();
        ToyGameState::new(player_order, first, TurnNumber::FIRST, player_states)
    }
}

/// Player intent in a Toy Game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToyAction {
    /// The Active Player ends their Turn.
    PassTurn { player: PlayerId },
    /// The Active Player lowers the opposing Player's LifeTotal by `amount`.
    DealDamage {
        source: PlayerId,
        target: PlayerId,
        amount: i64,
    },
}

/// A fact produced by an accepted Toy Action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToyEvent {
    /// A Turn ended; `turn_number` is the resulting Turn Number.
    TurnPassed {
        previous: PlayerId,
        next: PlayerId,
        turn_number: TurnNumber,
    },
    /// A target's LifeTotal was reduced.
    DamageDealt {
        source: PlayerId,
        target: PlayerId,
        amount: i64,
        resulting_life_total: LifeTotal,
    },
}

/// Why the Toy Ruleset refuses an Action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToyRejection {
    /// The Player is not in the Game.
    UnknownPlayer { player: PlayerId },
    /// The acting Player is not the Active Player.
    NotActivePlayer { player: PlayerId },
    /// DealDamage must target the opposing Player.
    SelfTarget { player: PlayerId },
    /// DealDamage needs an amount of at least 1.
    NonPositiveAmount { amount: i64 },
    /// The target's LifeTotal cannot go below `i64::MIN`.
    LifeTotalUnderflow {
        target: PlayerId,
        life_total: LifeTotal,
        amount: i64,
    },
    /// The Turn Number cannot go above `u32::MAX`.
    TurnNumberOverflow { turn_number: TurnNumber },
}

impl fmt::Display for ToyRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPlayer { player } => write!(f, "Player {} is not in the Game", player.0),
            Self::NotActivePlayer { player } => {
                write!(f, "Player {} is not the Active Player", player.0)
            }
            Self::SelfTarget { player } => {
                write!(f, "Player {} cannot target themselves", player.0)
            }
            Self::NonPositiveAmount { amount } => {
                write!(f, "damage amount {amount} is not positive")
            }
            Self::LifeTotalUnderflow {
                target,
                life_total,
                amount,
            } => write!(
                f,
                "dealing {amount} to Player {} at LifeTotal {} would underflow",
                target.0, life_total.0
            ),
            Self::TurnNumberOverflow { turn_number } => {
                write!(f, "Turn Number {} cannot be incremented", turn_number.get())
            }
        }
    }
}

impl std::error::Error for ToyRejection {}

impl Ruleset for ToyRuleset {
    type State = ToyGameState;
    type Action = ToyAction;
    type Event = ToyEvent;
    type Rejection = ToyRejection;

    /// Applies one Toy Action. The first failure wins, checked in this order:
    ///
    /// - `PassTurn`: unknown Player, not the Active Player, Turn Number
    ///   overflow.
    /// - `DealDamage`: unknown source, source not the Active Player, unknown
    ///   target, self-target, non-positive amount, LifeTotal underflow.
    fn transition(
        &self,
        state: &ToyGameState,
        action: &ToyAction,
    ) -> Result<Transition<ToyGameState, ToyEvent>, ToyRejection> {
        match *action {
            ToyAction::PassTurn { player } => pass_turn(state, player),
            ToyAction::DealDamage {
                source,
                target,
                amount,
            } => deal_damage(state, source, target, amount),
        }
    }
}

fn require_active(state: &ToyGameState, player: PlayerId) -> Result<(), ToyRejection> {
    if !state.player_order.contains(player) {
        return Err(ToyRejection::UnknownPlayer { player });
    }
    if player != state.active_player {
        return Err(ToyRejection::NotActivePlayer { player });
    }
    Ok(())
}

fn pass_turn(
    state: &ToyGameState,
    player: PlayerId,
) -> Result<Transition<ToyGameState, ToyEvent>, ToyRejection> {
    require_active(state, player)?;
    let turn_number = state
        .turn_number
        .checked_next()
        .ok_or(ToyRejection::TurnNumberOverflow {
            turn_number: state.turn_number,
        })?;
    let next = state
        .player_order
        .next_after(player)
        .ok_or(ToyRejection::UnknownPlayer { player })?;

    let mut next_state = state.clone();
    next_state.active_player = next;
    next_state.turn_number = turn_number;
    Ok(Transition {
        state: next_state,
        trace: EventTrace::new(vec![ToyEvent::TurnPassed {
            previous: player,
            next,
            turn_number,
        }]),
    })
}

fn deal_damage(
    state: &ToyGameState,
    source: PlayerId,
    target: PlayerId,
    amount: i64,
) -> Result<Transition<ToyGameState, ToyEvent>, ToyRejection> {
    require_active(state, source)?;
    let life_total = state
        .life_total(target)
        .ok_or(ToyRejection::UnknownPlayer { player: target })?;
    if source == target {
        return Err(ToyRejection::SelfTarget { player: source });
    }
    if amount <= 0 {
        return Err(ToyRejection::NonPositiveAmount { amount });
    }
    let resulting_life_total = life_total.0.checked_sub(amount).map(LifeTotal).ok_or(
        ToyRejection::LifeTotalUnderflow {
            target,
            life_total,
            amount,
        },
    )?;

    let mut next_state = state.clone();
    for player_state in &mut next_state.player_states {
        if player_state.player == target {
            player_state.life_total = resulting_life_total;
        }
    }
    Ok(Transition {
        state: next_state,
        trace: EventTrace::new(vec![ToyEvent::DamageDealt {
            source,
            target,
            amount,
            resulting_life_total,
        }]),
    })
}
