//! Resolving triggered Card Effects against the Events of a transition (ADR-0023, ADR-0033).
//!
//! The Events of the transition are scanned in order, from its first, cost Events included. An
//! Event that matches the trigger of a card enqueues it, first in first out; the front of the queue
//! fires by evaluating its `if` and running its `do`, and the Events it emits are scanned in turn.
//! Resolution ends when every Event was scanned and the queue is empty. Eligibility is decided when
//! an Event is scanned: a trigger once enqueued fires even if its card has moved since, and only
//! its `if` is read when it fires.
//!
//! A `do` may reach a Choice, which pauses the Resolution. It spans the Answers that follow: the
//! queue, the Events not yet scanned and the Trigger Cap count are carried by the
//! [`PausedResolution`], and [`resume`] goes on from them. The Events of a paused `do` are scanned
//! only after the `do` completes, together with those after the pause, so a pause changes no order.

use std::collections::VecDeque;

use super::action::Rejection;
use super::action_effects::{self, Flow};
use super::arguments::{self, BoundCards};
use super::creation::DeclarativeRuleset;
use super::decision::{Answer, BoundName, DecisionKind, PausedResolution, QueuedTrigger};
use super::event::Event;
use super::execution::Execution;
use super::expression;
use super::state::{CardInstance, CardInstanceId, GameState};
use crate::card_catalog::{CardEffect, TriggerMatcher, TriggeredCardEffect};
use crate::player::PlayerId;
use crate::ruleset_definition::Effect;
use crate::ruleset_definition::ParamName;

/// The Trigger Cap: the hard limit of trigger resolutions in one Resolution, which spans the pauses
/// it makes (ADR-0023, ADR-0033).
pub(super) const TRIGGER_CAP: u32 = 100;

/// A Resolution that stopped at a Choice: who is asked, for what, and what goes on afterwards.
#[derive(Debug)]
pub(super) struct Pause {
    pub(super) decider: PlayerId,
    pub(super) kind: DecisionKind,
    pub(super) paused: PausedResolution,
}

/// Where a Resolution is: the queue, the Events still to scan (the ones a pause carried, then the
/// transition's own from `scanned`) and the Trigger Cap count.
struct Progress {
    queue: VecDeque<QueuedTrigger>,
    carried: Vec<Event>,
    scanned: usize,
    resolutions: u32,
}

/// Resolves triggered Card Effects against the transition's Events until none is pending or a `do`
/// pauses at a Choice: scans Events in order from the first (cost Events included), enqueues woken
/// triggers FIFO, fires each by evaluating its `if` and running its `do`, whose Events are scanned
/// in turn.
///
/// Every dequeue counts toward [`TRIGGER_CAP`], a false `if` included; the one after the cap is a
/// `TriggerCapExceeded` rejection of the whole transition.
pub(super) fn resolve(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
) -> Result<Option<Pause>, Rejection> {
    run(
        ruleset,
        execution,
        Progress {
            queue: VecDeque::new(),
            carried: Vec::new(),
            scanned: 0,
            resolutions: 0,
        },
    )
}

/// Goes on with a paused Resolution after the decision was answered with `answer`. The first Event
/// of `execution` is the `DecisionAnswered` that was applied: the scan skips it. The rest of the
/// `do` runs first (a `do` that ends early at a Card Choice with no Candidate is a `do` that
/// completed), then the carried Events are scanned, then those of this trace, with the queue and
/// the Trigger Cap count of the pause. It may pause again.
pub(super) fn resume(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    paused: &PausedResolution,
    answer: Answer,
) -> Result<Option<Pause>, Rejection> {
    let triggered = triggered_of(ruleset, &execution.state, paused.trigger)?;
    let bound = BoundCards::of(ParamName::new("self"), paused.trigger.card);
    let flow = action_effects::resume(
        ruleset,
        execution,
        &bound,
        triggered.body(),
        &paused.at,
        answer,
        &paused.bound,
    )?;
    if let Flow::Paused(pause) = flow {
        // Paused again before anything was scanned: everything not scanned is carried on, in
        // order: what the first pause carried, then what this trace emitted after the Answer.
        let mut unscanned = paused.unscanned.clone();
        unscanned.extend(execution.events.iter().skip(1).cloned());
        return Ok(Some(Pause {
            decider: pause.decider,
            kind: pause.kind,
            paused: PausedResolution {
                trigger: paused.trigger,
                at: pause.at,
                queue: paused.queue.clone(),
                unscanned,
                resolutions: paused.resolutions,
                bound: pause.bound,
            },
        }));
    }
    run(
        ruleset,
        execution,
        Progress {
            queue: paused.queue.iter().copied().collect(),
            carried: paused.unscanned.clone(),
            scanned: 1,
            resolutions: paused.resolutions,
        },
    )
}

fn run(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    progress: Progress,
) -> Result<Option<Pause>, Rejection> {
    let Progress {
        mut queue,
        carried,
        mut scanned,
        mut resolutions,
    } = progress;
    let mut carried_at = 0;
    loop {
        while let Some(event) = carried.get(carried_at) {
            queue.extend(woken_by(ruleset, execution, event)?);
            carried_at = carried_at.saturating_add(1);
        }
        while let Some(event) = execution.events.get(scanned) {
            queue.extend(woken_by(ruleset, execution, event)?);
            scanned = scanned.saturating_add(1);
        }
        let Some(trigger) = queue.pop_front() else {
            return Ok(None);
        };
        resolutions = resolutions.saturating_add(1);
        if resolutions > TRIGGER_CAP {
            return Err(Rejection::TriggerCapExceeded { cap: TRIGGER_CAP });
        }
        let before = execution.events.len();
        // A `do` that ended early at a Card Choice with no Candidate is a `do` that completed.
        if let Flow::Paused(pause) = fire(ruleset, execution, trigger)? {
            return Ok(Some(Pause {
                decider: pause.decider,
                kind: pause.kind,
                paused: PausedResolution {
                    trigger,
                    at: pause.at,
                    queue: queue.into_iter().collect(),
                    unscanned: execution.events.iter().skip(before).cloned().collect(),
                    resolutions,
                    bound: pause.bound,
                },
            }));
        }
    }
}

