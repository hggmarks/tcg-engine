//! Plain data that a frontend fills before validation. No field has an invariant.

use crate::ruleset_definition::{EffectDraft, ExpressionDraft, FieldValue};

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
    pub effects: Vec<CardEffectDraft>,
}

/// An unvalidated Card Effect: one element of a card's `effects` list. The keys of both shapes are
/// optional here; validation decides which shape the element is and rejects a mix (ADR-0022).
///
/// A triggered Card Effect uses `on`, `condition` (`if`) and `body` (`do`); a continuous one uses
/// `while_condition` (`while`) and `modify`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardEffectDraft {
    pub on: Option<TriggerMatcherDraft>,
    pub condition: Option<ExpressionDraft>,
    pub body: Option<Vec<EffectDraft>>,
    pub while_condition: Option<ExpressionDraft>,
    pub modify: Option<ModifyDraft>,
}

/// An unvalidated `on`: a single-key map. `Other` keeps the key of a matcher the engine does not
/// know, so that every frontend gets the same engine error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerMatcherDraft {
    /// `enter: <zone>`.
    Enter(String),
    /// `turn_start`.
    TurnStart,
    Other(String),
}

/// An unvalidated `modify: { field, amount }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifyDraft {
    pub field: String,
    pub amount: i64,
}
