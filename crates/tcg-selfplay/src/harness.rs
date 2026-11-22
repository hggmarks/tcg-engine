//! Seeded self-play: whole Games played by the Uniform Random Policy with the invariants checked
//! after every transition, and the verification of a Game Record (ADR-0020).
//!
//! Nothing here reads the clock or the environment, and the policy never touches the GameState's
//! Random State, so a Game is a pure function of the Ruleset, the Deck Lists, the Seed and the
//! Step Cap.

use std::num::NonZeroU64;

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, DeclarativeRuleset, Event, GameCreation, GameCreationError, GameRecord, GameState,
    ObservedEvent, ObservedGameState, Observer, Rejection, Replay, ReplayError, Viewer,
};
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset::{EventTrace, LegalActions, Ruleset, Transition};

mod observing;

use observing::Observing;

use crate::invariants::{
    check_opening, check_recomputation, check_rejection, check_transition, Invariant,
    InvariantViolation,
};
use crate::observations::{
    check_hidden_cards_ignored, check_observed_actions, check_observed_consistency, ObservedStep,
    ReplayedStep,
};

/// The constant the policy's generator is derived with: the policy draws from
/// `RandomState::from_seed(Seed(seed.0 ^ POLICY_STREAM))`, a stream separate from the Random
/// State inside the GameState, which the Game creation seeds with `seed` itself. The policy
/// therefore never reads or advances the GameState's Random State.
pub const POLICY_STREAM: u64 = 0x706f_6c69_6379_5f31;

/// Whether self-play checks the invariants. `Off` exists so that the example can report the
/// throughput of the engine alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checks {
    /// All twelve Invariants: the nine engine ones and the three over Observations (ADR-0031).
    On,
    /// The nine engine Invariants without the observation checks. It exists so that the example
    /// can measure the cost of observing.
    EngineOnly,
    Off,
}

/// A Game that ended: its record and everything a replay must reproduce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfPlayGame {
    /// The identities, the Seed and the accepted Actions.
    pub record: GameRecord,
    /// The Game creation.
    pub creation: GameCreation,
    /// The Event Trace of each Action of the record.
    pub traces: Vec<EventTrace<Event>>,
    /// The final GameState, which has a Terminal Outcome.
    pub state: GameState,
    /// How many Trigger Cap Rejections the listings of this Game saw: candidates that the Trigger
    /// Cap refused (ADR-0023), counted over every listing the policy drew from. It is a count of
    /// an outcome, never a failure. Only [`Checks::Off`] does not measure it: it lists through
    /// `legal_actions` and does not see the refused candidates (ADR-0036).
    pub trigger_cap_rejections: TriggerCapRejections,
}

/// How many Trigger Cap Rejections the listings of a Game saw (ADR-0023, ADR-0036).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerCapRejections {
    /// The Game ran with `Checks::Off`, whose listings never see refused candidates.
    NotMeasured,
    /// The Game ran with `Checks::On` or `Checks::EngineOnly`: this many candidates were refused,
    /// zero included.
    Counted(usize),
}

impl std::fmt::Display for TriggerCapRejections {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMeasured => write!(formatter, "Trigger Cap Rejections not measured"),
            Self::Counted(count) => write!(formatter, "{count} Trigger Cap Rejections"),
        }
    }
}

/// Why a Game did not end well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureKind {
    /// One or more invariants were violated, in ticket order.
    InvariantViolated(Vec<InvariantViolation>),
    /// The Game had not ended after `max_steps` accepted Actions (the Step Cap).
    StepCapReached { max_steps: usize },
    /// The Game had not ended and the Ruleset listed no Legal Action.
    NoLegalAction,
    /// An Action the Ruleset listed as legal was rejected.
    LegalActionRejected {
        action: Action,
        rejection: Rejection,
    },
    /// The Game could not be created.
    Creation(GameCreationError),
    /// A record could not be replayed by [`verify_record`].
    Replay(ReplayError),
}

