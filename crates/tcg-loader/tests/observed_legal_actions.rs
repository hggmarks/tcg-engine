//! Acceptance tests for ticket #65: acting from an Observation (ADR-0030), on the whole real
//! Effects Ruleset, its real catalog and its two real sample decks, then on the Small Ruleset, and
//! on two test-only rulesets (the Small Ruleset with one action added) for the leak case.
//!
//! A Viewer lists its **Observed Legal Actions**: the engine's Legal Actions (ADR-0018) whose Player
//! is the Viewer, in Listing Order, each card argument as its **Observed Card**. **Translating** an
//! **Observed Action** back gives the engine Action, which `transition` decides as usual: legality
//! stays with `transition`. A Legal Action naming a card its Player cannot see is an error, not a
//! leak.
//!
//! The oracles are computed here from `legal_actions`, `transition` and the Observer's mapping
//! (`observed_card`, `card_instance`); no test reads an expectation from the code under test.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState,
    ObservationError, ObservedAction, ObservedArgument, ObservedCard, Observer, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, ZoneId};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");
const EFFECTS_DECK_A: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-a.yaml");
const EFFECTS_DECK_B: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-b.yaml");

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const SMALL_DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const SMALL_DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// The Seed of every Effects Game of these tests. With the Effects sample decks seat 0 holds a
/// creature (card 14) it plays on Turn 1, seat 1 holds one (card 92) it plays on Turn 2, and the
/// Leaders are card 0 (seat 0) and card 61 (seat 1). Each test that relies on it asserts it.
const EFFECTS_SEED: u64 = 7;

/// The Seed of every Small Game of these tests: seat 0 holds a `goblin_scout` (card 1) and seat 1 a
/// `moss_sprite` (card 63); seat 1's Leader is card 61.
const SMALL_SEED: u64 = 9;

const CREATURE_0: CardInstanceId = CardInstanceId(14);
const CREATURE_1: CardInstanceId = CardInstanceId(92);
const LEADER_0: CardInstanceId = CardInstanceId(0);
const LEADER_1: CardInstanceId = CardInstanceId(61);

const SMALL_CREATURE_0: CardInstanceId = CardInstanceId(1);
const SMALL_CREATURE_1: CardInstanceId = CardInstanceId(63);

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

fn ruleset_from(ruleset_yaml: &str, catalog_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog_yaml).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn effects() -> DeclarativeRuleset {
    ruleset_from(EFFECTS_RULESET, EFFECTS_CATALOG)
}

fn small() -> DeclarativeRuleset {
    ruleset_from(SMALL_RULESET, SMALL_CATALOG)
}

