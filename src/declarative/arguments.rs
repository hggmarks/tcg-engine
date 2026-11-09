//! Binding the arguments of an Action to the params of its Action Definition.
//!
//! Arguments bind to params by position (ADR-0015). The checks run in this order: the number of
//! arguments, then for each param in order: the card exists, its owner is the acting Player, it is
//! in the param's zone of the acting Player, and its card type is the param's.

use super::action::{Action, Argument, Rejection};
use super::creation::DeclarativeRuleset;
use super::state::{CardInstanceId, GameState};
use crate::ruleset_definition::{ActionDefinition, ParamName};

/// The cards an Action supplied, by the name of the param each fills.
pub(super) struct BoundCards {
    cards: Vec<(ParamName, CardInstanceId)>,
}

impl BoundCards {
    /// The card bound to `param`. Definition validation guarantees every ref names a param, so a
    /// missing one is an internal inconsistency.
    pub(super) fn card(&self, param: &ParamName) -> Result<CardInstanceId, Rejection> {
        self.cards
            .iter()
            .find(|(name, _)| name == param)
            .map(|(_, card)| *card)
            .ok_or_else(|| Rejection::CardDataMismatch {
                card: self
                    .cards
                    .first()
                    .map_or(CardInstanceId(0), |(_, card)| *card),
            })
    }
}

/// Checks the arguments of `action` against the params of `declared`, in the documented order.
pub(super) fn bind(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    declared: &ActionDefinition,
    action: &Action,
) -> Result<BoundCards, Rejection> {
    let params = declared.params();
    if params.len() != action.arguments.len() {
        return Err(Rejection::ArgumentCountMismatch {
            expected: params.len(),
            found: action.arguments.len(),
        });
    }
    let mut cards = Vec::with_capacity(params.len());
    for (param, argument) in params.iter().zip(&action.arguments) {
        let Argument::Card(card) = *argument;
        let name = param.name().clone();
        let Some(instance) = state.card(card) else {
            return Err(Rejection::UnknownCard { param: name, card });
        };
        if instance.owner() != action.player {
            return Err(Rejection::NotCardOwner {
                param: name,
                card,
                owner: instance.owner(),
            });
        }
        let in_zone = state
            .zone(action.player, param.zone())
            .is_some_and(|held| held.contains(&card));
        if !in_zone {
            return Err(Rejection::CardNotInZone {
                param: name,
                card,
                zone: param.zone().clone(),
            });
        }
        let Some(definition) = ruleset.catalog().card(instance.definition()) else {
            return Err(Rejection::CardDataMismatch { card });
        };
        if definition.card_type() != param.card_type() {
            return Err(Rejection::WrongCardType {
                param: name,
                card,
                expected: param.card_type().clone(),
                found: definition.card_type().clone(),
            });
        }
        cards.push((name, card));
    }
    Ok(BoundCards { cards })
}
