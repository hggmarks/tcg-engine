//! The validated, typed Card Catalog. Values of these types are built only by
//! [`CardCatalog::validate`], so their invariants hold.

use super::draft::CardCatalogDraft;
use super::error::CatalogError;
use crate::id::id_type;
use crate::ruleset_definition::{
    CardTypeId, FieldName, FieldValue, RulesetDefinition, RulesetId, RulesetVersion,
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
}
