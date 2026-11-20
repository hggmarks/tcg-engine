//! Paying the costs and running the effects of an Action Definition (ADR-0015, ADR-0016).
//!
//! Everything here runs on the Execution's clone of the GameState, so an Action that fails half
//! way changes nothing (ADR-0002). Refs were resolved to a field or an instance state when the
//! definition was validated; here they only read that value.

use super::action::Rejection;
use super::arguments::BoundCards;
use super::creation::DeclarativeRuleset;
use super::decision::{Branch, BranchKind, DecisionKind, EffectPath};
use super::event::Event;
use super::execution::Execution;
use super::expression;
use super::state::CardInstanceId;
use super::turn_cycle;
use crate::player::PlayerId;
use crate::ruleset_definition::{
    Cost, Decider, Effect, EffectPlayer, FieldValue, ParamName, StateName, ValueRef, ZoneId,
};

/// Pays one cost on behalf of `player`, the acting Player.
pub(super) fn pay(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    player: PlayerId,
    cost: &Cost,
) -> Result<(), Rejection> {
    match cost {
        Cost::Tap { zone, count } => tap(ruleset, execution, bound, player, zone, count),
    }
}

/// Taps the first `count` untapped cards of the acting Player's `zone`, in list order. A card is
/// untapped when its card type declares `tapped` and the value is false.
fn tap(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    player: PlayerId,
    zone: &ZoneId,
    count: &ValueRef,
) -> Result<(), Rejection> {
    let card = bound.card(count.param())?;
    let FieldValue::Int(value) = expression::read(ruleset, &execution.state, card, count.value())?
    else {
        return Err(Rejection::CardDataMismatch { card });
    };
    let Ok(required) = u64::try_from(value) else {
        return Err(Rejection::NegativeCount { value });
    };

    let tapped = StateName::new("tapped");
    let untapped: Vec<CardInstanceId> = execution
        .cards_of(player, zone)
        .map_err(Rejection::EventRejected)?
        .into_iter()
        .filter(|&candidate| {
            execution
                .state
                .card(candidate)
                .and_then(|instance| instance.state_value(&tapped))
                == Some(&FieldValue::Bool(false))
        })
        .collect();
    let available = u64::try_from(untapped.len()).unwrap_or(u64::MAX);
    if required > available {
        return Err(Rejection::NotEnoughUntappedCards {
            zone: zone.clone(),
            required,
            available,
        });
    }
    let required = usize::try_from(required).unwrap_or(usize::MAX);
    for card in untapped.into_iter().take(required) {
        execution
            .apply(Event::CardStateChanged {
                card,
                state: tapped.clone(),
                value: FieldValue::Bool(true),
            })
            .map_err(Rejection::EventRejected)?;
    }
    Ok(())
}

/// How running effects ended: to the end, or at a Choice that asks a Player (ADR-0033).
#[derive(Debug)]
pub(super) enum Flow {
    Done,
    Paused(Pause),
}

/// A Choice reached: who is asked, what for, and where in the `do` it is.
#[derive(Debug)]
pub(super) struct Pause {
    pub(super) decider: PlayerId,
    pub(super) kind: DecisionKind,
    pub(super) at: EffectPath,
}

impl Pause {
    /// The same pause seen from one list out: the effect at `branch` was entered on the way to it.
    fn entered(mut self, branch: Branch) -> Pause {
        self.at.branches.insert(0, branch);
        self
    }
}