/// The triggers `event` wakes, in the canonical order: cards of the active Player first, then of
/// the other Players in seat order, within a Player in `state.cards()` order, within a card in the
/// order of its `effects`.
fn woken_by(
    ruleset: &DeclarativeRuleset,
    execution: &Execution,
    event: &Event,
) -> Result<Vec<QueuedTrigger>, Rejection> {
    let mut woken: Vec<(usize, QueuedTrigger)> = Vec::new();
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
                woken.push((
                    rank,
                    QueuedTrigger {
                        card,
                        effect: u32::try_from(index).unwrap_or(u32::MAX),
                    },
                ));
            }
        }
    }
    // Stable: the order of the scan is kept within a rank.
    woken.sort_by_key(|&(rank, _)| rank);
    Ok(woken.into_iter().map(|(_, trigger)| trigger).collect())
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

/// Whether `event` wakes a trigger with this matcher on `instance`. Without an Active Zone no
/// trigger is eligible. `enter` matches `CardMoved` only, arriving in the matcher's own zone
/// whichever zone that is; `turn_start` matches only while the card is in the Active Zone.
fn matches(
    ruleset: &DeclarativeRuleset,
    execution: &Execution,
    event: &Event,
    instance: &CardInstance,
    matcher: &TriggerMatcher,
) -> bool {
    let Some(active) = ruleset.definition().effects_active_zone() else {
        return false;
    };
    match (matcher, event) {
        (TriggerMatcher::Enter(zone), Event::CardMoved { card, to, .. }) => {
            *card == instance.id() && to == zone
        }
        (TriggerMatcher::TurnStart, Event::TurnBegan { player, .. }) => {
            *player == instance.owner() && execution.state.zone_of(instance.id()) == Some(active)
        }
        _ => false,
    }
}

/// The Triggered Card Effect a queued trigger names, read from the catalog through the card it
/// sits on.
fn triggered_of<'a>(
    ruleset: &'a DeclarativeRuleset,
    state: &GameState,
    trigger: QueuedTrigger,
) -> Result<&'a TriggeredCardEffect, Rejection> {
    let card: CardInstanceId = trigger.card;
    let mismatch = || Rejection::CardDataMismatch { card };
    let instance = state.card(card).ok_or_else(mismatch)?;
    let definition = ruleset
        .catalog()
        .card(instance.definition())
        .ok_or_else(mismatch)?;
    let index = usize::try_from(trigger.effect).map_err(|_| mismatch())?;
    match definition.effects().get(index) {
        Some(CardEffect::Triggered(triggered)) => Ok(triggered),
        _ => Err(mismatch()),
    }
}

/// What the Choice a Paused Resolution is paused at asks, read from the catalog through its path.
pub(super) enum Asked {
    /// An Option Choice with this many options.
    Options(u32),
    /// A Card Choice with these Candidates, in ascending Card Instance Id: recomputed now, on the
    /// GameState the Choice was reached in, with `self` and the names the pause kept (ADR-0034).
    Candidates(Vec<CardInstanceId>),
}

/// What the Choice `paused` is paused at asks. A path that does not lead to a Choice is a data
/// mismatch.
pub(super) fn asked(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    paused: &PausedResolution,
) -> Result<Asked, Rejection> {
    let card = paused.trigger.card;
    let mismatch = || Rejection::CardDataMismatch { card };
    let triggered = triggered_of(ruleset, state, paused.trigger)?;
    match action_effects::choice_at(triggered.body(), &paused.at) {
        Some(Effect::ChooseOne { options, .. }) => Ok(Asked::Options(
            u32::try_from(options.len()).map_err(|_| mismatch())?,
        )),
        Some(Effect::ChooseCard { param, .. }) => {
            let controller = state.card(card).ok_or_else(mismatch)?.owner();
            let mut scope = BoundCards::of(ParamName::new("self"), card);
            for BoundName { name, card } in &paused.bound {
                scope = scope.with(name.clone(), *card);
            }
            Ok(Asked::Candidates(arguments::candidates(
                ruleset, state, controller, &scope, param,
            )))
        }
        _ => Err(mismatch()),
    }
}

/// Fires one trigger: its `if` reads the carrying card as `self` in the state as it is now, and a
/// false one emits nothing; otherwise each effect of its `do` runs, in order, until the end or a
/// Choice.
fn fire(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    trigger: QueuedTrigger,
) -> Result<Flow, Rejection> {
    let triggered = triggered_of(ruleset, &execution.state, trigger)?;
    let bound = BoundCards::of(ParamName::new("self"), trigger.card);
    if let Some(condition) = triggered.condition() {
        if !expression::holds(ruleset, &execution.state, &bound, condition)? {
            return Ok(Flow::Done);
        }
    }
    action_effects::run_from(ruleset, execution, &bound, triggered.body(), 0)
}
