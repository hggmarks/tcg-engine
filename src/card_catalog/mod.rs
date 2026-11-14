//! The typed Card Catalog: the Card Definitions of a game, validated against a Ruleset Definition.
//!
//! A frontend fills a plain-data [`CardCatalogDraft`] and calls [`CardCatalog::validate`]. This is
//! stage 2 of ADR-0004's two-stage loading: the catalog is checked against an already loaded
//! [`RulesetDefinition`](crate::ruleset_definition::RulesetDefinition).
//!
//! Validation reports the first error only, in a fixed order: the meta (ruleset id, then ruleset
//! version), then each card in file order. Within a card: its id grammar, its id against earlier
//! cards, its card type, then its fields in file order (a repeat, an unknown field, a type
//! mismatch), then the missing fields in schema order, then its effects (ADR-0022): the list against
//! the card type's `effects` flag, then each element in order, which is checked fully and, if it is
//! continuous (not supported yet), rejected as unsupported (ADR-0023).

mod draft;
mod error;
mod model;
mod validate;

pub use draft::{
    CardCatalogDraft, CardDraft, CardEffectDraft, CatalogMetaDraft, ModifyDraft,
    TriggerMatcherDraft,
};
pub use error::{CatalogError, CatalogErrorKind};
pub use model::{
    CardCatalog, CardDefinition, CardEffect, CardId, CatalogMeta, ContinuousCardEffect,
    FieldModifier, TriggerMatcher, TriggeredCardEffect,
};