/// An index of an effect list as a path number. No list is long enough to saturate it.
fn number(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

/// Runs one effect of an action. A Choice is never expected there (validation refuses it), so a
/// pause is an internal inconsistency.
pub(super) fn run(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    effect: &Effect,
) -> Result<(), Rejection> {
    match run_at(ruleset, execution, bound, effect, 0)? {
        Flow::Done => Ok(()),
        Flow::Paused(_) => Err(Rejection::ChoiceOutsideTriggeredEffect),
    }
}

/// Runs `effects` in order, from the one at `start`, stopping at the first Choice. The index of an
/// effect in a path is its index in `effects`.
pub(super) fn run_from(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    effects: &[Effect],
    start: usize,
) -> Result<Flow, Rejection> {
    for (index, effect) in effects.iter().enumerate().skip(start) {
        if let Flow::Paused(pause) = run_at(ruleset, execution, bound, effect, index)? {
            return Ok(Flow::Paused(pause));
        }
    }
    Ok(Flow::Done)
}

/// The list an effect enters through `into`: `then` or `else` of an `if`, an option of a Choice.
fn branch_of<'a>(effect: &'a Effect, into: &BranchKind) -> Option<&'a [Effect]> {
    match (effect, into) {
        (Effect::If { then, .. }, BranchKind::Then) => Some(then),
        (Effect::If { otherwise, .. }, BranchKind::Else) => Some(otherwise),
        (Effect::ChooseOne { options, .. }, BranchKind::Option(option)) => options
            .get(usize::try_from(*option).ok()?)
            .map(Vec::as_slice),
        _ => None,
    }
}

/// The number of options of the Choice at `at` in `body`, the `do` of a Triggered Card Effect;
/// `None` when `at` does not lead to a Choice.
pub(super) fn option_count(body: &[Effect], at: &EffectPath) -> Option<u32> {
    let mut list = body;
    for branch in &at.branches {
        let effect = list.get(usize::try_from(branch.effect).ok()?)?;
        list = branch_of(effect, &branch.into)?;
    }
    match list.get(usize::try_from(at.choice).ok()?)? {
        Effect::ChooseOne { options, .. } => u32::try_from(options.len()).ok(),
        _ => None,
    }
}

/// Resumes a paused `do` after its Choice at `at` was answered with `option`: runs that option,
/// then the rest of each enclosing list, innermost first. No `if` on the path is read again: the
/// path says which branch the pause was in. It may pause again, at a path of its own.
pub(super) fn resume(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    body: &[Effect],
    at: &EffectPath,
    option: u32,
) -> Result<Flow, Rejection> {
    resume_in(
        ruleset,
        execution,
        bound,
        body,
        &at.branches,
        at.choice,
        option,
    )
}

fn resume_in(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    list: &[Effect],
    branches: &[Branch],
    choice: u32,
    option: u32,
) -> Result<Flow, Rejection> {
    let card = bound.card(&ParamName::new("self"))?;
    let mismatch = || Rejection::CardDataMismatch { card };
    let position = |index: u32| usize::try_from(index).map_err(|_| mismatch());
    match branches.split_first() {
        None => {
            let choice = position(choice)?;
            let Some(chosen) = list
                .get(choice)
                .and_then(|effect| branch_of(effect, &BranchKind::Option(option)))
            else {
                return Err(mismatch());
            };
            if let Flow::Paused(pause) = run_from(ruleset, execution, bound, chosen, 0)? {
                let entered = Branch {
                    effect: number(choice),
                    into: BranchKind::Option(option),
                };
                return Ok(Flow::Paused(pause.entered(entered)));
            }
            run_from(ruleset, execution, bound, list, choice.saturating_add(1))
        }
        Some((branch, rest)) => {
            let at = position(branch.effect)?;
            let Some(inner) = list
                .get(at)
                .and_then(|effect| branch_of(effect, &branch.into))
            else {
                return Err(mismatch());
            };
            let flow = resume_in(ruleset, execution, bound, inner, rest, choice, option)?;
            if let Flow::Paused(pause) = flow {
                return Ok(Flow::Paused(pause.entered(branch.clone())));
            }
            run_from(ruleset, execution, bound, list, at.saturating_add(1))
        }
    }
}

