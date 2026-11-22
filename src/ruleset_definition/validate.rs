//! Semantic validation of a [`RulesetDraft`], in the order documented on the module.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use super::draft::{
    ActionDraft, AutomaticStepDraft, CardTypeDraft, ConditionDraft, CostDraft, DeckInputDraft,
    EffectDraft, ExpressionDraft, FieldTypeDraft, InitializerDraft, InstanceStateDraft,
    ParamAlternativeDraft, ParamCardsDraft, ParamDraft, PhaseDraft, RulesetDraft, SetupStepDraft,
    TurnDraft, WinConditionDraft, WinConditionWhenDraft, WindowDraft,
};
use super::error::{DefinitionError, DefinitionErrorKind, ReferenceTarget};
use super::expression::{
    check_condition, fail, params_read, validate_card_ref, validate_value_ref, Refs, ScopedParam,
};
use super::model::{
    ActionDefinition, ActionId, AutomaticStep, CardType, CardTypeId, Condition, Cost, Decider,
    DeckInput, DeckInputId, Effect, EffectPlayer, Expression, FieldName, FieldSchema, FieldType,
    Initializer, InstanceStateSchema, Loser, Meta, Param, ParamAlternative, ParamName, ParamOwner,
    Parameters, Phase, PhaseId, PhaseKind, PlayerCount, PlayerPosition, PlayerSelector, Position,
    RulesetDefinition, RulesetId, RulesetVersion, SetupStep, StateName, TurnStructure, Visibility,
    WinCondition, WinConditionId, WinConditionWhen, Window, WindowId, Zone, ZoneId,
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
        windows,
        turn,
        win_conditions,
        effects,
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

    let valid_windows = validate_windows(
        &windows,
        &actions,
        &valid_actions,
        &valid_zones,
        players.get(),
    )?;

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

    // Every action is listed by a phase or by a window's `actions`; without a `turn` no phase lists
    // anything (ADR-0027, ADR-0037). Being in a window's `opened_by` lists nothing.
    for (index, action) in valid_actions.iter().enumerate() {
        let listed = valid_turn.as_ref().is_some_and(|turn| {
            turn.phases.iter().any(|phase| match &phase.kind {
                PhaseKind::Actions(ids) => ids.contains(&action.id),
                PhaseKind::Automatic(_) => false,
            })
        }) || valid_windows
            .iter()
            .any(|window| window.actions.contains(&action.id));
        if !listed {
            return fail(
                actions_path.clone().id(&actions[index].id),
                DefinitionErrorKind::UnlistedAction,
            );
        }
    }

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

    let effects_active_zone = effects
        .as_ref()
        .map(|effects| {
            validate_zone_reference(
                &effects.active_zone,
                &valid_zones,
                DefinitionPath::root().key("effects").key("active_zone"),
            )
        })
        .transpose()?;

    // A window that allows abilities in a ruleset where no Card Effect is ever active could never
    // do anything (ADR-0038).
    if let Some(window) = valid_windows.iter().find(|window| window.abilities) {
        let at = DefinitionPath::root()
            .key("windows")
            .id(window.id.as_str())
            .key("abilities");
        let Some(active) = &effects_active_zone else {
            return fail(at, DefinitionErrorKind::AbilitiesWithoutActiveZone);
        };
        // The card of an ability is bound in the Active Zone and the Stack is public (ADR-0038).
        let public = valid_zones
            .iter()
            .any(|zone| &zone.id == active && zone.visibility == Visibility::Public);
        if !public {
            return fail(
                at,
                DefinitionErrorKind::AbilitiesActiveZoneNotPublic {
                    zone: active.as_str().to_string(),
                },
            );
        }
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
        windows: valid_windows,
        turn: valid_turn,
        win_conditions: valid_win_conditions,
        effects_active_zone,
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
                has_self: false,
                choices: false,
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
        has_self: false,
        choices: false,
    };

    let cost_path = at.clone().key("cost");
    let mut costs = Vec::with_capacity(action.cost.len());
    for (index, cost) in action.cost.iter().enumerate() {
        costs.push(validate_cost(cost, &refs, cost_path.clone().index(index))?);
    }

    let effects_path = at.key("effects");
    let effects = validate_effects(&action.effects, &refs, effects_path.clone())?;
    // After all the effects were checked, as for a Card Effect (ADR-0022).
    check_end_turn_placement(&effects, &effects_path)?;

    Ok(ActionDefinition {
        id: ActionId::new(&action.id),
        params,
        costs,
        effects,
    })
}

