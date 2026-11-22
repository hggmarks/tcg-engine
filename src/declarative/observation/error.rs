//! Why an Observation cannot be built or read (ADR-0028). No variant carries a Card Instance Id or
//! a Random State: an error is shown to the caller of a Viewer's Observation.

use std::fmt;

use super::viewer::{ObservedCard, Viewer};
use crate::player::PlayerId;
use crate::ruleset_definition::{ActionId, FieldName};

/// Why an Observer, an Observed GameState, a Derived Read or an Observed Action (listing the
/// Viewer's Observed Legal Actions, or translating an Observed Action) cannot be produced.
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
    /// The Events or the GameState do not fit the Observer or the Ruleset: a `CardCreated` of a
    /// card that already has a handle; a `CardMoved` out of a zone the Viewer sees of a card the
    /// Observer has no handle for, or out of an unseen zone of a card it holds one for; a zone the
    /// Ruleset does not declare; a GameState that shows a card the Observer has no handle for, or
    /// that lacks a card the Observer holds a handle for; a card missing from the GameState or its
    /// catalog; an Event of a trace that does not apply to the GameState it is followed from, or a
    /// transition whose GameState is not the one its trace leads to (ADR-0029). An internal inconsistency, never expected from a Game followed from its Empty
    /// GameState.
    ObserverOutOfStep,
    /// There are more Observed Cards than a handle can number.
    ObservedCardOverflow,
    /// An Observed Action names a Player other than the Viewer.
    ActionOfAnotherPlayer { player: PlayerId },
    /// An Observed Answer names a hidden card: the Viewer cannot name a card it cannot see, so
    /// there is no Answer to translate (ADR-0034).
    AnswerNamesHiddenCard,
    /// A Legal Action of the Viewer names a card in a zone the Viewer cannot see; listing it would
    /// expose the card.
    LegalActionNamesUnseenCard { action: ActionId },
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
            Self::ActionOfAnotherPlayer { player } => {
                write!(
                    f,
                    "the Observed Action is of Player {}, not the Viewer",
                    player.0
                )
            }
            Self::AnswerNamesHiddenCard => {
                write!(f, "the Observed Answer names a card the Viewer cannot see")
            }
            Self::LegalActionNamesUnseenCard { action } => write!(
                f,
                "the Legal Action `{}` names a card the Viewer cannot see",
                action.as_str()
            ),
        }
    }
}

impl std::error::Error for ObservationError {}
