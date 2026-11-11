//! Semantic validation of a [`RulesetDraft`], in the order documented on the module.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use super::draft::{
    ActionDraft, AutomaticStepDraft, CardTypeDraft, ConditionDraft, CostDraft, DeckInputDraft,
    EffectDraft, FieldTypeDraft, InitializerDraft, InstanceStateDraft, ParamAlternativeDraft,
    ParamCardsDraft, ParamDraft, PhaseDraft, RulesetDraft, SetupStepDraft, TurnDraft,
    WinConditionDraft, WinConditionWhenDraft,
};
use super::error::{DefinitionError, DefinitionErrorKind, ReferenceTarget};
use super::expression::{
    check_condition, fail, params_read, validate_card_ref, validate_value_ref, Refs, ScopedParam,
};
use super::model::{
    ActionDefinition, ActionId, AutomaticStep, CardType, CardTypeId, Condition, Cost, DeckInput,
    DeckInputId, Effect, Expression, FieldName, FieldSchema, FieldType, Initializer,
    InstanceStateSchema, Loser, Meta, Param, ParamAlternative, ParamName, ParamOwner, Parameters,
    Phase, PhaseId, PhaseKind, PlayerCount, PlayerPosition, PlayerSelector, Position,
    RulesetDefinition, RulesetId, RulesetVersion, SetupStep, StateName, TurnStructure,
    WinCondition, WinConditionId, WinConditionWhen, Zone, ZoneId,
};
use crate::definition_path::DefinitionPath;
use crate::id::is_valid_id;

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
        setup,
        actions,
        turn,
        win_conditions,
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

    let setup_path = DefinitionPath::root().key("setup");
    let mut valid_setup = Vec::with_capacity(setup.len());
    for (index, step) in setup.iter().enumerate() {
        valid_setup.push(validate_step(
            step,
            &valid_zones,
            setup_path.clone().index(index),
        )?);
    }

    let actions_path = DefinitionPath::root().key("actions");
    let mut seen = Seen::new(actions_path.clone());
    let mut valid_actions = Vec::with_capacity(actions.len());
    for (index, action) in actions.iter().enumerate() {
        check_id(&actions_path, index, "id", &action.id)?;
        seen.insert(&action.id, index)?;
        valid_actions.push(validate_action(action, &valid_zones, &valid_card_types)?);
    }

    let valid_turn = match &turn {
        Some(turn) => {
            let context = Context {
                zones: &valid_zones,
                card_types: &valid_card_types,
                actions: &valid_actions,
            };
            Some(validate_turn(turn, &context)?)
        }
        None => None,
    };

    let win_conditions_path = DefinitionPath::root().key("win_conditions");
    let mut seen = Seen::new(win_conditions_path.clone());
    let mut valid_win_conditions = Vec::with_capacity(win_conditions.len());
    for (index, condition) in win_conditions.iter().enumerate() {
        check_id(&win_conditions_path, index, "id", &condition.id)?;
        seen.insert(&condition.id, index)?;
        valid_win_conditions.push(validate_win_condition(
            condition,
            &valid_zones,
            &valid_card_types,
        )?);
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
        setup: valid_setup,
        actions: valid_actions,
        turn: valid_turn,
        win_conditions: valid_win_conditions,
    })
}

/// What the steps of a turn are validated against.
struct Context<'a> {
    zones: &'a [Zone],
    card_types: &'a [CardType],
    actions: &'a [ActionDefinition],
}