/// A failure of self-play or of record verification. It carries what is needed to reproduce it.
/// The functions return it boxed: it is large, and `Result`'s error type should not be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfPlayFailure {
    /// The Seed of the Game.
    pub seed: Seed,
    /// The index in `record.actions()` of the last accepted Action when the failure occurred;
    /// `None` when it occurred before any Action was accepted, at the Opening Position.
    pub action_index: Option<usize>,
    /// A record that reproduces the failure. What reproduces it depends on the kind:
    ///
    /// - `InvariantViolated`: [`verify_record`] on it fails with the same violation at the same
    ///   `action_index`. The one exception is `RejectionLeavesStateIntact`, which is found while
    ///   listing: the record replays to the GameState in which the Action was rejected, and
    ///   `check_rejection` on that Action reports it.
    /// - `StepCapReached` and `NoLegalAction`: it verifies as `Ok` and replays to the GameState
    ///   the failure stopped in, which has not ended (for `NoLegalAction`, with an empty Legal
    ///   Action listing).
    /// - `LegalActionRejected`: it replays to the GameState in which the Action carried by the
    ///   kind is rejected (that Action is not in the record).
    /// - `Creation`: replaying it fails with `ReplayError::Creation`, which [`verify_record`]
    ///   reports as `Replay(Creation)`.
    pub record: GameRecord,
    /// What went wrong.
    pub kind: FailureKind,
}

fn failure(record: GameRecord, kind: FailureKind) -> Box<SelfPlayFailure> {
    Box::new(SelfPlayFailure {
        seed: record.seed(),
        action_index: record.actions().len().checked_sub(1),
        record,
        kind,
    })
}

/// Plays one Game of `ruleset` from `decks` (`decks[i]` for seat `i`) and `seed`, until it has a
/// Terminal Outcome. Each turn the policy picks one of the Legal Actions in Listing Order,
/// uniformly, with a generator of its own derived from `seed` (see [`POLICY_STREAM`]).
///
/// It fails, with a typed [`SelfPlayFailure`] and never a panic, if the invariants do not hold
/// (when `checks` is [`Checks::On`]), if the Game has not ended after `max_steps` accepted
/// Actions, if no Legal Action is left in a Game that is on, if a listed Action is rejected, or if
/// the Game cannot be created. A Game that ends on its `max_steps`-th Action is not a failure.
pub fn self_play(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    seed: Seed,
    max_steps: usize,
    checks: Checks,
) -> Result<SelfPlayGame, Box<SelfPlayFailure>> {
    let checker = match checks {
        Checks::On => Some(&REAL_CHECKS),
        Checks::EngineOnly => Some(&ENGINE_ONLY_CHECKS),
        Checks::Off => None,
    };
    play(ruleset, decks, seed, max_steps, checker)
}

/// The invariant checks that self-play and verification run. The public functions pass
/// [`REAL_CHECKS`]; the unit tests of this module pass checks that report a violation they choose,
/// because the engine cannot be made to violate an invariant through its public API.
pub(crate) struct Checker<'a> {
    pub(crate) opening: &'a OpeningCheck<'a>,
    pub(crate) transition: &'a TransitionCheck<'a>,
    pub(crate) recomputation: &'a RecomputationCheck<'a>,
    pub(crate) rejection: &'a RejectionCheck<'a>,
    /// The three checks over Observations. A `None` is not run, and when all three are `None` the
    /// harness observes nothing (`Checks::EngineOnly`).
    pub(crate) consistency: Option<&'a ConsistencyCheck<'a>>,
    pub(crate) hidden_cards: Option<&'a HiddenCardsCheck<'a>>,
    pub(crate) observed_actions: Option<&'a ObservedActionsCheck<'a>>,
}

/// The shape of [`check_opening`].
pub(crate) type OpeningCheck<'a> =
    dyn Fn(&DeclarativeRuleset, &[DeckList], &GameCreation) -> Vec<InvariantViolation> + 'a;

/// The shape of [`check_transition`].
pub(crate) type TransitionCheck<'a> = dyn Fn(&DeclarativeRuleset, &GameState, &EventTrace<Event>, &GameState) -> Vec<InvariantViolation>
    + 'a;

/// The shape of [`check_recomputation`].
pub(crate) type RecomputationCheck<'a> =
    dyn Fn(&GameState, &GameState) -> Vec<InvariantViolation> + 'a;

/// The shape of [`check_rejection`].
pub(crate) type RejectionCheck<'a> =
    dyn Fn(&DeclarativeRuleset, &GameState, &Action, &Rejection) -> Vec<InvariantViolation> + 'a;

