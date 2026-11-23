//! Unit tests of the Hidden Cards, the Relabelling and the leak check (ADR-0031). They live in the
//! crate because the Relabelling takes the explicit set of cards to relabel, so that a test can
//! relabel a **seen** card, which is what a leak looks like, and see the check catch it.

use std::collections::BTreeSet;

use tcg_engine::card_catalog::CardId;
use tcg_engine::declarative::{Action, CardInstanceId, DeclarativeRuleset, Event};
use tcg_engine::player::PlayerId;
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset_definition::{ActionId, ZoneId};

use super::{
    check_hidden_cards_ignored, hidden_cards, relabel, replay_observed_step, GameEvents,
    ObservedStep,
};
use crate::invariants::Invariant;
use crate::test_support::{effects, independent_fold, player_state, viewer};
use crate::{self_play, Checks, SelfPlayGame};

const MAX_STEPS: usize = 1_000;

fn id(card: u32) -> CardInstanceId {
    CardInstanceId(card)
}

fn created(card: u32, owner: u32, zone: &str) -> Event {
    Event::CardCreated {
        card: id(card),
        owner: PlayerId(owner),
        definition: CardId::new("goblin_scout"),
        zone: ZoneId::new(zone),
        state: Vec::new(),
    }
}

fn moved(card: u32, player: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card: id(card),
        player: PlayerId(player),
        from: ZoneId::new(from),
        to: ZoneId::new(to),
    }
}

fn set(cards: &[u32]) -> BTreeSet<CardInstanceId> {
    cards.iter().copied().map(id).collect()
}

#[test]
fn hidden_cards_are_the_cards_never_in_a_zone_the_viewer_sees_after_any_event_of_the_game() {
    let (ruleset, _) = effects();
    // Effects zones: `deck` is hidden, `hand` is the owner's, `field` and `leader_zone` are public.
    let events = GameEvents {
        creation: vec![
            created(0, 0, "leader_zone"), // public: both Viewers see it
            created(1, 0, "deck"),
            created(2, 0, "deck"),
            created(3, 1, "deck"),
            created(4, 1, "hand"), // seat 1's hand: seen by seat 1 only
            created(5, 0, "field"),
        ],
        traces: vec![
            // Seat 0 draws card 1, and card 2 goes to its hand and back into the deck within one
            // trace: it was seen in the middle of the trace.
            vec![
                moved(1, 0, "deck", "hand"),
                moved(2, 0, "deck", "hand"),
                moved(2, 0, "hand", "deck"),
            ],
            // The card 5 leaves the field for the deck: it was seen, so it is not hidden.
            vec![moved(3, 1, "deck", "hand"), moved(5, 0, "field", "deck")],
        ],
    };
    assert_eq!(hidden_cards(&ruleset, viewer(0), &events), set(&[3, 4]));
    assert_eq!(hidden_cards(&ruleset, viewer(1), &events), set(&[1, 2]));

    // Before the transitions, only the creation trace counts.
    let creation_only = GameEvents {
        creation: events.creation.clone(),
        traces: Vec::new(),
    };
    assert_eq!(
        hidden_cards(&ruleset, viewer(0), &creation_only),
        set(&[1, 2, 3, 4])
    );
    assert_eq!(
        hidden_cards(&ruleset, viewer(1), &creation_only),
        set(&[1, 2, 3])
    );
}

/// The Events of `game` up to the step `step` (`None` is the Opening Position, `Some(i)` is the
/// transition of Action index `i`), and the Action of the step.
fn events_and_action(game: &SelfPlayGame, step: Option<usize>) -> (GameEvents, Option<&Action>) {
    let traces = match step {
        None => Vec::new(),
        Some(index) => game.traces[..=index]
            .iter()
            .map(|trace| trace.events().to_vec())
            .collect(),
    };
    let events = GameEvents {
        creation: game.creation.opening.trace.events().to_vec(),
        traces,
    };
    (events, step.map(|index| &game.record.actions()[index]))
}