fn decks(ruleset: &DeclarativeRuleset, yamls: [&str; 2]) -> Vec<DeckList> {
    yamls
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn create(ruleset: &DeclarativeRuleset, yamls: [&str; 2], seed: u64) -> GameCreation {
    ruleset
        .create_game(&decks(ruleset, yamls), Seed(seed))
        .expect("the sample decks create a game")
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

/// The action id and the arguments of a Declared Action: every Action these tests list is one.
fn declared(action: &Action) -> (&ActionId, &[Argument]) {
    match action {
        Action::Declared {
            action, arguments, ..
        } => (action, arguments),
        Action::Answer { .. } => panic!("an Answer is listed, but no decision is pending"),
        Action::Pass { .. } => panic!("a Pass is listed, but no window is open"),
    }
}

/// The same for an Observed Action.
fn observed_declared(action: &ObservedAction) -> (&ActionId, &[ObservedArgument]) {
    match action {
        ObservedAction::Declared {
            action, arguments, ..
        } => (action, arguments),
        ObservedAction::Answer { .. } => panic!("an Observed Answer is listed"),
        ObservedAction::Pass { .. } => panic!("an Observed Pass is listed"),
    }
}

fn observed_action(player: u32, id: &str, cards: &[ObservedCard]) -> ObservedAction {
    ObservedAction::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(ObservedArgument::Card).collect(),
    }
}

/// A GameState of a Game and every Event applied to the Empty GameState to reach it: the creation
/// trace, then each transition's trace in order.
struct Position {
    state: GameState,
    events: Vec<Event>,
}

impl Position {
    /// The Opening Position of `creation`.
    fn opening(creation: &GameCreation) -> Position {
        Position {
            state: creation.opening.state.clone(),
            events: creation.opening.trace.events().to_vec(),
        }
    }

    /// This position after `script`, every action accepted by `transition`.
    fn after(mut self, ruleset: &DeclarativeRuleset, script: &[Action]) -> Position {
        for (index, step) in script.iter().enumerate() {
            let transition = ruleset
                .transition(&self.state, step)
                .unwrap_or_else(|rejection| {
                    panic!("action {} is rejected: {rejection:?}", index + 1)
                });
            self.events.extend_from_slice(transition.trace.events());
            self.state = transition.state;
        }
        self
    }

    /// The Observer of seat `seat` over every Event of this position.
    fn observer(
        &self,
        ruleset: &DeclarativeRuleset,
        creation: &GameCreation,
        seat: u32,
    ) -> Observer {
        ruleset
            .observer(viewer(seat), &creation.empty, &self.events)
            .expect("the Observer is built")
    }
}

/// Seat 0 plays its creature, both pass, seat 1 plays its creature, both pass: Turn 3, a Main phase
/// of seat 0 whose listing holds `play_creature`s and an `attack` on seat 1's Leader.
fn effects_turn_3(ruleset: &DeclarativeRuleset, creation: &GameCreation) -> Position {
    Position::opening(creation).after(
        ruleset,
        &[
            action(0, "play_creature", &[CREATURE_0]),
            action(0, "pass_turn", &[]),
            action(1, "play_creature", &[CREATURE_1]),
            action(1, "pass_turn", &[]),
        ],
    )
}

/// Turn 3 and then seat 0 attacks seat 1's Leader, both pass, seat 1's creature attacks seat 0's
/// Leader and taps, both pass: Turn 5, where seat 0's untapped creature can attack seat 1's tapped
/// creature and seat 1's Leader.
fn effects_turn_5(ruleset: &DeclarativeRuleset, creation: &GameCreation) -> Position {
    effects_turn_3(ruleset, creation).after(
        ruleset,
        &[
            action(0, "attack", &[CREATURE_0, LEADER_1]),
            action(0, "pass_turn", &[]),
            action(1, "attack", &[CREATURE_1, LEADER_0]),
            action(1, "pass_turn", &[]),
        ],
    )
}

/// Seat 0 plays, both pass, seat 1 plays, both pass: the Small Game's Turn 3.
fn small_turn_3(ruleset: &DeclarativeRuleset, creation: &GameCreation) -> Position {
    Position::opening(creation).after(
        ruleset,
        &[
            action(0, "play_creature", &[SMALL_CREATURE_0]),
            action(0, "pass_turn", &[]),
            action(1, "play_creature", &[SMALL_CREATURE_1]),
            action(1, "pass_turn", &[]),
        ],
    )
}

/// The oracle: `legal` filtered to the Actions of seat `seat`, each card argument as the handle the
/// Observer holds for it.
fn expected_observed(observer: &Observer, legal: &[Action], seat: u32) -> Vec<ObservedAction> {
    legal
        .iter()
        .filter(|legal_action| legal_action.player() == p(seat))
        .map(|legal_action| {
            let (id, arguments) = declared(legal_action);
            ObservedAction::Declared {
                player: legal_action.player(),
                action: id.clone(),
                arguments: arguments
                    .iter()
                    .map(|&Argument::Card(id)| {
                        ObservedArgument::Card(
                            observer
                                .observed_card(id)
                                .expect("the Viewer sees every card of its own Legal Actions"),
                        )
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Both seats list their Observed Legal Actions in `position`: the active seat's list is the
/// engine's listing mapped through the Observer, whole, in order, and the other seat's is empty.
fn assert_observed_legal_actions_match(
    ruleset: &DeclarativeRuleset,
    creation: &GameCreation,
    position: &Position,
) {
    let state = &position.state;
    let legal = ruleset.legal_actions(state);
    assert!(!legal.is_empty(), "the position has Legal Actions");
    let active = state.active_player();
    for seat in [0, 1] {
        let observer = position.observer(ruleset, creation, seat);
        let listed = ruleset
            .observed_legal_actions(&observer, state)
            .expect("the Observed Legal Actions are listed");
        if p(seat) == active {
            assert!(!listed.is_empty(), "the active seat {seat} has actions");
            assert_eq!(listed, expected_observed(&observer, &legal, seat));
            assert_eq!(listed.len(), legal.len());
            // Each handle maps back to the Card Instance of the engine's Legal Action.
            for (observed, engine) in listed.iter().zip(&legal) {
                let (shown_arguments, engine_arguments) =
                    (observed_declared(observed).1, declared(engine).1);
                assert_eq!(shown_arguments.len(), engine_arguments.len());
                for (shown, &Argument::Card(id)) in shown_arguments.iter().zip(engine_arguments) {
                    let ObservedArgument::Card(handle) = *shown;
                    assert_eq!(observer.card_instance(handle), Some(id));
                }
            }
        } else {
            assert_eq!(listed, Vec::<ObservedAction>::new(), "seat {seat}");
        }
    }
}

/// Translating each Observed Legal Action of the active seat gives its engine Action, and
/// `transition` accepts it exactly as it accepts the Legal Action.
fn assert_translating_gives_the_accepted_engine_action(
    ruleset: &DeclarativeRuleset,
    creation: &GameCreation,
    position: &Position,
) {
    let state = &position.state;
    let legal = ruleset.legal_actions(state);
    assert!(!legal.is_empty(), "the position has Legal Actions");
    let seat = state.active_player().0;
    let observer = position.observer(ruleset, creation, seat);
    let listed = ruleset
        .observed_legal_actions(&observer, state)
        .expect("the Observed Legal Actions are listed");
    assert_eq!(listed.len(), legal.len());
    for (index, observed) in listed.iter().enumerate() {
        let translated = ruleset
            .translate_action(&observer, observed)
            .expect("an Observed Legal Action translates");
        assert_eq!(translated, legal[index], "action {index}");
        let accepted = ruleset.transition(state, &translated);
        assert!(accepted.is_ok(), "action {index} is accepted: {accepted:?}");
        assert_eq!(accepted, ruleset.transition(state, &legal[index]));
    }
}

/// `yaml` with the one occurrence of `from` replaced by `to`.
fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(
        yaml.matches(from).count(),
        1,
        "`{from}` occurs once in the fixture"
    );
    yaml.replace(from, to)
}

/// The Small Ruleset plus one action `peek`, listed in the Main phase, whose one param is over
/// the zone `param` declares (with its `owner`, if given). Test-only: nothing under `rulesets/` changes.
fn small_with_peek(param: &str) -> DeclarativeRuleset {
    let with_phase = edit(
        SMALL_RULESET,
        "actions: [play_creature, attack, pass_turn]",
        "actions: [play_creature, attack, pass_turn, peek]",
    );
    let with_action = edit(
        &with_phase,
        "  - id: pass_turn\n    effects: [ { end_turn: {} } ]\n",
        &format!(
            "  - id: pass_turn\n    effects: [ {{ end_turn: {{}} }} ]\n  - id: peek\n    params:\n      - {param}\n    effects:\n      - set_state: {{ card: {{ ref: card }}, state: tapped, value: true }}\n"
        ),
    );
    ruleset_from(&with_action, SMALL_CATALOG)
}

// ---- tests

#[test]
fn the_active_players_observed_legal_actions_match_legal_actions_at_the_opening_position() {
    let ruleset = effects();
    let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], EFFECTS_SEED);
    let position = Position::opening(&creation);
    assert_eq!(position.state.active_player(), p(0));
    assert_observed_legal_actions_match(&ruleset, &creation, &position);
}

#[test]
fn the_active_players_observed_legal_actions_match_legal_actions_at_a_later_main_phase() {
    let ruleset = effects();
    let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], EFFECTS_SEED);
    let position = effects_turn_3(&ruleset, &creation);
    assert_eq!(position.state.active_player(), p(0));
    let legal = ruleset.legal_actions(&position.state);
    for wanted in ["play_creature", "attack"] {
        assert!(
            legal
                .iter()
                .any(|legal_action| declared(legal_action).0.as_str() == wanted),
            "the later Main phase lists `{wanted}`"
        );
    }
    assert_observed_legal_actions_match(&ruleset, &creation, &position);
}

#[test]
fn translating_each_observed_legal_action_gives_the_engine_action_that_transition_accepts() {
    let ruleset = effects();
    let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], EFFECTS_SEED);
    assert_translating_gives_the_accepted_engine_action(
        &ruleset,
        &creation,
        &Position::opening(&creation),
    );
    assert_translating_gives_the_accepted_engine_action(
        &ruleset,
        &creation,
        &effects_turn_3(&ruleset, &creation),
    );
}