/// `end_turn` is only the last top-level effect of an action (ADR-0027): the first one, in document
/// order (depth first, `then` before `else`), that is inside an `if` or followed by another effect
/// is the error, at its own path. `list` is the path of `effects`.
fn check_end_turn_placement(
    effects: &[Effect],
    list: &DefinitionPath,
) -> Result<(), DefinitionError> {
    for (index, effect) in effects.iter().enumerate() {
        let at = list.clone().index(index);
        match effect {
            Effect::EndTurn if index + 1 != effects.len() => {
                return fail(at, DefinitionErrorKind::EndTurnNotLast)
            }
            Effect::If {
                then, otherwise, ..
            } => {
                forbid_end_turn_in_if(then, &at.clone().key("then"))?;
                forbid_end_turn_in_if(otherwise, &at.key("else"))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Rejects an `end_turn` anywhere in the effects of a branch of an `if`, at any depth.
fn forbid_end_turn_in_if(effects: &[Effect], list: &DefinitionPath) -> Result<(), DefinitionError> {
    for (index, effect) in effects.iter().enumerate() {
        let at = list.clone().index(index);
        match effect {
            Effect::EndTurn => return fail(at, DefinitionErrorKind::EndTurnInIfEffect),
            Effect::If {
                then, otherwise, ..
            } => {
                forbid_end_turn_in_if(then, &at.clone().key("then"))?;
                forbid_end_turn_in_if(otherwise, &at.key("else"))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Whether the action ends the Turn: its effects end with a top-level `end_turn`. Only meaningful
/// once `check_end_turn_placement` passed, when that is the only place an `end_turn` can be.
fn ends_the_turn(action: &ActionDefinition) -> bool {
    action.effects.last() == Some(&Effect::EndTurn)
}

/// The scope of a Card Effect (ADR-0022): the one name `self`, the carrying card, typed as the
/// card's own card type. The seam through which the Card Catalog checks the conditions and the
/// effects of a Card Effect with the machinery of ADR-0015 and ADR-0016.
pub(crate) struct SelfScope<'a> {
    refs: Refs<'a>,
    /// The number of Players of the ruleset: `decider: opponent` is defined for exactly 2.
    players: u32,
}

impl<'a> SelfScope<'a> {
    pub(crate) fn new(definition: &'a RulesetDefinition, card_type: &CardTypeId) -> Self {
        Self::scoped(definition, card_type, true)
    }

    /// The scope of an activated ability (ADR-0038): `self` as in a triggered Card Effect, but no
    /// Choice may stand in its `cost` or its `do`.
    pub(crate) fn for_ability(definition: &'a RulesetDefinition, card_type: &CardTypeId) -> Self {
        Self::scoped(definition, card_type, false)
    }

    fn scoped(definition: &'a RulesetDefinition, card_type: &CardTypeId, choices: bool) -> Self {
        Self {
            players: definition.parameters().player_count().get(),
            refs: Refs {
                zones: definition.zones(),
                card_types: definition.card_types(),
                params: vec![ScopedParam {
                    name: ParamName::new("self"),
                    types: vec![card_type.clone()],
                }],
                missing: ReferenceTarget::Param,
                has_self: true,
                choices,
            },
        }
    }

    /// The `cost` of an activated ability: the action cost steps, each at its position under `at`,
    /// with only `self` in scope. The Stack is public, so the zone `self` is in once the costs are
    /// paid must be: the `to` of the last `move: self` cost must be `public` (`PushedCardNotPublic`
    /// at `cost[k].move.to`, ADR-0038). Without a `move`, the Active Zone, which the window
    /// declaration checks.
    pub(crate) fn check_costs(
        &self,
        costs: &[CostDraft],
        at: DefinitionPath,
    ) -> Result<Vec<Cost>, DefinitionError> {
        let checked = costs
            .iter()
            .enumerate()
            .map(|(index, cost)| validate_cost(cost, &self.refs, at.clone().index(index)))
            .collect::<Result<Vec<_>, _>>()?;
        let last_move = checked
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, cost)| match cost {
                Cost::Move { card, to } if card.param.as_str() == "self" => Some((index, to)),
                _ => None,
            });
        if let Some((index, to)) = last_move {
            let public = self
                .refs
                .zones
                .iter()
                .any(|zone| &zone.id == to && zone.visibility == Visibility::Public);
            if !public {
                return fail(
                    at.index(index).key("move").key("to"),
                    DefinitionErrorKind::PushedCardNotPublic {
                        param: "self".to_string(),
                        zone: to.as_str().to_string(),
                    },
                );
            }
        }
        Ok(checked)
    }

    /// An `if` or a `while`: an expression that must be bool. `at` is its path.
    pub(crate) fn check_condition(
        &self,
        draft: &ExpressionDraft,
        at: DefinitionPath,
    ) -> Result<Expression, DefinitionError> {
        check_condition(draft, &self.refs, at)
    }

    /// A `do`: Action Effects, each at its position under `at`.
    pub(crate) fn check_effects(
        &self,
        effects: &[EffectDraft],
        at: DefinitionPath,
    ) -> Result<Vec<Effect>, DefinitionError> {
        let checked = validate_effects(effects, &self.refs, at.clone())?;
        if self.players != 2 {
            if let Some(decider) = first_opponent_decider(&checked, &at) {
                return fail(
                    decider,
                    DefinitionErrorKind::OpponentDeciderNeedsTwoPlayers {
                        players: u64::from(self.players),
                    },
                );
            }
        }
        Ok(checked)
    }
}

/// The path of the `decider` of the first Choice with `decider: opponent` in `effects`, in document
/// order (a Choice before its options, `then` before `else`). `list` is the path of `effects`.
fn first_opponent_decider(effects: &[Effect], list: &DefinitionPath) -> Option<DefinitionPath> {
    for (index, effect) in effects.iter().enumerate() {
        let at = list.clone().index(index);
        match effect {
            Effect::If {
                then, otherwise, ..
            } => {
                if let Some(found) = first_opponent_decider(then, &at.clone().key("then")) {
                    return Some(found);
                }
                if let Some(found) = first_opponent_decider(otherwise, &at.key("else")) {
                    return Some(found);
                }
            }
            Effect::ChooseOne { decider, options } => {
                let choice = at.key("choose_one");
                if *decider == Decider::Opponent {
                    return Some(choice.key("decider"));
                }
                let lists = choice.key("options");
                for (position, option) in options.iter().enumerate() {
                    if let Some(found) =
                        first_opponent_decider(option, &lists.clone().index(position))
                    {
                        return Some(found);
                    }
                }
            }
            Effect::ChooseCard { decider, .. } => {
                if *decider == Decider::Opponent {
                    return Some(at.key("choose_card").key("decider"));
                }
            }
            Effect::EndTurn
            | Effect::Move { .. }
            | Effect::SetState { .. }
            | Effect::AddState { .. }
            | Effect::Draw { .. } => {}
        }
    }
    None
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
                has_self: false,
                choices: false,
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
        CostDraft::TapCard { card } => {
            let at = at.key("tap").key("card");
            let card = validate_card_ref(card, refs, at.clone())?;
            // Every card type the param may take must declare a bool `tapped`, so the cost can
            // always read it and set it.
            for id in refs.types_of(&card.param) {
                let declares = refs
                    .card_types
                    .iter()
                    .find(|card_type| &card_type.id == id)
                    .is_some_and(|card_type| {
                        card_type.instance_state.iter().any(|schema| {
                            schema.name.as_str() == "tapped" && schema.state_type == FieldType::Bool
                        })
                    });
                if !declares {
                    return fail(
                        at,
                        DefinitionErrorKind::TapCardWithoutTappedState {
                            card_type: id.as_str().to_string(),
                        },
                    );
                }
            }
            Ok(Cost::TapCard { card })
        }
        CostDraft::Move { card, to } => {
            let at = at.key("move");
            let card = validate_card_ref(card, refs, at.clone().key("card"))?;
            let to = validate_zone_reference(to, refs.zones, at.key("to"))?;
            Ok(Cost::Move { card, to })
        }
    }
}

/// The Response Windows (ADR-0037), after the actions they name: with exactly 2 players; per window
/// in order its id (grammar, duplicate), its `opened_by` (each action declared, and opening no other
/// window), then its `actions` (each declared, none repeated); then the checks of the pushed
/// actions, in declaration order: their card params, then their effects.
fn validate_windows(
    windows: &[WindowDraft],
    action_drafts: &[ActionDraft],
    actions: &[ActionDefinition],
    zones: &[Zone],
    players: u32,
) -> Result<Vec<Window>, DefinitionError> {
    let list = DefinitionPath::root().key("windows");
    if !windows.is_empty() && players != 2 {
        return fail(
            list,
            DefinitionErrorKind::WindowsNeedTwoPlayers {
                players: u64::from(players),
            },
        );
    }
    let declared = |id: &str| actions.iter().any(|action| action.id.as_str() == id);
    let dangling = |at: DefinitionPath, id: &str| {
        fail(
            at,
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Action,
                id: id.to_string(),
            },
        )
    };
    let mut seen = Seen::new(list.clone());
    let mut openers: BTreeMap<&str, DefinitionPath> = BTreeMap::new();
    let mut valid = Vec::with_capacity(windows.len());
    for (index, window) in windows.iter().enumerate() {
        check_id(&list, index, "id", &window.id)?;
        seen.insert(&window.id, index)?;
        let at = list.clone().id(&window.id);
        let opened_at = at.clone().key("opened_by");
        for (position, id) in window.opened_by.iter().enumerate() {
            let here = opened_at.clone().index(position);
            if !declared(id) {
                return dangling(here, id);
            }
            if let Some(first) = openers.get(id.as_str()) {
                return fail(
                    here,
                    DefinitionErrorKind::DuplicateWindowOpener {
                        action: id.clone(),
                        first: first.clone(),
                    },
                );
            }
            openers.insert(id, here);
        }
        let actions_at = at.key("actions");
        let mut listed = Seen::new(actions_at.clone());
        for (position, id) in window.actions.iter().enumerate() {
            if !declared(id) {
                return dangling(actions_at.clone().index(position), id);
            }
            listed.insert(id, position)?;
        }
        valid.push(Window {
            id: WindowId::new(&window.id),
            opened_by: window.opened_by.iter().map(ActionId::new).collect(),
            actions: window.actions.iter().map(ActionId::new).collect(),
            abilities: window.abilities,
        });
    }

    let pushed = |id: &ActionId| {
        valid
            .iter()
            .any(|window| window.opened_by.contains(id) || window.actions.contains(id))
    };
    for (action, draft) in actions.iter().zip(action_drafts) {
        if !pushed(&action.id) {
            continue;
        }
        check_pushed_params(action, draft, zones)?;
        if let Some(position) = action
            .effects
            .iter()
            .position(|effect| *effect == Effect::EndTurn)
        {
            return fail(
                DefinitionPath::root()
                    .key("actions")
                    .id(action.id.as_str())
                    .key("effects")
                    .index(position),
                DefinitionErrorKind::EndTurnInPushedAction,
            );
        }
    }
    Ok(valid)
}

/// The card params of a pushed action must be public once the costs are paid, because the Stack
/// shows the card in the zone it is in then (ADR-0037): the zone of a param is the `to` of its last
/// `move` cost if it has one (a single zone, reported at the param), else the zone of each
/// alternative (reported at the alternative). That zone must be `public`.
fn check_pushed_params(
    action: &ActionDefinition,
    draft: &ActionDraft,
    zones: &[Zone],
) -> Result<(), DefinitionError> {
    let is_public = |zone: &ZoneId| {
        zones
            .iter()
            .any(|declared| &declared.id == zone && declared.visibility == Visibility::Public)
    };
    for (param, param_draft) in action.params.iter().zip(&draft.params) {
        let at = DefinitionPath::root()
            .key("actions")
            .id(action.id.as_str())
            .key("params")
            .id(param.name.as_str());
        let not_public = |at: DefinitionPath, zone: &ZoneId| {
            fail(
                at,
                DefinitionErrorKind::PushedCardNotPublic {
                    param: param.name.as_str().to_string(),
                    zone: zone.as_str().to_string(),
                },
            )
        };
        let last_move = action.costs.iter().rev().find_map(|cost| match cost {
            Cost::Move { card, to } if card.param == param.name => Some(to),
            _ => None,
        });
        if let Some(to) = last_move {
            if !is_public(to) {
                return not_public(at, to);
            }
            continue;
        }
        for (index, alternative) in param.alternatives.iter().enumerate() {
            if is_public(&alternative.zone) {
                continue;
            }
            let at = match &param_draft.cards {
                ParamCardsDraft::Single(_) => at.clone(),
                ParamCardsDraft::OneOf(_) => at.clone().key("one_of").index(index),
            };
            return not_public(at, &alternative.zone);
        }
    }
    Ok(())
}

/// The effects of the list at `list`, in order. A Card Choice binds its name for the effects after
/// it in the list, and so for everything nested in them: the scope of each effect is the scope of
/// the list plus the names the Choices before it bound (ADR-0034). The names do not leave the
/// list.
fn validate_effects<'a>(
    effects: &[EffectDraft],
    refs: &Refs<'a>,
    list: DefinitionPath,
) -> Result<Vec<Effect>, DefinitionError> {
    let mut checked = Vec::with_capacity(effects.len());
    let mut widened: Option<Refs<'a>> = None;
    for (index, effect) in effects.iter().enumerate() {
        let scope = widened.as_ref().unwrap_or(refs);
        let effect = validate_effect(effect, scope, list.clone().index(index))?;
        if let Effect::ChooseCard { param, .. } = &effect {
            let mut next = scope.clone();
            next.params.push(ScopedParam::of_alternatives(
                param.name.clone(),
                &param.alternatives,
            ));
            widened = Some(next);
        }
        checked.push(effect);
    }
    Ok(checked)
}

/// Whether `decider` sees every card of `zone`, among those of the Players an alternative with
/// this `owner` takes from: the zone is `public`, or `owner` and it is the decider's own.
fn decider_sees(visibility: Visibility, owner: ParamOwner, decider: Decider) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Owner => matches!(
            (owner, decider),
            (ParamOwner::Actor, Decider::Controller) | (ParamOwner::Opponent, Decider::Opponent)
        ),
        Visibility::Hidden => false,
    }
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
        EffectDraft::Draw {
            player,
            from,
            to,
            count,
        } => {
            let at = at.key("draw");
            let player = match player.as_str() {
                "controller" if refs.has_self => EffectPlayer::Controller,
                "controller" => {
                    return fail(
                        at.key("player"),
                        DefinitionErrorKind::ControllerOutsideCardEffect,
                    )
                }
                other => {
                    return fail(
                        at.key("player"),
                        DefinitionErrorKind::UnknownEffectPlayer {
                            player: other.to_string(),
                        },
                    )
                }
            };
            let from = validate_ordered_zone(from, refs.zones, at.clone().key("from"))?;
            let to = validate_zone_reference(to, refs.zones, at.clone().key("to"))?;
            let count = validate_count(*count, at.key("count"))?;
            Ok(Effect::Draw {
                player,
                from,
                to,
                count,
            })
        }
        EffectDraft::If {
            condition,
            then,
            otherwise,
        } => {
            let condition = check_condition(condition, refs, at.clone().key("if"))?;
            // A condition that is exactly `is_card_type` narrows what the param may be in each
            // branch: the param's types that are the tested one in `then` (none is an error), the
            // other types in `else` (all of them, if nothing would be left). ADR-0032.
            let (then_refs, else_refs) = match &condition {
                Expression::IsCardType { card, card_type } => {
                    let types = refs.types_of(&card.param);
                    let then_refs = refs.narrowed(&card.param, std::slice::from_ref(card_type));
                    if then_refs.types_of(&card.param).is_empty() {
                        return fail(
                            at.key("if").key("is_card_type"),
                            DefinitionErrorKind::ImpossibleCardType {
                                param: card.param.as_str().to_string(),
                                card_type: card_type.as_str().to_string(),
                            },
                        );
                    }
                    let mut others: Vec<CardTypeId> = types
                        .iter()
                        .filter(|candidate| *candidate != card_type)
                        .cloned()
                        .collect();
                    if others.is_empty() {
                        others = types.to_vec();
                    }
                    (then_refs, refs.narrowed(&card.param, &others))
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
        EffectDraft::ChooseOne { decider, options } => {
            let at = at.key("choose_one");
            // Only a triggered Card Effect can pause for a decision (ADR-0033); an activated
            // one has `self` but no Choice (ADR-0038).
            if !refs.choices {
                return fail(at, DefinitionErrorKind::ChoiceOutsideTriggeredEffect);
            }
            let decider = match decider.as_str() {
                "controller" => Decider::Controller,
                "opponent" => Decider::Opponent,
                other => {
                    return fail(
                        at.key("decider"),
                        DefinitionErrorKind::UnknownDecider {
                            decider: other.to_string(),
                        },
                    )
                }
            };
            let list = at.key("options");
            if options.len() < 2 {
                return fail(
                    list,
                    DefinitionErrorKind::TooFewOptions {
                        found: u64::try_from(options.len()).unwrap_or(u64::MAX),
                    },
                );
            }
            let options = options
                .iter()
                .enumerate()
                .map(|(index, option)| validate_effects(option, refs, list.clone().index(index)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Effect::ChooseOne { decider, options })
        }
        EffectDraft::ChooseCard { decider, param } => {
            let at = at.key("choose_card");
            // Only a triggered Card Effect can pause for a decision (ADR-0033); an activated
            // one has `self` but no Choice (ADR-0038).
            if !refs.choices {
                return fail(at, DefinitionErrorKind::ChoiceOutsideTriggeredEffect);
            }
            let name_at = at.clone().key("name");
            if !is_valid_id(&param.name) {
                return fail(
                    name_at,
                    DefinitionErrorKind::InvalidId {
                        id: param.name.clone(),
                    },
                );
            }
            if refs
                .params
                .iter()
                .any(|scoped| scoped.name.as_str() == param.name)
            {
                return fail(
                    name_at,
                    DefinitionErrorKind::DuplicateChoiceName {
                        name: param.name.clone(),
                    },
                );
            }
            let decider = match decider.as_str() {
                "controller" => Decider::Controller,
                "opponent" => Decider::Opponent,
                other => {
                    return fail(
                        at.key("decider"),
                        DefinitionErrorKind::UnknownDecider {
                            decider: other.to_string(),
                        },
                    )
                }
            };
            // The candidates are those of an Action Param, so it is validated as one: its `where`
            // reads `self`, the names before it and its own candidate.
            let validated =
                validate_param(param, refs.zones, refs.card_types, &refs.params, at.clone())?;
            // The decider must see every candidate, alternative by alternative in order: choosing
            // from a zone it cannot see would be a peek (ADR-0034).
            for (index, alternative) in validated.alternatives.iter().enumerate() {
                let visibility = refs
                    .zones
                    .iter()
                    .find(|zone| zone.id == alternative.zone)
                    .map(|zone| zone.visibility);
                if visibility.is_some_and(|seen| !decider_sees(seen, alternative.owner, decider)) {
                    let alternative_at = match &param.cards {
                        ParamCardsDraft::Single(_) => at.clone(),
                        ParamCardsDraft::OneOf(_) => at.clone().key("one_of").index(index),
                    };
                    return fail(
                        alternative_at.key("zone"),
                        DefinitionErrorKind::DeciderCannotSeeZone {
                            zone: alternative.zone.as_str().to_string(),
                        },
                    );
                }
            }
            Ok(Effect::ChooseCard {
                decider,
                param: validated,
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
            let list = at.clone().key("actions");
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
            // The Game waits here until some action ends the Turn (ADR-0027).
            let can_leave = context
                .actions
                .iter()
                .any(|action| valid.contains(&action.id) && ends_the_turn(action));
            if !can_leave {
                return fail(at, DefinitionErrorKind::NoTurnEndingAction);
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