/// The steps the leak tests look at: the Opening Position, the first transition and one in the
/// middle of the Game.
fn steps_under_test(game: &SelfPlayGame) -> Vec<Option<usize>> {
    vec![None, Some(0), Some(game.record.actions().len() / 2)]
}

fn played(ruleset: &DeclarativeRuleset, decks: &[tcg_engine::deck_list::DeckList]) -> SelfPlayGame {
    // The harness's own checks are not in play here: this test is about the check itself.
    self_play(ruleset, decks, Seed(3), MAX_STEPS, Checks::Off).expect("the Game is played")
}

/// The first part in which two Observed Steps differ, as the words the violation must contain.
fn first_difference(one: &ObservedStep, other: &ObservedStep) -> Option<String> {
    if let Some(index) = one
        .events
        .iter()
        .zip(&other.events)
        .position(|(a, b)| a != b)
    {
        return Some(format!("Observed Event at index {index}"));
    }
    if one.events.len() != other.events.len() {
        return Some("number of Observed Events".to_string());
    }
    if one.taken != other.taken {
        return Some("Taken Action".to_string());
    }
    if one.state != other.state {
        return Some("Observed GameState".to_string());
    }
    (one.derived_reads != other.derived_reads).then(|| "Derived Read".to_string())
}

#[test]
fn invariant_observations_ignore_hidden_cards_detects_a_violation() {
    let (ruleset, decks) = effects();
    let game = played(&ruleset, &decks);
    let empty = &game.creation.empty;
    let mut leaks_caught = 0;
    for seat in 0..2 {
        let viewer = viewer(seat);
        let player = PlayerId(seat);
        let folded = independent_fold(&ruleset, &game, viewer);
        for step in steps_under_test(&game) {
            let (events, action) = events_and_action(&game, step);
            let at = step.map_or(0, |index| index + 1);
            let observed = replay_observed_step(&ruleset, viewer, empty, &events, action)
                .expect("the Game replays from the Empty GameState");
            assert_eq!(
                observed.step.events, folded[at].events,
                "seat {seat}, step {step:?}"
            );
            let hidden = hidden_cards(&ruleset, viewer, &events);
            assert!(!hidden.is_empty(), "the Viewer cannot see every card");

            // Relabelling exactly the Hidden Cards changes nothing the Viewer observes: no
            // violation, with every stream of the Relabelling, and the Relabelling is not a no-op.
            let mut changed = false;
            for stream in 0..8 {
                let mut rng = RandomState::from_seed(Seed(stream));
                let relabelled = relabel(&ruleset, viewer, &hidden, &events, action, &mut rng);
                changed |= relabelled.events != events;
                let replayed = replay_observed_step(
                    &ruleset,
                    viewer,
                    empty,
                    &relabelled.events,
                    relabelled.action.as_ref(),
                )
                .expect("a Relabelling of the Hidden Cards applies");
                assert_eq!(
                    check_hidden_cards_ignored(&observed.step, &replayed.step),
                    Vec::new(),
                    "seat {seat}, step {step:?}, stream {stream}"
                );
            }
            assert!(changed, "relabelling the Hidden Cards rewrote nothing");

            // Relabelling nothing is the identity.
            let mut rng = RandomState::from_seed(Seed(0));
            let untouched = relabel(
                &ruleset,
                viewer,
                &BTreeSet::new(),
                &events,
                action,
                &mut rng,
            );
            assert_eq!(untouched.events, events);
            assert_eq!(untouched.action.as_ref(), action);

            // Relabelling cards the Viewer SEES (the creatures of its hand) is what a leak looks
            // like: the Viewer observes different cards. The check catches it, with one violation
            // that names the first part that differs.
            let seen: BTreeSet<CardInstanceId> = folded[at]
                .state
                .zone(player, &ZoneId::new("hand"))
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|card| {
                    folded[at]
                        .state
                        .card(*card)
                        .and_then(|instance| ruleset.catalog().card(instance.definition()))
                        .is_some_and(|definition| definition.card_type().as_str() == "creature")
                })
                .collect();
            let definitions: BTreeSet<&CardId> = seen
                .iter()
                .filter_map(|card| folded[at].state.card(*card))
                .map(|instance| instance.definition())
                .collect();
            if definitions.len() < 2 {
                assert_ne!(
                    step, None,
                    "the Opening Position hand holds two kinds of creature"
                );
                continue;
            }
            assert!(seen.is_disjoint(&hidden));
            let mut caught = false;
            for stream in 0..32 {
                let mut rng = RandomState::from_seed(Seed(stream));
                let relabelled = relabel(&ruleset, viewer, &seen, &events, action, &mut rng);
                let replayed = replay_observed_step(
                    &ruleset,
                    viewer,
                    empty,
                    &relabelled.events,
                    relabelled.action.as_ref(),
                )
                .expect("a Relabelling of creatures applies");
                let violations = check_hidden_cards_ignored(&observed.step, &replayed.step);
                match first_difference(&observed.step, &replayed.step) {
                    None => assert_eq!(violations, Vec::new()),
                    Some(part) => {
                        caught = true;
                        assert_eq!(violations.len(), 1, "stream {stream}: {violations:?}");
                        assert_eq!(
                            violations[0].invariant,
                            Invariant::ObservationsIgnoreHiddenCards
                        );
                        assert_eq!(violations[0].viewer, Some(viewer));
                        assert!(
                            violations[0].detail.contains(&part),
                            "the detail should name `{part}`: {}",
                            violations[0].detail
                        );
                    }
                }
            }
            assert!(
                caught,
                "seat {seat}, step {step:?}: no stream made the leak visible"
            );
            leaks_caught += 1;
        }
    }
    assert!(
        leaks_caught >= 2,
        "the leak was exercised {leaks_caught} times"
    );
}

