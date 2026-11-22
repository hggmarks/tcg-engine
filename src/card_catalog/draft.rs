//! Plain data that a frontend fills before validation. No field has an invariant.

use crate::ruleset_definition::{CostDraft, EffectDraft, ExpressionDraft, FieldValue};

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
/// `while_condition` (`while`) and `modify`; an activated one uses `activated` (ADR-0038).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardEffectDraft {
    pub on: Option<TriggerMatcherDraft>,
    pub condition: Option<ExpressionDraft>,
    pub body: Option<Vec<EffectDraft>>,
    pub while_condition: Option<ExpressionDraft>,
    pub modify: Option<ModifyDraft>,
    pub scope: Option<ScopeDraft>,
    pub activated: Option<ActivatedDraft>,
}

/// An unvalidated `activated: { cost, do }`. `cost` defaults to no steps; `body` is `None` when the
/// `do` key is missing, so that every frontend gets the same engine error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivatedDraft {
    pub cost: Vec<CostDraft>,
    pub body: Option<Vec<EffectDraft>>,
}

/// An unvalidated `on`: a single-key map. `Other` keeps the key of a matcher the engine does not
/// know, so that every frontend gets the same engine error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerMatcherDraft {
    /// `enter: <zone>`.
    Enter(String),
    /// `turn_start`: the controller's Turn began.
    TurnStart,
    Other(String),
}

/// An unvalidated `modify: { field, amount }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifyDraft {
    pub field: String,
    pub amount: i64,
}

/// An unvalidated `scope: { zone, card_type, owner?, include_self? }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeDraft {
    pub zone: String,
    pub card_type: String,
    pub owner: Option<String>,
    pub include_self: Option<bool>,
}
