//! Field Modifiers as derived reads (ADR-0024, ADR-0025).
//!
//! A Continuous Card Effect adds an amount to an int field of the card that carries it, or of every
//! card its Modifier Scope matches, while its carrier is in the Active Zone and a condition holds.
//! Nothing here writes: the sum is computed from the GameState each time a field is read, so it can
//! never disagree with the state it was derived from.
//!
//! [`modifier_sum`] takes the whole GameState and the card being read, not just that card's own
//! definition: it scans the carriers in the Active Zone. Nothing that calls it knows about scopes.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::expression;
use super::state::{CardInstanceId, GameState};
use crate::card_catalog::{CardDefinition, CardEffect, ScopeOwner};
use crate::ruleset_definition::{FieldName, ParamName};

/// The sum the active Field Modifiers add to the int field `field` of `card`'s definition,
/// computed at read time (ADR-0024, ADR-0025). A modifier is active while its carrying card is in
/// the effects active zone and its `while` holds; `while` reads base values only. It reaches `card`
/// when it has no scope and `card` is its carrier, or when `card` matches its Modifier Scope.
pub(super) fn modifier_sum(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    card: CardInstanceId,
    field: &FieldName,
) -> Result<i64, Rejection> {
    let Some(active_zone) = ruleset.definition().effects_active_zone() else {
        return Ok(0);
    };
    let overflow = || Rejection::DerivedValueOverflow {
        card,
        field: field.clone(),
    };
    // The card being read is resolved once: its owner, its definition and its zone serve its own
    // modifiers and every carrier's scope.
    let Some(target) = state.card(card) else {
        return Err(Rejection::CardDataMismatch { card });
    };
    let target_owner = target.owner();
    let target_definition = ruleset
        .catalog()
        .card(target.definition())
        .ok_or(Rejection::CardDataMismatch { card })?;
    let target_zone = state.zone_of(card);
    let mut sum = if target_zone == Some(active_zone) {
        own_modifier_sum(ruleset, state, card, target_definition, field)?
    } else {
        0
    };
    // The carriers of set-scoped modifiers are in the active zone, whichever zone `card` is in.
    for &player in state.player_order().players() {
        let Some(carriers) = state.zone(player, active_zone) else {
            continue;
        };
        for &carrier in carriers {
            let Some(carrier_definition) = state
                .card(carrier)
                .and_then(|instance| ruleset.catalog().card(instance.definition()))
            else {
                return Err(Rejection::CardDataMismatch { card: carrier });
            };
            for effect in carrier_definition.effects() {
                let CardEffect::Continuous(continuous) = effect else {
                    continue;
                };
                let Some(scope) = continuous.scope() else {
                    continue;
                };
                if continuous.modify().field() != field
                    || target_zone != Some(scope.zone())
                    || target_definition.card_type() != scope.card_type()
                    || (carrier == card && !scope.include_self())
                {
                    continue;
                }
                let reaches = match scope.owner() {
                    ScopeOwner::Controller => target_owner == player,
                    ScopeOwner::Opponent => target_owner != player,
                };
                if !reaches {
                    continue;
                }
                let bound = BoundCards::of(ParamName::new("self"), carrier);
                if !expression::holds_base(ruleset, state, &bound, continuous.condition())? {
                    continue;
                }
                sum = sum
                    .checked_add(continuous.modify().amount())
                    .ok_or_else(overflow)?;
            }
        }
    }
    Ok(sum)
}

/// The part of the sum that comes from the Card Effects of `card` itself that have no scope; `card`
/// is in the active zone and `definition` is its Card Definition.
fn own_modifier_sum(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    card: CardInstanceId,
    definition: &CardDefinition,
    field: &FieldName,
) -> Result<i64, Rejection> {
    let bound = BoundCards::of(ParamName::new("self"), card);
    let mut sum: i64 = 0;
    for effect in definition.effects() {
        let CardEffect::Continuous(continuous) = effect else {
            continue;
        };
        if continuous.scope().is_some() || continuous.modify().field() != field {
            continue;
        }
        if !expression::holds_base(ruleset, state, &bound, continuous.condition())? {
            continue;
        }
        sum = sum
            .checked_add(continuous.modify().amount())
            .ok_or_else(|| Rejection::DerivedValueOverflow {
                card,
                field: field.clone(),
            })?;
    }
    Ok(sum)
}
