//! The validated, typed Deck List. Values of these types are built only by
//! [`DeckList::validate`], so a deck list is always legal.

use std::num::NonZeroU32;

use super::draft::DeckListDraft;
use super::error::DeckError;
use crate::card_catalog::{CardCatalog, CardId};
use crate::ruleset_definition::{DeckInputId, RulesetDefinition};

/// A legal Deck List: every input of the ruleset provided exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckList {
    pub(super) inputs: Vec<DeckListInput>,
}

impl DeckList {
    /// Validates a draft against the ruleset's deck inputs and the catalog, reporting the first
    /// error.
    pub fn validate(
        ruleset: &RulesetDefinition,
        catalog: &CardCatalog,
        draft: DeckListDraft,
    ) -> Result<DeckList, DeckError> {
        super::validate::validate(ruleset, catalog, draft)
    }

    /// The inputs in the order of the ruleset's deck inputs.
    pub fn inputs(&self) -> &[DeckListInput] {
        &self.inputs
    }

    pub fn input(&self, id: &DeckInputId) -> Option<&DeckListInput> {
        self.inputs.iter().find(|input| input.input() == id)
    }
}

/// The cards provided for one deck input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckListInput {
    pub(super) input: DeckInputId,
    pub(super) entries: Vec<DeckEntry>,
}

impl DeckListInput {
    pub fn input(&self) -> &DeckInputId {
        &self.input
    }

    /// The entries in the order of the file.
    pub fn entries(&self) -> &[DeckEntry] {
        &self.entries
    }
}

/// A card and the number of copies of it; at least 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckEntry {
    pub(super) card: CardId,
    pub(super) count: NonZeroU32,
}

impl DeckEntry {
    pub fn card(&self) -> &CardId {
        &self.card
    }

    pub fn count(&self) -> NonZeroU32 {
        self.count
    }
}
