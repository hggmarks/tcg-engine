//! Paying the costs and running the effects of an Action Definition (ADR-0015, ADR-0016).
//!
//! Everything here runs on the Execution's clone of the GameState, so an Action that fails half
//! way changes nothing (ADR-0002). Refs were resolved to a field or an instance state when the
//! definition was validated; here they only read that value.

use super::action::Rejection;
use super::arguments::{self, BoundCards};
use super::creation::DeclarativeRuleset;
use super::decision::{Answer, BoundName, Branch, BranchKind, DecisionKind, EffectPath};
use super::event::Event;
use super::execution::Execution;
use super::expression;
use super::state::CardInstanceId;
use super::turn_cycle;
use crate::player::PlayerId;
use crate::ruleset_definition::{
    Cost, Decider, Effect, EffectPlayer, FieldValue, ParamName, PlayerStateId, PlayerTarget,
    StateName, ValueRef, ZoneId,
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
        Cost::TapCard { card } => tap_card(execution, bound, card.param()),
        Cost::Move { card, to } => move_card(execution, bound, card.param(), to),
        Cost::SetPlayerState {
            player,
            state,
            value,
        } => set_player_state(execution, bound, *player, state, value),
        Cost::AddPlayerState {
            player,
            state,
            amount,
        } => add_player_state(execution, bound, *player, state, *amount),
    }
}

/// Sets the Player state `state` of each Player `target` names to `value`, in seat order for
/// `each`.
fn set_player_state(
    execution: &mut Execution,
    bound: &BoundCards,
    target: PlayerTarget,
    state: &PlayerStateId,
    value: &FieldValue,
) -> Result<(), Rejection> {
    for player in expression::target_players(&execution.state, bound, target)? {
        execution
            .set_player_state(player, state, value)
            .map_err(Rejection::EventRejected)?;
    }
    Ok(())
}

/// Adds `amount` to the int Player state `state` of each Player `target` names, with checked
/// arithmetic: an overflow refuses the whole Action.
fn add_player_state(
    execution: &mut Execution,
    bound: &BoundCards,
    target: PlayerTarget,
    state: &PlayerStateId,
    amount: i64,
) -> Result<(), Rejection> {
    for player in expression::target_players(&execution.state, bound, target)? {
        execution.add_player_state(player, state, amount)?;
    }
    Ok(())
}

/// Taps the card bound to `param`, which must be untapped.
fn tap_card(
    execution: &mut Execution,
    bound: &BoundCards,
    param: &ParamName,
) -> Result<(), Rejection> {
    let card = bound.card(param)?;
    let tapped = StateName::new("tapped");
    match execution
        .state
        .card(card)
        .and_then(|instance| instance.state_value(&tapped))
    {
        Some(FieldValue::Bool(false)) => {}
        Some(FieldValue::Bool(true)) => return Err(Rejection::CardAlreadyTapped { card }),
        _ => return Err(Rejection::CardDataMismatch { card }),
    }
    execution
        .apply(Event::CardStateChanged {
            card,
            state: tapped,
            value: FieldValue::Bool(true),
        })
        .map_err(Rejection::EventRejected)
}

/// Moves the card bound to `param` from the zone it is in onto the Top of `to`, a zone of its
/// owner.
fn move_card(
    execution: &mut Execution,
    bound: &BoundCards,
    param: &ParamName,
    to: &ZoneId,
) -> Result<(), Rejection> {
    let card = bound.card(param)?;
    let (Some(from), Some(instance)) = (execution.state.zone_of(card), execution.state.card(card))
    else {
        return Err(Rejection::CardDataMismatch { card });
    };
    let event = Event::CardMoved {
        card,
        player: instance.owner(),
        from: from.clone(),
        to: to.clone(),
    };
    execution.apply(event).map_err(Rejection::EventRejected)
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

/// How running effects ended: to the end, at a Choice that asks a Player (ADR-0033), or early
/// because a Card Choice had no Candidate (ADR-0034).
#[derive(Debug)]
pub(super) enum Flow {
    Done,
    Paused(Pause),
    /// A Card Choice had no Candidate: the rest of the whole `do` is skipped, every enclosing list
    /// included. Nothing runs after it; the Events emitted so far stand.
    Skipped,
}

/// A Choice reached: who is asked, what for, where in the `do` it is, and the names in scope there.
#[derive(Debug)]
pub(super) struct Pause {
    pub(super) decider: PlayerId,
    pub(super) kind: DecisionKind,
    pub(super) at: EffectPath,
    /// The names in scope at the Choice, in binding order, `self` left out.
    pub(super) bound: Vec<BoundName>,
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
        Flow::Paused(_) | Flow::Skipped => Err(Rejection::ChoiceOutsideTriggeredEffect),
    }
}