#[test]
fn the_relabelling_of_one_stream_is_the_same_every_time_and_keeps_the_action_and_the_trace_in_step()
{
    let (ruleset, decks) = effects();
    let game = played(&ruleset, &decks);
    let folded = independent_fold(&ruleset, &game, viewer(0));

    // The first Action that plays a creature: its card is one of the Player's own, in its hand.
    let index = game
        .record
        .actions()
        .iter()
        .position(|action| {
            matches!(
                action,
                tcg_engine::declarative::Action::Declared { action, .. }
                    if *action == ActionId::new("play_creature")
            )
        })
        .expect("the Game plays a creature");
    let action = &game.record.actions()[index];
    let acting = action.player();
    let prior = &folded[index].state;
    let hand: BTreeSet<CardInstanceId> = prior
        .zone(acting, &ZoneId::new("hand"))
        .unwrap_or_default()
        .iter()
        .copied()
        .collect();
    let (events, _) = events_and_action(&game, Some(index));

    let relabel_with = |stream: u64| {
        let mut rng = RandomState::from_seed(Seed(stream));
        relabel(
            &ruleset,
            viewer(acting.0),
            &hand,
            &events,
            Some(action),
            &mut rng,
        )
    };
    assert_eq!(relabel_with(5), relabel_with(5));

    let played_card = {
        let tcg_engine::declarative::Action::Declared { arguments, .. } = action else {
            panic!("the play is a Declared Action");
        };
        let tcg_engine::declarative::Argument::Card(card) = arguments[0];
        card
    };
    let position = events.traces[index]
        .iter()
        .position(|event| matches!(event, Event::CardMoved { card, .. } if *card == played_card))
        .expect("the trace moves the played card");
    let mut moved_ids = false;
    for stream in 0..16 {
        let relabelled = relabel_with(stream);
        let relabelled_action = relabelled.action.as_ref().expect("the Action is kept");
        let tcg_engine::declarative::Action::Declared { arguments, .. } = relabelled_action else {
            panic!("the relabelled play is a Declared Action");
        };
        let tcg_engine::declarative::Argument::Card(argument) = arguments[0];
        // The Action's card is mapped as the trace's: the same position moves the same card.
        assert!(
            matches!(
                relabelled.events.traces[index][position],
                Event::CardMoved { card, .. } if card == argument
            ),
            "stream {stream}: the Action and its trace were relabelled apart"
        );
        moved_ids |= argument != played_card;
        // The relabelled Game applies, and holds as many cards.
        let replayed = replay_observed_step(
            &ruleset,
            viewer(acting.0),
            &game.creation.empty,
            &relabelled.events,
            relabelled.action.as_ref(),
        )
        .expect("the relabelled Game applies");
        assert_eq!(replayed.state.cards().len(), game.state.cards().len());
    }
    assert!(moved_ids, "no stream moved the played card's id");

    // Another stream gives another Relabelling.
    assert_ne!(relabel_with(1), relabel_with(2));
}