#[test]
fn an_attack_on_an_opponents_creature_and_leader_is_expressed_and_translated_with_observed_cards() {
    let ruleset = effects();
    let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], EFFECTS_SEED);
    let position = effects_turn_5(&ruleset, &creation);
    let state = &position.state;
    assert_eq!(state.active_player(), p(0));
    // The position: seat 0's creature is untapped, seat 1's creature is tapped.
    assert_eq!(state.zone_of(CREATURE_0), Some(&zone("field")));
    assert_eq!(state.zone_of(CREATURE_1), Some(&zone("field")));
    let legal = ruleset.legal_actions(state);
    assert!(legal.contains(&action(0, "attack", &[CREATURE_0, CREATURE_1])));
    assert!(legal.contains(&action(0, "attack", &[CREATURE_0, LEADER_1])));

    let observer = position.observer(&ruleset, &creation, 0);
    let observed_state = ruleset
        .observe(&observer, state)
        .expect("the GameState is observed");
    // The handles of the Viewer's Observed GameState: its creature in its `field`, the opponent's
    // creature in the opponent's `field` and the opponent's Leader in its `leader_zone`.
    let only_card = |owner: u32, zone_id: &str, index: usize| -> ObservedCard {
        observed_state
            .zone(p(owner), &zone(zone_id))
            .and_then(|observed_zone| observed_zone.cards())
            .and_then(|cards| cards.get(index).copied())
            .unwrap_or_else(|| panic!("{zone_id} of seat {owner} shows a card at {index}"))
    };
    let attacker = only_card(0, "field", 0);
    let target_creature = only_card(1, "field", 0);
    let target_leader = only_card(1, "leader_zone", 0);
    assert_eq!(observer.card_instance(attacker), Some(CREATURE_0));
    assert_eq!(observer.card_instance(target_creature), Some(CREATURE_1));
    assert_eq!(observer.card_instance(target_leader), Some(LEADER_1));

    let listed = ruleset
        .observed_legal_actions(&observer, state)
        .expect("the Observed Legal Actions are listed");
    let attacks: Vec<&ObservedAction> = listed
        .iter()
        .filter(|observed| observed_declared(observed).0.as_str() == "attack")
        .collect();
    assert_eq!(
        attacks,
        [
            &observed_action(0, "attack", &[attacker, target_leader]),
            &observed_action(0, "attack", &[attacker, target_creature]),
        ]
    );

    for (observed, engine) in [
        (
            observed_action(0, "attack", &[attacker, target_creature]),
            action(0, "attack", &[CREATURE_0, CREATURE_1]),
        ),
        (
            observed_action(0, "attack", &[attacker, target_leader]),
            action(0, "attack", &[CREATURE_0, LEADER_1]),
        ),
    ] {
        let translated = ruleset
            .translate_action(&observer, &observed)
            .expect("an Observed attack translates");
        assert_eq!(translated, engine);
        assert!(ruleset.transition(state, &translated).is_ok());
    }
}

