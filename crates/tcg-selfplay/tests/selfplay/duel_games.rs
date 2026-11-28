//! The correctness net over the Duel Ruleset (ticket #96, ADR-0041; v1 since ticket #97, ADR-0042; v2 since ticket #98, ADR-0043): its sample decks are legal
//! through the engine's Deck List validation, and Games over Seeds 0..100 are played to a Terminal
//! Outcome with every Invariant checked (`Checks::On`: the twelve Invariants, the leak check
//! included), deterministically from the same Seed, and replay exactly. The 1,000-Seed net, its pin
//! and the Golden Game are ticket [43]'s.
//!
//! Everything runs on the real Duel Ruleset, catalog and sample decks.

use tcg_engine::declarative::{
    Action, Argument, Event, GameState, ObservedEvent, ObservedGameState, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Observations, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, FieldName, FieldValue, PhaseId, StateName, ZoneId};
use tcg_selfplay::{self_play, Checks, FailureKind, SelfPlayGame};

use crate::common::{declared, duel, duel_decks, edit, DUEL_DECK_A, DUEL_DECK_B, MAX_STEPS};
use tcg_loader::load_deck_list;

/// The Seeds played with every Invariant: `0..SEEDS`.
const SEEDS: u64 = 100;

/// Whether the record of `game` holds the declared action `id`.
fn holds_action(game: &SelfPlayGame, id: &str) -> bool {
    game.record.actions().iter().any(|action| {
        declared(action).is_some_and(|(_, declared_id, _)| declared_id == &ActionId::new(id))
    })
}

/// Whether the record of `game` holds a tribute summon or a tribute set.
fn holds_tribute(game: &SelfPlayGame) -> bool {
    [
        "tribute_summon_one",
        "tribute_summon_two",
        "tribute_set_one",
        "tribute_set_two",
    ]
    .iter()
    .any(|id| holds_action(game, id))
}

#[test]
fn the_duel_sample_decks_are_legal() {
    let ruleset = duel();
    for (name, yaml) in [("A", DUEL_DECK_A), ("B", DUEL_DECK_B)] {
        // The engine's validation accepts the deck: the input exists, every card is a monster, the
        // 40 cards have at most 3 copies each.
        let deck = load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
            .unwrap_or_else(|error| panic!("deck {name} is not legal: {error:?}"));
        assert_eq!(deck.inputs().len(), 1);
    }

    // The main deck admits monsters and spells (ticket #99): both decks hold spells, and the
    // validation accepted them.
    let creation = ruleset
        .create_game(&duel_decks(&ruleset), Seed(0))
        .expect("the decks create a game");
    for seat in 0..2 {
        let spells = creation
            .opening
            .state
            .cards()
            .iter()
            .filter(|card| card.owner() == PlayerId(seat))
            .filter(|card| {
                ruleset
                    .catalog()
                    .card(card.definition())
                    .is_some_and(|definition| definition.card_type().as_str() == "spell")
            })
            .count();
        assert!(spells > 0, "the deck of seat {seat} holds no spell");
    }

    // The same validation refuses a deck that is not legal, so the check above means something: a
    // fourth copy of a card, and a deck of 39 cards.
    let four_copies = edit(
        DUEL_DECK_A,
        "{ card: lantern_moth, count: 3 }",
        "{ card: lantern_moth, count: 4 }",
    );
    assert!(load_deck_list(ruleset.definition(), ruleset.catalog(), &four_copies).is_err());
    let short = edit(
        DUEL_DECK_A,
        "{ card: lantern_moth, count: 3 }",
        "{ card: lantern_moth, count: 2 }",
    );
    assert!(load_deck_list(ruleset.definition(), ruleset.catalog(), &short).is_err());
}