#[test]
fn the_relabelling_shuffles_only_the_hidden_positions_of_an_unseen_zones_shuffle() {
    let (ruleset, decks) = effects();
    let empty = ruleset
        .create_game(&decks, Seed(1))
        .expect("the sample decks create a game")
        .empty;

    // Seen from seat 0: seat 1's `deck` and `mana_deck` are hidden, its `field` is public. Cards 0
    // and 1 are hidden, created in two different zones, so each is alone in its class (the same
    // owner, the same creation zone) and the ids cannot move; both have one definition, so the
    // definitions cannot change either. Only the order of the shuffle of the unseen `deck`, whose
    // middle position holds the seen card 2, can change.
    let shuffle = |order: &[u32]| Event::ZoneShuffled {
        player: PlayerId(1),
        zone: ZoneId::new("deck"),
        order: order.iter().copied().map(id).collect(),
        random_state: RandomState::from_seed(Seed(1)),
    };
    let events = GameEvents {
        creation: vec![
            created(0, 1, "deck"),
            created(1, 1, "mana_deck"),
            created(2, 1, "field"),
            moved(1, 1, "mana_deck", "deck"),
            moved(2, 1, "field", "deck"),
            shuffle(&[0, 2, 1]),
        ],
        traces: Vec::new(),
    };
    let hidden = hidden_cards(&ruleset, viewer(0), &events);
    assert_eq!(hidden, set(&[0, 1]));
    assert!(empty.apply_trace(&events.creation).is_ok());

    let mut reordered = false;
    for stream in 0..32 {
        let mut rng = RandomState::from_seed(Seed(stream));
        let relabelled = relabel(&ruleset, viewer(0), &hidden, &events, None, &mut rng);
        // Nothing but the order of the shuffle changed.
        assert_eq!(relabelled.events.creation[..5], events.creation[..5]);
        let Event::ZoneShuffled { order, .. } = &relabelled.events.creation[5] else {
            panic!("the last Event is still the shuffle");
        };
        // The seen card keeps its position; the Hidden Cards swap or keep theirs.
        assert_eq!(order[1], id(2), "stream {stream}: a seen position moved");
        assert!(
            *order == [id(0), id(2), id(1)] || *order == [id(1), id(2), id(0)],
            "stream {stream}: {order:?}"
        );
        reordered |= *order == [id(1), id(2), id(0)];
        // The relabelled Events still apply, to the same cards in the zone.
        let folded = empty
            .apply_trace(&relabelled.events.creation)
            .expect("the relabelled Events apply");
        assert_eq!(folded.cards().len(), 3);
    }
    assert!(reordered, "no stream changed the order of the shuffle");
}

