//! Binding the arguments of an Action to the params of its Action Definition.
//!
//! Arguments bind to params by position (ADR-0015). The checks run in this order: the number of
//! arguments, then for each param in order: the card exists, then it must fit one of the param's
//! alternatives (ADR-0016). A card fits an alternative when, in this order, its owner is the
//! acting Player (or, for `owner: opponent`, another Player), it is in the alternative's zone of
//! its owner, its card type is the alternative's, and its `where` condition holds. A param with
//! one alternative reports the first check that fails; a param with several binds the first
//! alternative the card fits, and reports that none did.

use super::action::{Action, Argument, Rejection};
use super::creation::DeclarativeRuleset;
use super::expression;
use super::state::{CardInstanceId, GameState};
use crate::player::PlayerId;
use crate::ruleset_definition::{ActionDefinition, Param, ParamAlternative, ParamName, ParamOwner};

/// The cards an Action supplied, by the name of the param each fills.
#[derive(Debug, Clone)]
pub(super) struct BoundCards {
    cards: Vec<(ParamName, CardInstanceId)>,
}

impl BoundCards {
    /// One card bound to one name.
    pub(super) fn of(param: ParamName, card: CardInstanceId) -> Self {
        Self {
            cards: vec![(param, card)],
        }
    }

    /// These bindings and one more.
    fn with(&self, param: ParamName, card: CardInstanceId) -> Self {
        let mut cards = self.cards.clone();
        cards.push((param, card));
        Self { cards }
    }

    /// The card bound to `param`. Definition validation guarantees every ref names a param, so a
    /// missing one is an internal inconsistency.
    pub(super) fn card(&self, param: &ParamName) -> Result<CardInstanceId, Rejection> {
        self.cards
            .iter()
            .find(|(name, _)| name == param)
            .map(|(_, card)| *card)
            .ok_or_else(|| Rejection::UnboundParam {
                param: param.clone(),
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
            expected: u64::try_from(params.len()).unwrap_or(u64::MAX),
            found: u64::try_from(action.arguments.len()).unwrap_or(u64::MAX),
        });
    }
    let mut bound = BoundCards { cards: Vec::new() };
    for (param, argument) in params.iter().zip(&action.arguments) {
        let Argument::Card(card) = *argument;
        if state.card(card).is_none() {
            return Err(Rejection::UnknownCard {
                param: param.name().clone(),
                card,
            });
        }
        bound = bind_param(ruleset, state, action.player, &bound, param, card)?;
    }
    Ok(bound)
}

/// `bound` and `card` bound to `param`, or why it cannot be. `bound` is not copied unless the
/// card fits an alternative or an alternative reads it in a `where`.
fn bind_param(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    actor: PlayerId,
    bound: &BoundCards,
    param: &Param,
    card: CardInstanceId,
) -> Result<BoundCards, Rejection> {
    let alternatives = param.alternatives();
    let mut failure = None;
    for alternative in alternatives {
        match check_alternative(ruleset, state, actor, bound, param, alternative, card)? {
            None => return Ok(bound.with(param.name().clone(), card)),
            Some(rejection) => {
                failure.get_or_insert(rejection);
            }
        }
    }
    // The first failure of a lone alternative is its own Rejection; with several, none fit.
    match (alternatives.len(), failure) {
        (1, Some(rejection)) => Err(rejection),
        _ => Err(Rejection::NoAlternativeMatched {
            param: param.name().clone(),
            card,
        }),
    }
}

/// `Ok(None)` if `card` fits `alternative`, `Ok(Some(rejection))` with the first check it fails,
/// and `Err` for a GameState or catalog that does not fit the ruleset. `bound` holds the bindings
/// made before `param`; the `where` reads them and `card` for `param`.
fn check_alternative(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    actor: PlayerId,
    bound: &BoundCards,
    param: &Param,
    alternative: &ParamAlternative,
    card: CardInstanceId,
) -> Result<Option<Rejection>, Rejection> {
    let name = param.name().clone();
    let Some(instance) = state.card(card) else {
        return Err(Rejection::UnknownCard { param: name, card });
    };
    let owner = instance.owner();
    match alternative.owner() {
        ParamOwner::Actor if owner != actor => {
            return Ok(Some(Rejection::NotCardOwner {
                param: name,
                card,
                owner,
            }))
        }
        ParamOwner::Opponent if owner == actor => {
            return Ok(Some(Rejection::NotOpponentCard { param: name, card }))
        }
        ParamOwner::Actor | ParamOwner::Opponent => {}
    }
    // The card is in a zone of its owner, who is the opponent for `owner: opponent`.
    let in_zone = state
        .zone(owner, alternative.zone())
        .is_some_and(|held| held.contains(&card));
    if !in_zone {
        return Ok(Some(Rejection::CardNotInZone {
            param: name,
            card,
            zone: alternative.zone().clone(),
        }));
    }
    let Some(definition) = ruleset.catalog().card(instance.definition()) else {
        return Err(Rejection::CardDataMismatch { card });
    };
    if definition.card_type() != alternative.card_type() {
        return Ok(Some(Rejection::WrongCardType {
            param: name,
            card,
            expected: alternative.card_type().clone(),
            found: definition.card_type().clone(),
        }));
    }
    if let Some(condition) = alternative.condition() {
        let candidate = bound.with(name.clone(), card);
        if !expression::holds(ruleset, state, &candidate, condition)? {
            return Ok(Some(Rejection::WhereConditionNotMet { param: name, card }));
        }
    }
    Ok(None)
}

/// The cards that can fit `param` for `actor`: those in the zone one of its alternatives names, of
/// the Players that alternative takes cards from, in ascending Card Instance Id, each once. Every
/// other card fails the owner or the zone check of every alternative, so visiting only these
/// finds what visiting every card finds, in the same order (ADR-0021).
fn candidate_cards(state: &GameState, actor: PlayerId, param: &Param) -> Vec<CardInstanceId> {
    let mut candidates = Vec::new();
    for alternative in param.alternatives() {
        for &player in state.player_order().players() {
            let takes = match alternative.owner() {
                ParamOwner::Actor => player == actor,
                ParamOwner::Opponent => player != actor,
            };
            if takes {
                if let Some(held) = state.zone(player, alternative.zone()) {
                    candidates.extend_from_slice(held);
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

/// Every argument list that binds the params of `declared` for `actor`, in Listing Order.
pub(super) fn fillings(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    actor: PlayerId,
    declared: &ActionDefinition,
) -> Vec<Vec<CardInstanceId>> {
    let mut partial: Vec<(BoundCards, Vec<CardInstanceId>)> =
        vec![(BoundCards { cards: Vec::new() }, Vec::new())];
    for param in declared.params() {
        let candidates = candidate_cards(state, actor, param);
        let mut next = Vec::new();
        for (bound, cards) in &partial {
            for &id in &candidates {
                if let Ok(candidate) = bind_param(ruleset, state, actor, bound, param, id) {
                    let mut list = cards.clone();
                    list.push(id);
                    next.push((candidate, list));
                }
            }
        }
        partial = next;
    }
    partial.into_iter().map(|(_, cards)| cards).collect()
}