/// Runs the effect at `index` of its list. An `if` runs the effects of the branch its condition
/// picks, on the state the effects before it produced; `end_turn` is the Turn cycle's. A Choice
/// stops the run and says who is asked.
fn run_at(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    effect: &Effect,
    index: usize,
) -> Result<Flow, Rejection> {
    match effect {
        Effect::EndTurn => turn_cycle::end_turn(ruleset.definition(), execution).map(done),
        Effect::Move { card, to } => {
            let card = bound.card(card.param())?;
            let (Some(from), Some(instance)) =
                (execution.state.zone_of(card), execution.state.card(card))
            else {
                return Err(Rejection::CardDataMismatch { card });
            };
            let event = Event::CardMoved {
                card,
                player: instance.owner(),
                from: from.clone(),
                to: to.clone(),
            };
            execution
                .apply(event)
                .map(done)
                .map_err(Rejection::EventRejected)
        }
        Effect::SetState { card, state, value } => {
            let card = bound.card(card.param())?;
            change_state(execution, card, state, value.clone()).map(done)
        }
        Effect::AddState {
            card,
            state,
            amount,
        } => {
            let card = bound.card(card.param())?;
            let Some(FieldValue::Int(current)) = execution
                .state
                .card(card)
                .and_then(|instance| instance.state_value(state))
            else {
                return Err(Rejection::CardDataMismatch { card });
            };
            let Some(sum) = current.checked_add(*amount) else {
                return Err(Rejection::StateOverflow {
                    card,
                    state: state.clone(),
                });
            };
            change_state(execution, card, state, FieldValue::Int(sum)).map(done)
        }
        Effect::Draw {
            player,
            from,
            to,
            count,
        } => {
            let player = match player {
                EffectPlayer::Controller => owner_of_self(execution, bound)?,
            };
            turn_cycle::draw(execution, player, from, to, count.get())
                .map(done)
                .map_err(Rejection::EventRejected)
        }
        Effect::If {
            condition,
            then,
            otherwise,
        } => {
            let (branch, into) = if expression::holds(ruleset, &execution.state, bound, condition)?
            {
                (then, BranchKind::Then)
            } else {
                (otherwise, BranchKind::Else)
            };
            match run_from(ruleset, execution, bound, branch, 0)? {
                Flow::Done => Ok(Flow::Done),
                Flow::Paused(pause) => Ok(Flow::Paused(pause.entered(Branch {
                    effect: number(index),
                    into,
                }))),
            }
        }
        Effect::ChooseOne { decider, .. } => {
            let owner = owner_of_self(execution, bound)?;
            let decider = match decider {
                Decider::Controller => owner,
                Decider::Opponent => {
                    let other = execution
                        .state
                        .player_order()
                        .players()
                        .iter()
                        .copied()
                        .find(|&other| other != owner);
                    match other {
                        Some(other) => other,
                        None => {
                            let card = bound.card(&ParamName::new("self"))?;
                            return Err(Rejection::CardDataMismatch { card });
                        }
                    }
                }
            };
            Ok(Flow::Paused(Pause {
                decider,
                kind: DecisionKind::Option,
                at: EffectPath {
                    branches: Vec::new(),
                    choice: number(index),
                },
            }))
        }
    }
}

/// An effect that ran to its end.
fn done(_: ()) -> Flow {
    Flow::Done
}

/// The owner of `self`, the carrying card: the controller.
fn owner_of_self(execution: &Execution, bound: &BoundCards) -> Result<PlayerId, Rejection> {
    let card = bound.card(&ParamName::new("self"))?;
    let Some(instance) = execution.state.card(card) else {
        return Err(Rejection::CardDataMismatch { card });
    };
    Ok(instance.owner())
}

/// Sets the instance state `state` of `card` to `value` if it changes: an unchanged value emits no
/// Event.
fn change_state(
    execution: &mut Execution,
    card: CardInstanceId,
    state: &StateName,
    value: FieldValue,
) -> Result<(), Rejection> {
    let current = execution
        .state
        .card(card)
        .and_then(|instance| instance.state_value(state));
    if current == Some(&value) {
        return Ok(());
    }
    execution
        .apply(Event::CardStateChanged {
            card,
            state: state.clone(),
            value,
        })
        .map_err(Rejection::EventRejected)
}