#[test]
fn a_relabelling_reshuffles_a_shuffle_carried_in_a_paused_resolution() {
    use tcg_engine::declarative::{DecisionKind, EffectPath, PausedResolution, QueuedTrigger};

    let (ruleset, _) = effects();
    // Seen from seat 0, as in the test of the shuffle in a trace: the Hidden Cards 0 and 1 are
    // alone in their classes (ids and definitions cannot change), and the middle position of the
    // shuffle of seat 1's hidden `deck` holds the seen card 2.
    let shuffle = |zone: &str, order: &[u32]| Event::ZoneShuffled {
        player: PlayerId(1),
        zone: ZoneId::new(zone),
        order: order.iter().copied().map(id).collect(),
        random_state: RandomState::from_seed(Seed(1)),
    };
    let creation = vec![
        created(0, 1, "deck"),
        created(1, 1, "mana_deck"),
        created(2, 1, "field"),
        moved(1, 1, "mana_deck", "deck"),
        moved(2, 1, "field", "deck"),
    ];
    // A shuffle of the unseen `deck`, and one of the public `field`, which the Viewer sees whole.
    let carried = [shuffle("deck", &[0, 2, 1]), shuffle("field", &[1, 2, 0])];
    let pause = |unscanned: Vec<Event>| Event::DecisionPending {
        decider: PlayerId(0),
        kind: DecisionKind::Option,
        paused: PausedResolution {
            trigger: QueuedTrigger {
                card: id(2),
                effect: 0,
            },
            at: EffectPath {
                branches: Vec::new(),
                choice: 0,
            },
            bound: Vec::new(),
            queue: Vec::new(),
            unscanned,
            resolutions: 1,
        },
    };
    // The same Events once in a trace and once carried by the pause at the same position.
    let in_trace = GameEvents {
        creation: creation.clone(),
        traces: vec![vec![
            moved(0, 1, "deck", "deck"),
            carried[0].clone(),
            carried[1].clone(),
        ]],
    };
    let in_pause = GameEvents {
        creation,
        traces: vec![vec![moved(0, 1, "deck", "deck"), pause(carried.to_vec())]],
    };
    let hidden = set(&[0, 1]);
    assert_eq!(hidden_cards(&ruleset, viewer(0), &in_pause), hidden);

    let mut reordered = false;
    for stream in 0..32 {
        let relabel_of = |events: &GameEvents| {
            let mut rng = RandomState::from_seed(Seed(stream));
            relabel(&ruleset, viewer(0), &hidden, events, None, &mut rng)
        };
        let (plain, paused) = (relabel_of(&in_trace), relabel_of(&in_pause));
        let Event::DecisionPending {
            paused: resolution, ..
        } = &paused.events.traces[0][1]
        else {
            panic!("the pause is still the last Event");
        };
        // Exactly what the same Events in a trace become with the same stream.
        assert_eq!(
            resolution.unscanned,
            plain.events.traces[0][1..],
            "stream {stream}: the carried shuffles"
        );
        let Event::ZoneShuffled { order, .. } = &resolution.unscanned[0] else {
            panic!("the carried Events are still the shuffles");
        };
        // The seen card keeps its position; the Hidden Cards swap or keep theirs.
        assert_eq!(order[1], id(2), "stream {stream}: a seen position moved");
        assert!(
            *order == [id(0), id(2), id(1)] || *order == [id(1), id(2), id(0)],
            "stream {stream}: {order:?}"
        );
        reordered |= *order == [id(1), id(2), id(0)];
        // The shuffle of a zone the Viewer sees is left as it was.
        assert_eq!(
            resolution.unscanned[1], carried[1],
            "stream {stream}: a seen zone"
        );
        // Nothing else changed, and the draws after the pause are the same ones.
        assert_eq!(paused.events.creation, in_pause.creation);
        assert_eq!(paused.events.traces[0][0], in_pause.traces[0][0]);
    }
    assert!(
        reordered,
        "no stream changed the order of the carried shuffle"
    );
}

