//! Acceptance tests for ticket #75, observing a pending decision (ADR-0033, ADR-0028 to ADR-0030):
//! every Viewer sees that a decision is pending, its Deciding Player and its kind, and never the
//! Paused Resolution; an Answer is observed as its option index and is acted from an Observation
//! like any other Action.
//!
//! The Effects Ruleset and its real catalog (`curious_owl`), plus `meddler`, a test-only card whose
//! opponent decides, appended to the catalog text. Games are driven Event by Event through one
//! Observer per Viewer, so an arrangement is made of Events both Observers follow.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Answer, Argument, CardInstanceId, DecisionKind, DeclarativeRuleset, Event, GameRecord,
    GameState, Loss, ObservationError, ObservedAction, ObservedAnswer, ObservedApplyError,
    ObservedEvent, ObservedGameState, ObservedPendingDecision, ObservedTransition, Observer,
    TakenAction, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, PhaseId, WinConditionId, ZoneId};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");

/// The controller's opponent decides whether the controller draws. Costs 0.
const MEDDLER: &str = "
  - id: meddler
    type: creature
    fields: { name: \"Meddler\", cost: 0, power: 1 }
    effects:
      - on: { enter: field }
        do: [ { choose_one: { decider: opponent, options: [ [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ], [] ] } } ]
";

/// Four of each of the two cards and then real creatures, 50 in all.
fn deck_yaml(leader: &str) -> String {
    format!(
        "inputs:
  - input: leader
    cards:
      - {{ card: {leader}, count: 1 }}
  - input: main_deck
    cards:
      - {{ card: curious_owl,   count: 4 }}
      - {{ card: meddler,       count: 4 }}
      - {{ card: goblin_scout,  count: 4 }}
      - {{ card: moss_sprite,   count: 4 }}
      - {{ card: ember_hound,   count: 4 }}
      - {{ card: river_otter,   count: 4 }}
      - {{ card: stone_golem,   count: 4 }}
      - {{ card: shadow_cat,    count: 4 }}
      - {{ card: iron_guard,    count: 4 }}
      - {{ card: wind_hawk,     count: 4 }}
      - {{ card: thorn_boar,    count: 4 }}
      - {{ card: frost_wolf,    count: 4 }}
      - {{ card: ash_drake,     count: 2 }}
  - input: mana_deck
    cards:
      - {{ card: mana, count: 10 }}
"
    )
}

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(p(seat))
}

fn play(player: u32, card: CardInstanceId) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new("play_creature"),
        arguments: vec![Argument::Card(card)],
    }
}

fn answer(player: u32, option: u32) -> Action {
    Action::Answer {
        player: p(player),
        answer: Answer::Option(option),
    }
}

fn pending(decider: u32) -> ObservedPendingDecision {
    ObservedPendingDecision {
        decider: p(decider),
        kind: DecisionKind::Option,
    }
}

fn ruleset() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(EFFECTS_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, &format!("{EFFECTS_CATALOG}{MEDDLER}"))
        .expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    ["captain_ardel", "warden_ilse"]
        .iter()
        .map(|leader| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), &deck_yaml(leader))
                .expect("the test deck is legal")
        })
        .collect()
}

/// A Game followed by one Observer per Viewer, in seat order.
struct Game {
    ruleset: DeclarativeRuleset,
    state: GameState,
    observers: Vec<Observer>,
    observed: Vec<ObservedGameState>,
}

impl Game {
    /// Seed 1's opening, followed by both Viewers.
    fn new() -> Game {
        let ruleset = ruleset();
        let creation = ruleset
            .create_game(&decks(&ruleset), Seed(1))
            .expect("the test decks create a game");
        let mut observers = Vec::new();
        let mut observed = Vec::new();
        for seat in 0..2 {
            let mut observer = ruleset
                .observer(viewer(seat), &creation.empty, &[])
                .expect("the Observer is built");
            ruleset
                .follow_events(
                    &mut observer,
                    &creation.empty,
                    creation.opening.trace.events(),
                )
                .expect("the creation trace is followed");
            observed.push(
                ruleset
                    .observe(&observer, &creation.opening.state)
                    .expect("the Opening Position is observed"),
            );
            observers.push(observer);
        }
        Game {
            ruleset,
            state: creation.opening.state,
            observers,
            observed,
        }
    }

    /// The first card of `player` made from `id`.
    fn card(&self, player: u32, id: &str) -> CardInstanceId {
        self.state
            .cards()
            .iter()
            .find(|card| card.owner() == p(player) && card.definition().as_str() == id)
            .unwrap_or_else(|| panic!("player {player} has no `{id}`"))
            .id()
    }

