//! Semantic validation of a [`RulesetDraft`], in the order documented on the module.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use super::draft::{
    CardTypeDraft, DeckInputDraft, FieldTypeDraft, InitializerDraft, InstanceStateDraft,
    RulesetDraft,
};
use super::error::{DefinitionError, DefinitionErrorKind, ReferenceTarget};
use super::model::{
    CardType, CardTypeId, DeckInput, DeckInputId, FieldName, FieldSchema, FieldType, Initializer,
    InstanceStateSchema, Meta, Parameters, PlayerCount, PlayerPosition, RulesetDefinition,
    RulesetId, RulesetVersion, StateName, Zone, ZoneId,
};
use crate::definition_path::DefinitionPath;
use crate::id::is_valid_id;

fn fail<T>(path: DefinitionPath, kind: DefinitionErrorKind) -> Result<T, DefinitionError> {
    Err(DefinitionError { path, kind })
}

/// Rejects an id that breaks the identifier grammar. `key` is the key that holds the id in the
/// element at `index` of `list`.
fn check_id(
    list: &DefinitionPath,
    index: usize,
    key: &'static str,
    id: &str,
) -> Result<(), DefinitionError> {
    if is_valid_id(id) {
        Ok(())
    } else {
        fail(
            list.clone().index(index).key(key),
            DefinitionErrorKind::InvalidId { id: id.to_string() },
        )
    }
}

/// Ids seen so far in one list, to reject a repeated id with the position of its first use.
struct Seen<'a> {
    list: DefinitionPath,
    positions: BTreeMap<&'a str, usize>,
}

impl<'a> Seen<'a> {
    fn new(list: DefinitionPath) -> Self {
        Self {
            list,
            positions: BTreeMap::new(),
        }
    }

    fn insert(&mut self, id: &'a str, index: usize) -> Result<(), DefinitionError> {
        match self.positions.get(id) {
            Some(&first) => fail(
                self.list.clone().index(index),
                DefinitionErrorKind::DuplicateId {
                    id: id.to_string(),
                    first: self.list.clone().index(first),
                },
            ),
            None => {
                self.positions.insert(id, index);
                Ok(())
            }
        }
    }
}

fn supported(
    field_type: FieldTypeDraft,
    path: DefinitionPath,
) -> Result<FieldType, DefinitionError> {
    match field_type {
        FieldTypeDraft::Int => Ok(FieldType::Int),
        FieldTypeDraft::Bool => Ok(FieldType::Bool),
        FieldTypeDraft::Text => Ok(FieldType::Text),
        FieldTypeDraft::Enum | FieldTypeDraft::EnumSet => fail(
            path.key("type"),
            DefinitionErrorKind::UnsupportedFieldType { field_type },
        ),
    }
}

pub(super) fn validate(draft: RulesetDraft) -> Result<RulesetDefinition, DefinitionError> {
    let RulesetDraft {
        meta,
        parameters,
        zones,
        card_types,
        inputs,
    } = draft;

    if !is_valid_id(&meta.id) {
        return fail(
            DefinitionPath::root().key("meta").key("id"),
            DefinitionErrorKind::InvalidId { id: meta.id },
        );
    }

    let parameters_path = DefinitionPath::root().key("parameters");
    let Some(players) = NonZeroU32::new(parameters.players) else {
        return fail(
            parameters_path.key("players"),
            DefinitionErrorKind::InvalidPlayerCount { players: 0 },
        );
    };
    if parameters.starting_player >= parameters.players {
        return fail(
            parameters_path.key("starting_player"),
            DefinitionErrorKind::InvalidStartingPlayer {
                starting_player: parameters.starting_player,
                players: parameters.players,
            },
        );
    }

    let zones_path = DefinitionPath::root().key("zones");
    let mut seen = Seen::new(zones_path.clone());
    let mut valid_zones = Vec::with_capacity(zones.len());
    for (index, zone) in zones.iter().enumerate() {
        check_id(&zones_path, index, "id", &zone.id)?;
        seen.insert(&zone.id, index)?;
        valid_zones.push(Zone {
            id: ZoneId::new(&zone.id),
            visibility: zone.visibility,
            ordered: zone.ordered,
        });
    }

    let card_types_path = DefinitionPath::root().key("card_types");
    let mut seen = Seen::new(card_types_path.clone());
    let mut valid_card_types = Vec::with_capacity(card_types.len());
    for (index, card_type) in card_types.iter().enumerate() {
        check_id(&card_types_path, index, "id", &card_type.id)?;
        seen.insert(&card_type.id, index)?;
        valid_card_types.push(validate_card_type(card_type)?);
    }

    let inputs_path = DefinitionPath::root().key("inputs");
    let mut seen = Seen::new(inputs_path.clone());
    let mut valid_inputs = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        check_id(&inputs_path, index, "id", &input.id)?;
        seen.insert(&input.id, index)?;
        valid_inputs.push(validate_input(input, &valid_zones, &valid_card_types)?);
    }

    Ok(RulesetDefinition {
        meta: Meta {
            id: RulesetId::new(meta.id),
            version: RulesetVersion(meta.version),
        },
        parameters: Parameters {
            player_count: PlayerCount(players),
            starting_player: PlayerPosition(parameters.starting_player),
        },
        zones: valid_zones,
        card_types: valid_card_types,
        deck_inputs: valid_inputs,
    })
}