#[test]
fn the_relabelling_maps_every_card_a_paused_resolution_and_an_answer_names() {
    use tcg_engine::declarative::{
        Answer, BoundName, DecisionKind, EffectPath, PausedResolution, QueuedTrigger,
    };
    use tcg_engine::ruleset_definition::ParamName;
    use tcg_engine::ruleset_definition::{FieldValue, StateName};

    let (ruleset, _) = effects();
    let changed = |card: u32| Event::CardStateChanged {
        card: id(card),
        state: StateName::new("tapped"),
        value: FieldValue::Bool(true),
    };
    // Six cards of one class, in a hidden zone for both Viewers; the Events of the step name five
    // of them: marker Events (a state change each) show where the permutation sends a card, and the
    // pause carries the same cards in its trigger, its queue, its unscanned Events and its bound
    // names (ADR-0034); the Answer that follows it names a card, and so does the recorded Action.
    let creation: Vec<Event> = (0..6).map(|card| created(card, 0, "deck")).collect();
    let pause = |trigger: u32, queue: &[u32], unscanned: Vec<Event>| Event::DecisionPending {
        decider: PlayerId(0),
        kind: DecisionKind::Option,
        paused: PausedResolution {
            trigger: QueuedTrigger {
                card: id(trigger),
                effect: 0,
            },
            at: EffectPath {
                branches: Vec::new(),
                choice: 0,
            },
            bound: vec![
                BoundName {
                    name: ParamName::new("first"),
                    card: id(4),
                },
                BoundName {
                    name: ParamName::new("second"),
                    card: id(5),
                },
            ],
            queue: queue
                .iter()
                .map(|&card| QueuedTrigger {
                    card: id(card),
                    effect: 1,
                })
                .collect(),
            unscanned,
            resolutions: 1,
        },
    };
    // The carried shuffle is of a zone Viewer 0 sees, so that only the ids in it move: a shuffle of
    // an unseen zone is re-shuffled too (ADR-0036), which another test covers.
    let shuffled = |order: Vec<CardInstanceId>| Event::ZoneShuffled {
        player: PlayerId(0),
        zone: ZoneId::new("hand"),
        order,
        random_state: RandomState::from_seed(Seed(1)),
    };
    let events = GameEvents {
        creation,
        traces: vec![vec![
            changed(1),
            changed(2),
            changed(3),
            changed(4),
            changed(5),
            pause(
                1,
                &[2, 3],
                vec![
                    moved(4, 0, "deck", "mana_deck"),
                    changed(5),
                    shuffled(vec![id(5), id(4)]),
                ],
            ),
            Event::DecisionAnswered {
                player: PlayerId(0),
                answer: Answer::Card(id(3)),
            },
        ]],
    };
    let recorded = Action::Answer {
        player: PlayerId(0),
        answer: Answer::Card(id(2)),
    };
    let hidden = hidden_cards(&ruleset, viewer(0), &events);
    assert_eq!(hidden, set(&[0, 1, 2, 3, 4, 5]));

    let mut moved_a_card = false;
    for stream in 0..16 {
        let mut rng = RandomState::from_seed(Seed(stream));
        let relabelled = relabel(
            &ruleset,
            viewer(0),
            &hidden,
            &events,
            Some(&recorded),
            &mut rng,
        );
        let trace = &relabelled.events.traces[0];
        // Where each card went, read off the marker Events.
        let to = |card: u32| match &trace[usize::try_from(card - 1).expect("an index")] {
            Event::CardStateChanged { card, .. } => *card,
            other => panic!("a marker Event, got {other:?}"),
        };
        moved_a_card |= (1..=5).any(|card| to(card) != id(card));
        let Event::DecisionPending { paused, .. } = &trace[5] else {
            panic!("the pause is still the last Event");
        };
        assert_eq!(paused.trigger.card, to(1), "stream {stream}: the trigger");
        assert_eq!(
            paused
                .queue
                .iter()
                .map(|queued| queued.card)
                .collect::<Vec<_>>(),
            [to(2), to(3)],
            "stream {stream}: the queue"
        );
        assert_eq!(
            paused.unscanned,
            vec![
                moved(to(4).0, 0, "deck", "mana_deck"),
                changed(to(5).0),
                shuffled(vec![to(5), to(4)]),
            ],
            "stream {stream}: the carried Events"
        );
        assert_eq!(
            paused.bound,
            vec![
                BoundName {
                    name: ParamName::new("first"),
                    card: to(4)
                },
                BoundName {
                    name: ParamName::new("second"),
                    card: to(5)
                },
            ],
            "stream {stream}: the bound names"
        );
        // What is not a card is kept.
        assert_eq!(paused.resolutions, 1);
        // The card of the Answer goes through the same permutation, in its Event and in the
        // recorded Action.
        assert_eq!(
            trace[6],
            Event::DecisionAnswered {
                player: PlayerId(0),
                answer: Answer::Card(to(3)),
            },
            "stream {stream}: the Answer Event"
        );
        assert_eq!(
            relabelled.action,
            Some(Action::Answer {
                player: PlayerId(0),
                answer: Answer::Card(to(2)),
            }),
            "stream {stream}: the recorded Action"
        );
    }
    assert!(moved_a_card, "no stream relabelled a card");
}

