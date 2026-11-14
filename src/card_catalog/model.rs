//! The validated, typed Card Catalog. Values of these types are built only by
//! [`CardCatalog::validate`], so their invariants hold.

use super::draft::CardCatalogDraft;
use super::error::CatalogError;
use crate::id::id_type;
use crate::ruleset_definition::{
    CardTypeId, Effect, Expression, FieldName, FieldValue, RulesetDefinition, RulesetId,
    RulesetVersion, ZoneId,
};

id_type!(
    /// The identity of a Card Definition, unique within its catalog.
    CardId
);

/// A validated Card Catalog: the Card Definitions of one ruleset version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardCatalog {
    pub(super) meta: CatalogMeta,
    pub(super) cards: Vec<CardDefinition>,
}

impl CardCatalog {
    /// Validates a draft against the ruleset it targets, reporting the first error.
    pub fn validate(
        ruleset: &RulesetDefinition,
        draft: CardCatalogDraft,
    ) -> Result<CardCatalog, CatalogError> {
        super::validate::validate(ruleset, draft)
    }

    pub fn meta(&self) -> &CatalogMeta {
        &self.meta
    }

    /// The Card Definitions in file order.
    pub fn cards(&self) -> &[CardDefinition] {
        &self.cards
    }

    pub fn card(&self, id: &CardId) -> Option<&CardDefinition> {
        self.cards.iter().find(|card| card.id() == id)
    }
}

/// The ruleset a catalog was validated against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMeta {
    pub(super) ruleset: RulesetId,
    pub(super) ruleset_version: RulesetVersion,
}

impl CatalogMeta {
    pub fn ruleset(&self) -> &RulesetId {
        &self.ruleset
    }

    pub fn ruleset_version(&self) -> RulesetVersion {
        self.ruleset_version
    }
}

/// The immutable definition of a card: its id, its Card Type and one value per field of that type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDefinition {
    pub(super) id: CardId,
    pub(super) card_type: CardTypeId,
    pub(super) fields: Vec<(FieldName, FieldValue)>,
    pub(super) effects: Vec<CardEffect>,
}

impl CardDefinition {
    pub fn id(&self) -> &CardId {
        &self.id
    }

    pub fn card_type(&self) -> &CardTypeId {
        &self.card_type
    }

    /// The field values in the order of the Card Type's schema, whatever the order of the file.
    pub fn fields(&self) -> &[(FieldName, FieldValue)] {
        &self.fields
    }

    pub fn field(&self, name: &FieldName) -> Option<&FieldValue> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
    }

    /// The Card Effects the card carries, in file order: triggered ones are resolved from Events
    /// (ADR-0023) and continuous ones read as Field Modifiers (ADR-0022, ADR-0024).
    pub fn effects(&self) -> &[CardEffect] {
        &self.effects
    }
}

/// An ability carried by a Card Definition, in one of two shapes (ADR-0022).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardEffect {
    Triggered(TriggeredCardEffect),
    Continuous(ContinuousCardEffect),
}

/// `on`, an optional `if` and `do`: when an Event matches, and the condition holds, the effects
/// run. The condition and the effects read the carrying card as `self`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggeredCardEffect {
    pub(super) matcher: TriggerMatcher,
    pub(super) condition: Option<Expression>,
    pub(super) body: Vec<Effect>,
}

impl TriggeredCardEffect {
    pub fn matcher(&self) -> &TriggerMatcher {
        &self.matcher
    }

    pub fn condition(&self) -> Option<&Expression> {
        self.condition.as_ref()
    }

    /// The Action Effects to run; never empty.
    pub fn body(&self) -> &[Effect] {
        &self.body
    }
}

/// What a Triggered Card Effect matches among Events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerMatcher {
    /// The card enters the zone.
    Enter(ZoneId),
    /// The controller's Turn began.
    TurnStart,
}

/// `while` and `modify`: while the condition holds, the Field Modifier applies to the carrying
/// card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuousCardEffect {
    pub(super) condition: Expression,
    pub(super) scope: Option<ModifierScope>,
    pub(super) modify: FieldModifier,
}

impl ContinuousCardEffect {
    pub fn condition(&self) -> &Expression {
        &self.condition
    }

    /// The cards the Field Modifier reaches; `None` for a modifier of the carrying card alone.
    pub fn scope(&self) -> Option<&ModifierScope> {
        self.scope.as_ref()
    }

    pub fn modify(&self) -> &FieldModifier {
        &self.modify
    }
}

/// `scope: { zone, card_type, owner?, include_self? }`: the cards a Field Modifier reaches besides
/// (or instead of) its carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifierScope {
    pub(super) zone: ZoneId,
    pub(super) card_type: CardTypeId,
    pub(super) owner: ScopeOwner,
    pub(super) include_self: bool,
}

impl ModifierScope {
    /// The zone of its owner that a target is in.
    pub fn zone(&self) -> &ZoneId {
        &self.zone
    }

    /// The Card Type of a target's Card Definition.
    pub fn card_type(&self) -> &CardTypeId {
        &self.card_type
    }

    pub fn owner(&self) -> ScopeOwner {
        self.owner
    }

    /// Whether the carrying card is itself a target, when it matches.
    pub fn include_self(&self) -> bool {
        self.include_self
    }
}

/// Whose cards a Modifier Scope reaches, relative to the carrying card's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeOwner {
    /// The carrying card's owner (`owner` absent).
    Controller,
    /// Any Player other than the carrying card's owner (`owner: opponent`).
    Opponent,
}

/// An additive integer on an int field of the Card Definition of each card a Continuous Card
/// Effect reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldModifier {
    pub(super) field: FieldName,
    pub(super) amount: i64,
}

impl FieldModifier {
    pub fn field(&self) -> &FieldName {
        &self.field
    }

    pub fn amount(&self) -> i64 {
        self.amount
    }
}
