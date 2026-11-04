//! The typed Ruleset Definition: the structural part of a ruleset (meta, parameters, zones, card
//! types and deck inputs) and its semantic validation.
//!
//! A frontend (a YAML loader, code, a script) fills a plain-data [`RulesetDraft`] and calls
//! [`RulesetDefinition::validate`]. Every semantic check lives here, so every frontend gets
//! identical errors. This is stage 1 of ADR-0004's two-stage loading; the card catalog stage
//! validates against the resulting [`RulesetDefinition`].
//!
//! Validation reports the first error only, in a fixed order: `parameters.players`, then
//! `parameters.starting_player`, then zones, card types and inputs, each list in declaration
//! order. Within an element its id is checked for duplicates against earlier elements before its
//! content, in key order.

mod draft;
mod error;
mod model;
mod validate;

pub use draft::{
    CardTypeDraft, DeckInputDraft, FieldDraft, FieldTypeDraft, InitializerDraft,
    InstanceStateDraft, MetaDraft, ParametersDraft, RulesetDraft, ZoneDraft,
};
pub use error::{DefinitionError, DefinitionErrorKind, ReferenceTarget};
pub use model::{
    CardType, CardTypeId, DeckInput, DeckInputId, FieldName, FieldSchema, FieldType, FieldValue,
    Initializer, InstanceStateSchema, Meta, Parameters, PlayerCount, PlayerPosition,
    RulesetDefinition, RulesetId, RulesetVersion, StateName, Visibility, Zone, ZoneId,
};