// ---- Player state (ticket #96, step 4, ADR-0041)
//
// Player state is fully public and no Event of it names a card, so the Relabelling leaves
// `PlayerStateChanged` alone, and the leak check must still hold on steps that change it.

/// The Game of the Player-state fixture the tests below play, as the Events of each step: seat 0
/// summons its `tithe_unit` (a card move and three Player-state changes), then drains each Player.
struct StateGame {
    ruleset: DeclarativeRuleset,
    creation: tcg_engine::declarative::GameCreation,
    actions: Vec<Action>,
    traces: Vec<Vec<Event>>,
    /// The engine's GameState after the creation, then after each Action.
    states: Vec<tcg_engine::declarative::GameState>,
}

fn state_game() -> StateGame {
    use tcg_engine::ruleset::Ruleset;
    let (ruleset, decks) = player_state();
    let creation = ruleset
        .create_game(&decks, Seed(1))
        .expect("the decks create a game");
    let declared = |player: u32, name: &str, cards: &[u32]| Action::Declared {
        player: PlayerId(player),
        action: ActionId::new(name),
        arguments: cards
            .iter()
            .map(|card| tcg_engine::declarative::Argument::Card(id(*card)))
            .collect(),
    };
    let actions = vec![
        declared(0, "summon", &[5]),
        declared(0, "drain_all", &[]),
        declared(0, "drain", &[]),
    ];
    let mut state = creation.opening.state.clone();
    let mut states = vec![state.clone()];
    let mut traces = Vec::new();
    for action in &actions {
        let transition = ruleset
            .transition(&state, action)
            .expect("the Action is accepted");
        traces.push(transition.trace.events().to_vec());
        state = transition.state;
        states.push(state.clone());
    }
    StateGame {
        ruleset,
        creation,
        actions,
        traces,
        states,
    }
}

impl StateGame {
    /// The Events up to the step `step` (`None` is the Opening Position) and the step's Action.
    fn up_to(&self, step: Option<usize>) -> (GameEvents, Option<&Action>) {
        let traces = match step {
            None => Vec::new(),
            Some(index) => self.traces[..=index].to_vec(),
        };
        let events = GameEvents {
            creation: self.creation.opening.trace.events().to_vec(),
            traces,
        };
        (events, step.map(|index| &self.actions[index]))
    }
}

fn is_player_state(event: &Event) -> bool {
    matches!(event, Event::PlayerStateChanged { .. })
}

