//! Semantic validation of a [`CardCatalogDraft`], in the order documented on the module.

use std::collections::BTreeMap;

use super::draft::{
    CardCatalogDraft, CardDraft, CardEffectDraft, ModifyDraft, TriggerMatcherDraft,
};
use super::error::{CatalogError, CatalogErrorKind};
use super::model::{
    CardCatalog, CardDefinition, CardEffect, CardId, CatalogMeta, ContinuousCardEffect,
    FieldModifier, TriggerMatcher, TriggeredCardEffect,
};
use crate::definition_path::DefinitionPath;
use crate::id::is_valid_id;
use crate::ruleset_definition::{
    CardType, CardTypeId, DefinitionError, EffectDraft, ExpressionDraft, FieldName, FieldType,
    FieldValue, ReferenceTarget, RulesetDefinition, SelfScope, ZoneId,
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

    Ok(CardCatalog {
        meta: CatalogMeta {
            ruleset: ruleset.meta().id().clone(),
            ruleset_version: ruleset.meta().version(),
        },
        cards,
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

    let effects = validate_effects(ruleset, card_type, card, &at)?;

    Ok(CardDefinition {
        id: CardId::new(&card.id),
        card_type: CardTypeId::new(&card.card_type),
        fields,
        effects,
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

/// The Card Effects of a card (ADR-0022): the list against the card type's `effects` flag, then each
/// element in order. Every element is checked and built; the first one that is well-formed is then
/// rejected as unsupported, so the list that is returned is always empty today.
fn validate_effects(
    ruleset: &RulesetDefinition,
    card_type: &CardType,
    card: &CardDraft,
    at: &DefinitionPath,
) -> Result<Vec<CardEffect>, CatalogError> {
    if card.effects.is_empty() {
        return Ok(Vec::new());
    }
    let list = at.clone().key("effects");
    if !card_type.allows_effects() {
        return fail(list, CatalogErrorKind::EffectsNotAllowed);
    }
    let scope = SelfScope::new(ruleset, card_type.id());
    let mut effects = Vec::with_capacity(card.effects.len());
    for (index, draft) in card.effects.iter().enumerate() {
        let at = list.clone().index(index);
        let effect = validate_card_effect(ruleset, card_type, &scope, draft, &at)?;
        if let Some(kind) = unsupported(&effect) {
            return fail(at, kind);
        }
        effects.push(effect);
    }
    Ok(effects)
}

/// Why a well-formed Card Effect cannot be loaded yet. Ticket [17] lifts it for a triggered one and
/// ticket [18] for a continuous one, each by returning `None` here.
fn unsupported(effect: &CardEffect) -> Option<CatalogErrorKind> {
    match effect {
        CardEffect::Triggered(_) => Some(CatalogErrorKind::UnsupportedTriggeredCardEffect),
        CardEffect::Continuous(_) => Some(CatalogErrorKind::UnsupportedContinuousCardEffect),
    }
}

/// A Card Effect is triggered (`on`, `if`, `do`) or continuous (`while`, `modify`). An element with
/// keys of both is mixed; one with neither `on` nor `while` has no shape.
fn validate_card_effect(
    ruleset: &RulesetDefinition,
    card_type: &CardType,
    scope: &SelfScope<'_>,
    draft: &CardEffectDraft,
    at: &DefinitionPath,
) -> Result<CardEffect, CatalogError> {
    let triggered_keys = draft.on.is_some() || draft.condition.is_some() || draft.body.is_some();
    let continuous_keys = draft.while_condition.is_some() || draft.modify.is_some();
    if triggered_keys && continuous_keys {
        return fail(at.clone(), CatalogErrorKind::MixedCardEffectShapes);
    }
    if let Some(matcher) = &draft.on {
        validate_triggered(ruleset, scope, draft, matcher, at).map(CardEffect::Triggered)
    } else if let Some(condition) = &draft.while_condition {
        validate_continuous(card_type, scope, condition, draft.modify.as_ref(), at)
            .map(CardEffect::Continuous)
    } else {
        fail(at.clone(), CatalogErrorKind::MissingCardEffectShape)
    }
}

/// `on`, then `if`, then `do`.
fn validate_triggered(
    ruleset: &RulesetDefinition,
    scope: &SelfScope<'_>,
    draft: &CardEffectDraft,
    matcher: &TriggerMatcherDraft,
    at: &DefinitionPath,
) -> Result<TriggeredCardEffect, CatalogError> {
    let on = at.clone().key("on");
    let matcher = match matcher {
        TriggerMatcherDraft::Enter(zone) => {
            let zone_id = ZoneId::new(zone);
            if ruleset.zone(&zone_id).is_none() {
                return fail(
                    on.key("enter"),
                    CatalogErrorKind::DanglingReference {
                        target: ReferenceTarget::Zone,
                        id: zone.clone(),
                    },
                );
            }
            TriggerMatcher::Enter(zone_id)
        }
        TriggerMatcherDraft::TurnStart => TriggerMatcher::TurnStart,
        TriggerMatcherDraft::Other(key) => {
            return fail(
                on,
                CatalogErrorKind::UnknownTriggerMatcher {
                    matcher: key.clone(),
                },
            )
        }
    };

    let condition = draft
        .condition
        .as_ref()
        .map(|condition| scope.check_condition(condition, at.clone().key("if")))
        .transpose()
        .map_err(invalid)?;

    let do_path = at.clone().key("do");
    let body = match &draft.body {
        None => return fail(do_path, CatalogErrorKind::MissingDo),
        Some(body) if body.is_empty() => return fail(do_path, CatalogErrorKind::EmptyDo),
        Some(body) => body,
    };
    let checked = scope
        .check_effects(body, do_path.clone())
        .map_err(invalid)?;
    if let Some(end_turn) = find_end_turn(body, &do_path) {
        return fail(end_turn, CatalogErrorKind::EndTurnInCardEffect);
    }

    Ok(TriggeredCardEffect {
        matcher,
        condition,
        body: checked,
    })
}

/// `while`, then `modify`: a declared int field of the carrying card's card type.
fn validate_continuous(
    card_type: &CardType,
    scope: &SelfScope<'_>,
    condition: &ExpressionDraft,
    modify: Option<&ModifyDraft>,
    at: &DefinitionPath,
) -> Result<ContinuousCardEffect, CatalogError> {
    let condition = scope
        .check_condition(condition, at.clone().key("while"))
        .map_err(invalid)?;
    let Some(modify) = modify else {
        return fail(at.clone().key("modify"), CatalogErrorKind::MissingModify);
    };
    let field_path = at.clone().key("modify").key("field");
    let Some(schema) = card_type
        .fields()
        .iter()
        .find(|schema| schema.name().as_str() == modify.field)
    else {
        return fail(
            field_path,
            CatalogErrorKind::UnknownModifyField {
                field: modify.field.clone(),
            },
        );
    };
    if schema.field_type() != FieldType::Int {
        return fail(
            field_path,
            CatalogErrorKind::ModifyFieldNotInt {
                field: modify.field.clone(),
                found: schema.field_type(),
            },
        );
    }
    Ok(ContinuousCardEffect {
        condition,
        modify: FieldModifier {
            field: schema.name().clone(),
            amount: modify.amount,
        },
    })
}

/// The path of the first `end_turn` in `effects`, in order, `then` before `else`.
fn find_end_turn(effects: &[EffectDraft], list: &DefinitionPath) -> Option<DefinitionPath> {
    for (index, effect) in effects.iter().enumerate() {
        let at = list.clone().index(index);
        match effect {
            EffectDraft::EndTurn => return Some(at),
            EffectDraft::If {
                then, otherwise, ..
            } => {
                if let Some(found) = find_end_turn(then, &at.clone().key("then")) {
                    return Some(found);
                }
                if let Some(found) = find_end_turn(otherwise, &at.key("else")) {
                    return Some(found);
                }
            }
            EffectDraft::Move { .. }
            | EffectDraft::SetState { .. }
            | EffectDraft::AddState { .. } => {}
        }
    }
    None
}

/// An error of the expression and effect checking, wrapped with the path it already has.
fn invalid(error: DefinitionError) -> CatalogError {
    CatalogError {
        path: error.path,
        kind: CatalogErrorKind::InvalidCardEffect {
            error: Box::new(error.kind),
        },
    }
}