/// Runs `effects` in order, from the one at `start`, stopping at the first Choice or when a Card
/// Choice has no Candidate. The index of an effect in a path is its index in `effects`.
pub(super) fn run_from(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    effects: &[Effect],
    start: usize,
) -> Result<Flow, Rejection> {
    for (index, effect) in effects.iter().enumerate().skip(start) {
        match run_at(ruleset, execution, bound, effect, index)? {
            Flow::Done => {}
            stopped @ (Flow::Paused(_) | Flow::Skipped) => return Ok(stopped),
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

/// The Choice at `at` in `body`, the `do` of a Triggered Card Effect; `None` when `at` does not
/// lead to a Choice.
pub(super) fn choice_at<'a>(body: &'a [Effect], at: &EffectPath) -> Option<&'a Effect> {
    let mut list = body;
    for branch in &at.branches {
        let effect = list.get(usize::try_from(branch.effect).ok()?)?;
        list = branch_of(effect, &branch.into)?;
    }
    match list.get(usize::try_from(at.choice).ok()?)? {
        choice @ (Effect::ChooseOne { .. } | Effect::ChooseCard { .. }) => Some(choice),
        _ => None,
    }
}

/// The scope at `list[upto]`: `outer` plus the names the Card Choices of `list[..upto]` bound, each
/// with the card `names` holds for it. A missing name is an inconsistency (ADR-0034).
fn level_scope(
    outer: &BoundCards,
    list: &[Effect],
    upto: usize,
    names: &[BoundName],
) -> Result<BoundCards, Rejection> {
    let mut scope = outer.clone();
    for effect in list.iter().take(upto) {
        if let Effect::ChooseCard { param, .. } = effect {
            let Some(bound) = names.iter().find(|bound| &bound.name == param.name()) else {
                let card = outer.card(&ParamName::new("self"))?;
                return Err(Rejection::CardDataMismatch { card });
            };
            scope = scope.with(bound.name.clone(), bound.card);
        }
    }
    Ok(scope)
}

/// Resumes a paused `do` after its Choice at `at` was answered with `answer`: an option runs and a
/// card is bound, then the rest of each enclosing list goes on, innermost first. No `if` on the
/// path is read again: the path says which branch the pause was in. It may pause again, at a path
/// of its own, or end the `do` early at a Card Choice with no Candidate.
///
/// `bound` holds `self`; `names` are those the Paused Resolution kept. The scope of each list is
/// rebuilt from the Card Choices before the path in it, so the names bound inside a branch never
/// reach the list that follows it (ADR-0034).
pub(super) fn resume(
    ruleset: &DeclarativeRuleset,
    execution: &mut Execution,
    bound: &BoundCards,
    body: &[Effect],
    at: &EffectPath,
    answer: Answer,
    names: &[BoundName],
) -> Result<Flow, Rejection> {
    let resumption = Resumption {
        ruleset,
        names,
        answer,
        choice: at.choice,
    };
    resumption.resume_in(execution, bound, body, &at.branches)
}

/// What is the same at every level of a resumption.
struct Resumption<'a> {
    ruleset: &'a DeclarativeRuleset,
    names: &'a [BoundName],
    answer: Answer,
    choice: u32,
}

