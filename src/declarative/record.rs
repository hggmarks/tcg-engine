//! Game Records and exact Replay (ADR-0019).
//!
//! A [`GameRecord`] holds everything needed to play a Game again: the identities of the ruleset and
//! of the catalog, the Deck Lists, the Seed and the accepted Actions. [`DeclarativeRuleset::replay`]
//! creates the Game again and runs each Action, and returns the Event Traces and the final
//! GameState. The record is a plain in-memory value: it has no serialization and no content hash.

use std::fmt;

use super::action::{Action, Rejection};
use super::creation::{DeclarativeRuleset, GameCreation, GameCreationError};
use super::event::Event;
use super::state::GameState;
use crate::card_catalog::CatalogMeta;
use crate::deck_list::DeckList;
use crate::random::Seed;
use crate::ruleset::{EventTrace, Ruleset};
use crate::ruleset_definition::{RulesetId, RulesetVersion};

/// A Game as played: the ruleset, catalog and Deck List identities, the Seed and the accepted
/// Actions, in order.
///
/// The ruleset is identified by its id and version, the catalog by its [`CatalogMeta`], and a
/// Deck List, which has no id of its own, by its value: one per Seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRecord {
    ruleset_id: RulesetId,
    ruleset_version: RulesetVersion,
    catalog: CatalogMeta,
    decks: Vec<DeckList>,
    seed: Seed,
    actions: Vec<Action>,
}

impl GameRecord {
    /// The record of a Game of `ruleset` created from `decks` (`decks[i]` for seat `i`) and
    /// `seed`, with no Action yet.
    pub fn new(ruleset: &DeclarativeRuleset, decks: &[DeckList], seed: Seed) -> GameRecord {
        let meta = ruleset.definition().meta();
        GameRecord {
            ruleset_id: meta.id().clone(),
            ruleset_version: meta.version(),
            catalog: ruleset.catalog().meta().clone(),
            decks: decks.to_vec(),
            seed,
            actions: Vec::new(),
        }
    }

    /// Appends an Action. The caller appends only Actions the Ruleset accepted, in order; replay
    /// reports the first one it does not accept.
    pub fn push_accepted(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// The id of the ruleset the Game was played under.
    pub fn ruleset_id(&self) -> &RulesetId {
        &self.ruleset_id
    }

    /// The version of the ruleset the Game was played under.
    pub fn ruleset_version(&self) -> RulesetVersion {
        self.ruleset_version
    }

    /// The identity of the Card Catalog the Game was played with.
    pub fn catalog(&self) -> &CatalogMeta {
        &self.catalog
    }

    /// The Deck Lists, one per seat in seat order.
    pub fn decks(&self) -> &[DeckList] {
        &self.decks
    }

    /// The Seed the Game was created with.
    pub fn seed(&self) -> Seed {
        self.seed
    }

    /// The accepted Actions, in the order they were taken.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
}

/// The result of replaying a [`GameRecord`]: the same creation, Event Traces and final GameState
/// the Game produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replay {
    /// The Game creation, from the record's Deck Lists and Seed.
    pub creation: GameCreation,
    /// The Event Trace of each Action of the record, in order.
    pub traces: Vec<EventTrace<Event>>,
    /// The GameState after the last Action; the Opening Position for a record without Actions.
    pub state: GameState,
}

/// Why a record cannot be replayed. The first error wins: the ruleset identity is checked first,
/// then the catalog identity, then creation, then each Action in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// The record is of another ruleset id or version than this Ruleset's definition.
    RulesetMismatch {
        record_id: RulesetId,
        record_version: RulesetVersion,
        ruleset_id: RulesetId,
        ruleset_version: RulesetVersion,
    },
    /// The record's catalog identity is not that of this Ruleset's catalog.
    CatalogMismatch {
        record: CatalogMeta,
        ruleset: CatalogMeta,
    },
    /// The Game cannot be created from the record's Deck Lists and Seed.
    Creation(GameCreationError),
    /// The Action at `index` of the record is rejected by the GameState the earlier ones made.
    ActionRejected { index: usize, rejection: Rejection },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RulesetMismatch {
                record_id,
                record_version,
                ruleset_id,
                ruleset_version,
            } => write!(
                f,
                "the record is of the ruleset `{}` version {}, not `{}` version {}",
                record_id.as_str(),
                record_version.get(),
                ruleset_id.as_str(),
                ruleset_version.get()
            ),
            Self::CatalogMismatch { record, ruleset } => write!(
                f,
                "the record is of the catalog for `{}` version {}, not `{}` version {}",
                record.ruleset().as_str(),
                record.ruleset_version().get(),
                ruleset.ruleset().as_str(),
                ruleset.ruleset_version().get()
            ),
            Self::Creation(error) => write!(f, "the Game cannot be created: {error}"),
            Self::ActionRejected { index, rejection } => {
                write!(f, "the Action at index {index} is rejected: {rejection}")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

impl DeclarativeRuleset {
    /// Plays `record` again: checks the identities, creates the Game from the record's Deck Lists
    /// and Seed, and runs each Action in order on the GameState the previous one made.
    ///
    /// The Event Traces and the final GameState are exactly those of the original Game
    /// (Deterministic Execution). The first failing step is the error.
    pub fn replay(&self, record: &GameRecord) -> Result<Replay, ReplayError> {
        let meta = self.definition().meta();
        if record.ruleset_id() != meta.id() || record.ruleset_version() != meta.version() {
            return Err(ReplayError::RulesetMismatch {
                record_id: record.ruleset_id().clone(),
                record_version: record.ruleset_version(),
                ruleset_id: meta.id().clone(),
                ruleset_version: meta.version(),
            });
        }
        if record.catalog() != self.catalog().meta() {
            return Err(ReplayError::CatalogMismatch {
                record: record.catalog().clone(),
                ruleset: self.catalog().meta().clone(),
            });
        }
        let creation = self
            .create_game(record.decks(), record.seed())
            .map_err(ReplayError::Creation)?;
        let mut state = creation.opening.state.clone();
        let mut traces = Vec::with_capacity(record.actions().len());
        for (index, action) in record.actions().iter().enumerate() {
            let transition = self
                .transition(&state, action)
                .map_err(|rejection| ReplayError::ActionRejected { index, rejection })?;
            state = transition.state;
            traces.push(transition.trace);
        }
        Ok(Replay {
            creation,
            traces,
            state,
        })
    }
}