    /// Moves the card to the hand of its owner with an Event both Viewers follow: an arrangement.
    fn put_in_hand(&mut self, card: CardInstanceId) {
        let owner = self.state.card(card).expect("the card exists").owner();
        let from = self
            .state
            .zone_of(card)
            .expect("the card has a zone")
            .clone();
        if from == zone("hand") {
            return;
        }
        let events = [Event::CardMoved {
            card,
            player: owner,
            from,
            to: zone("hand"),
        }];
        for (observer, observed) in self.observers.iter_mut().zip(&mut self.observed) {
            self.ruleset
                .follow_events(observer, &self.state, &events)
                .expect("the arrangement is followed");
            let next = self
                .state
                .apply_trace(&events)
                .expect("the arrangement applies");
            *observed = self
                .ruleset
                .observe(observer, &next)
                .expect("the arranged GameState is observed");
        }
        self.state = self
            .state
            .apply_trace(&events)
            .expect("the arrangement applies");
    }

    /// Takes `action`: it must be accepted. Returns what each Viewer observed.
    fn take(&mut self, action: &Action) -> (Vec<Event>, Vec<ObservedTransition>) {
        let transition = self
            .ruleset
            .transition(&self.state, action)
            .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"));
        let seen: Vec<ObservedTransition> = self
            .observers
            .iter_mut()
            .map(|observer| {
                self.ruleset
                    .follow_transition(observer, &self.state, action, &transition)
                    .expect("the transition is followed")
            })
            .collect();
        for (observed, seen) in self.observed.iter_mut().zip(&seen) {
            observed.clone_from(&seen.state);
        }
        let events = transition.trace.into_events();
        self.state = transition.state;
        (events, seen)
    }
}

/// A Game in which Player 0 has played the owl and the Resolution is paused.
fn owl_paused() -> (Game, Vec<ObservedTransition>) {
    let mut game = Game::new();
    let owl = game.card(0, "curious_owl");
    game.put_in_hand(owl);
    let (_, seen) = game.take(&play(0, owl));
    (game, seen)
}

/// A Game in which Player 0 has played the meddler and Player 1 is asked.
fn meddler_paused() -> (Game, Vec<ObservedTransition>) {
    let mut game = Game::new();
    let meddler = game.card(0, "meddler");
    game.put_in_hand(meddler);
    let (_, seen) = game.take(&play(0, meddler));
    (game, seen)
}

// ---- what a Viewer sees

#[test]
fn every_viewer_sees_a_pending_decision_and_its_decider() {
    let before = Game::new();
    for observed in &before.observed {
        assert_eq!(observed.pending_decision(), None);
        assert_eq!(observed.deciding_player(), p(0));
    }

    // The owl asks its controller.
    let (game, seen) = owl_paused();
    for (seat, seen) in seen.iter().enumerate() {
        assert_eq!(
            seen.state.pending_decision(),
            Some(&pending(0)),
            "viewer {seat}"
        );
        assert_eq!(seen.state.deciding_player(), p(0), "viewer {seat}");
        assert_eq!(
            seen.events.last(),
            Some(&ObservedEvent::DecisionPending {
                decider: p(0),
                kind: DecisionKind::Option
            }),
            "viewer {seat}"
        );
        // The Observed GameState is the one the Observer observes of the real GameState.
        assert_eq!(
            game.ruleset
                .observe(&game.observers[seat], &game.state)
                .as_ref(),
            Ok(&seen.state)
        );
    }

    // The meddler asks its controller's opponent, and both Viewers see who.
    let (game, seen) = meddler_paused();
    for (seat, seen) in seen.iter().enumerate() {
        assert_eq!(
            seen.state.pending_decision(),
            Some(&pending(1)),
            "viewer {seat}"
        );
        assert_eq!(seen.state.deciding_player(), p(1), "viewer {seat}");
        assert_eq!(seen.state.active_player(), p(0), "viewer {seat}");
    }
    assert_eq!(game.state.deciding_player(), p(1));

    // An Answer clears it, for both.
    let mut game = game;
    let (_, seen) = game.take(&answer(1, 1));
    for seen in &seen {
        assert_eq!(seen.state.pending_decision(), None);
        assert_eq!(seen.state.deciding_player(), p(0));
    }
}

