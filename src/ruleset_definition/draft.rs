//! Plain data that a frontend fills before validation. No field has an invariant.

use super::model::{FieldValue, Visibility};

/// An unvalidated Ruleset Definition. Field names mirror the keys of the ruleset format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesetDraft {
    pub meta: MetaDraft,
    pub parameters: ParametersDraft,
    pub zones: Vec<ZoneDraft>,
    pub card_types: Vec<CardTypeDraft>,
    pub inputs: Vec<DeckInputDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaDraft {
    pub id: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParametersDraft {
    pub players: u32,
    pub starting_player: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneDraft {
    pub id: String,
    pub visibility: Visibility,
    pub ordered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardTypeDraft {
    pub id: String,
    pub fields: Vec<FieldDraft>,
    pub instance_state: Vec<InstanceStateDraft>,
    pub effects: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDraft {
    pub name: String,
    pub field_type: FieldTypeDraft,
}

/// The closed set of field types of ADR-0004. `Enum` and `EnumSet` are recognized but not yet
/// supported: validation rejects them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldTypeDraft {
    Int,
    Bool,
    Text,
    Enum,
    EnumSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceStateDraft {
    pub name: String,
    pub state_type: FieldTypeDraft,
    pub init: InitializerDraft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitializerDraft {
    Literal(FieldValue),
    FromField(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckInputDraft {
    pub id: String,
    pub card_types: Vec<String>,
    pub size: u32,
    pub max_copies: Option<u32>,
    pub zone: String,
}
