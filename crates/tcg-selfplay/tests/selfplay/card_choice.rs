//! Acceptance tests for ticket #76 over self-play (ADR-0034): Games that pause for a Card Choice
//! and are answered by the Uniform Random Policy, with every Invariant checked, including the three
//! over Observations. These Games pause and answer under those checks; that the Relabelling maps
//! the bound names of a Paused Resolution and the card of an Answer is pinned by the unit test
//! `the_relabelling_maps_every_card_a_paused_resolution_and_an_answer_names` in the crate.
//!
//! The deck lists four `snare_vine`s and four `mind_leech`es, more than the shipped decks do: on
//! entering the field the first lets its controller tap a creature in an opponent's
//! field, the second lets the opponent move a creature of its own hand to its trash.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{Action, Answer, DecisionKind, DeclarativeRuleset, Event};
use tcg_engine::random::Seed;
use tcg_selfplay::{self_play, verify_record, Checks, SelfPlayGame};

use crate::common::{effects, effects_deck, MAX_STEPS};

/// A Deck List for the Effects Ruleset that holds four `snare_vine`s and four `mind_leech`es.
const CHOICE_DECK: &str = "inputs:
  - input: leader
    cards:
      - { card: captain_ardel, count: 1 }
  - input: main_deck
    cards:
      - { card: snare_vine,     count: 4 }
      - { card: mind_leech,     count: 4 }
      - { card: goblin_scout,   count: 4 }
      - { card: moss_sprite,    count: 4 }
      - { card: ember_hound,    count: 4 }
      - { card: river_otter,    count: 4 }
      - { card: stone_golem,    count: 4 }
      - { card: shadow_cat,     count: 4 }
      - { card: iron_guard,     count: 4 }
      - { card: wind_hawk,      count: 4 }
      - { card: thorn_boar,     count: 4 }
      - { card: frost_wolf,     count: 4 }
      - { card: ash_drake,      count: 2 }
  - input: mana_deck
    cards:
      - { card: mana, count: 10 }
";

/// The Seeds played: far more than the few that deal a card early enough to be played.
const SEEDS: u64 = 60;

fn choice_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![
        effects_deck(ruleset, CHOICE_DECK),
        effects_deck(ruleset, CHOICE_DECK),
    ]
}

/// How many pauses of `kind` the Event Traces of `game` hold.
fn pauses(game: &SelfPlayGame, kind: DecisionKind) -> usize {
    game.traces
        .iter()
        .flat_map(|trace| trace.events())
        .filter(
            |event| matches!(event, Event::DecisionPending { kind: found, .. } if *found == kind),
        )
        .count()
}

/// How many Card Answers the Game Record of `game` holds.
fn card_answers(game: &SelfPlayGame) -> usize {
    game.record
        .actions()
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::Answer {
                    answer: Answer::Card(_),
                    ..
                }
            )
        })
        .count()
}

#[test]
fn self_play_with_observation_checks_over_card_choices() {
    let ruleset = effects();
    let decks = choice_decks(&ruleset);
    let (mut total_pauses, mut total_answers) = (0, 0);
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .unwrap_or_else(|failure| panic!("seed {seed} fails: {failure:?}"));
        // A Game ends with a Terminal Outcome, which clears any pending decision. Every Card pause
        // is answered by a later Card Answer, except one whose own transition ended the Game.
        assert!(game.state.outcome().is_some(), "seed {seed}");
        assert!(game.state.pending_decision().is_none(), "seed {seed}");
        let ended_paused = game.traces.last().is_some_and(|trace| {
            trace
                .events()
                .iter()
                .any(|event| matches!(event, Event::DecisionPending { .. }))
        });
        assert_eq!(
            pauses(&game, DecisionKind::Card),
            card_answers(&game) + usize::from(ended_paused),
            "seed {seed}"
        );
        // The record, verified with every Invariant, replays the same Game.
        let replay = verify_record(&ruleset, &game.record)
            .unwrap_or_else(|failure| panic!("seed {seed} does not verify: {failure:?}"));
        assert_eq!(replay.state, game.state, "seed {seed}");
        assert_eq!(replay.traces, game.traces, "seed {seed}");
        total_pauses += pauses(&game, DecisionKind::Card);
        total_answers += card_answers(&game);
    }
    // At least one Game paused for a Card Choice and was answered, so the checks over Observations
    // saw a Pending Decision of that kind and the Answers that name a card.
    assert!(total_pauses > 0, "no Game of {SEEDS} Seeds paused");
    assert!(
        total_answers > 0,
        "no Game of {SEEDS} Seeds had a Card Answer"
    );
}

#[test]
fn two_self_play_runs_over_card_choices_from_the_same_seed_are_equal() {
    let ruleset = effects();
    let decks = choice_decks(&ruleset);
    for seed in 0..SEEDS {
        let play = || self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On);
        assert_eq!(play(), play(), "seed {seed}");
    }
}
