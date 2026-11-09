//! Errors of Card Catalog validation.

use std::fmt;

use crate::definition_path::DefinitionPath;
use crate::ruleset_definition::{FieldName, FieldType, ReferenceTarget, RulesetId, RulesetVersion};

/// The first error found in a draft, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError {
    pub path: DefinitionPath,
    pub kind: CatalogErrorKind,
}

/// The kinds of error. The comment on each variant is the path its error carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogErrorKind {
    /// `meta.ruleset`.
    RulesetIdMismatch { expected: RulesetId, found: String },
    /// `meta.ruleset_version`.
    RulesetVersionMismatch {
        expected: RulesetVersion,
        found: u32,
    },
    /// A card id that breaks the identifier grammar, at `cards[3].id`.
    InvalidId { id: String },
    /// The duplicate card, such as `cards[3]`; `first` is the earlier one, such as `cards[0]`.
    DuplicateId { id: String, first: DefinitionPath },
    /// `cards[goblin].type`.
    DanglingReference { target: ReferenceTarget, id: String },
    /// The repeated field, such as `cards[goblin].fields[2]`; `first` is the earlier one.
    DuplicateField {
        field: String,
        first: DefinitionPath,
    },
    /// `cards[goblin].fields[rarity]`.
    UnknownField { field: String },
    /// `cards[goblin].fields[power]`.
    FieldTypeMismatch {
        declared: FieldType,
        found: FieldType,
    },
    /// `cards[goblin].fields`: the first field of the schema that the card does not give.
    MissingField { field: FieldName },
    /// `cards[goblin].effects`: the card type is declared with `effects: false`.
    EffectsNotAllowed,
    /// `cards[goblin].effects[0]`: no effect primitive exists yet.
    UnsupportedEffect { primitive: String },
}

impl fmt::Display for CatalogErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RulesetIdMismatch { expected, found } => write!(
                f,
                "the catalog targets the ruleset `{found}`, not `{}`",
                expected.as_str()
            ),
            Self::RulesetVersionMismatch { expected, found } => write!(
                f,
                "the catalog targets ruleset version {found}, not {}",
                expected.get()
            ),
            Self::InvalidId { id } => write!(
                f,
                "`{id}` is not a valid id: it must match [a-z][a-z0-9_]* and not be true, false or null"
            ),
            Self::DuplicateId { id, first } => {
                write!(f, "duplicate card id `{id}`, first declared at `{first}`")
            }
            Self::DanglingReference { target, id } => {
                let target = match target {
                    ReferenceTarget::Zone => "zone",
                    ReferenceTarget::CardType => "card type",
                    ReferenceTarget::DeckInput => "deck input",
                    ReferenceTarget::Card => "card",
                    ReferenceTarget::Action => "action",
                    ReferenceTarget::InstanceState => "instance state",
                    ReferenceTarget::Param => "param",
                    ReferenceTarget::FieldOrInstanceState => "field or instance state",
                };
                write!(f, "no {target} has the id `{id}`")
            }
            Self::DuplicateField { field, first } => {
                write!(f, "duplicate field `{field}`, first given at `{first}`")
            }
            Self::UnknownField { field } => {
                write!(f, "the card type has no field named `{field}`")
            }
            Self::FieldTypeMismatch { declared, found } => {
                write!(f, "the value is {found:?} but the field is {declared:?}")
            }
            Self::MissingField { field } => {
                write!(f, "the field `{}` is missing", field.as_str())
            }
            Self::EffectsNotAllowed => {
                write!(f, "the card type does not allow effects")
            }
            Self::UnsupportedEffect { primitive } => {
                write!(f, "the effect `{primitive}` is not supported yet")
            }
        }
    }
}

impl std::error::Error for CatalogErrorKind {}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.kind)
    }
}

impl std::error::Error for CatalogError {}
