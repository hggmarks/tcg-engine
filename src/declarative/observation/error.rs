//! Why an Observation cannot be built or read (ADR-0028). No variant carries a Card Instance Id or
//! a Random State: an error is shown to the caller of a Viewer's Observation.

use std::fmt;

use super::viewer::{ObservedCard, Viewer};
use crate::player::PlayerId;
use crate::ruleset_definition::FieldName;

/// Why an Observer, an Observed GameState or a Derived Read cannot be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationError {
    /// The Viewer is not a Player of the Game.
    UnknownViewer { viewer: Viewer },
    /// The Viewer holds no such Observed Card now: never given, or retired when the card left
    /// their sight.
    UnknownObservedCard { card: ObservedCard },
    /// The card's Card Definition has no int field of that name.
    NotAnIntField {
        card: ObservedCard,
        field: FieldName,
    },
    /// The Derived Read of the int field would go beyond what an int can hold (ADR-0024).
    DerivedValueOverflow {
        card: ObservedCard,
        field: FieldName,
    },
    /// The Events or the GameState do not fit the Observer or the Ruleset: an Event moves a card
    /// out of a zone the Viewer sees that the Observer never showed, or names a zone the Ruleset
    /// does not declare; a GameState shows a card the Observer has no handle for, or the Observer
    /// holds a handle for a card the GameState does not show. An internal inconsistency, never
    /// expected from a Game followed from its Empty GameState.
    ObserverOutOfStep,
    /// There are more Observed Cards than a handle can number.
    ObservedCardOverflow,
}

impl fmt::Display for ObservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownViewer { viewer } => {
                let Viewer::Player(PlayerId(seat)) = viewer;
                write!(f, "Player {seat} is not a Player of the Game")
            }
            Self::UnknownObservedCard { card } => {
                write!(f, "the Viewer holds no Observed Card {}", card.0)
            }
            Self::NotAnIntField { card, field } => write!(
                f,
                "the Observed Card {} has no int field `{}`",
                card.0,
                field.as_str()
            ),
            Self::DerivedValueOverflow { card, field } => write!(
                f,
                "reading the field `{}` of the Observed Card {} overflows",
                field.as_str(),
                card.0
            ),
            Self::ObserverOutOfStep => {
                write!(f, "the Observer does not fit the Events or the GameState")
            }
            Self::ObservedCardOverflow => write!(f, "there are too many Observed Cards to number"),
        }
    }
}

impl std::error::Error for ObservationError {}