fn validate_win_condition(
    condition: &WinConditionDraft,
    zones: &[Zone],
    card_types: &[CardType],
) -> Result<WinCondition, DefinitionError> {
    let at = DefinitionPath::root()
        .key("win_conditions")
        .id(&condition.id);
    let when_at = at.clone().key("when");
    let when = match &condition.when {
        WinConditionWhenDraft::Expression(draft) => {
            // Every card type is a name a ref may start with, and takes only itself.
            let refs = Refs {
                zones,
                card_types,
                params: card_types
                    .iter()
                    .map(|card_type| ScopedParam {
                        name: ParamName::new(card_type.id.as_str()),
                        types: vec![card_type.id.clone()],
                    })
                    .collect(),
                missing: ReferenceTarget::CardType,
            };
            let condition = check_condition(draft, &refs, when_at.clone())?;
            let subject = match params_read(&condition).as_slice() {
                [subject] => CardTypeId::new(subject.as_str()),
                other => {
                    return fail(
                        when_at,
                        DefinitionErrorKind::InvalidSubjectCount { found: other.len() },
                    )
                }
            };
            WinConditionWhen::Expression { subject, condition }
        }
        WinConditionWhenDraft::DrawFromEmpty { zone } => WinConditionWhen::DrawFromEmpty(
            validate_zone_reference(zone, zones, when_at.key("draw_from_empty"))?,
        ),
    };
    let loser = match condition.loser.as_str() {
        "owner" => Loser::Owner,
        other => {
            return fail(
                at.key("loser"),
                DefinitionErrorKind::UnknownLoser {
                    loser: other.to_string(),
                },
            )
        }
    };
    Ok(WinCondition {
        id: WinConditionId::new(&condition.id),
        when,
        loser,
    })
}

/// An action: its params, then its cost, then its effects, each in declaration order.
fn validate_action(
    action: &ActionDraft,
    zones: &[Zone],
    card_types: &[CardType],
) -> Result<ActionDefinition, DefinitionError> {
    let at = DefinitionPath::root().key("actions").id(&action.id);

    let params_path = at.clone().key("params");
    let mut seen = Seen::new(params_path.clone());
    let mut params = Vec::with_capacity(action.params.len());
    // The params declared so far, which a `where` may read.
    let mut scope: Vec<ScopedParam> = Vec::with_capacity(action.params.len());
    for (index, param) in action.params.iter().enumerate() {
        check_id(&params_path, index, "name", &param.name)?;
        seen.insert(&param.name, index)?;
        let param = validate_param(
            param,
            zones,
            card_types,
            &scope,
            params_path.clone().id(&param.name),
        )?;
        scope.push(ScopedParam::of_alternatives(
            param.name.clone(),
            &param.alternatives,
        ));
        params.push(param);
    }

    let refs = Refs {
        zones,
        card_types,
        params: scope,
        missing: ReferenceTarget::Param,
    };

    let cost_path = at.clone().key("cost");
    let mut costs = Vec::with_capacity(action.cost.len());
    for (index, cost) in action.cost.iter().enumerate() {
        costs.push(validate_cost(cost, &refs, cost_path.clone().index(index))?);
    }

    let effects = validate_effects(&action.effects, &refs, at.key("effects"))?;

    Ok(ActionDefinition {
        id: ActionId::new(&action.id),
        params,
        costs,
        effects,
    })
}

/// What a param is validated against: the zones, the card types and the params before it.
struct ParamScope<'a> {
    zones: &'a [Zone],
    card_types: &'a [CardType],
    earlier: &'a [ScopedParam],
}

/// A param: each alternative in order, and in each its zone, its card type, its owner and its
/// `where`. `at` is the param; the alternatives of a `one_of` are at `one_of[i]` and the one
/// alternative of the simple form is the param itself.
fn validate_param(
    param: &ParamDraft,
    zones: &[Zone],
    card_types: &[CardType],
    earlier: &[ScopedParam],
    at: DefinitionPath,
) -> Result<Param, DefinitionError> {
    let name = ParamName::new(&param.name);
    let scope = ParamScope {
        zones,
        card_types,
        earlier,
    };
    let mut alternatives = Vec::new();
    match &param.cards {
        ParamCardsDraft::Single(alternative) => {
            alternatives.push(validate_alternative(alternative, &name, &scope, at)?);
        }
        ParamCardsDraft::OneOf(listed) => {
            let list = at.key("one_of");
            if listed.is_empty() {
                return fail(list, DefinitionErrorKind::NoAlternatives);
            }
            for (index, alternative) in listed.iter().enumerate() {
                alternatives.push(validate_alternative(
                    alternative,
                    &name,
                    &scope,
                    list.clone().index(index),
                )?);
            }
        }
    }
    Ok(Param { name, alternatives })
}

