//! Acceptance tests for ticket #96, step 4 (ADR-0041, ADR-0028, ADR-0029): observing Player state.
//!
//! Player state is **fully public**: every Viewer sees every Player's values, in Player order and
//! in the order of the Player-State Schemas, and `PlayerStateChanged` is observed in full, the same
//! for every Viewer. The Observed Events and the Observed GameState name no card for it. The Games
//! are those of the Player-state fixture (`fixtures/player-state-*.yaml`), whose Players open with
//! 4000 `life_points` and `summoned: false`; the round-trip tests add the `life_out` win condition
//! of step 2 and start the Players at 700.

use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameRecord,
    GameState, ObservedEvent, ObservedGameState, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Observations, Ruleset, Transition};
use tcg_engine::ruleset_definition::{ActionId, FieldValue, PlayerStateId};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const RULESET: &str = include_str!("fixtures/player-state-ruleset.yaml");
const CATALOG: &str = include_str!("fixtures/player-state-catalog.yaml");
const DECK: &str = include_str!("fixtures/player-state-deck.yaml");

/// Seat 0's opening hand holds the `tithe_unit` 5, which writes Player state when it enters the
/// field.
const TITHE_0: CardInstanceId = CardInstanceId(5);

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(p(seat))
}

fn int(value: i64) -> FieldValue {
    FieldValue::Int(value)
}

/// The Player state of a Player in declared order: `life_points`, then `summoned`.
fn values(life_points: i64, summoned: bool) -> Vec<(PlayerStateId, FieldValue)> {
    vec![
        (PlayerStateId::new("life_points"), int(life_points)),
        (PlayerStateId::new("summoned"), FieldValue::Bool(summoned)),
    ]
}

fn changed(player: u32, state: &str, value: FieldValue) -> ObservedEvent {
    ObservedEvent::PlayerStateChanged {
        player: p(player),
        state: PlayerStateId::new(state),
        value,
    }
}

fn act(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

/// `yaml` with the one occurrence of `from` replaced by `to`.
fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(yaml.matches(from).count(), 1, "`{from}` occurs once");
    yaml.replacen(from, to, 1)
}

fn game_of(ruleset_yaml: &str) -> (DeclarativeRuleset, GameCreation) {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, CATALOG).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks: Vec<_> = (0..2)
        .map(|_| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), DECK)
                .expect("the deck is legal")
        })
        .collect();
    let creation = ruleset
        .create_game(&decks, Seed(1))
        .expect("the decks create a game");
    (ruleset, creation)
}

fn game() -> (DeclarativeRuleset, GameCreation) {
    game_of(RULESET)
}

/// A Game whose Players start at 700 and lose at 0 or less.
fn mortal_game() -> (DeclarativeRuleset, GameCreation) {
    let smaller = edit(
        RULESET,
        "{ id: life_points, type: int, init: 4000 }",
        "{ id: life_points, type: int, init: 700 }",
    );
    let text = format!(
        "{smaller}\nwin_conditions:\n  - id: life_out\n    when: {{ lte: [{{ ref: player.life_points }}, 0] }}\n    loser: owner\n"
    );
    game_of(&text)
}

/// Several changes of Player state, with a card move among them, ending the Game in a draw.
fn script() -> Vec<Action> {
    vec![
        act(0, "drain", &[]),
        act(0, "summon", &[TITHE_0]),
        act(0, "pass_turn", &[]),
        act(1, "drain_all", &[]),
        act(1, "pay_to_hurt", &[]),
    ]
}

fn take(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
) -> Transition<GameState, Event> {
    ruleset
        .transition(state, action)
        .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"))
}

/// What a Viewer observed at each step of a Game: the Observed Events and the Observed GameState
/// after the creation, then after each Action.
type Observed = Vec<(Vec<ObservedEvent>, ObservedGameState)>;

