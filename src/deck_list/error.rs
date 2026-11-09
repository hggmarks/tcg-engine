//! Errors of Deck List validation. Elements of a deck list are references, so their paths are
//! positional (`inputs[1].cards[3]`) and the ids are in the kind.

use std::fmt;

use crate::card_catalog::CardId;
use crate::definition_path::DefinitionPath;
use crate::ruleset_definition::{
    CardTypeId, DeckInputId, ReferenceTarget, RulesetId, RulesetVersion,
};

/// The first error found in a draft, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckError {
    pub path: DefinitionPath,
    pub kind: DeckErrorKind,
}

/// The kinds of error. The comment on each variant is the path its error carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckErrorKind {
    /// The root: the catalog was validated against another ruleset id or version.
    CatalogFromOtherRuleset {
        expected_id: RulesetId,
        expected_version: RulesetVersion,
        found_id: RulesetId,
        found_version: RulesetVersion,
    },
    /// `inputs[1].input` or `inputs[1].cards[3].card`.
    DanglingReference { target: ReferenceTarget, id: String },
    /// The repeated input, such as `inputs[3]`; `first` is the earlier one.
    DuplicateInput {
        input: DeckInputId,
        first: DefinitionPath,
    },
    /// `inputs[1].cards[3].count`.
    ZeroCount,
    /// The repeated entry, such as `inputs[1].cards[4]`; `first` is the earlier one.
    DuplicateCard { card: CardId, first: DefinitionPath },
    /// `inputs[1].cards[3].card`.
    CardTypeNotAllowed { card: CardId, card_type: CardTypeId },
    /// `inputs[1].cards[3].count`.
    TooManyCopies {
        card: CardId,
        count: u32,
        max_copies: u32,
    },
    /// `inputs[1].cards`: the total number of cards is not the size of the input.
    WrongDeckSize { expected: u32, actual: u64 },
    /// `inputs[1].cards`: the total number of cards does not fit in 64 bits.
    DeckSizeOverflow,
    /// `inputs`: the first input of the ruleset that the deck list does not provide.
    MissingInput { input: DeckInputId },
}

impl fmt::Display for DeckErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogFromOtherRuleset {
                expected_id,
                expected_version,
                found_id,
                found_version,
            } => write!(
                f,
                "the catalog is for ruleset `{}` version {}, not `{}` version {}",
                found_id.as_str(),
                found_version.get(),
                expected_id.as_str(),
                expected_version.get()
            ),
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
            Self::DuplicateInput { input, first } => write!(
                f,
                "the input `{}` is provided twice, first at `{first}`",
                input.as_str()
            ),
            Self::ZeroCount => write!(f, "the count must be at least 1"),
            Self::DuplicateCard { card, first } => write!(
                f,
                "the card `{}` is listed twice, first at `{first}`",
                card.as_str()
            ),
            Self::CardTypeNotAllowed { card, card_type } => write!(
                f,
                "the card `{}` is a {}, which this input does not allow",
                card.as_str(),
                card_type.as_str()
            ),
            Self::TooManyCopies {
                card,
                count,
                max_copies,
            } => write!(
                f,
                "{count} copies of `{}` exceed the limit of {max_copies}",
                card.as_str()
            ),
            Self::WrongDeckSize { expected, actual } => {
                write!(f, "the input must hold {expected} cards, found {actual}")
            }
            Self::DeckSizeOverflow => write!(f, "the number of cards does not fit in 64 bits"),
            Self::MissingInput { input } => {
                write!(f, "the input `{}` is not provided", input.as_str())
            }
        }
    }
}

impl std::error::Error for DeckErrorKind {}

impl fmt::Display for DeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.segments().is_empty() {
            write!(f, "{}", self.kind)
        } else {
            write!(f, "{}: {}", self.path, self.kind)
        }
    }
}

impl std::error::Error for DeckError {}