fn validate_alternative(
    alternative: &ParamAlternativeDraft,
    name: &ParamName,
    scope: &ParamScope<'_>,
    at: DefinitionPath,
) -> Result<ParamAlternative, DefinitionError> {
    let zone = validate_zone_reference(&alternative.zone, scope.zones, at.clone().key("zone"))?;
    if !scope
        .card_types
        .iter()
        .any(|card_type| card_type.id.as_str() == alternative.card_type)
    {
        return fail(
            at.key("card_type"),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::CardType,
                id: alternative.card_type.clone(),
            },
        );
    }
    let card_type = CardTypeId::new(&alternative.card_type);
    let owner = match alternative.owner.as_deref() {
        None => ParamOwner::Actor,
        Some("opponent") => ParamOwner::Opponent,
        Some(other) => {
            return fail(
                at.key("owner"),
                DefinitionErrorKind::UnknownOwner {
                    owner: other.to_string(),
                },
            )
        }
    };
    let condition = match &alternative.condition {
        None => None,
        Some(draft) => {
            // The `where` reads the params before this one and, typed as this alternative's card
            // type alone, the param itself.
            let mut params = scope.earlier.to_vec();
            params.push(ScopedParam {
                name: name.clone(),
                types: vec![card_type.clone()],
            });
            let refs = Refs {
                zones: scope.zones,
                card_types: scope.card_types,
                params,
                missing: ReferenceTarget::Param,
            };
            Some(check_condition(draft, &refs, at.key("where"))?)
        }
    };
    Ok(ParamAlternative {
        zone,
        card_type,
        owner,
        condition,
    })
}