/// The Game followed by a fresh Observer of `viewer`, from the Empty GameState.
fn observed_game(
    ruleset: &DeclarativeRuleset,
    creation: &GameCreation,
    actions: &[Action],
    seat: u32,
) -> Observed {
    let empty = &creation.empty;
    let mut observer = ruleset
        .observer(viewer(seat), empty, &[])
        .expect("the Observer is built");
    let events = ruleset
        .follow_events(&mut observer, empty, creation.opening.trace.events())
        .expect("the creation trace is followed");
    let mut state = creation.opening.state.clone();
    let observed = ruleset
        .observe(&observer, &state)
        .expect("the Opening Position is observed");
    let mut steps = vec![(events, observed)];
    for action in actions {
        let transition = take(ruleset, &state, action);
        let followed = ruleset
            .follow_transition(&mut observer, &state, action, &transition)
            .expect("the transition is followed");
        state = transition.state;
        steps.push((followed.events, followed.state));
    }
    steps
}

/// The `PlayerStateChanged` Observed Events of a list, in order.
fn player_state_events(events: &[ObservedEvent]) -> Vec<ObservedEvent> {
    events
        .iter()
        .filter(|event| matches!(event, ObservedEvent::PlayerStateChanged { .. }))
        .cloned()
        .collect()
}

// ---- shown to every Viewer

#[test]
fn every_viewer_sees_every_players_player_state() {
    let (ruleset, creation) = game();
    let opening = creation.opening.state.clone();
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

    // The Opening Position: both Viewers see both Players' values, and the same ones.
    let seen: Vec<ObservedGameState> = observers
        .iter()
        .map(|observer| {
            ruleset
                .observe(observer, &opening)
                .expect("the Opening Position is observed")
        })
        .collect();
    for observed in &seen {
        assert_eq!(
            observed.player_state(p(0)),
            Some(values(4000, false).as_slice())
        );
        assert_eq!(
            observed.player_state(p(1)),
            Some(values(4000, false).as_slice())
        );
        assert_eq!(observed.player_state(p(2)), None);
    }

    // After a change that writes both Players' state (`tithe_unit` gives seat 0 life and marks
    // seat 1): the same again, whoever the Viewer and whoever acted.
    let action = act(0, "summon", &[TITHE_0]);
    let transition = take(&ruleset, &opening, &action);
    let after: Vec<ObservedGameState> = observers
        .iter_mut()
        .map(|observer| {
            ruleset
                .follow_transition(observer, &opening, &action, &transition)
                .expect("the transition is followed")
                .state
        })
        .collect();
    for (seat, observed) in after.iter().enumerate() {
        assert_eq!(
            observed.player_state(p(0)),
            Some(values(4500, true).as_slice()),
            "viewer {seat}"
        );
        assert_eq!(
            observed.player_state(p(1)),
            Some(values(4000, true).as_slice()),
            "viewer {seat}"
        );
        // It is what `observe` says of the new GameState.
        assert_eq!(
            ruleset.observe(&observers[seat], &transition.state),
            Ok(observed.clone())
        );
    }
    for player in 0..2 {
        assert_eq!(
            after[0].player_state(p(player)),
            after[1].player_state(p(player))
        );
    }
}

#[test]
fn the_observed_empty_game_state_holds_player_state_at_its_init() {
    let (ruleset, creation) = game();
    for seat in 0..2 {
        // A fresh Observer that has followed nothing observes the Empty GameState.
        let observer = ruleset
            .observer(viewer(seat), &creation.empty, &[])
            .expect("the Observer is built");
        let observed = ruleset
            .observe(&observer, &creation.empty)
            .expect("the Empty GameState is observed");
        for player in 0..2 {
            assert_eq!(
                observed.player_state(p(player)),
                Some(values(4000, false).as_slice()),
                "viewer {seat}, Player {player}"
            );
        }
    }
}

#[test]
fn player_state_changed_is_observed_in_full_by_every_viewer() {
    let (ruleset, creation) = game();
    let drain = act(0, "drain", &[]);
    let drain_all = act(0, "drain_all", &[]);

    let mut seen: Vec<Vec<Vec<ObservedEvent>>> = Vec::new();
    for seat in 0..2 {
        let mut observer = ruleset
            .observer(
                viewer(seat),
                &creation.empty,
                creation.opening.trace.events(),
            )
            .expect("the Observer is built");
        let first = take(&ruleset, &creation.opening.state, &drain);
        let followed_first = ruleset
            .follow_transition(&mut observer, &creation.opening.state, &drain, &first)
            .expect("the transition is followed");
        let second = take(&ruleset, &first.state, &drain_all);
        let followed_second = ruleset
            .follow_transition(&mut observer, &first.state, &drain_all, &second)
            .expect("the transition is followed");
        seen.push(vec![followed_first.events, followed_second.events]);
    }

    // The opponent's field, then an `each` write: in trace order, the exact values, whichever seat
    // acted and whichever seat looks.
    let expected = vec![
        vec![changed(1, "life_points", int(3700))],
        vec![
            changed(0, "life_points", int(3900)),
            changed(1, "life_points", int(3600)),
        ],
    ];
    for (seat, per_step) in seen.iter().enumerate() {
        assert_eq!(per_step, &expected, "viewer {seat}");
    }
}