impl Resumption<'_> {
    fn resume_in(
        &self,
        execution: &mut Execution,
        outer: &BoundCards,
        list: &[Effect],
        branches: &[Branch],
    ) -> Result<Flow, Rejection> {
        let card = outer.card(&ParamName::new("self"))?;
        let mismatch = || Rejection::CardDataMismatch { card };
        let position = |index: u32| usize::try_from(index).map_err(|_| mismatch());
        match branches.split_first() {
            None => {
                let choice = position(self.choice)?;
                let scope = level_scope(outer, list, choice, self.names)?;
                let after = choice.saturating_add(1);
                match (list.get(choice), self.answer) {
                    (Some(effect @ Effect::ChooseOne { .. }), Answer::Option(option)) => {
                        let Some(chosen) = branch_of(effect, &BranchKind::Option(option)) else {
                            return Err(mismatch());
                        };
                        match run_from(self.ruleset, execution, &scope, chosen, 0)? {
                            Flow::Done => {}
                            Flow::Paused(pause) => {
                                let entered = Branch {
                                    effect: number(choice),
                                    into: BranchKind::Option(option),
                                };
                                return Ok(Flow::Paused(pause.entered(entered)));
                            }
                            Flow::Skipped => return Ok(Flow::Skipped),
                        }
                        run_from(self.ruleset, execution, &scope, list, after)
                    }
                    (Some(Effect::ChooseCard { param, .. }), Answer::Card(picked)) => {
                        let scope = scope.with(param.name().clone(), picked);
                        run_from(self.ruleset, execution, &scope, list, after)
                    }
                    _ => Err(mismatch()),
                }
            }
            Some((branch, rest)) => {
                let at = position(branch.effect)?;
                let level = level_scope(outer, list, at, self.names)?;
                let Some(inner) = list
                    .get(at)
                    .and_then(|effect| branch_of(effect, &branch.into))
                else {
                    return Err(mismatch());
                };
                match self.resume_in(execution, &level, inner, rest)? {
                    Flow::Done => {}
                    Flow::Paused(pause) => {
                        return Ok(Flow::Paused(pause.entered(branch.clone())));
                    }
                    Flow::Skipped => return Ok(Flow::Skipped),
                }
                run_from(self.ruleset, execution, &level, list, at.saturating_add(1))
            }
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
            move_card(execution, bound, card.param(), to)?;
            Ok(Flow::Done)
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
        Effect::SetPlayerState {
            player,
            state,
            value,
        } => {
            set_player_state(execution, bound, *player, state, value)?;
            Ok(Flow::Done)
        }
        Effect::AddPlayerState {
            player,
            state,
            amount,
        } => {
            add_player_state(execution, bound, *player, state, *amount)?;
            Ok(Flow::Done)
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
                Flow::Skipped => Ok(Flow::Skipped),
            }
        }
        Effect::ChooseOne { decider, .. } => Ok(Flow::Paused(Pause {
            decider: decider_player(execution, bound, *decider)?,
            kind: DecisionKind::Option,
            at: EffectPath {
                branches: Vec::new(),
                choice: number(index),
            },
            bound: bound.names(),
        })),
        Effect::ChooseCard { decider, param } => {
            // The Candidates are those of an action param for the controller, on the GameState of
            // this moment, with `self` and the earlier names readable (ADR-0034).
            let controller = owner_of_self(execution, bound)?;
            let candidates =
                arguments::candidates(ruleset, &execution.state, controller, bound, param);
            if candidates.is_empty() {
                return Ok(Flow::Skipped);
            }
            Ok(Flow::Paused(Pause {
                decider: decider_player(execution, bound, *decider)?,
                kind: DecisionKind::Card,
                at: EffectPath {
                    branches: Vec::new(),
                    choice: number(index),
                },
                bound: bound.names(),
            }))
        }
    }
}

/// The Player a Choice asks: the owner of `self` for `controller`, the other Player for `opponent`.
fn decider_player(
    execution: &Execution,
    bound: &BoundCards,
    decider: Decider,
) -> Result<PlayerId, Rejection> {
    let owner = owner_of_self(execution, bound)?;
    match decider {
        Decider::Controller => Ok(owner),
        Decider::Opponent => {
            let other = execution
                .state
                .player_order()
                .players()
                .iter()
                .copied()
                .find(|&other| other != owner);
            match other {
                Some(other) => Ok(other),
                None => {
                    let card = bound.card(&ParamName::new("self"))?;
                    Err(Rejection::CardDataMismatch { card })
                }
            }
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
