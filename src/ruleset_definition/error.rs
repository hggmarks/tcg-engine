//! Errors of Ruleset Definition validation.

use std::fmt;

use super::draft::FieldTypeDraft;
use super::model::FieldType;
use crate::definition_path::DefinitionPath;

/// The first error found in a draft, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionError {
    pub path: DefinitionPath,
    pub kind: DefinitionErrorKind,
}

/// What a reference points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceTarget {
    Zone,
    CardType,
    DeckInput,
    Card,
    Action,
    /// An instance-state name that no card type declares.
    InstanceState,
}

/// The kinds of error. The comment on each variant is the path its error carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionErrorKind {
    /// An id that breaks the identifier grammar. The path is positional and ends with the key that
    /// holds the id, such as `zones[3].id`, `card_types[c].fields[2].name` or `meta.id`.
    InvalidId { id: String },
    /// `parameters.players` (it is 0).
    InvalidPlayerCount { players: u32 },
    /// `parameters.starting_player`.
    InvalidStartingPlayer { starting_player: u32, players: u32 },
    /// The duplicate element, such as `zones[3]`; `first` is the earlier one, such as `zones[0]`.
    DuplicateId { id: String, first: DefinitionPath },
    /// `inputs[main_deck].size` (it is 0).
    InvalidDeckSize,
    /// `inputs[main_deck].max_copies` (it is 0 or above the size).
    InvalidMaxCopies { max_copies: u32, size: u32 },
    /// `inputs[main_deck].card_types` (it is empty).
    NoCardTypes,
    /// `inputs[main_deck].zone` or `inputs[main_deck].card_types[0]`; a step's zone; the zone of a
    /// win condition (`win_conditions[deck_out].when.draw_from_empty`); an action of a
    /// phase (`turn.phases[main].actions[0]`); the `state` of a `set_state`, or the step itself for
    /// an `untap` (no card type declares `tapped`).
    DanglingReference { target: ReferenceTarget, id: String },
    /// `card_types[c].fields[f].type` or `card_types[c].instance_state[s].type`.
    UnsupportedFieldType { field_type: FieldTypeDraft },
    /// `win_conditions[leader_dead].when`: a condition written as an expression, named by its
    /// operator (`lte`), which ticket [11] adds.
    UnsupportedWinCondition { operator: String },
    /// `win_conditions[deck_out].loser`: only `owner` exists.
    UnknownLoser { loser: String },
    /// `setup[0].shuffle.player` or `turn.phases[start].automatic[0].untap.player`: only `each` and
    /// `active` exist.
    UnknownPlayerSelector { selector: String },
    /// A `count` of a draw or a move (it is 0), such as `setup[1].draw.count` or
    /// `turn.phases[start].automatic[3].move.count`.
    InvalidCount,
    /// The source of a shuffle, a draw or a move (ADR-0013): a zone with `ordered: false`, such as
    /// `setup[0].shuffle.zone` or `turn.phases[start].automatic[2].else[0].draw.from`.
    ZoneNotOrdered { zone: String },
    /// `turn.phases[start].automatic[3].move.position`: only `top` exists.
    UnknownPosition { position: String },
    /// `turn.phases[end]`: a phase has exactly one of `automatic` and `actions`.
    InvalidPhaseKind,
    /// `turn.phases`: a turn has exactly one phase with `actions`.
    InvalidActionPhaseCount { found: usize },
    /// `turn.phases[start].automatic[1].set_state.value`, or the step itself for an `untap`: the
    /// value is not the type some card type declares for the state.
    StateValueTypeMismatch {
        state: String,
        declared: FieldType,
        found: FieldType,
    },
    /// `card_types[c].instance_state[s].init`.
    InitializerTypeMismatch {
        declared: FieldType,
        found: FieldType,
    },
    /// `card_types[c].instance_state[s].init.from_field`.
    FromFieldMissing { field: String },
    /// `card_types[c].instance_state[s].init.from_field`.
    FromFieldTypeMismatch {
        field: String,
        declared: FieldType,
        found: FieldType,
    },
}

impl fmt::Display for DefinitionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId { id } => write!(
                f,
                "`{id}` is not a valid id: it must match [a-z][a-z0-9_]* and not be true, false or null"
            ),
            Self::InvalidDeckSize => write!(f, "the size of a deck input must be at least 1"),
            Self::InvalidMaxCopies { max_copies, size } => write!(
                f,
                "max_copies must be between 1 and the size {size}, found {max_copies}"
            ),
            Self::NoCardTypes => write!(f, "a deck input must allow at least one card type"),
            Self::InvalidPlayerCount { players } => {
                write!(
                    f,
                    "the number of players must be at least 1, found {players}"
                )
            }
            Self::InvalidStartingPlayer {
                starting_player,
                players,
            } => write!(
                f,
                "the starting player {starting_player} is not a position among {players} players"
            ),
            Self::DuplicateId { id, first } => {
                write!(f, "duplicate id `{id}`, first declared at `{first}`")
            }
            Self::DanglingReference { target, id } => {
                let target = match target {
                    ReferenceTarget::Zone => "zone",
                    ReferenceTarget::CardType => "card type",
                    ReferenceTarget::DeckInput => "deck input",
                    ReferenceTarget::Card => "card",
                    ReferenceTarget::Action => "action",
                    ReferenceTarget::InstanceState => "instance state",
                };
                write!(f, "no {target} has the id `{id}`")
            }
            Self::UnknownPlayerSelector { selector } => write!(
                f,
                "`{selector}` is not a player selector; the selectors are `each` and `active`"
            ),
            Self::InvalidCount => write!(f, "a draw or a move must take at least 1 card"),
            Self::ZoneNotOrdered { zone } => write!(
                f,
                "the zone `{zone}` is not ordered, so cards cannot be shuffled or taken from it"
            ),
            Self::UnknownPosition { position } => {
                write!(f, "`{position}` is not a position; the only one is `top`")
            }
            Self::InvalidPhaseKind => write!(
                f,
                "a phase must have exactly one of `automatic` and `actions`"
            ),
            Self::InvalidActionPhaseCount { found } => write!(
                f,
                "a turn must have exactly one phase with actions, found {found}"
            ),
            Self::StateValueTypeMismatch {
                state,
                declared,
                found,
            } => write!(
                f,
                "the state `{state}` is {declared:?} but the value is {found:?}"
            ),
            Self::UnsupportedFieldType { field_type } => {
                write!(f, "the field type {field_type:?} is not supported yet")
            }
            Self::UnsupportedWinCondition { operator } => write!(
                f,
                "the win condition operator `{operator}` is not supported yet; only `draw_from_empty` is"
            ),
            Self::UnknownLoser { loser } => {
                write!(f, "`{loser}` is not a loser; the only one is `owner`")
            }
            Self::InitializerTypeMismatch { declared, found } => {
                write!(
                    f,
                    "the initializer is {found:?} but the state is {declared:?}"
                )
            }
            Self::FromFieldMissing { field } => {
                write!(f, "the card type has no field named `{field}`")
            }
            Self::FromFieldTypeMismatch {
                field,
                declared,
                found,
            } => write!(
                f,
                "the field `{field}` is {found:?} but the state is {declared:?}"
            ),
        }
    }
}

impl std::error::Error for DefinitionErrorKind {}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.kind)
    }
}

impl std::error::Error for DefinitionError {}