fn validate_card_type(card_type: &CardTypeDraft) -> Result<CardType, DefinitionError> {
    let at = DefinitionPath::root().key("card_types").id(&card_type.id);

    let fields_path = at.clone().key("fields");
    let mut seen = Seen::new(fields_path.clone());
    let mut fields = Vec::with_capacity(card_type.fields.len());
    for (index, field) in card_type.fields.iter().enumerate() {
        check_id(&fields_path, index, "name", &field.name)?;
        seen.insert(&field.name, index)?;
        let field_at = at.clone().key("fields").id(&field.name);
        fields.push(FieldSchema {
            name: FieldName::new(&field.name),
            field_type: supported(field.field_type, field_at)?,
        });
    }

    let states_path = at.clone().key("instance_state");
    let mut seen = Seen::new(states_path.clone());
    let mut instance_state = Vec::with_capacity(card_type.instance_state.len());
    for (index, state) in card_type.instance_state.iter().enumerate() {
        check_id(&states_path, index, "name", &state.name)?;
        seen.insert(&state.name, index)?;
        let state_at = at.clone().key("instance_state").id(&state.name);
        instance_state.push(validate_state(state, &fields, state_at)?);
    }

    Ok(CardType {
        id: CardTypeId::new(&card_type.id),
        fields,
        instance_state,
        allows_effects: card_type.effects,
    })
}

fn validate_state(
    state: &InstanceStateDraft,
    fields: &[FieldSchema],
    at: DefinitionPath,
) -> Result<InstanceStateSchema, DefinitionError> {
    let declared = supported(state.state_type, at.clone())?;
    let init_at = at.key("init");
    let init = match &state.init {
        InitializerDraft::Literal(value) => {
            if value.field_type() != declared {
                return fail(
                    init_at,
                    DefinitionErrorKind::InitializerTypeMismatch {
                        declared,
                        found: value.field_type(),
                    },
                );
            }
            Initializer::Literal(value.clone())
        }
        InitializerDraft::FromField(name) => {
            let from_field_at = init_at.key("from_field");
            let Some(field) = fields.iter().find(|field| field.name.as_str() == name) else {
                return fail(
                    from_field_at,
                    DefinitionErrorKind::FromFieldMissing {
                        field: name.clone(),
                    },
                );
            };
            if field.field_type != declared {
                return fail(
                    from_field_at,
                    DefinitionErrorKind::FromFieldTypeMismatch {
                        field: name.clone(),
                        declared,
                        found: field.field_type,
                    },
                );
            }
            Initializer::FromField(FieldName::new(name))
        }
    };
    Ok(InstanceStateSchema {
        name: StateName::new(&state.name),
        state_type: declared,
        init,
    })
}

fn validate_input(
    input: &DeckInputDraft,
    zones: &[Zone],
    card_types: &[CardType],
) -> Result<DeckInput, DefinitionError> {
    let at = DefinitionPath::root().key("inputs").id(&input.id);

    let card_types_path = at.clone().key("card_types");
    if input.card_types.is_empty() {
        return fail(card_types_path, DefinitionErrorKind::NoCardTypes);
    }
    let mut seen = Seen::new(card_types_path.clone());
    let mut references = Vec::with_capacity(input.card_types.len());
    for (index, id) in input.card_types.iter().enumerate() {
        if !card_types
            .iter()
            .any(|card_type| card_type.id.as_str() == id)
        {
            return fail(
                card_types_path.index(index),
                DefinitionErrorKind::DanglingReference {
                    target: ReferenceTarget::CardType,
                    id: id.clone(),
                },
            );
        }
        seen.insert(id, index)?;
        references.push(CardTypeId::new(id));
    }

    if input.size == 0 {
        return fail(at.key("size"), DefinitionErrorKind::InvalidDeckSize);
    }
    if let Some(max_copies) = input.max_copies {
        if max_copies == 0 || max_copies > input.size {
            return fail(
                at.key("max_copies"),
                DefinitionErrorKind::InvalidMaxCopies {
                    max_copies,
                    size: input.size,
                },
            );
        }
    }

    if !zones.iter().any(|zone| zone.id.as_str() == input.zone) {
        return fail(
            at.key("zone"),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: input.zone.clone(),
            },
        );
    }

    Ok(DeckInput {
        id: DeckInputId::new(&input.id),
        card_types: references,
        size: input.size,
        max_copies: input.max_copies,
        zone: ZoneId::new(&input.zone),
    })
}
