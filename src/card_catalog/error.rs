//! Errors of Card Catalog validation.

use std::fmt;

use crate::definition_path::DefinitionPath;
use crate::ruleset_definition::{
    DefinitionErrorKind, FieldName, FieldType, ReferenceTarget, RulesetId, RulesetVersion,
};

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
    /// `cards[goblin].effects[0]`: the element has keys of two shapes (`on` or `if` or `do`; `while`
    /// or `modify` or `scope`; `activated`).
    MixedCardEffectShapes,
    /// `cards[goblin].effects[0]`: the element has none of `on`, `while` and `activated`.
    MissingCardEffectShape,
    /// `cards[goblin].effects[0].do`: a triggered Card Effect without `do`; or
    /// `cards[goblin].effects[0].activated.do`, an activated one.
    MissingDo,
    /// `cards[goblin].effects[0].do` (or `.activated.do`): the list is empty.
    EmptyDo,
    /// `cards[goblin].effects[0].modify`: a continuous Card Effect without `modify`.
    MissingModify,
    /// `cards[goblin].effects[0].on`: only `enter` and `turn_start` exist.
    UnknownTriggerMatcher { matcher: String },
    /// `cards[goblin].effects[0].modify.field`: the card type declares no field of that name (an
    /// instance state is not a field).
    UnknownModifyField { field: String },
    /// `cards[goblin].effects[0].modify.field`: the field is not an int.
    ModifyFieldNotInt { field: String, found: FieldType },
    /// The `end_turn` itself, such as `cards[goblin].effects[0].do[1]` or `...do[0].then[0]`: a
    /// Card Effect does not end the Turn.
    EndTurnInCardEffect,
    /// The `advance_phase` itself, such as `cards[goblin].effects[0].do[1]` or `...do[0].then[0]`: a
    /// Card Effect does not move the Turn to another phase (ADR-0042).
    AdvancePhaseInCardEffect,
    /// An expression, ref or Action Effect of a Card Effect that Ruleset Definition checking
    /// rejects; the error keeps its own path, such as `cards[goblin].effects[0].if.and[0].eq[1]`.
    InvalidCardEffect { error: Box<DefinitionErrorKind> },
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
                    ReferenceTarget::PlayerState => "player state",
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
            Self::MixedCardEffectShapes => write!(
                f,
                "a Card Effect is triggered (`on`, `if`, `do`), continuous (`while`, `modify`) or activated (`activated`), not a mix"
            ),
            Self::MissingCardEffectShape => {
                write!(f, "a Card Effect needs an `on`, a `while` or an `activated`")
            }
            Self::MissingDo => write!(f, "a triggered or activated Card Effect needs a `do`"),
            Self::EmptyDo => write!(f, "`do` must list at least one effect"),
            Self::MissingModify => write!(f, "a continuous Card Effect needs a `modify`"),
            Self::UnknownTriggerMatcher { matcher } => write!(
                f,
                "`{matcher}` is not a trigger matcher; they are `enter` and `turn_start`"
            ),
            Self::UnknownModifyField { field } => {
                write!(f, "the card type has no field named `{field}`")
            }
            Self::ModifyFieldNotInt { field, found } => {
                write!(f, "the field `{field}` is {found:?}, not an int")
            }
            Self::EndTurnInCardEffect => {
                write!(f, "a Card Effect cannot end the Turn")
            }
            Self::AdvancePhaseInCardEffect => {
                write!(f, "a Card Effect cannot advance the phase")
            }
            Self::InvalidCardEffect { error } => write!(f, "{error}"),
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