/// The shape of [`check_observed_consistency`].
pub(crate) type ConsistencyCheck<'a> = dyn Fn(Viewer, &ObservedGameState, &[ObservedEvent], &ObservedGameState) -> Vec<InvariantViolation>
    + 'a;

/// The shape of [`check_hidden_cards_ignored`]: the real Observed Step, and the Relabelled Game
/// with the GameState it was folded to and the relabelled Observed Step.
pub(crate) type HiddenCardsCheck<'a> =
    dyn Fn(&ObservedStep, &ReplayedStep) -> Vec<InvariantViolation> + 'a;

/// The shape of [`check_observed_actions`].
pub(crate) type ObservedActionsCheck<'a> =
    dyn Fn(&DeclarativeRuleset, &Observer, &GameState, &[Action]) -> Vec<InvariantViolation> + 'a;

pub(crate) const REAL_CHECKS: Checker<'static> = Checker {
    opening: &check_opening,
    transition: &check_transition,
    recomputation: &check_recomputation,
    rejection: &check_rejection,
    consistency: Some(&check_observed_consistency),
    hidden_cards: Some(&|observed, relabelled| {
        check_hidden_cards_ignored(observed, &relabelled.step)
    }),
    observed_actions: Some(&check_observed_actions),
};

/// The nine engine Invariants only: the checks over Observations are not run.
pub(crate) const ENGINE_ONLY_CHECKS: Checker<'static> = Checker {
    consistency: None,
    hidden_cards: None,
    observed_actions: None,
    ..REAL_CHECKS
};

/// The Recomputed GameState: the Empty GameState with every Event of the Game so far applied, in
/// order. It is never started from a GameState the engine carried. A fold that cannot be applied
/// is itself a violation of `DerivedReadsDoNotLeak`, not a panic.
fn fold(recomputed: &GameState, events: &[Event]) -> Result<GameState, Vec<InvariantViolation>> {
    recomputed.apply_trace(events).map_err(|error| {
        vec![InvariantViolation {
            invariant: Invariant::DerivedReadsDoNotLeak,
            viewer: None,
            detail: format!("the Events of the Game do not apply to the Empty GameState: {error}"),
        }]
    })
}

/// The Opening Position's Recomputed GameState, the Viewers' Observers, and the violations of the
/// Opening Position: the opening check's, then the recomputation's, then the checks over
/// Observations.
fn open(
    checker: &Checker<'_>,
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    creation: &GameCreation,
    seed: Seed,
) -> (GameState, Option<Observing>, Vec<InvariantViolation>) {
    let mut violations = (checker.opening)(ruleset, decks, creation);
    let recomputed = match fold(&creation.empty, creation.opening.trace.events()) {
        Ok(recomputed) => {
            violations.extend((checker.recomputation)(
                &recomputed,
                &creation.opening.state,
            ));
            recomputed
        }
        Err(found) => {
            violations.extend(found);
            creation.empty.clone()
        }
    };
    let (observing, observed) = Observing::open(checker, ruleset, seed, creation);
    violations.extend(observed);
    (recomputed, observing, violations)
}

/// The violations of one accepted transition, and the Recomputed GameState after it: the
/// transition check's, then the recomputation's, then the checks over Observations (consistency
/// for each Viewer, then the leak check for each Viewer).
fn step_checks(
    checker: &Checker<'_>,
    ruleset: &DeclarativeRuleset,
    prior: &GameState,
    action: &Action,
    transition: &Transition<GameState, Event>,
    recomputed: &GameState,
    observing: Option<&mut Observing>,
) -> (GameState, Vec<InvariantViolation>) {
    let (trace, next) = (&transition.trace, &transition.state);
    let mut violations = (checker.transition)(ruleset, prior, trace, next);
    let folded = match fold(recomputed, trace.events()) {
        Ok(folded) => {
            violations.extend((checker.recomputation)(&folded, next));
            folded
        }
        Err(found) => {
            violations.extend(found);
            recomputed.clone()
        }
    };
    if let Some(observing) = observing {
        violations.extend(observing.step(checker, ruleset, prior, action, transition));
    }
    (folded, violations)
}

/// What the policy draws from in one GameState, and what the listing refused.
struct Listing {
    /// The Legal Actions, in Listing Order.
    legal: Vec<Action>,
    /// The candidates the Trigger Cap refused, in Listing Order, with their Rejection.
    cap_rejections: Vec<(Action, Rejection)>,
}