// ---- round-trips

#[test]
fn folding_observed_player_state_changes_gives_the_observed_game_state() {
    let (ruleset, creation) = mortal_game();
    let actions = script();
    for seat in 0..2 {
        let observed = observed_game(&ruleset, &creation, &actions, seat);

        // The fold starts from the Observed Empty GameState, not from the Opening Position.
        let observer = ruleset
            .observer(viewer(seat), &creation.empty, &[])
            .expect("the Observer is built");
        let mut folded = ruleset
            .observe(&observer, &creation.empty)
            .expect("the Empty GameState is observed");
        let mut state = creation.empty.clone();
        let mut real = vec![creation.opening.trace.events().to_vec()];
        let mut replay_state = creation.opening.state.clone();
        for action in &actions {
            let transition = take(&ruleset, &replay_state, action);
            real.push(transition.trace.events().to_vec());
            replay_state = transition.state;
        }

        for (step, ((events, expected), trace)) in observed.iter().zip(&real).enumerate() {
            folded = folded
                .apply_events(events)
                .unwrap_or_else(|error| panic!("viewer {seat}, step {step}: {error:?}"));
            assert_eq!(&folded, expected, "viewer {seat}, step {step}");
            // The fold agrees with the engine's GameState, Player state included.
            state = state.apply_trace(trace).expect("the trace applies");
            for player in 0..2 {
                assert_eq!(
                    folded.player_state(p(player)),
                    state.player_state(p(player)),
                    "viewer {seat}, step {step}, Player {player}"
                );
            }
        }

        // The Game changed Player state several times and ended on it.
        let all: Vec<ObservedEvent> = observed
            .iter()
            .flat_map(|(events, _)| player_state_events(events))
            .collect();
        assert!(all.len() >= 8, "{} changes", all.len());
        let outcome = folded.outcome().expect("the Game ended on life");
        assert!(outcome.is_drawn_game());
    }
}

#[test]
fn a_replayed_game_gives_the_same_observed_player_state() {
    let (ruleset, creation) = mortal_game();
    let actions = script();
    let decks: Vec<_> = (0..2)
        .map(|_| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), DECK)
                .expect("the deck is legal")
        })
        .collect();
    let mut record = GameRecord::new(&ruleset, &decks, Seed(1));
    for action in &actions {
        record.push_accepted(action.clone());
    }
    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(replay.creation, creation);

    for seat in 0..2 {
        let original = observed_game(&ruleset, &creation, &actions, seat);

        // Follow the replayed Game from its own Empty GameState, trace by trace.
        let mut observer = ruleset
            .observer(viewer(seat), &replay.creation.empty, &[])
            .expect("the Observer is built");
        let events = ruleset
            .follow_events(
                &mut observer,
                &replay.creation.empty,
                replay.creation.opening.trace.events(),
            )
            .expect("the creation trace is followed");
        let mut state = replay.creation.opening.state.clone();
        let observed = ruleset
            .observe(&observer, &state)
            .expect("the Opening Position is observed");
        let mut again: Observed = vec![(events, observed)];
        for trace in &replay.traces {
            let events = ruleset
                .follow_events(&mut observer, &state, trace.events())
                .expect("the trace is followed");
            state = state
                .apply_trace(trace.events())
                .expect("the trace applies");
            let observed = ruleset
                .observe(&observer, &state)
                .expect("the GameState is observed");
            again.push((events, observed));
        }
        assert_eq!(state, replay.state);
        assert_eq!(original, again, "viewer {seat}");
    }
}
