//! Resolving triggered Card Effects against the Events of a transition (ADR-0023).
//!
//! The Events of the transition are scanned in order, from its first, cost Events included. An
//! Event that matches the trigger of a card enqueues it, first in first out; the front of the queue
//! fires by evaluating its `if` and running its `do`, and the Events it emits are scanned in turn.
//! Resolution ends when every Event was scanned and the queue is empty. Eligibility is decided when
//! an Event is scanned: a trigger once enqueued fires even if its card has moved since, and only
//! its `if` is read when it fires.

use std::collections::VecDeque;

use super::action::Rejection;
use super::action_effects;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::event::Event;
use super::execution::Execution;
use super::expression;
use super::state::{CardInstance, CardInstanceId};
use crate::card_catalog::{CardEffect, TriggerMatcher};
use crate::ruleset_definition::ParamName;

/// The Trigger Cap: the hard limit of trigger resolutions in one transition (ADR-0023).
pub(super) const TRIGGER_CAP: u32 = 100;

/// A trigger waiting to fire: the carrying card and the index of the Card Effect in the `effects`
/// of its Card Definition.
type Pending = (CardInstanceId, usize);

/// Resolves triggered Card Effects against the transition's Events until none is pending: scans
/// Events in order from the first (cost Events included), enqueues woken triggers FIFO, fires each
/// by evaluating its `if` and running its `do`, whose Events are scanned in turn.
///
/// Every dequeue counts toward [`TRIGGER_CAP`], a false `if` included; the one after the cap is a
/// `TriggerCapExceeded` rejection of the whole transition.
pub(super) fn resolve(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
) -> Result<(), Rejection> {
    let mut queue: VecDeque<Pending> = VecDeque::new();
    let mut scanned = 0;
    let mut resolutions: u32 = 0;
    loop {
        while let Some(event) = execution.events.get(scanned) {
            let woken = woken_by(ruleset, execution, event)?;
            queue.extend(woken);
            scanned = scanned.saturating_add(1);
        }
        let Some((card, index)) = queue.pop_front() else {
            return Ok(());
        };
        resolutions = resolutions.saturating_add(1);
        if resolutions > TRIGGER_CAP {
            return Err(Rejection::TriggerCapExceeded { cap: TRIGGER_CAP });
        }
        fire(ruleset, execution, card, index)?;
    }
}

/// The triggers `event` wakes, in the canonical order: cards of the active Player first, then of
/// the other Players in seat order, within a Player in `state.cards()` order, within a card in the
/// order of its `effects`.
fn woken_by(
    ruleset: &DeclarativeRuleset,
    execution: &Execution,
    event: &Event,
) -> Result<Vec<Pending>, Rejection> {
    let mut woken: Vec<(usize, Pending)> = Vec::new();
    let state = &execution.state;
    for instance in state.cards() {
        if !could_wake(event, instance) {
            continue;
        }
        let card = instance.id();
        let definition = ruleset
            .catalog()
            .card(instance.definition())
            .ok_or(Rejection::CardDataMismatch { card })?;
        for (index, effect) in definition.effects().iter().enumerate() {
            let CardEffect::Triggered(triggered) = effect else {
                continue;
            };
            if matches(ruleset, execution, event, instance, triggered.matcher()) {
                let owner = instance.owner();
                let rank = if owner == state.active_player() {
                    0
                } else {
                    state
                        .player_order()
                        .players()
                        .iter()
                        .position(|&player| player == owner)
                        .map_or(usize::MAX, |seat| seat.saturating_add(1))
                };
                woken.push((rank, (card, index)));
            }
        }
    }
    // Stable: the order of the scan is kept within a rank.
    woken.sort_by_key(|&(rank, _)| rank);
    Ok(woken.into_iter().map(|(_, pending)| pending).collect())
}

/// A cheap test that keeps the scan of every card from reading catalogs for Events that no matcher
/// reads: only a card that moved (`enter`) and the cards of the Player whose Turn began
/// (`turn_start`) can be woken. A matcher that reads other Events must widen this.
fn could_wake(event: &Event, instance: &CardInstance) -> bool {
    match event {
        Event::CardMoved { card, .. } => *card == instance.id(),
        Event::TurnBegan { player, .. } => *player == instance.owner(),
        _ => false,
    }
}

/// Whether `event` wakes a trigger with this matcher on `instance`. `enter` matches `CardMoved`
/// only, in any zone, and `turn_start` matches only while the card is in the active zone.
fn matches(
    ruleset: &DeclarativeRuleset,
    execution: &Execution,
    event: &Event,
    instance: &CardInstance,
    matcher: &TriggerMatcher,
) -> bool {
    match (matcher, event) {
        (TriggerMatcher::Enter(zone), Event::CardMoved { card, to, .. }) => {
            *card == instance.id() && to == zone
        }
        (TriggerMatcher::TurnStart, Event::TurnBegan { player, .. }) => {
            *player == instance.owner()
                && ruleset
                    .definition()
                    .effects_active_zone()
                    .is_some_and(|active| execution.state.zone_of(instance.id()) == Some(active))
        }
        _ => false,
    }
}

/// Fires one trigger: its `if` reads the carrying card as `self` in the state as it is now, and a
/// false one emits nothing; otherwise each effect of its `do` runs, in order, to completion.
fn fire(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    card: CardInstanceId,
    index: usize,
) -> Result<(), Rejection> {
    let mismatch = || Rejection::CardDataMismatch { card };
    let instance = execution.state.card(card).ok_or_else(mismatch)?;
    let definition = ruleset
        .catalog()
        .card(instance.definition())
        .ok_or_else(mismatch)?;
    let Some(CardEffect::Triggered(triggered)) = definition.effects().get(index) else {
        return Err(mismatch());
    };
    let bound = BoundCards::of(ParamName::new("self"), card);
    if let Some(condition) = triggered.condition() {
        if !expression::holds(ruleset, &execution.state, &bound, condition)? {
            return Ok(());
        }
    }
    for effect in triggered.body() {
        action_effects::run(ruleset, execution, &bound, effect)?;
    }
    Ok(())
}