fn validate_cost(
    cost: &CostDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<Cost, DefinitionError> {
    match cost {
        CostDraft::Tap { zone, count } => {
            let at = at.key("tap");
            let zone = validate_zone_reference(zone, refs.zones, at.clone().key("zone"))?;
            let count = validate_value_ref(count, FieldType::Int, refs, at.clone().key("count"))?;
            check_state_value(refs.card_types, "tapped", FieldType::Bool, (at.clone(), at))?;
            Ok(Cost::Tap { zone, count })
        }
    }
}

/// The effects of the list at `list`, in order.
fn validate_effects(
    effects: &[EffectDraft],
    refs: &Refs<'_>,
    list: DefinitionPath,
) -> Result<Vec<Effect>, DefinitionError> {
    effects
        .iter()
        .enumerate()
        .map(|(index, effect)| validate_effect(effect, refs, list.clone().index(index)))
        .collect()
}

fn validate_effect(
    effect: &EffectDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<Effect, DefinitionError> {
    match effect {
        EffectDraft::EndTurn => Ok(Effect::EndTurn),
        EffectDraft::Move { card, to } => {
            let at = at.key("move");
            let card = validate_card_ref(card, refs, at.clone().key("card"))?;
            let to = validate_zone_reference(to, refs.zones, at.key("to"))?;
            Ok(Effect::Move { card, to })
        }
        EffectDraft::SetState { card, state, value } => {
            let at = at.key("set_state");
            let card = validate_card_ref(card, refs, at.clone().key("card"))?;
            let declared = declared_states(refs, &card.param, state, at.clone().key("state"))?;
            if let Some(&declared) = declared.iter().find(|&&one| one != value.field_type()) {
                return fail(
                    at.key("value"),
                    DefinitionErrorKind::StateValueTypeMismatch {
                        state: state.clone(),
                        declared,
                        found: value.field_type(),
                    },
                );
            }
            Ok(Effect::SetState {
                card,
                state: StateName::new(state),
                value: value.clone(),
            })
        }
        EffectDraft::AddState {
            card,
            state,
            amount,
        } => {
            let at = at.key("add_state");
            let card = validate_card_ref(card, refs, at.clone().key("card"))?;
            let declared = declared_states(refs, &card.param, state, at.clone().key("state"))?;
            if let Some(&declared) = declared.iter().find(|&&one| one != FieldType::Int) {
                return fail(
                    at.key("amount"),
                    DefinitionErrorKind::StateValueTypeMismatch {
                        state: state.clone(),
                        declared,
                        found: FieldType::Int,
                    },
                );
            }
            Ok(Effect::AddState {
                card,
                state: StateName::new(state),
                amount: *amount,
            })
        }
        EffectDraft::If {
            condition,
            then,
            otherwise,
        } => {
            let condition = check_condition(condition, refs, at.clone().key("if"))?;
            // A condition that is exactly `is_card_type` narrows what the param may be in each
            // branch: the one type in `then`, the other types in `else` (all of them, if nothing
            // would be left).
            let (then_refs, else_refs) = match &condition {
                Expression::IsCardType { card, card_type } => {
                    let types = refs.types_of(&card.param);
                    let mut others: Vec<CardTypeId> = types
                        .iter()
                        .filter(|candidate| *candidate != card_type)
                        .cloned()
                        .collect();
                    if others.is_empty() {
                        others = types.to_vec();
                    }
                    (
                        refs.narrowed(&card.param, vec![card_type.clone()]),
                        refs.narrowed(&card.param, others),
                    )
                }
                _ => (refs.clone(), refs.clone()),
            };
            let then = validate_effects(then, &then_refs, at.clone().key("then"))?;
            let otherwise = validate_effects(otherwise, &else_refs, at.key("else"))?;
            Ok(Effect::If {
                condition,
                then,
                otherwise,
            })
        }
    }
}

/// The types that the card types the param `param` may take give the instance state `state`, one
/// for each, in order. Every possible card type must declare it: `at` is the path of a state that
/// one lacks.
fn declared_states(
    refs: &Refs<'_>,
    param: &ParamName,
    state: &str,
    at: DefinitionPath,
) -> Result<Vec<FieldType>, DefinitionError> {
    let mut declared = Vec::new();
    for id in refs.types_of(param) {
        let schema = refs
            .card_types
            .iter()
            .find(|card_type| &card_type.id == id)
            .and_then(|card_type| {
                card_type
                    .instance_state
                    .iter()
                    .find(|schema| schema.name.as_str() == state)
            });
        let Some(schema) = schema else {
            return fail(
                at,
                DefinitionErrorKind::DanglingReference {
                    target: ReferenceTarget::InstanceState,
                    id: state.to_string(),
                },
            );
        };
        declared.push(schema.state_type);
    }
    Ok(declared)
}

fn validate_turn(
    turn: &TurnDraft,
    context: &Context<'_>,
) -> Result<TurnStructure, DefinitionError> {
    let phases_path = DefinitionPath::root().key("turn").key("phases");
    let mut seen = Seen::new(phases_path.clone());
    let mut phases = Vec::with_capacity(turn.phases.len());
    let mut action_phases = 0usize;
    for (index, phase) in turn.phases.iter().enumerate() {
        check_id(&phases_path, index, "id", &phase.id)?;
        seen.insert(&phase.id, index)?;
        let at = phases_path.clone().id(&phase.id);
        let kind = validate_phase_kind(phase, context, at)?;
        if matches!(kind, PhaseKind::Actions(_)) {
            action_phases += 1;
        }
        phases.push(Phase {
            id: PhaseId::new(&phase.id),
            kind,
        });
    }
    if action_phases != 1 {
        return fail(
            phases_path,
            DefinitionErrorKind::InvalidActionPhaseCount {
                found: action_phases,
            },
        );
    }
    Ok(TurnStructure { phases })
}

fn validate_phase_kind(
    phase: &PhaseDraft,
    context: &Context<'_>,
    at: DefinitionPath,
) -> Result<PhaseKind, DefinitionError> {
    match (&phase.automatic, &phase.actions) {
        (Some(steps), None) => Ok(PhaseKind::Automatic(validate_steps(
            steps,
            context,
            at.key("automatic"),
        )?)),
        (None, Some(actions)) => {
            let list = at.key("actions");
            let mut seen = Seen::new(list.clone());
            let mut valid = Vec::with_capacity(actions.len());
            for (index, id) in actions.iter().enumerate() {
                if !context
                    .actions
                    .iter()
                    .any(|action| action.id.as_str() == id)
                {
                    return fail(
                        list.index(index),
                        DefinitionErrorKind::DanglingReference {
                            target: ReferenceTarget::Action,
                            id: id.clone(),
                        },
                    );
                }
                seen.insert(id, index)?;
                valid.push(ActionId::new(id));
            }
            Ok(PhaseKind::Actions(valid))
        }
        _ => fail(at, DefinitionErrorKind::InvalidPhaseKind),
    }
}

/// The steps of the list at `list`, in order.
fn validate_steps(
    steps: &[AutomaticStepDraft],
    context: &Context<'_>,
    list: DefinitionPath,
) -> Result<Vec<AutomaticStep>, DefinitionError> {
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| validate_automatic_step(step, context, list.clone().index(index)))
        .collect()
}

fn validate_automatic_step(
    step: &AutomaticStepDraft,
    context: &Context<'_>,
    at: DefinitionPath,
) -> Result<AutomaticStep, DefinitionError> {
    match step {
        AutomaticStepDraft::Untap { player, zones } => {
            let at = at.key("untap");
            let player = validate_selector(player, at.clone().key("player"))?;
            let zones_path = at.clone().key("zones");
            let mut seen = Seen::new(zones_path.clone());
            let mut valid = Vec::with_capacity(zones.len());
            for (index, id) in zones.iter().enumerate() {
                valid.push(validate_zone_reference(
                    id,
                    context.zones,
                    zones_path.clone().index(index),
                )?);
                seen.insert(id, index)?;
            }
            check_state_value(
                context.card_types,
                "tapped",
                FieldType::Bool,
                (at.clone(), at),
            )?;
            Ok(AutomaticStep::Untap {
                player,
                zones: valid,
            })
        }
        AutomaticStepDraft::SetState {
            player,
            zone,
            state,
            value,
        } => {
            let at = at.key("set_state");
            let player = validate_selector(player, at.clone().key("player"))?;
            let zone = validate_zone_reference(zone, context.zones, at.clone().key("zone"))?;
            check_state_value(
                context.card_types,
                state,
                value.field_type(),
                (at.clone().key("state"), at.key("value")),
            )?;
            Ok(AutomaticStep::SetState {
                player,
                zone,
                state: StateName::new(state),
                value: value.clone(),
            })
        }
        AutomaticStepDraft::Draw {
            player,
            from,
            to,
            count,
        } => {
            let at = at.key("draw");
            let player = validate_selector(player, at.clone().key("player"))?;
            let from = validate_ordered_zone(from, context.zones, at.clone().key("from"))?;
            let to = validate_zone_reference(to, context.zones, at.clone().key("to"))?;
            let count = validate_count(*count, at.key("count"))?;
            Ok(AutomaticStep::Draw {
                player,
                from,
                to,
                count,
            })
        }
        AutomaticStepDraft::Move {
            player,
            from,
            to,
            count,
            position,
        } => {
            let at = at.key("move");
            let player = validate_selector(player, at.clone().key("player"))?;
            let from = validate_ordered_zone(from, context.zones, at.clone().key("from"))?;
            let to = validate_zone_reference(to, context.zones, at.clone().key("to"))?;
            let count = validate_count(*count, at.clone().key("count"))?;
            let position = match position.as_str() {
                "top" => Position::Top,
                other => {
                    return fail(
                        at.key("position"),
                        DefinitionErrorKind::UnknownPosition {
                            position: other.to_string(),
                        },
                    )
                }
            };
            Ok(AutomaticStep::Move {
                player,
                from,
                to,
                count,
                position,
            })
        }
        AutomaticStepDraft::If {
            condition,
            then,
            otherwise,
        } => {
            let condition = match condition {
                ConditionDraft::FirstTurn => Condition::FirstTurn,
            };
            let then = validate_steps(then, context, at.clone().key("then"))?;
            let otherwise = validate_steps(otherwise, context, at.key("else"))?;
            Ok(AutomaticStep::If {
                condition,
                then,
                otherwise,
            })
        }
    }
}

/// A count of at least 1.
fn validate_count(count: u32, at: DefinitionPath) -> Result<NonZeroU32, DefinitionError> {
    match NonZeroU32::new(count) {
        Some(count) => Ok(count),
        None => fail(at, DefinitionErrorKind::InvalidCount),
    }
}

/// Checks that some card type declares the instance state `state`, and that every card type that
/// does declares it with the type of `found`. `at` holds the path of a missing state and of a
/// mismatch.
fn check_state_value(
    card_types: &[CardType],
    state: &str,
    found: FieldType,
    at: (DefinitionPath, DefinitionPath),
) -> Result<(), DefinitionError> {
    let (missing_at, mismatch_at) = at;
    let mut declared = card_types
        .iter()
        .flat_map(|card_type| card_type.instance_state.iter())
        .filter(|schema| schema.name.as_str() == state)
        .peekable();
    if declared.peek().is_none() {
        return fail(
            missing_at,
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::InstanceState,
                id: state.to_string(),
            },
        );
    }
    for schema in declared {
        if schema.state_type != found {
            return fail(
                mismatch_at,
                DefinitionErrorKind::StateValueTypeMismatch {
                    state: state.to_string(),
                    declared: schema.state_type,
                    found,
                },
            );
        }
    }
    Ok(())
}