/// Lists `state`. With `keep_rejections` (checks on) it uses `listing_with_rejections`, whose
/// accepted subsequence is exactly `legal_actions`, so the Game played is the same with or without
/// it; without, it is `legal_actions` itself and refused candidates are not seen.
fn list(ruleset: &DeclarativeRuleset, state: &GameState, keep_rejections: bool) -> Listing {
    if !keep_rejections {
        return Listing {
            legal: ruleset.legal_actions(state),
            cap_rejections: Vec::new(),
        };
    }
    let mut legal = Vec::new();
    let mut cap_rejections = Vec::new();
    for (action, rejection) in ruleset.listing_with_rejections(state) {
        match rejection {
            None => legal.push(action),
            Some(rejection @ Rejection::TriggerCapExceeded { .. }) => {
                cap_rejections.push((action, rejection));
            }
            Some(_) => {}
        }
    }
    Listing {
        legal,
        cap_rejections,
    }
}

/// Self-play with `checker`, or without invariant checks for `None`.
fn play(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    seed: Seed,
    max_steps: usize,
    checker: Option<&Checker<'_>>,
) -> Result<SelfPlayGame, Box<SelfPlayFailure>> {
    let mut record = GameRecord::new(ruleset, decks, seed);
    let creation = match ruleset.create_game(decks, seed) {
        Ok(creation) => creation,
        Err(error) => return Err(failure(record, FailureKind::Creation(error))),
    };
    let mut recomputed = None;
    let mut observing = None;
    if let Some(checker) = checker {
        let (folded, viewers, violations) = open(checker, ruleset, decks, &creation, seed);
        if !violations.is_empty() {
            return Err(failure(record, FailureKind::InvariantViolated(violations)));
        }
        recomputed = Some(folded);
        observing = viewers;
    }

    let mut policy = RandomState::from_seed(Seed(seed.0 ^ POLICY_STREAM));
    let mut state = creation.opening.state.clone();
    let mut traces = Vec::new();
    let mut trigger_cap_rejections = 0_usize;
    while state.outcome().is_none() {
        if record.actions().len() >= max_steps {
            return Err(failure(record, FailureKind::StepCapReached { max_steps }));
        }
        let Listing {
            legal,
            cap_rejections,
        } = list(ruleset, &state, checker.is_some());
        if let Some(checker) = checker {
            for (action, rejection) in &cap_rejections {
                trigger_cap_rejections = trigger_cap_rejections.saturating_add(1);
                let violations = (checker.rejection)(ruleset, &state, action, rejection);
                if !violations.is_empty() {
                    return Err(failure(record, FailureKind::InvariantViolated(violations)));
                }
            }
            if let Some(observing) = &observing {
                let violations = observing.round_trip(checker, ruleset, &state, &legal);
                if !violations.is_empty() {
                    return Err(failure(record, FailureKind::InvariantViolated(violations)));
                }
            }
        }
        let chosen = u64::try_from(legal.len())
            .ok()
            .and_then(NonZeroU64::new)
            .and_then(|bound| usize::try_from(policy.next_below(bound)).ok())
            .and_then(|index| legal.into_iter().nth(index));
        let Some(action) = chosen else {
            return Err(failure(record, FailureKind::NoLegalAction));
        };
        let transition = match ruleset.transition(&state, &action) {
            Ok(transition) => transition,
            Err(rejection) => {
                return Err(failure(
                    record,
                    FailureKind::LegalActionRejected { action, rejection },
                ));
            }
        };
        record.push_accepted(action.clone());
        if let (Some(checker), Some(folded)) = (checker, recomputed.as_ref()) {
            let (next, violations) = step_checks(
                checker,
                ruleset,
                &state,
                &action,
                &transition,
                folded,
                observing.as_mut(),
            );
            if !violations.is_empty() {
                return Err(failure(record, FailureKind::InvariantViolated(violations)));
            }
            recomputed = Some(next);
        }
        traces.push(transition.trace);
        state = transition.state;
    }
    // The ended GameState has no Legal Action, and the Viewers' Observed Legal Actions agree.
    if let (Some(checker), Some(observing)) = (checker, &observing) {
        let violations =
            observing.round_trip(checker, ruleset, &state, &ruleset.legal_actions(&state));
        if !violations.is_empty() {
            return Err(failure(record, FailureKind::InvariantViolated(violations)));
        }
    }
    Ok(SelfPlayGame {
        record,
        creation,
        traces,
        state,
        trigger_cap_rejections: if checker.is_some() {
            TriggerCapRejections::Counted(trigger_cap_rejections)
        } else {
            TriggerCapRejections::NotMeasured
        },
    })
}

