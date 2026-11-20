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
use crate::test_support::{effects, independent_fold, viewer};
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
