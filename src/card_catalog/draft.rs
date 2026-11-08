//! Plain data that a frontend fills before validation. No field has an invariant.

use crate::ruleset_definition::FieldValue;

/// An unvalidated Card Catalog. Field names mirror the keys of the catalog format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardCatalogDraft {
    pub meta: CatalogMetaDraft,
    pub cards: Vec<CardDraft>,
}

/// The ruleset a catalog targets, by id and version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMetaDraft {
    pub ruleset: String,
    pub ruleset_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDraft {
    pub id: String,
    pub card_type: String,
    /// The field values in the order the frontend gave them; a name may repeat.
    pub fields: Vec<(String, FieldValue)>,
    pub effects: Vec<EffectDraft>,
}

/// An effect of a card. No effect primitive exists yet, so only its name is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectDraft {
    pub primitive: String,
}
