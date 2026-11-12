//! Semantic validation of a [`CardCatalogDraft`], in the order documented on the module.

use std::collections::BTreeMap;

use super::draft::{CardCatalogDraft, CardDraft};
use super::error::{CatalogError, CatalogErrorKind};
use super::model::{CardCatalog, CardDefinition, CardId, CatalogMeta};
use crate::definition_path::DefinitionPath;
use crate::id::is_valid_id;
use crate::ruleset_definition::{
    CardType, CardTypeId, FieldName, FieldValue, ReferenceTarget, RulesetDefinition,
};

fn fail<T>(path: DefinitionPath, kind: CatalogErrorKind) -> Result<T, CatalogError> {
    Err(CatalogError { path, kind })
}

pub(super) fn validate(
    ruleset: &RulesetDefinition,
    draft: CardCatalogDraft,
) -> Result<CardCatalog, CatalogError> {
    let meta_path = DefinitionPath::root().key("meta");
    if draft.meta.ruleset != ruleset.meta().id().as_str() {
        return fail(
            meta_path.key("ruleset"),
            CatalogErrorKind::RulesetIdMismatch {
                expected: ruleset.meta().id().clone(),
                found: draft.meta.ruleset,
            },
        );
    }
    if draft.meta.ruleset_version != ruleset.meta().version().get() {
        return fail(
            meta_path.key("ruleset_version"),
            CatalogErrorKind::RulesetVersionMismatch {
                expected: ruleset.meta().version(),
                found: draft.meta.ruleset_version,
            },
        );
    }

    let cards_path = DefinitionPath::root().key("cards");
    let mut positions: BTreeMap<&str, usize> = BTreeMap::new();
    let mut cards = Vec::with_capacity(draft.cards.len());
    for (index, card) in draft.cards.iter().enumerate() {
        if !is_valid_id(&card.id) {
            return fail(
                cards_path.clone().index(index).key("id"),
                CatalogErrorKind::InvalidId {
                    id: card.id.clone(),
                },
            );
        }
        if let Some(&first) = positions.get(card.id.as_str()) {
            return fail(
                cards_path.clone().index(index),
                CatalogErrorKind::DuplicateId {
                    id: card.id.clone(),
                    first: cards_path.clone().index(first),
                },
            );
        }
        positions.insert(&card.id, index);
        cards.push(validate_card(ruleset, card)?);
    }

    let index = cards
        .iter()
        .enumerate()
        .map(|(position, card)| (card.id().clone(), position))
        .collect();
    Ok(CardCatalog {
        meta: CatalogMeta {
            ruleset: ruleset.meta().id().clone(),
            ruleset_version: ruleset.meta().version(),
        },
        cards,
        index,
    })
}

fn validate_card(
    ruleset: &RulesetDefinition,
    card: &CardDraft,
) -> Result<CardDefinition, CatalogError> {
    let at = DefinitionPath::root().key("cards").id(&card.id);

    let Some(card_type) = ruleset
        .card_types()
        .iter()
        .find(|card_type| card_type.id().as_str() == card.card_type)
    else {
        return fail(
            at.key("type"),
            CatalogErrorKind::DanglingReference {
                target: ReferenceTarget::CardType,
                id: card.card_type.clone(),
            },
        );
    };

    let fields = validate_fields(card_type, card, &at)?;

    if !card.effects.is_empty() {
        let effects_path = at.key("effects");
        if !card_type.allows_effects() {
            return fail(effects_path, CatalogErrorKind::EffectsNotAllowed);
        }
        // No effect primitive exists yet, so the first entry is already unsupported.
        if let Some(effect) = card.effects.first() {
            return fail(
                effects_path.index(0),
                CatalogErrorKind::UnsupportedEffect {
                    primitive: effect.primitive.clone(),
                },
            );
        }
    }

    Ok(CardDefinition {
        id: CardId::new(&card.id),
        card_type: CardTypeId::new(&card.card_type),
        fields,
    })
}

/// Checks the fields given by the card and returns their values in the order of the schema.
fn validate_fields(
    card_type: &CardType,
    card: &CardDraft,
    at: &DefinitionPath,
) -> Result<Vec<(FieldName, FieldValue)>, CatalogError> {
    let fields_path = at.clone().key("fields");
    let mut positions: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, (name, value)) in card.fields.iter().enumerate() {
        if let Some(&first) = positions.get(name.as_str()) {
            return fail(
                fields_path.clone().index(index),
                CatalogErrorKind::DuplicateField {
                    field: name.clone(),
                    first: fields_path.clone().index(first),
                },
            );
        }
        positions.insert(name, index);

        let Some(schema) = card_type
            .fields()
            .iter()
            .find(|schema| schema.name().as_str() == name)
        else {
            return fail(
                fields_path.id(name),
                CatalogErrorKind::UnknownField {
                    field: name.clone(),
                },
            );
        };
        if schema.field_type() != value.field_type() {
            return fail(
                fields_path.id(name),
                CatalogErrorKind::FieldTypeMismatch {
                    declared: schema.field_type(),
                    found: value.field_type(),
                },
            );
        }
    }

    let mut ordered = Vec::with_capacity(card_type.fields().len());
    for schema in card_type.fields() {
        let Some(&position) = positions.get(schema.name().as_str()) else {
            return fail(
                fields_path,
                CatalogErrorKind::MissingField {
                    field: schema.name().clone(),
                },
            );
        };
        if let Some((_, value)) = card.fields.get(position) {
            ordered.push((schema.name().clone(), value.clone()));
        }
    }
    Ok(ordered)
}