#[test]
fn a_handle_the_viewer_never_received_is_an_unknown_observed_card() {
    let ruleset = effects();
    let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], EFFECTS_SEED);
    let position = effects_turn_5(&ruleset, &creation);
    let observer = position.observer(&ruleset, &creation, 0);
    let held = observer
        .observed_card(CREATURE_0)
        .expect("seat 0 sees its creature on the field");
    let never_given = ObservedCard(100_000);
    let also_never_given = ObservedCard(100_001);
    assert_eq!(observer.card_instance(never_given), None);

    let error = ruleset
        .translate_action(
            &observer,
            &observed_action(0, "play_creature", &[never_given]),
        )
        .expect_err("the Viewer holds no such handle");
    assert_eq!(
        error,
        ObservationError::UnknownObservedCard { card: never_given }
    );
    assert!(!format!("{error:?}").contains("CardInstanceId"));
    assert!(!format!("{error}").is_empty());

    // The first argument is held, the second is not: the error names the second.
    let error = ruleset
        .translate_action(
            &observer,
            &observed_action(0, "attack", &[held, also_never_given]),
        )
        .expect_err("the second handle is not held");
    assert_eq!(
        error,
        ObservationError::UnknownObservedCard {
            card: also_never_given
        }
    );
    // With both unknown, the first one in argument order is named.
    let error = ruleset
        .translate_action(
            &observer,
            &observed_action(0, "attack", &[never_given, also_never_given]),
        )
        .expect_err("neither handle is held");
    assert_eq!(
        error,
        ObservationError::UnknownObservedCard { card: never_given }
    );
}

