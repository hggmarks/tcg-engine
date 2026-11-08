//! Plain data that a frontend fills before validation. No field has an invariant.

/// An unvalidated Deck List: the cards provided for some deck inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckListDraft {
    pub inputs: Vec<DeckInputCardsDraft>,
}

/// The cards provided for one deck input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckInputCardsDraft {
    /// The id of a deck input of the ruleset.
    pub input: String,
    pub cards: Vec<DeckEntryDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckEntryDraft {
    /// The id of a card of the catalog.
    pub card: String,
    pub count: u32,
}