#[test]
fn the_relabelling_leaves_player_state_changed_unchanged() {
    let game = state_game();
    let (events, _) = game.up_to(Some(game.actions.len() - 1));
    let changes: usize = events
        .traces
        .iter()
        .map(|trace| trace.iter().filter(|event| is_player_state(event)).count())
        .sum();
    assert_eq!(
        changes,
        3 + 2 + 1,
        "the script changes Player state six times"
    );

    let every_card: BTreeSet<CardInstanceId> = game
        .creation
        .opening
        .state
        .cards()
        .iter()
        .map(|card| card.id())
        .collect();
    let mut moved_a_card = false;
    for seat in 0..2 {
        let who = viewer(seat);
        let hidden = hidden_cards(&game.ruleset, who, &events);
        assert!(!hidden.is_empty());
        for to_relabel in [&hidden, &every_card] {
            for stream in 0..8 {
                let mut rng = RandomState::from_seed(Seed(stream));
                let relabelled = relabel(&game.ruleset, who, to_relabel, &events, None, &mut rng);
                moved_a_card |= relabelled.events != events;
                // The same positions of each list hold the same `PlayerStateChanged`, equal.
                let lists = std::iter::once((&events.creation, &relabelled.events.creation))
                    .chain(events.traces.iter().zip(&relabelled.events.traces));
                for (list, (before, after)) in lists.enumerate() {
                    assert_eq!(before.len(), after.len());
                    for (position, (one, other)) in before.iter().zip(after).enumerate() {
                        assert_eq!(
                            is_player_state(one),
                            is_player_state(other),
                            "seat {seat}, stream {stream}, list {list}, position {position}"
                        );
                        if is_player_state(one) {
                            assert_eq!(
                                one, other,
                                "seat {seat}, stream {stream}, list {list}, position {position}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(moved_a_card, "the Relabelling rewrote no card Event");
}

#[test]
fn observations_ignore_hidden_cards_on_steps_that_change_player_state() {
    let game = state_game();
    let empty = &game.creation.empty;
    let mut changed_a_step = 0;
    for seat in 0..2 {
        let who = viewer(seat);
        for step in [None, Some(0), Some(1), Some(2)] {
            let (events, action) = game.up_to(step);
            let at = step.map_or(0, |index| index + 1);
            let observed = replay_observed_step(&game.ruleset, who, empty, &events, action)
                .expect("the Game replays from the Empty GameState");
            // The step shows the engine's Player state of both Players.
            for player in 0..2 {
                assert_eq!(
                    observed.step.state.player_state(PlayerId(player)),
                    game.states[at].player_state(PlayerId(player)),
                    "seat {seat}, step {step:?}, Player {player}"
                );
            }
            let hidden = hidden_cards(&game.ruleset, who, &events);
            for stream in 0..8 {
                let mut rng = RandomState::from_seed(Seed(stream));
                let relabelled = relabel(&game.ruleset, who, &hidden, &events, action, &mut rng);
                let replayed = replay_observed_step(
                    &game.ruleset,
                    who,
                    empty,
                    &relabelled.events,
                    relabelled.action.as_ref(),
                )
                .expect("a Relabelling of the Hidden Cards applies");
                assert_eq!(
                    check_hidden_cards_ignored(&observed.step, &replayed.step),
                    Vec::new(),
                    "seat {seat}, step {step:?}, stream {stream}"
                );
            }
            if step.is_some() {
                assert!(events
                    .traces
                    .last()
                    .is_some_and(|trace| trace.iter().any(is_player_state)));
                changed_a_step += 1;
            }
        }
    }
    assert_eq!(
        changed_a_step, 6,
        "every step of the script changes Player state"
    );
}

#[test]
fn the_observation_checks_name_a_player_state_that_differs() {
    use tcg_engine::declarative::ObservedEvent;
    use tcg_engine::ruleset_definition::{FieldValue, PlayerStateId};

    let game = state_game();
    let who = viewer(0);
    let (events, action) = game.up_to(Some(0));
    let observed = replay_observed_step(&game.ruleset, who, &game.creation.empty, &events, action)
        .expect("the Game replays");

    // The same step with one Player's `life_points` altered, and nothing else: only the Player
    // state differs, so a violation must be about it.
    let alter = ObservedEvent::PlayerStateChanged {
        player: PlayerId(1),
        state: PlayerStateId::new("life_points"),
        value: FieldValue::Int(1),
    };
    let mut tampered = observed.step.clone();
    tampered.state = observed
        .step
        .state
        .apply_events(&[alter])
        .expect("the change applies");
    let named = "the Player state `life_points` of Player 1 is Int(4000) in observed and Int(1) in relabelled";

    let violations = check_hidden_cards_ignored(&observed.step, &tampered);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(
        violations[0].invariant,
        Invariant::ObservationsIgnoreHiddenCards
    );
    assert!(
        violations[0].detail.contains(named),
        "the detail should say `{named}`: {}",
        violations[0].detail
    );

    // The consistency check names it too, between the previous Observed GameState folded with the
    // Observed Events and the next one.
    let violations =
        crate::check_observed_consistency(who, &observed.step.state, &[], &tampered.state);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].detail.contains(
            "the Player state `life_points` of Player 1 is Int(4000) in applying the Observed Events and Int(1) in the next one"
        ),
        "{}",
        violations[0].detail
    );
}
