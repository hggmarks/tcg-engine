//! Field Modifiers as derived reads (ADR-0024).
//!
//! A Continuous Card Effect adds an amount to an int field of the card that carries it, while a
//! condition holds. Nothing here writes: the sum is computed from the GameState each time a field is
//! read, so it can never disagree with the state it was derived from.
//!
//! [`modifier_sum`] takes the whole GameState and the card being read, not just that card's own
//! definition: the Card Effects of other cards that scope their modifier over a set of cards
//! (ticket [19]) widen its body and nothing that calls it.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::expression;
use super::state::{CardInstanceId, GameState};
use crate::card_catalog::CardEffect;
use crate::ruleset_definition::{FieldName, ParamName};

/// The sum the active Field Modifiers add to the int field `field` of `card`'s definition,
/// computed at read time (ADR-0024). A modifier is active while its carrying card is in the
/// effects active zone and its `while` holds; `while` reads base values only.
pub(super) fn modifier_sum(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    card: CardInstanceId,
    field: &FieldName,
) -> Result<i64, Rejection> {
    let Some(active_zone) = ruleset.definition().effects_active_zone() else {
        return Ok(0);
    };
    if state.zone_of(card) != Some(active_zone) {
        return Ok(0);
    }
    let Some(definition) = state
        .card(card)
        .and_then(|instance| ruleset.catalog().card(instance.definition()))
    else {
        return Err(Rejection::CardDataMismatch { card });
    };
    let bound = BoundCards::of(ParamName::new("self"), card);
    let mut sum: i64 = 0;
    for effect in definition.effects() {
        let CardEffect::Continuous(continuous) = effect else {
            continue;
        };
        if continuous.modify().field() != field {
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
