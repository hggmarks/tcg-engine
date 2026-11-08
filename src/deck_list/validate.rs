//! Semantic validation of a [`DeckListDraft`], in the order documented on the module.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use super::draft::{DeckInputCardsDraft, DeckListDraft};
use super::error::{DeckError, DeckErrorKind};
use super::model::{DeckEntry, DeckList, DeckListInput};
use crate::card_catalog::{CardCatalog, CardId};
use crate::definition_path::DefinitionPath;
use crate::ruleset_definition::{DeckInput, ReferenceTarget, RulesetDefinition};

fn fail<T>(path: DefinitionPath, kind: DeckErrorKind) -> Result<T, DeckError> {
    Err(DeckError { path, kind })
}

pub(super) fn validate(
    ruleset: &RulesetDefinition,
    catalog: &CardCatalog,
    draft: DeckListDraft,
) -> Result<DeckList, DeckError> {
    let meta = catalog.meta();
    if meta.ruleset() != ruleset.meta().id() || meta.ruleset_version() != ruleset.meta().version() {
        return fail(
            DefinitionPath::root(),
            DeckErrorKind::CatalogFromOtherRuleset {
                expected_id: ruleset.meta().id().clone(),
                expected_version: ruleset.meta().version(),
                found_id: meta.ruleset().clone(),
                found_version: meta.ruleset_version(),
            },
        );
    }

    let inputs_path = DefinitionPath::root().key("inputs");
    let mut positions: BTreeMap<&str, usize> = BTreeMap::new();
    let mut provided: Vec<DeckListInput> = Vec::with_capacity(draft.inputs.len());
    for (index, provided_input) in draft.inputs.iter().enumerate() {
        let at = inputs_path.clone().index(index);
        let Some(input) = ruleset
            .deck_inputs()
            .iter()
            .find(|input| input.id().as_str() == provided_input.input)
        else {
            return fail(
                at.key("input"),
                DeckErrorKind::DanglingReference {
                    target: ReferenceTarget::DeckInput,
                    id: provided_input.input.clone(),
                },
            );
        };
        if let Some(&first) = positions.get(provided_input.input.as_str()) {
            return fail(
                at,
                DeckErrorKind::DuplicateInput {
                    input: input.id().clone(),
                    first: inputs_path.clone().index(first),
                },
            );
        }
        positions.insert(&provided_input.input, index);
        provided.push(validate_input(catalog, input, provided_input, &at)?);
    }

    let mut inputs = Vec::with_capacity(ruleset.deck_inputs().len());
    for input in ruleset.deck_inputs() {
        match provided.iter().position(|p| p.input() == input.id()) {
            Some(position) => inputs.push(provided.swap_remove(position)),
            None => {
                return fail(
                    inputs_path,
                    DeckErrorKind::MissingInput {
                        input: input.id().clone(),
                    },
                )
            }
        }
    }
    Ok(DeckList { inputs })
}

fn validate_input(
    catalog: &CardCatalog,
    input: &DeckInput,
    provided: &DeckInputCardsDraft,
    at: &DefinitionPath,
) -> Result<DeckListInput, DeckError> {
    let cards_path = at.clone().key("cards");
    let mut positions: BTreeMap<&str, usize> = BTreeMap::new();
    let mut entries = Vec::with_capacity(provided.cards.len());
    let mut total: u64 = 0;
    for (index, entry) in provided.cards.iter().enumerate() {
        let entry_path = cards_path.clone().index(index);
        let Some(count) = NonZeroU32::new(entry.count) else {
            return fail(entry_path.key("count"), DeckErrorKind::ZeroCount);
        };
        let Some(card) = catalog.card(&CardId::new(&entry.card)) else {
            return fail(
                entry_path.key("card"),
                DeckErrorKind::DanglingReference {
                    target: ReferenceTarget::Card,
                    id: entry.card.clone(),
                },
            );
        };
        if let Some(&first) = positions.get(entry.card.as_str()) {
            return fail(
                entry_path,
                DeckErrorKind::DuplicateCard {
                    card: card.id().clone(),
                    first: cards_path.clone().index(first),
                },
            );
        }
        positions.insert(&entry.card, index);
        if !input.card_types().contains(card.card_type()) {
            return fail(
                entry_path.key("card"),
                DeckErrorKind::CardTypeNotAllowed {
                    card: card.id().clone(),
                    card_type: card.card_type().clone(),
                },
            );
        }
        if let Some(max_copies) = input.max_copies() {
            if count.get() > max_copies {
                return fail(
                    entry_path.key("count"),
                    DeckErrorKind::TooManyCopies {
                        card: card.id().clone(),
                        count: count.get(),
                        max_copies,
                    },
                );
            }
        }
        let Some(sum) = total.checked_add(u64::from(count.get())) else {
            return fail(cards_path, DeckErrorKind::DeckSizeOverflow);
        };
        total = sum;
        entries.push(DeckEntry {
            card: card.id().clone(),
            count,
        });
    }

    if total != u64::from(input.size()) {
        return fail(
            cards_path,
            DeckErrorKind::WrongDeckSize {
                expected: input.size(),
                actual: total,
            },
        );
    }
    Ok(DeckListInput {
        input: input.id().clone(),
        entries,
    })
}