#[test]
fn duel_self_play_ends_every_game_over_seeds_0_to_100_with_every_invariant_holding() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    let (mut life_out, mut deck_out, mut drawn) = (0usize, 0usize, 0usize);
    let mut longest = 0usize;
    // The Games that hold each action of the positions and the combat (ticket #98, ADR-0043).
    let (mut attacks, mut sets, mut changes) = (0usize, 0usize, 0usize);
    // The Games that hold a tribute summon or set and a spell (ticket #99, ADR-0044).
    let (mut tributes, mut spells) = (0usize, 0usize);
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).unwrap_or_else(
            |failure| {
                let reason = match &failure.kind {
                    FailureKind::InvariantViolated(violations) => format!("{violations:?}"),
                    other => format!("{other:?}"),
                };
                panic!(
                    "Seed {seed} failed after {:?} Actions: {reason}",
                    failure.action_index
                )
            },
        );
        let outcome = game
            .state
            .outcome()
            .unwrap_or_else(|| panic!("Seed {seed} has no Terminal Outcome"));
        longest = longest.max(game.record.actions().len());
        attacks += usize::from(holds_action(&game, "attack"));
        sets += usize::from(holds_action(&game, "set_monster"));
        changes += usize::from(holds_action(&game, "change_position"));
        tributes += usize::from(holds_tribute(&game));
        spells += usize::from(holds_action(&game, "play_spell"));
        if outcome.is_drawn_game() {
            drawn += 1;
        }
        for loss in outcome.losses() {
            match loss.reason.as_str() {
                "life_out" => life_out += 1,
                "deck_out" => deck_out += 1,
                other => panic!("Seed {seed}: unknown reason `{other}`"),
            }
        }
    }
    // The self-play explores the new rules: some Game holds an attack between monsters, a set and
    // a position change, and all of them passed every Invariant, the Observed Game round-trip and
    // the Relabelling among them.
    assert!(attacks > 0, "no Game holds an `attack`");
    assert!(sets > 0, "no Game holds a `set_monster`");
    assert!(changes > 0, "no Game holds a `change_position`");
    // ... and a tribute summon or set and a spell, with the spells' Choices and triggers passing
    // every Invariant.
    assert!(tributes > 0, "no Game holds a tribute summon or set");
    assert!(spells > 0, "no Game holds a `play_spell`");
    // Reported, not asserted: the Losses by reason (a drawn Game counts both), the drawn Games and
    // the longest Game, in Actions, and the Games holding each new action.
    println!(
        "duel self-play, Seeds 0..{SEEDS}: {life_out} life_out losses, {deck_out} deck_out losses, {drawn} drawn Games, longest {longest} Actions; Games holding an attack {attacks}, a set {sets}, a position change {changes}, a tribute summon or set {tributes}, a spell {spells}"
    );
}

#[test]
fn duel_self_play_is_deterministic_from_the_same_seed() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    for seed in [0, 1, 7, 42] {
        let play = || {
            self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
                .expect("the Game is played")
        };
        let (first, second) = (play(), play());
        assert_eq!(first.record, second.record, "Seed {seed}: the record");
        assert_eq!(first.creation, second.creation, "Seed {seed}: the creation");
        assert_eq!(
            first.traces, second.traces,
            "Seed {seed}: every Event Trace"
        );
        assert_eq!(
            first.state, second.state,
            "Seed {seed}: the final GameState"
        );
    }

    // Another Seed plays another Game.
    let one = self_play(&ruleset, &decks, Seed(0), MAX_STEPS, Checks::On).expect("played");
    let other = self_play(&ruleset, &decks, Seed(1), MAX_STEPS, Checks::On).expect("played");
    assert_ne!(one.record.actions(), other.record.actions());
}

#[test]
fn replay_reproduces_a_duel_game() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    for seed in [0, 3, 11] {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        let replay = ruleset
            .replay(&game.record)
            .expect("the record of a self-played Game replays");
        assert_eq!(replay.creation, game.creation, "Seed {seed}: the creation");
        assert_eq!(replay.traces, game.traces, "Seed {seed}: every Event Trace");
        assert_eq!(replay.state, game.state, "Seed {seed}: the final GameState");
        assert!(
            replay.state.outcome().is_some(),
            "Seed {seed}: the Terminal Outcome is reproduced"
        );
        assert_eq!(replay.state.outcome(), game.state.outcome());
        // The Game visits Battle (ticket #97): the replay reproduces the phase change.
        let battle = Event::PhaseBegan {
            phase: PhaseId::new("battle"),
        };
        assert!(
            replay
                .traces
                .iter()
                .any(|trace| trace.events().contains(&battle)),
            "Seed {seed}: the replayed Game has a PhaseBegan of Battle"
        );
    }
}

#[test]
fn replay_reproduces_a_duel_game_holding_a_fight_between_monsters() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    // Games that hold an `attack`, a fight between monsters with its destruction or its Life Point
    // change, replay exactly: the full GameState and the full Event Trace.
    let mut replayed = 0usize;
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        if !holds_action(&game, "attack") {
            continue;
        }
        let replay = ruleset
            .replay(&game.record)
            .expect("the record of a self-played Game replays");
        assert_eq!(replay.creation, game.creation, "Seed {seed}: the creation");
        assert_eq!(replay.traces, game.traces, "Seed {seed}: every Event Trace");
        assert_eq!(replay.state, game.state, "Seed {seed}: the final GameState");
        replayed += 1;
        if replayed == 3 {
            break;
        }
    }
    assert_eq!(replayed, 3, "three Games hold a fight between monsters");
}

