//! The typed Ruleset Definition: a ruleset's meta, parameters, zones, card types, deck inputs,
//! setup, actions and turn, and their semantic validation.
//!
//! A frontend (a YAML loader, code, a script) fills a plain-data [`RulesetDraft`] and calls
//! [`RulesetDefinition::validate`]. Every semantic check lives here, so every frontend gets
//! identical errors. This is stage 1 of ADR-0004's two-stage loading; the card catalog stage
//! validates against the resulting [`RulesetDefinition`].
//!
//! Validation reports the first error only, in a fixed order: `meta.id`, then `parameters.players`,
//! then `parameters.starting_player`, then zones, card types and inputs, each list in declaration
//! order. Within an element its id is checked against the identifier grammar and then for
//! duplicates against earlier elements, before its content, in key order. An input's content is
//! its card types (non-empty, known, without repeats), then `size`, then `max_copies`, then its
//! zone. Setup steps follow, in declaration order, then the actions (each: id grammar, duplicate,
//! then its params, its cost and its effects, in that order, ADR-0015), then the turn: per phase,
//! in declaration order, its id (grammar, duplicate), its kind (exactly one of `automatic` and
//! `actions`) and its content, and after all the phases that exactly one has actions. Phases are
//! addressed by id (`turn.phases[main].actions[0]`) and steps by position, with
//! `then` and `else` as the keys of nested steps (`turn.phases[start].automatic[2].else[0]`).
//! Within a step: `player`, then its zones in key order (a source that cards are taken from is
//! checked for existing and then for being ordered; the zones of an `untap` for existing and then
//! for repeating), then `state`, `value`, `count` and `position` as the step has them; an `if`
//! checks its `then` steps and then its `else` steps. A phase's action list checks each action for
//! existing and then for repeating. `set_state` needs a state some card type declares, with a value
//! of the type that card type gives it; `untap` needs the same of `tapped` and a bool.
//!
//! A param is checked after its name: each alternative in order (the simple form is one, addressed
//! by the param itself; `one_of[i]` addresses the others), and in each its zone, card type, owner
//! and `where`. Refs and expressions follow ADR-0015 and ADR-0016: a ref is resolved on every
//! possible card type of its param, an expression is checked depth-first and left to right, with
//! the operands of an operator before its type rule, and errors name the operator key and operand
//! position (`where.and[0].eq[1]`). An `if` effect checks its condition, then its `then` effects,
//! then its `else` effects, each at its position (`effects[1].then[0]`).
//!
//! Win conditions come last, in declaration order: per condition its id (grammar, duplicate), then
//! its `when` (`draw_from_empty` needs a declared zone; an expression is checked with the card
//! types as the names a ref may start with, and must read exactly one), then its `loser`. A win
//! condition is addressed by id (`win_conditions[deck_out].when`).

mod draft;
mod error;
mod expression;
mod model;
mod validate;

pub(crate) use validate::SelfScope;

pub use draft::{
    ActionDraft, AutomaticStepDraft, CardTypeDraft, ConditionDraft, CostDraft, DeckInputDraft,
    EffectDraft, ExpressionDraft, FieldDraft, FieldTypeDraft, InitializerDraft, InstanceStateDraft,
    MetaDraft, ParamAlternativeDraft, ParamCardsDraft, ParamDraft, ParametersDraft, PhaseDraft,
    RefDraft, RulesetDraft, SetupStepDraft, TurnDraft, WinConditionDraft, WinConditionWhenDraft,
    ZoneDraft,
};
pub use error::{DefinitionError, DefinitionErrorKind, RefType, ReferenceTarget};
pub use model::{
    ActionDefinition, ActionId, AutomaticStep, CardRef, CardType, CardTypeId, CardValue, Condition,
    Cost, DeckInput, DeckInputId, Effect, Expression, FieldName, FieldSchema, FieldType,
    FieldValue, Initializer, InstanceStateSchema, Loser, Meta, Param, ParamAlternative, ParamName,
    ParamOwner, Parameters, Phase, PhaseId, PhaseKind, PlayerCount, PlayerPosition, PlayerSelector,
    Position, RulesetDefinition, RulesetId, RulesetVersion, SetupStep, StateName, TurnStructure,
    ValueRef, Visibility, WinCondition, WinConditionId, WinConditionWhen, Zone, ZoneId,
};
