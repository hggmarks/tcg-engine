//! The validated, typed Ruleset Definition. Values of these types are built only by
//! [`RulesetDefinition::validate`], so their invariants hold.

use std::num::NonZeroU32;

use super::draft::RulesetDraft;
use super::error::DefinitionError;
use crate::id::id_type;

id_type!(
    /// The identity of a Ruleset (`meta.id`).
    RulesetId
);
id_type!(
    /// The identity of a zone, unique among the zones.
    ZoneId
);
id_type!(
    /// The identity of a card type, unique among the card types.
    CardTypeId
);
id_type!(
    /// The name of a field, unique within its card type.
    FieldName
);
id_type!(
    /// The name of an instance-state entry, unique within its card type.
    StateName
);
id_type!(
    /// The identity of a deck input, unique among the inputs.
    DeckInputId
);

/// A validated Ruleset Definition (the typed model ADR-0004 first called `Ruleset`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesetDefinition {
    pub(super) meta: Meta,
    pub(super) parameters: Parameters,
    pub(super) zones: Vec<Zone>,
    pub(super) card_types: Vec<CardType>,
    pub(super) deck_inputs: Vec<DeckInput>,
    pub(super) setup: Vec<SetupStep>,
}

impl RulesetDefinition {
    /// Validates a draft and converts it into the typed model, reporting the first error.
    pub fn validate(draft: RulesetDraft) -> Result<RulesetDefinition, DefinitionError> {
        super::validate::validate(draft)
    }

    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    pub fn parameters(&self) -> &Parameters {
        &self.parameters
    }

    /// The zones in declared order.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    pub fn zone(&self, id: &ZoneId) -> Option<&Zone> {
        self.zones.iter().find(|zone| zone.id() == id)
    }

    /// The card types in declared order.
    pub fn card_types(&self) -> &[CardType] {
        &self.card_types
    }

    pub fn card_type(&self, id: &CardTypeId) -> Option<&CardType> {
        self.card_types
            .iter()
            .find(|card_type| card_type.id() == id)
    }

    /// The deck inputs in declared order.
    pub fn deck_inputs(&self) -> &[DeckInput] {
        &self.deck_inputs
    }

    /// The Setup Steps in declared order; empty when the ruleset declares none.
    pub fn setup(&self) -> &[SetupStep] {
        &self.setup
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub(super) id: RulesetId,
    pub(super) version: RulesetVersion,
}

impl Meta {
    pub fn id(&self) -> &RulesetId {
        &self.id
    }

    pub fn version(&self) -> RulesetVersion {
        self.version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RulesetVersion(pub(super) u32);

impl RulesetVersion {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameters {
    pub(super) player_count: PlayerCount,
    pub(super) starting_player: PlayerPosition,
}

impl Parameters {
    pub fn player_count(&self) -> PlayerCount {
        self.player_count
    }

    pub fn starting_player(&self) -> PlayerPosition {
        self.starting_player
    }
}

/// The number of Players in a Game; at least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerCount(pub(super) NonZeroU32);

impl PlayerCount {
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// A 0-based position in the Player Order; always below the [`PlayerCount`] it was validated with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerPosition(pub(super) u32);

impl PlayerPosition {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub(super) id: ZoneId,
    pub(super) visibility: Visibility,
    pub(super) ordered: bool,
}

impl Zone {
    pub fn id(&self) -> &ZoneId {
        &self.id
    }

    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// Whether the zone keeps its cards in order. Stored, not yet enforced.
    pub fn ordered(&self) -> bool {
        self.ordered
    }
}

/// Who may see the cards of a zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Hidden,
    Owner,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardType {
    pub(super) id: CardTypeId,
    pub(super) fields: Vec<FieldSchema>,
    pub(super) instance_state: Vec<InstanceStateSchema>,
    pub(super) allows_effects: bool,
}

impl CardType {
    pub fn id(&self) -> &CardTypeId {
        &self.id
    }

    /// The fields in declared order.
    pub fn fields(&self) -> &[FieldSchema] {
        &self.fields
    }

    /// The instance-state entries in declared order.
    pub fn instance_state(&self) -> &[InstanceStateSchema] {
        &self.instance_state
    }

    pub fn allows_effects(&self) -> bool {
        self.allows_effects
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSchema {
    pub(super) name: FieldName,
    pub(super) field_type: FieldType,
}

impl FieldSchema {
    pub fn name(&self) -> &FieldName {
        &self.name
    }

    pub fn field_type(&self) -> FieldType {
        self.field_type
    }
}

/// The supported field types. `enum` and `enum set` belong to ADR-0004's closed set but are not
/// supported yet, so they are not represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Int,
    Bool,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceStateSchema {
    pub(super) name: StateName,
    pub(super) state_type: FieldType,
    pub(super) init: Initializer,
}

impl InstanceStateSchema {
    pub fn name(&self) -> &StateName {
        &self.name
    }

    pub fn state_type(&self) -> FieldType {
        self.state_type
    }

    pub fn init(&self) -> &Initializer {
        &self.init
    }
}

/// How an instance-state entry gets its value when a card instance is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initializer {
    Literal(FieldValue),
    /// The value of the named field of the same card type.
    FromField(FieldName),
}

/// A value of one of the supported field types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Int(i64),
    Bool(bool),
    Text(String),
}

impl FieldValue {
    pub fn field_type(&self) -> FieldType {
        match self {
            Self::Int(_) => FieldType::Int,
            Self::Bool(_) => FieldType::Bool,
            Self::Text(_) => FieldType::Text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckInput {
    pub(super) id: DeckInputId,
    pub(super) card_types: Vec<CardTypeId>,
    pub(super) size: u32,
    pub(super) max_copies: Option<u32>,
    pub(super) zone: ZoneId,
}

impl DeckInput {
    pub fn id(&self) -> &DeckInputId {
        &self.id
    }

    /// The card types a deck for this input may hold, in declared order.
    pub fn card_types(&self) -> &[CardTypeId] {
        &self.card_types
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn max_copies(&self) -> Option<u32> {
        self.max_copies
    }

    /// The zone the cards of this input are placed in.
    pub fn zone(&self) -> &ZoneId {
        &self.zone
    }
}

/// Which Players a Setup Step applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSelector {
    /// Every Player, in seat order. A step runs fully for one Player before the next Player.
    Each,
}

/// One step of Game setup, run after the cards of the Deck Lists are placed. Its zones exist in the
/// Ruleset Definition it was validated with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupStep {
    /// Shuffles `zone` of each selected Player with the GameState's Random State.
    Shuffle {
        player: PlayerSelector,
        zone: ZoneId,
    },
    /// Moves `count` cards, one at a time from the top of `from` onto the top of `to`.
    Draw {
        player: PlayerSelector,
        from: ZoneId,
        to: ZoneId,
        count: NonZeroU32,
    },
}
