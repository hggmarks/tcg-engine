//! What the harness keeps to check Observations (ADR-0031): for every Viewer, in seat order, an
//! Observer and its Observed GameState, followed incrementally through the Game, and the one
//! generator of the Relabelling.
//!
//! Nothing here touches the GameState's Random State or the policy's generator: the Relabelling
//! draws from `RandomState::from_seed(Seed(seed.0 ^ RELABEL_STREAM))`, per Game, in a fixed order
//! (per step, the Viewers in seat order, then the draws of `relabel`), so verifying a record
//! reproduces the same Relabelling.

use tcg_engine::declarative::{
    Action, DeclarativeRuleset, Event, GameCreation, GameState, ObservationError, ObservedEvent,
    ObservedGameState, Observer, TakenAction, Viewer,
};
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset::{Observations, Transition};

use super::Checker;
use crate::invariants::{Invariant, InvariantViolation};
use crate::observations::{
    hidden_cards, observed_step, relabel, replay_observed_step, GameEvents, RELABEL_STREAM,
};

/// One Viewer followed through the Game. An error stops observing that Viewer for the rest of the
/// Game (`live` is `None`), as ADR-0026 stopped the fold of a failed Recomputed GameState.
struct Followed {
    viewer: Viewer,
    live: Option<Live>,
}

struct Live {
    observer: Observer,
    /// The Observed GameState after the last step.
    observed: ObservedGameState,
}

/// What one Viewer observed of one step.
struct Observed {
    previous: ObservedGameState,
    events: Vec<ObservedEvent>,
    taken: Option<TakenAction>,
    next: ObservedGameState,
}

type Outcome = Option<Result<Observed, ObservationError>>;

/// The Viewers of one Game and the Events they have followed.
pub(super) struct Observing {
    empty: GameState,
    viewers: Vec<Followed>,
    /// Every Event of the Game so far, the current step's trace included.
    events: GameEvents,
    relabelling: RandomState,
}

fn failed_to_observe(viewer: Viewer, error: &ObservationError) -> InvariantViolation {
    InvariantViolation {
        invariant: Invariant::ObservationsStayConsistent,
        viewer: Some(viewer),
        detail: format!("the Viewer cannot follow the Game: {error} ({error:?})"),
    }
}

impl Observing {
    /// The Viewers at the Opening Position, and the violations found there. `None` when the
    /// checker runs no check over Observations.
    pub(super) fn open(
        checker: &Checker<'_>,
        ruleset: &DeclarativeRuleset,
        seed: Seed,
        creation: &GameCreation,
    ) -> (Option<Observing>, Vec<InvariantViolation>) {
        if checker.consistency.is_none()
            && checker.hidden_cards.is_none()
            && checker.observed_actions.is_none()
        {
            return (None, Vec::new());
        }
        let mut observing = Observing {
            empty: creation.empty.clone(),
            viewers: Vec::new(),
            events: GameEvents {
                creation: creation.opening.trace.events().to_vec(),
                traces: Vec::new(),
            },
            relabelling: RandomState::from_seed(Seed(seed.0 ^ RELABEL_STREAM)),
        };
        let mut outcomes = Vec::new();
        for &player in creation.empty.player_order().players() {
            let viewer = Viewer::Player(player);
            let (followed, outcome) = match open_viewer(ruleset, viewer, creation) {
                Ok((live, observed)) => (
                    Followed {
                        viewer,
                        live: Some(live),
                    },
                    Some(Ok(observed)),
                ),
                Err(error) => (Followed { viewer, live: None }, Some(Err(error))),
            };
            observing.viewers.push(followed);
            outcomes.push(outcome);
        }
        let violations = observing.check(checker, ruleset, outcomes, &creation.opening.state, None);
        (Some(observing), violations)
    }

    /// One accepted transition: every Viewer follows it, then the checks run, consistency for the
    /// Viewers in seat order and then the leak check for the Viewers in seat order.
    pub(super) fn step(
        &mut self,
        checker: &Checker<'_>,
        ruleset: &DeclarativeRuleset,
        prior: &GameState,
        action: &Action,
        transition: &Transition<GameState, Event>,
    ) -> Vec<InvariantViolation> {
        self.events.traces.push(transition.trace.events().to_vec());
        let outcomes: Vec<Outcome> = self
            .viewers
            .iter_mut()
            .map(|followed| {
                let live = followed.live.as_mut()?;
                Some(
                    ruleset
                        .follow_transition(&mut live.observer, prior, action, transition)
                        .map(|followed| {
                            let previous =
                                std::mem::replace(&mut live.observed, followed.state.clone());
                            Observed {
                                previous,
                                events: followed.events,
                                taken: Some(followed.action),
                                next: followed.state,
                            }
                        }),
                )
            })
            .collect();
        self.check(checker, ruleset, outcomes, &transition.state, Some(action))
    }