#[test]
fn no_observation_carries_the_paused_resolution() {
    let (mut game, seen) = owl_paused();
    let (_, answered) = game.take(&answer(0, 0));

    // The Observed Event is exactly the decider and the kind: it binds no other field.
    for seen in &seen {
        let Some(ObservedEvent::DecisionPending { decider, kind }) = seen.events.last() else {
            panic!("the transition ends with DecisionPending");
        };
        assert_eq!((*decider, *kind), (p(0), DecisionKind::Option));
    }
    // Nothing observed names the Paused Resolution, its queue or the Events it carries.
    let owl = game.card(0, "curious_owl");
    for seen in seen.iter().chain(&answered) {
        let text = format!("{seen:?}");
        for hidden in [
            "paused",
            "Paused",
            "QueuedTrigger",
            "unscanned",
            "EffectPath",
        ] {
            assert!(!text.contains(hidden), "{hidden} in {text}");
        }
        // The Card Instance Id of the owl is never shown either.
        assert!(
            !text.contains(&format!("CardInstanceId({})", owl.0)),
            "{text}"
        );
    }
    for observed in &game.observed {
        let text = format!("{observed:?}");
        assert!(
            !text.contains("paused") && !text.contains("Paused"),
            "{text}"
        );
    }
}

#[test]
fn observed_answers_round_trip() {
    let (mut game, _) = owl_paused();

    // The deciding Viewer lists the Answers; the other lists none.
    let listed: Vec<Vec<ObservedAction>> = game
        .observers
        .iter()
        .map(|observer| {
            game.ruleset
                .observed_legal_actions(observer, &game.state)
                .expect("the Observed Legal Actions are listed")
        })
        .collect();
    let answers = vec![
        ObservedAction::Answer {
            player: p(0),
            answer: ObservedAnswer::Option(0),
        },
        ObservedAction::Answer {
            player: p(0),
            answer: ObservedAnswer::Option(1),
        },
    ];
    assert_eq!(listed[0], answers);
    assert!(listed[1].is_empty());

    // Translating them back gives the engine's Legal Actions, in the same order.
    let translated: Vec<Action> = listed[0]
        .iter()
        .map(|observed| {
            game.ruleset
                .translate_action(&game.observers[0], observed)
                .expect("the Observed Action translates")
        })
        .collect();
    assert_eq!(translated, game.ruleset.legal_actions(&game.state));
    assert_eq!(translated, [answer(0, 0), answer(0, 1)]);

    // Another Viewer's Answer is not translated.
    assert_eq!(
        game.ruleset
            .translate_action(&game.observers[1], &answers[0]),
        Err(ObservationError::ActionOfAnotherPlayer { player: p(0) })
    );

    // Taking the translated Answer: the Taken Action and the first Observed Event show the index.
    let (events, seen) = game.take(&translated[0]);
    assert_eq!(
        events[0],
        Event::DecisionAnswered {
            player: p(0),
            answer: Answer::Option(0)
        }
    );
    for seen in &seen {
        assert_eq!(
            seen.action,
            TakenAction::Answer {
                player: p(0),
                answer: ObservedAnswer::Option(0)
            }
        );
        assert_eq!(
            seen.events.first(),
            Some(&ObservedEvent::DecisionAnswered {
                player: p(0),
                answer: ObservedAnswer::Option(0)
            })
        );
    }
}

#[test]
fn the_opponent_lists_and_takes_the_answers_of_an_opponent_decider() {
    let (mut game, _) = meddler_paused();
    let listed: Vec<Vec<ObservedAction>> = game
        .observers
        .iter()
        .map(|observer| {
            game.ruleset
                .observed_legal_actions(observer, &game.state)
                .expect("the Observed Legal Actions are listed")
        })
        .collect();
    assert!(listed[0].is_empty());
    assert_eq!(
        listed[1],
        [
            ObservedAction::Answer {
                player: p(1),
                answer: ObservedAnswer::Option(0)
            },
            ObservedAction::Answer {
                player: p(1),
                answer: ObservedAnswer::Option(1)
            }
        ]
    );
    let chosen = game
        .ruleset
        .translate_action(&game.observers[1], &listed[1][1])
        .expect("the Observed Action translates");
    assert_eq!(chosen, answer(1, 1));
    let (_, seen) = game.take(&chosen);
    assert_eq!(
        seen[0].action,
        TakenAction::Answer {
            player: p(1),
            answer: ObservedAnswer::Option(1)
        }
    );
}

#[test]
fn a_taken_declared_action_is_still_observed_as_declared() {
    let (_, seen) = owl_paused();
    let TakenAction::Declared {
        player,
        action,
        arguments,
    } = &seen[0].action
    else {
        panic!("a play is a Declared Action");
    };
    assert_eq!(
        (*player, action.as_str(), arguments.len()),
        (p(0), "play_creature", 1)
    );
}

// ---- the fold of Observed Events

