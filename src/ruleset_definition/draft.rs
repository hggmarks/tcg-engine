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
    /// The Setup Steps, in order. Empty means no steps.
    pub setup: Vec<SetupStepDraft>,
    /// The actions, in order. Empty means none.
    pub actions: Vec<ActionDraft>,
    /// The turn structure. `None` means the Game has no Turns.
    pub turn: Option<TurnDraft>,
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

/// An unvalidated Setup Step. The strings are the keys of the ruleset format: `player` is a Player
/// Selector such as `each`, and the zones are zone ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupStepDraft {
    Shuffle {
        player: String,
        zone: String,
    },
    Draw {
        player: String,
        from: String,
        to: String,
        count: u32,
    },
}

/// An unvalidated action, in the order validation checks it: params, then cost, then effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDraft {
    pub id: String,
    pub params: Vec<ParamDraft>,
    pub cost: Vec<CostDraft>,
    pub effects: Vec<EffectDraft>,
}

/// An unvalidated param: its name, and the zone and card type of the card it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDraft {
    pub name: String,
    pub zone: String,
    pub card_type: String,
}

/// The text of a ref, such as `card` or `card.cost`; the engine parses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefDraft(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostDraft {
    Tap { zone: String, count: RefDraft },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectDraft {
    EndTurn,
    Move {
        card: RefDraft,
        to: String,
    },
    SetState {
        card: RefDraft,
        state: String,
        value: FieldValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnDraft {
    pub phases: Vec<PhaseDraft>,
}

/// An unvalidated phase. Exactly one of `automatic` and `actions` must be present; validation
/// rejects any other combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseDraft {
    pub id: String,
    pub automatic: Option<Vec<AutomaticStepDraft>>,
    /// The ids of the actions the Active Player may take in this phase.
    pub actions: Option<Vec<String>>,
}

/// An unvalidated step of an automatic phase. `player` is a Player Selector (`each` or `active`),
/// `position` is `top`, and the other strings are ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomaticStepDraft {
    Untap {
        player: String,
        zones: Vec<String>,
    },
    SetState {
        player: String,
        zone: String,
        state: String,
        value: FieldValue,
    },
    Draw {
        player: String,
        from: String,
        to: String,
        count: u32,
    },
    Move {
        player: String,
        from: String,
        to: String,
        count: u32,
        position: String,
    },
    If {
        condition: ConditionDraft,
        then: Vec<AutomaticStepDraft>,
        otherwise: Vec<AutomaticStepDraft>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionDraft {
    FirstTurn,
}