    /// The round-trip check of every Viewer that is still followed, in seat order, on `state` and
    /// its listing `legal`.
    pub(super) fn round_trip(
        &self,
        checker: &Checker<'_>,
        ruleset: &DeclarativeRuleset,
        state: &GameState,
        legal: &[Action],
    ) -> Vec<InvariantViolation> {
        let Some(check) = checker.observed_actions else {
            return Vec::new();
        };
        self.viewers
            .iter()
            .filter_map(|followed| followed.live.as_ref())
            .flat_map(|live| check(ruleset, &live.observer, state, legal))
            .collect()
    }

    fn check(
        &mut self,
        checker: &Checker<'_>,
        ruleset: &DeclarativeRuleset,
        outcomes: Vec<Outcome>,
        state: &GameState,
        action: Option<&Action>,
    ) -> Vec<InvariantViolation> {
        let mut violations = Vec::new();

        // Observations stay consistent, Viewer by Viewer. Everything that can fail to build a
        // Viewer's Observed Step is built here, so that a failure is reported with this Invariant
        // and in seat order, before any violation of the leak check.
        let mut followed_now = Vec::new();
        for (followed, outcome) in self.viewers.iter_mut().zip(outcomes) {
            let Some(outcome) = outcome else { continue };
            let viewer = followed.viewer;
            let observed = match outcome {
                Ok(observed) => observed,
                Err(error) => {
                    violations.push(failed_to_observe(viewer, &error));
                    followed.live = None;
                    continue;
                }
            };
            if let Some(check) = checker.consistency {
                violations.extend(check(
                    viewer,
                    &observed.previous,
                    &observed.events,
                    &observed.next,
                ));
            }
            if checker.hidden_cards.is_none() {
                continue;
            }
            let Some(live) = &followed.live else { continue };
            match observed_step(
                ruleset,
                &live.observer,
                state,
                observed.events,
                observed.taken,
                observed.next,
            ) {
                Ok(step) => followed_now.push((viewer, step)),
                Err(error) => {
                    violations.push(failed_to_observe(viewer, &error));
                    followed.live = None;
                }
            }
        }

        // Observations ignore hidden cards, Viewer by Viewer: the real Observed Step against the
        // Observed Step of the Relabelled Game.
        let Some(check) = checker.hidden_cards else {
            return violations;
        };
        for (viewer, step) in followed_now {
            let hidden = hidden_cards(ruleset, viewer, &self.events);
            let relabelled = relabel(
                ruleset,
                viewer,
                &hidden,
                &self.events,
                action,
                &mut self.relabelling,
            );
            match replay_observed_step(
                ruleset,
                viewer,
                &self.empty,
                &relabelled.events,
                relabelled.action.as_ref(),
            ) {
                Ok(replayed) => violations.extend(check(&step, &replayed)),
                Err(detail) => violations.push(InvariantViolation {
                    invariant: Invariant::ObservationsIgnoreHiddenCards,
                    viewer: Some(viewer),
                    detail,
                }),
            }
        }
        violations
    }
}

/// The Observer of `viewer` at the Opening Position: a fresh Observer from the Empty GameState,
/// the creation trace followed, and what it observed.
fn open_viewer(
    ruleset: &DeclarativeRuleset,
    viewer: Viewer,
    creation: &GameCreation,
) -> Result<(Live, Observed), ObservationError> {
    let empty = &creation.empty;
    let mut observer = ruleset.observer(viewer, empty, &[])?;
    let previous = ruleset.observe(&observer, empty)?;
    let events = ruleset.follow_events(&mut observer, empty, creation.opening.trace.events())?;
    let next = ruleset.observe(&observer, &creation.opening.state)?;
    Ok((
        Live {
            observer,
            observed: next.clone(),
        },
        Observed {
            previous,
            events,
            taken: None,
            next,
        },
    ))
}