#[test]
fn replay_reproduces_a_duel_game_holding_a_tribute_summon_and_a_spell() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    // Games that hold a tribute summon or set and a spell, with its cost, its trigger and, for
    // Breaking Bolt, its Choice, replay exactly: the full creation, every Event Trace and the final
    // GameState.
    let mut replayed = 0usize;
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        if !(holds_tribute(&game) && holds_action(&game, "play_spell")) {
            continue;
        }
        let replay = ruleset
            .replay(&game.record)
            .expect("the record of a self-played Game replays");
        assert_eq!(replay.creation, game.creation, "Seed {seed}: the creation");
        assert_eq!(replay.traces, game.traces, "Seed {seed}: every Event Trace");
        assert_eq!(replay.state, game.state, "Seed {seed}: the final GameState");
        replayed += 1;
        if replayed == 3 {
            break;
        }
    }
    assert_eq!(replayed, 3, "three Games hold a tribute summon and a spell");
}

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(PlayerId(seat))
}

/// The instance state `name` of each monster in the Monster Zone of `owner`, as the Viewer of
/// `observed` sees it.
fn observed_monster_states(
    observed: &ObservedGameState,
    owner: u32,
    name: &str,
) -> Vec<FieldValue> {
    let zone = observed
        .zone(PlayerId(owner), &ZoneId::new("monster_zone"))
        .expect("the Monster Zone is observed");
    zone.cards()
        .expect("the Monster Zone is public")
        .iter()
        .map(|&handle| {
            let card = observed.card(handle).expect("the monster is visible");
            card.state()
                .iter()
                .find(|(state, _)| state == &StateName::new(name))
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("the monster has no `{name}`"))
        })
        .collect()
}

#[test]
fn observations_show_each_monsters_position() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    let creation = ruleset.create_game(&decks, Seed(1)).expect("created");
    let declare = |player: u32, id: &str, arguments: Vec<Argument>| Action::Declared {
        player: PlayerId(player),
        action: ActionId::new(id),
        arguments,
    };
    // The first card of the hand that can be normal summoned or set: a monster of level 4 or less
    // (the hand may hold spells and monsters that need tributes since ticket #99).
    let first_in_hand = |state: &GameState, seat: u32| {
        state
            .zone(PlayerId(seat), &ZoneId::new("hand"))
            .expect("the hand exists")
            .iter()
            .copied()
            .find(|&card| {
                let definition = state.card(card).expect("the card exists").definition();
                matches!(
                    ruleset
                        .catalog()
                        .card(definition)
                        .and_then(|definition| definition.field(&FieldName::new("level"))),
                    Some(FieldValue::Int(1..=4))
                )
            })
            .expect("the hand holds a monster of level 4 or less")
    };

    // Both Viewers follow a Game from the Empty GameState: seat 0 summons, then seat 1 sets.
    let mut observers: Vec<_> = (0..2)
        .map(|seat| {
            ruleset
                .observer(
                    viewer(seat),
                    &creation.empty,
                    creation.opening.trace.events(),
                )
                .expect("the Observer is built")
        })
        .collect();
    let mut observed_events: Vec<Vec<ObservedEvent>> = vec![Vec::new(), Vec::new()];
    let mut state = creation.opening.state.clone();
    for step in 0..3 {
        let action = match step {
            0 => declare(
                0,
                "normal_summon",
                vec![Argument::Card(first_in_hand(&state, 0))],
            ),
            1 => declare(0, "end_turn", vec![]),
            _ => declare(
                1,
                "set_monster",
                vec![Argument::Card(first_in_hand(&state, 1))],
            ),
        };
        let transition = ruleset
            .transition(&state, &action)
            .expect("the script's Action is accepted");
        for (seat, observer) in observers.iter_mut().enumerate() {
            let followed = ruleset
                .follow_transition(observer, &state, &action, &transition)
                .expect("the transition is followed");
            observed_events[seat].extend(followed.events);
        }
        state = transition.state;
    }

    let defence = FieldValue::Text("defence".to_string());
    for (seat, observer) in observers.iter().enumerate() {
        let observed = ruleset
            .observe(observer, &state)
            .expect("the GameState is observed");
        // Every Viewer sees both monsters' position, the set one included (face-up this phase).
        assert_eq!(
            observed_monster_states(&observed, 0, "position"),
            vec![FieldValue::Text("attack".to_string())],
            "Viewer {seat}: the summoned monster"
        );
        assert_eq!(
            observed_monster_states(&observed, 1, "position"),
            vec![defence.clone()],
            "Viewer {seat}: the set monster"
        );
        // The set reached the Viewer as an Observed Event.
        assert!(
            observed_events[seat].iter().any(|event| matches!(
                event,
                ObservedEvent::CardStateChanged { state, value, .. }
                    if state == &StateName::new("position") && value == &defence
            )),
            "Viewer {seat}: the position change is observed"
        );
    }
}