#[test]
fn the_observed_fold_stores_and_clears_the_pending_decision() {
    let before = Game::new();
    let mut game = Game::new();
    let owl = game.card(0, "curious_owl");
    game.put_in_hand(owl);
    let arranged = game.observed[0].clone();
    let (_, seen) = game.take(&play(0, owl));
    let (_, after) = game.take(&answer(0, 1));

    // Applying the Observed Events of the pausing transition gives its Observed GameState: the
    // fold stores the Pending Decision.
    assert_eq!(
        arranged.apply_events(&seen[0].events),
        Ok(seen[0].state.clone())
    );
    // And the Answer clears it.
    assert_eq!(
        seen[0].state.apply_events(&after[0].events),
        Ok(after[0].state.clone())
    );
    assert_eq!(after[0].state.pending_decision(), None);
    assert_ne!(before.observed[0], seen[0].state);
}

fn pending_event(decider: u32) -> ObservedEvent {
    ObservedEvent::DecisionPending {
        decider: p(decider),
        kind: DecisionKind::Option,
    }
}

fn answered_event(player: u32) -> ObservedEvent {
    ObservedEvent::DecisionAnswered {
        player: p(player),
        answer: ObservedAnswer::Option(0),
    }
}

#[test]
fn the_observed_fold_rejects_a_misplaced_decision_event() {
    let idle = Game::new().observed[0].clone();
    let (_, seen) = owl_paused();
    let paused = seen[0].state.clone();
    let misfit = Err(ObservedApplyError::EventDoesNotFit);

    let try_apply = |state: &ObservedGameState, event: &ObservedEvent| {
        let mut working = state.clone();
        let result = working.apply(event);
        // Atomic: on an error the state is unchanged.
        if result.is_err() {
            assert_eq!(&working, state, "{event:?}");
        }
        result
    };

    // A decision pending while one is: no. A decider who is not in the Game: no.
    assert_eq!(try_apply(&paused, &pending_event(0)), misfit);
    assert_eq!(try_apply(&idle, &pending_event(9)), misfit);
    // An Answer with nothing pending, or from another Player than the decider: no.
    assert_eq!(try_apply(&idle, &answered_event(0)), misfit);
    assert_eq!(try_apply(&paused, &answered_event(1)), misfit);
    // Any other Observed Event while a decision is pending: no.
    assert_eq!(
        try_apply(
            &paused,
            &ObservedEvent::PhaseBegan {
                phase: PhaseId::new("main")
            }
        ),
        misfit
    );
    // The right ones apply.
    assert!(try_apply(&idle, &pending_event(0)).is_ok());
    assert!(try_apply(&paused, &answered_event(0)).is_ok());

    // `GameEnded` applies while a decision is pending, and clears it.
    let mut ended = paused.clone();
    ended
        .apply(&ObservedEvent::GameEnded {
            losses: vec![Loss {
                player: p(1),
                reason: WinConditionId::new("deck_out"),
            }],
        })
        .expect("the Game can end while a decision is pending");
    assert_eq!(ended.pending_decision(), None);
    assert!(ended.outcome().is_some());
    assert_eq!(
        ended.apply(&answered_event(0)),
        Err(ObservedApplyError::GameAlreadyEnded)
    );
}

#[test]
fn a_record_with_answers_is_followed_by_every_viewer() {
    // A scripted Game, played from the Seed alone, with an owl pause and its Answer.
    let ruleset = ruleset();
    let decks = decks(&ruleset);
    let (seed, owl) = (0..2000)
        .find_map(|seed| {
            let opening = ruleset
                .create_game(&decks, Seed(seed))
                .expect("the decks create a game")
                .opening
                .state;
            opening
                .zone(p(0), &zone("hand"))?
                .iter()
                .copied()
                .find(|&card| {
                    opening
                        .card(card)
                        .is_some_and(|c| c.definition().as_str() == "curious_owl")
                })
                .map(|card| (seed, card))
        })
        .expect("some Seed deals the owl to Player 0");
    let creation = ruleset
        .create_game(&decks, Seed(seed))
        .expect("the decks create a game");
    let mut record = GameRecord::new(&ruleset, &decks, Seed(seed));
    let mut observers: Vec<Observer> = (0..2)
        .map(|seat| {
            let mut observer = ruleset
                .observer(viewer(seat), &creation.empty, &[])
                .expect("the Observer is built");
            ruleset
                .follow_events(
                    &mut observer,
                    &creation.empty,
                    creation.opening.trace.events(),
                )
                .expect("the creation trace is followed");
            observer
        })
        .collect();
    let mut state = creation.opening.state;
    for action in [play(0, owl), answer(0, 0)] {
        let transition = ruleset
            .transition(&state, &action)
            .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"));
        for observer in &mut observers {
            ruleset
                .follow_transition(observer, &state, &action, &transition)
                .expect("every Viewer follows the transition");
        }
        record.push_accepted(action);
        state = transition.state;
    }
    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(replay.state, state);
    assert_eq!(state.pending_decision(), None);
}