#[test]
fn an_observed_action_of_another_player_is_rejected() {
    let ruleset = effects();
    let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], EFFECTS_SEED);
    let position = effects_turn_5(&ruleset, &creation);
    let observer = position.observer(&ruleset, &creation, 0);
    let held = observer
        .observed_card(CREATURE_0)
        .expect("seat 0 sees its creature on the field");

    // Viewer 0, an Observed Action of Player 1 with a handle the Viewer holds.
    let error = ruleset
        .translate_action(&observer, &observed_action(1, "play_creature", &[held]))
        .expect_err("the Observed Action is of another Player");
    assert_eq!(
        error,
        ObservationError::ActionOfAnotherPlayer { player: p(1) }
    );
    assert!(!format!("{error}").is_empty());

    // The Player check comes first: an unknown handle does not change the error.
    let error = ruleset
        .translate_action(
            &observer,
            &observed_action(1, "play_creature", &[ObservedCard(100_000)]),
        )
        .expect_err("the Observed Action is of another Player");
    assert_eq!(
        error,
        ObservationError::ActionOfAnotherPlayer { player: p(1) }
    );
}

#[test]
fn observed_legal_actions_work_on_the_small_ruleset() {
    let ruleset = small();
    let creation = create(&ruleset, [SMALL_DECK_A, SMALL_DECK_B], SMALL_SEED);
    let opening = Position::opening(&creation);
    let later = small_turn_3(&ruleset, &creation);
    assert!(ruleset
        .legal_actions(&later.state)
        .iter()
        .any(|legal_action| declared(legal_action).0.as_str() == "attack"));
    for position in [&opening, &later] {
        assert_observed_legal_actions_match(&ruleset, &creation, position);
        assert_translating_gives_the_accepted_engine_action(&ruleset, &creation, position);
    }
}

#[test]
fn a_legal_action_naming_a_card_its_player_cannot_see_is_an_error() {
    // One test-only ruleset per kind of unseen zone: the acting Player's own `deck` (hidden) and the
    // opponent's `hand` (owner).
    for (param, label) in [
        (
            "{ name: card, zone: deck, card_type: creature }",
            "own deck",
        ),
        (
            "{ name: card, zone: hand, card_type: creature, owner: opponent }",
            "opponent's hand",
        ),
    ] {
        let ruleset = small_with_peek(param);
        let creation = create(&ruleset, [SMALL_DECK_A, SMALL_DECK_B], SMALL_SEED);
        let position = Position::opening(&creation);
        let state = &position.state;

        // The engine does list `peek`: this is a real Legal Action of the Active Player.
        assert!(
            ruleset
                .legal_actions(state)
                .iter()
                .any(|legal_action| declared(legal_action).0.as_str() == "peek"),
            "{label}: the engine lists `peek`"
        );

        // Listing it would name an unseen card, so the whole call fails: no partial list.
        let observer = position.observer(&ruleset, &creation, 0);
        let error = ruleset
            .observed_legal_actions(&observer, state)
            .expect_err("the Active Player's list names an unseen card");
        assert_eq!(
            error,
            ObservationError::LegalActionNamesUnseenCard {
                action: ActionId::new("peek")
            },
            "{label}"
        );
        assert!(!format!("{error:?}").contains("CardInstanceId"));
        assert!(!format!("{error}").is_empty());

        // The other seat has no Legal Action, so nothing to leak and nothing to fail.
        let other = position.observer(&ruleset, &creation, 1);
        assert_eq!(
            ruleset.observed_legal_actions(&other, state),
            Ok(Vec::<ObservedAction>::new()),
            "{label}"
        );
    }
}

#[test]
fn an_observer_that_has_not_followed_the_game_state_is_out_of_step_when_listing() {
    // The Observer has folded the creation trace only; the GameState is two Turns later, so seat
    // 0's drawn card is in its hand, which it sees, and the Observer holds no handle for it.
    let ruleset = effects();
    let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], EFFECTS_SEED);
    let later = effects_turn_3(&ruleset, &creation);
    let stale = Position::opening(&creation).observer(&ruleset, &creation, 0);
    let legal = ruleset.legal_actions(&later.state);
    assert!(
        legal.iter().any(|legal_action| declared(legal_action)
            .1
            .iter()
            .any(|&Argument::Card(id)| stale.observed_card(id).is_none())),
        "a Legal Action names a card the stale Observer holds no handle for"
    );
    assert_eq!(
        ruleset.observed_legal_actions(&stale, &later.state),
        Err(ObservationError::ObserverOutOfStep)
    );
}