/// A zone reference that cards are taken from (a shuffle, a draw or a move source): the zone must
/// exist and be ordered (ADR-0013).
fn validate_ordered_zone(
    id: &str,
    zones: &[Zone],
    at: DefinitionPath,
) -> Result<ZoneId, DefinitionError> {
    let zone = validate_zone_reference(id, zones, at.clone())?;
    let ordered = zones
        .iter()
        .find(|declared| declared.id == zone)
        .is_some_and(|declared| declared.ordered);
    if ordered {
        Ok(zone)
    } else {
        fail(
            at,
            DefinitionErrorKind::ZoneNotOrdered {
                zone: id.to_string(),
            },
        )
    }
}

fn validate_selector(
    selector: &str,
    at: DefinitionPath,
) -> Result<PlayerSelector, DefinitionError> {
    match selector {
        "each" => Ok(PlayerSelector::Each),
        "active" => Ok(PlayerSelector::Active),
        other => fail(
            at,
            DefinitionErrorKind::UnknownPlayerSelector {
                selector: other.to_string(),
            },
        ),
    }
}

/// A zone reference of a step. A reference that breaks the identifier grammar fails here too,
/// because no declared zone has such an id.
fn validate_zone_reference(
    id: &str,
    zones: &[Zone],
    at: DefinitionPath,
) -> Result<ZoneId, DefinitionError> {
    if zones.iter().any(|zone| zone.id.as_str() == id) {
        Ok(ZoneId::new(id))
    } else {
        fail(
            at,
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: id.to_string(),
            },
        )
    }
}

fn validate_step(
    step: &SetupStepDraft,
    zones: &[Zone],
    at: DefinitionPath,
) -> Result<SetupStep, DefinitionError> {
    match step {
        SetupStepDraft::Shuffle { player, zone } => {
            let at = at.key("shuffle");
            let player = validate_selector(player, at.clone().key("player"))?;
            let zone = validate_ordered_zone(zone, zones, at.key("zone"))?;
            Ok(SetupStep::Shuffle { player, zone })
        }
        SetupStepDraft::Draw {
            player,
            from,
            to,
            count,
        } => {
            let at = at.key("draw");
            let player = validate_selector(player, at.clone().key("player"))?;
            let from = validate_ordered_zone(from, zones, at.clone().key("from"))?;
            let to = validate_zone_reference(to, zones, at.clone().key("to"))?;
            let count = validate_count(*count, at.key("count"))?;
            Ok(SetupStep::Draw {
                player,
                from,
                to,
                count,
            })
        }
    }
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