/// Replays `record` and checks every invariant at the Opening Position and after each Action.
///
/// It walks the record once, as `DeclarativeRuleset::replay` does, running each Action a single
/// time, and the first failing step is the failure. An identity mismatch, a failed creation or a
/// rejected Action is a [`FailureKind::Replay`], with the whole record. An invariant violation
/// comes with the record cut after the Action that caused it, so verifying that record fails
/// the same way. Other kinds of failure are reproduced as described on
/// [`SelfPlayFailure::record`].
pub fn verify_record(
    ruleset: &DeclarativeRuleset,
    record: &GameRecord,
) -> Result<Replay, Box<SelfPlayFailure>> {
    verify(ruleset, record, &REAL_CHECKS)
}

/// [`verify_record`] with `checker`.
fn verify(
    ruleset: &DeclarativeRuleset,
    record: &GameRecord,
    checker: &Checker<'_>,
) -> Result<Replay, Box<SelfPlayFailure>> {
    // One walk: each Action runs once, and its transition is both checked and kept.
    let replay_failure = |accepted: usize, error: ReplayError| {
        Box::new(SelfPlayFailure {
            seed: record.seed(),
            action_index: accepted.checked_sub(1),
            record: record.clone(),
            kind: FailureKind::Replay(error),
        })
    };
    ruleset
        .check_record_identities(record)
        .map_err(|error| replay_failure(0, error))?;
    let creation = ruleset
        .create_game(record.decks(), record.seed())
        .map_err(|error| replay_failure(0, ReplayError::Creation(error)))?;

    let (mut recomputed, mut observing, violations) =
        open(checker, ruleset, record.decks(), &creation, record.seed());
    if !violations.is_empty() {
        return Err(cut(ruleset, record, 0, violations));
    }
    let mut state = creation.opening.state.clone();
    if let Some(violations) = verify_round_trip(checker, ruleset, observing.as_ref(), &state) {
        return Err(cut(ruleset, record, 0, violations));
    }
    let mut traces = Vec::with_capacity(record.actions().len());
    for (index, action) in record.actions().iter().enumerate() {
        let transition = ruleset.transition(&state, action).map_err(|rejection| {
            replay_failure(index, ReplayError::ActionRejected { index, rejection })
        })?;
        let (folded, violations) = step_checks(
            checker,
            ruleset,
            &state,
            action,
            &transition,
            &recomputed,
            observing.as_mut(),
        );
        if !violations.is_empty() {
            return Err(cut(ruleset, record, index.saturating_add(1), violations));
        }
        recomputed = folded;
        state = transition.state;
        traces.push(transition.trace);
        if let Some(violations) = verify_round_trip(checker, ruleset, observing.as_ref(), &state) {
            return Err(cut(ruleset, record, index.saturating_add(1), violations));
        }
    }
    Ok(Replay {
        creation,
        traces,
        state,
    })
}

/// The round-trip violations of `state` when Viewers are observed; it lists `state` itself, once,
/// as the policy does in self-play. `None` when there are none, or no Viewers.
fn verify_round_trip(
    checker: &Checker<'_>,
    ruleset: &DeclarativeRuleset,
    observing: Option<&Observing>,
    state: &GameState,
) -> Option<Vec<InvariantViolation>> {
    let violations = observing?.round_trip(checker, ruleset, state, &ruleset.legal_actions(state));
    (!violations.is_empty()).then_some(violations)
}

/// The failure of an invariant violation after the first `actions` Actions of `record`.
fn cut(
    ruleset: &DeclarativeRuleset,
    record: &GameRecord,
    actions: usize,
    violations: Vec<InvariantViolation>,
) -> Box<SelfPlayFailure> {
    let mut prefix = GameRecord::new(ruleset, record.decks(), record.seed());
    for action in record.actions().iter().take(actions) {
        prefix.push_accepted(action.clone());
    }
    failure(prefix, FailureKind::InvariantViolated(violations))
}

#[cfg(test)]
mod observation_tests;
#[cfg(test)]
mod tests;