#[test]
fn an_observer_holding_a_handle_for_a_card_now_unseen_still_gets_the_unseen_card_error() {
    // The Observer folds the creation trace and then one crafted `CardMoved` (deck to hand) per card
    // any `peek` names, none of which the GameState shows: it holds a handle for every card a
    // `peek` names, all of them in the hidden `deck`. The unseen zone, read from the GameState,
    // decides: a held handle neither leaks the card nor turns the error into `ObserverOutOfStep`.
    let ruleset = small_with_peek("{ name: card, zone: deck, card_type: creature }");
    let creation = create(&ruleset, [SMALL_DECK_A, SMALL_DECK_B], SMALL_SEED);
    let position = Position::opening(&creation);
    let state = &position.state;
    let peeked: Vec<CardInstanceId> = ruleset
        .legal_actions(state)
        .iter()
        .filter(|legal_action| declared(legal_action).0.as_str() == "peek")
        .flat_map(|legal_action| declared(legal_action).1.iter().copied())
        .map(|Argument::Card(id)| id)
        .collect();
    assert!(!peeked.is_empty(), "the engine lists a `peek`");
    for &card in &peeked {
        assert_eq!(state.zone_of(card), Some(&zone("deck")));
    }

    let mut events = position.events.clone();
    for &card in &peeked {
        events.push(Event::CardMoved {
            card,
            player: p(0),
            from: zone("deck"),
            to: zone("hand"),
        });
    }
    let observer = ruleset
        .observer(viewer(0), &creation.empty, &events)
        .expect("the Observer is built");
    for &card in &peeked {
        assert!(
            observer.observed_card(card).is_some(),
            "the Observer holds a handle for every card a `peek` names"
        );
    }
    assert_eq!(
        ruleset.observed_legal_actions(&observer, state),
        Err(ObservationError::LegalActionNamesUnseenCard {
            action: ActionId::new("peek")
        })
    );
}

/// The Effects Ruleset with its `trash` hidden. Test-only: nothing under `rulesets/` changes.
fn effects_with_a_hidden_trash() -> DeclarativeRuleset {
    ruleset_from(
        &edit(
            EFFECTS_RULESET,
            "{ id: trash,       visibility: public, ordered: true }",
            "{ id: trash,       visibility: hidden, ordered: true }",
        ),
        EFFECTS_CATALOG,
    )
}

#[test]
fn translating_a_handle_whose_card_left_sight_is_an_unknown_observed_card() {
    // A `pyre_imp` enters the field, where its own `enter` trigger sends it on to the trash. With
    // the trash hidden the card leaves the Viewer's sight in the same transition, and the handle
    // it held in hand is retired (ADR-0028).
    let ruleset = effects_with_a_hidden_trash();
    let (creation, imp, seed) = (0..64)
        .find_map(|seed| {
            let creation = create(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], seed);
            let hand = creation.opening.state.zone(p(0), &zone("hand"))?.to_vec();
            let imp = hand.into_iter().find(|card| {
                creation
                    .opening
                    .state
                    .card(*card)
                    .is_some_and(|instance| instance.definition().as_str() == "pyre_imp")
            })?;
            Some((creation, imp, seed))
        })
        .expect("some Seed deals seat 0 a pyre_imp");
    let state = creation.opening.state.clone();
    let mut observer = ruleset
        .observer(viewer(0), &creation.empty, creation.opening.trace.events())
        .expect("the Observer is built");
    let handle = observer
        .observed_card(imp)
        .unwrap_or_else(|| panic!("seed {seed}: seat 0 sees the pyre_imp in its hand"));

    // Seat 0 lists `play_creature(h)` for the imp, and the listing translates back.
    let listed = ruleset
        .observed_legal_actions(&observer, &state)
        .expect("the Observed Legal Actions are listed");
    let old = observed_action(0, "play_creature", &[handle]);
    assert!(listed.contains(&old), "seed {seed}: the imp can be played");
    let play = action(0, "play_creature", &[imp]);
    assert_eq!(ruleset.translate_action(&observer, &old), Ok(play.clone()));

    // Take it: the card goes to the field and then to the (hidden) trash.
    let transition = ruleset
        .transition(&state, &play)
        .expect("the play is accepted");
    ruleset
        .follow_transition(&mut observer, &state, &play, &transition)
        .expect("the transition is followed");
    assert_eq!(transition.state.zone_of(imp), Some(&zone("trash")));
    assert_eq!(observer.observed_card(imp), None, "the card left sight");
    assert_eq!(
        observer.card_instance(handle),
        None,
        "the handle is retired"
    );

    // Translating the old Observed Action is a typed error naming the handle.
    assert_eq!(
        ruleset.translate_action(&observer, &old),
        Err(ObservationError::UnknownObservedCard { card: handle })
    );
}
