//! Acceptance tests for ticket #96, step 1 (ADR-0041): Per-Player state declared by the ruleset
//! (`player_state`), read as `{ ref: player.<id>, player: <selector> }` and written by `set_state`
//! and `add_state` with a `player:` target, through the one Event `PlayerStateChanged`.
//!
//! The ruleset, catalog and deck are the fixtures `player-state-*.yaml`: two Players, a card type
//! `unit` and an action for each context that reads or writes Player state. Each load error is
//! made by editing one line of the ruleset text, as `edit` says. The Players open with 4000
//! `life_points` and `summoned: false`, and each opens with the hand `tithe_unit` (card 5),
//! `tithe_unit` (4), `plain_unit` (3) for seat 0 and cards 11, 10, 9 for seat 1.

use tcg_engine::declarative::{
    Action, ApplyError, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation,
    GameState, Loss, Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, DefinitionError, DefinitionErrorKind, FieldType, FieldTypeDraft, FieldValue,
    ParamName, PlayerStateId, ReferenceTarget, WinConditionId, ZoneId,
};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition, LoadError};

const RULESET: &str = include_str!("fixtures/player-state-ruleset.yaml");
const CATALOG: &str = include_str!("fixtures/player-state-catalog.yaml");
const DECK: &str = include_str!("fixtures/player-state-deck.yaml");

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const SMALL_DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const SMALL_DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// The cards each Player opens with, by instance id (seat 0; seat 1 holds these plus 6).
const PLAIN_0: CardInstanceId = CardInstanceId(3);
const TITHE_0: CardInstanceId = CardInstanceId(5);
const OTHER_TITHE_0: CardInstanceId = CardInstanceId(4);

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn state_id(id: &str) -> PlayerStateId {
    PlayerStateId::new(id)
}

fn int(value: i64) -> FieldValue {
    FieldValue::Int(value)
}

fn flag(value: bool) -> FieldValue {
    FieldValue::Bool(value)
}

/// The Player state of a Player, in declared order: `life_points`, then `summoned`.
fn values(life_points: i64, summoned: bool) -> Vec<(PlayerStateId, FieldValue)> {
    vec![
        (state_id("life_points"), int(life_points)),
        (state_id("summoned"), flag(summoned)),
    ]
}

fn changed(player: u32, state: &str, value: FieldValue) -> Event {
    Event::PlayerStateChanged {
        player: p(player),
        state: state_id(state),
        value,
    }
}

/// The `PlayerStateChanged` Events of a trace, in order.
fn state_changes(events: &[Event]) -> Vec<Event> {
    events
        .iter()
        .filter(|event| matches!(event, Event::PlayerStateChanged { .. }))
        .cloned()
        .collect()
}

/// `yaml` with the one occurrence of `from` replaced by `to`.
fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(
        yaml.matches(from).count(),
        1,
        "`{from}` occurs exactly once in the text"
    );
    yaml.replacen(from, to, 1)
}

/// An Action of the fixture ruleset: the Player takes `id` with these cards as its arguments.
fn act(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn ruleset_of(ruleset_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn created(ruleset: &DeclarativeRuleset) -> GameCreation {
    let decks: Vec<_> = (0..2)
        .map(|_| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), DECK)
                .expect("the deck is legal")
        })
        .collect();
    ruleset
        .create_game(&decks, Seed(1))
        .expect("the decks create a game")
}

/// The fixture ruleset and its Opening Position.
fn opening() -> (DeclarativeRuleset, GameCreation) {
    let ruleset = ruleset_of(RULESET);
    let creation = created(&ruleset);
    let hand = ZoneId::new("hand");
    assert_eq!(
        creation.opening.state.zone(p(0), &hand),
        Some([TITHE_0, OTHER_TITHE_0, PLAIN_0].as_slice()),
        "seat 0 opens with this hand"
    );
    (ruleset, creation)
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

fn definition_error(yaml: &str) -> DefinitionError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Definition(error)) => error,
        other => panic!("expected a definition error, got {other:?}"),
    }
}

fn at_action(id: &str) -> DefinitionPath {
    DefinitionPath::root().key("actions").id(id)
}

/// The cards the Legal Actions take for the declared action `id`, in the order listed.
fn legal_cards(ruleset: &DeclarativeRuleset, state: &GameState, id: &str) -> Vec<CardInstanceId> {
    ruleset
        .legal_actions(state)
        .into_iter()
        .filter_map(|action| match action {
            Action::Declared {
                action, arguments, ..
            } if action.as_str() == id => match arguments.as_slice() {
                [Argument::Card(card)] => Some(*card),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

// ---- AC1: the declaration and the initial values

#[test]
fn every_player_starts_with_each_player_state_at_its_init() {
    let definition = load_ruleset_definition(RULESET).expect("the ruleset loads");
    let schemas = definition.player_state();
    assert_eq!(schemas.len(), 2, "declared order, one schema each");
    assert_eq!(schemas[0].id(), &state_id("life_points"));
    assert_eq!(schemas[0].state_type(), FieldType::Int);
    assert_eq!(schemas[0].init(), &int(4000));
    assert_eq!(schemas[1].id(), &state_id("summoned"));
    assert_eq!(schemas[1].state_type(), FieldType::Bool);
    assert_eq!(schemas[1].init(), &flag(false));

    let (_, creation) = opening();
    // The Empty GameState already holds the initial values, like its empty zones: no Event creates
    // them, and every later change is explained by an Event.
    for state in [&creation.empty, &creation.opening.state] {
        for seat in 0..2 {
            assert_eq!(
                state.player_state(p(seat)),
                Some(values(4000, false).as_slice()),
                "seat {seat}"
            );
        }
        assert_eq!(
            state.player_state_value(p(1), &state_id("life_points")),
            Some(&int(4000))
        );
        assert_eq!(
            state.player_state_value(p(0), &state_id("summoned")),
            Some(&flag(false))
        );
        // A Player who is not in the Game and a state nobody declared are not there.
        assert_eq!(state.player_state(p(2)), None);
        assert_eq!(
            state.player_state_value(p(2), &state_id("life_points")),
            None
        );
        assert_eq!(state.player_state_value(p(0), &state_id("mana")), None);
    }
    assert!(
        state_changes(creation.opening.trace.events()).is_empty(),
        "creation emits no Player state Event when nothing sets it"
    );
}

#[test]
fn a_ruleset_without_player_state_gives_every_player_none() {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the Small Ruleset loads");
    assert!(definition.player_state().is_empty());
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks: Vec<_> = [SMALL_DECK_A, SMALL_DECK_B]
        .into_iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect();
    let creation = ruleset.create_game(&decks, Seed(1)).expect("creation");
    for seat in 0..2 {
        assert_eq!(creation.empty.player_state(p(seat)), Some([].as_slice()));
        assert_eq!(
            creation.opening.state.player_state(p(seat)),
            Some([].as_slice())
        );
    }
}

#[test]
fn a_ref_to_an_undeclared_player_state_is_a_load_error() {
    // In a `where`: the slot of the ref.
    let read = edit(
        RULESET,
        "{ ref: player.summoned, player: active }",
        "{ ref: player.mana, player: active }",
    );
    assert_eq!(
        definition_error(&read),
        DefinitionError {
            path: at_action("summon")
                .key("params")
                .id("card")
                .key("where")
                .key("eq")
                .index(0),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::PlayerState,
                id: "mana".to_string(),
            },
        }
    );

    // In a write: the `state` key.
    let write = edit(
        RULESET,
        "set_state: { player: active, state: summoned, value: true }",
        "set_state: { player: active, state: mana, value: true }",
    );
    assert_eq!(
        definition_error(&write),
        DefinitionError {
            path: at_action("summon")
                .key("effects")
                .index(1)
                .key("set_state")
                .key("state"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::PlayerState,
                id: "mana".to_string(),
            },
        }
    );
}

#[test]
fn a_set_state_of_the_wrong_type_on_player_state_is_a_load_error() {
    let yaml = edit(
        RULESET,
        "set_state: { player: active, state: summoned, value: true }",
        "set_state: { player: active, state: summoned, value: 5 }",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: at_action("summon")
                .key("effects")
                .index(1)
                .key("set_state")
                .key("value"),
            kind: DefinitionErrorKind::StateValueTypeMismatch {
                state: "summoned".to_string(),
                declared: FieldType::Bool,
                found: FieldType::Int,
            },
        }
    );
}

#[test]
fn an_add_state_on_a_bool_player_state_is_a_load_error() {
    let yaml = edit(
        RULESET,
        "add_state: { player: active, state: life_points, amount: 500 }",
        "add_state: { player: active, state: summoned, amount: 500 }",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: at_action("heal")
                .key("effects")
                .index(0)
                .key("add_state")
                .key("amount"),
            kind: DefinitionErrorKind::StateValueTypeMismatch {
                state: "summoned".to_string(),
                declared: FieldType::Bool,
                found: FieldType::Int,
            },
        }
    );
}

#[test]
fn a_player_state_init_of_the_wrong_type_is_a_load_error() {
    let yaml = edit(
        RULESET,
        "{ id: life_points, type: int, init: 4000 }",
        "{ id: life_points, type: int, init: true }",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: DefinitionPath::root()
                .key("player_state")
                .id("life_points")
                .key("init"),
            kind: DefinitionErrorKind::InitializerTypeMismatch {
                declared: FieldType::Int,
                found: FieldType::Bool,
            },
        }
    );
}

#[test]
fn a_player_state_of_type_text_or_a_repeated_id_is_a_load_error() {
    let text = edit(
        RULESET,
        "  - { id: summoned, type: bool, init: false }",
        "  - { id: summoned, type: bool, init: false }\n  - { id: nickname, type: text, init: \"x\" }",
    );
    assert_eq!(
        definition_error(&text),
        DefinitionError {
            path: DefinitionPath::root()
                .key("player_state")
                .id("nickname")
                .key("type"),
            kind: DefinitionErrorKind::UnsupportedFieldType {
                field_type: FieldTypeDraft::Text,
            },
        }
    );

    let repeated = edit(
        RULESET,
        "  - { id: summoned, type: bool, init: false }",
        "  - { id: summoned, type: bool, init: false }\n  - { id: life_points, type: int, init: 1 }",
    );
    assert_eq!(
        definition_error(&repeated),
        DefinitionError {
            path: DefinitionPath::root().key("player_state").index(2),
            kind: DefinitionErrorKind::DuplicateId {
                id: "life_points".to_string(),
                first: DefinitionPath::root().key("player_state").index(0),
            },
        }
    );
}

#[test]
fn a_player_ref_of_the_wrong_type_for_its_use_is_a_load_error() {
    // A bool Player state where `lte` needs an int.
    let yaml = edit(
        RULESET,
        "{ ref: player.life_points, player: opponent }",
        "{ ref: player.summoned, player: opponent }",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: at_action("bully")
                .key("params")
                .id("card")
                .key("where")
                .key("lte")
                .index(0),
            kind: DefinitionErrorKind::ExpressionTypeMismatch {
                expected: FieldType::Int,
                found: FieldType::Bool,
            },
        }
    );
}

#[test]
fn a_player_ref_without_a_player_selector_is_a_load_error() {
    let yaml = edit(
        RULESET,
        "{ ref: player.life_points, player: opponent }",
        "{ ref: player.life_points }",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: at_action("bully")
                .key("params")
                .id("card")
                .key("where")
                .key("lte")
                .index(0),
            kind: DefinitionErrorKind::MissingPlayerSelector,
        }
    );
}

#[test]
fn controller_in_an_action_player_target_is_a_load_error() {
    let yaml = edit(
        RULESET,
        "add_state: { player: opponent, state: life_points, amount: -300 }",
        "add_state: { player: controller, state: life_points, amount: -300 }",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: at_action("drain")
                .key("effects")
                .index(0)
                .key("add_state")
                .key("player"),
            kind: DefinitionErrorKind::ControllerOutsideCardEffect,
        }
    );
}

#[test]
fn a_state_write_with_both_or_neither_of_card_and_player_is_a_load_error() {
    let at = at_action("mark_opponent")
        .key("effects")
        .index(0)
        .key("set_state");
    let expected = DefinitionError {
        path: at,
        kind: DefinitionErrorKind::InvalidStateTarget,
    };
    let both = edit(
        RULESET,
        "set_state: { player: opponent, state: summoned, value: true }",
        "set_state: { card: { ref: card }, player: opponent, state: summoned, value: true }",
    );
    assert_eq!(definition_error(&both), expected);
    let neither = edit(
        RULESET,
        "set_state: { player: opponent, state: summoned, value: true }",
        "set_state: { state: summoned, value: true }",
    );
    assert_eq!(definition_error(&neither), expected);
}

#[test]
fn a_param_named_player_is_a_load_error() {
    let yaml = edit(
        RULESET,
        "      - name: card\n        zone: hand\n        card_type: unit\n        where: { eq:",
        "      - name: player\n        zone: hand\n        card_type: unit\n        where: { eq:",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: at_action("summon").key("params").index(0).key("name"),
            kind: DefinitionErrorKind::ReservedId {
                id: "player".to_string(),
            },
        }
    );
}

#[test]
fn a_card_type_named_player_is_a_load_error() {
    let yaml = edit(RULESET, "  - id: unit\n", "  - id: player\n");
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: DefinitionPath::root().key("card_types").index(0).key("id"),
            kind: DefinitionErrorKind::ReservedId {
                id: "player".to_string(),
            },
        }
    );
}

// ---- AC2: writes, through the Event

#[test]
fn set_state_on_player_state_changes_only_that_players_field() {
    let (ruleset, creation) = opening();
    let input = creation.opening.state;
    let before = input.clone();

    // Seat 0 marks its opponent: only seat 1's `summoned` changes.
    let next = take(&ruleset, &input, &act(0, "mark_opponent", &[]));
    assert_eq!(
        next.trace.events().to_vec(),
        vec![changed(1, "summoned", flag(true))]
    );
    assert_eq!(
        next.state.player_state(p(0)),
        Some(values(4000, false).as_slice())
    );
    assert_eq!(
        next.state.player_state(p(1)),
        Some(values(4000, true).as_slice())
    );
    // The input GameState is the one it was before the call.
    assert_eq!(input, before);
}

#[test]
fn add_state_on_player_state_adds_its_amount() {
    let (ruleset, creation) = opening();
    let input = creation.opening.state;
    let before = input.clone();

    // A negative literal subtracts: seat 0 drains its opponent by 300.
    let drained = take(&ruleset, &input, &act(0, "drain", &[]));
    assert_eq!(
        drained.trace.events().to_vec(),
        vec![changed(1, "life_points", int(3700))]
    );
    assert_eq!(
        drained.state.player_state(p(0)),
        Some(values(4000, false).as_slice())
    );
    assert_eq!(
        drained.state.player_state(p(1)),
        Some(values(3700, false).as_slice())
    );

    // A positive one adds, on the acting Player.
    let healed = take(&ruleset, &input, &act(0, "heal", &[]));
    assert_eq!(
        healed.trace.events().to_vec(),
        vec![changed(0, "life_points", int(4500))]
    );
    assert_eq!(
        healed.state.player_state(p(0)),
        Some(values(4500, false).as_slice())
    );
    assert_eq!(
        healed.state.player_state(p(1)),
        Some(values(4000, false).as_slice())
    );
    assert_eq!(input, before);
}

#[test]
fn add_state_on_each_player_changes_every_player_in_seat_order() {
    let (ruleset, creation) = opening();
    // Seat 1 is the Active Player, and still seat 0 comes first.
    let passed = take(&ruleset, &creation.opening.state, &act(0, "pass_turn", &[]));
    assert_eq!(passed.state.active_player(), p(1));

    let next = take(&ruleset, &passed.state, &act(1, "drain_all", &[]));
    assert_eq!(
        next.trace.events().to_vec(),
        vec![
            changed(0, "life_points", int(3900)),
            changed(1, "life_points", int(3900)),
        ]
    );
    for seat in 0..2 {
        assert_eq!(
            next.state.player_state(p(seat)),
            Some(values(3900, false).as_slice())
        );
    }
}

// ---- AC2: the contexts that write

#[test]
fn player_state_is_written_in_a_setup_step() {
    let yaml = edit(
        RULESET,
        "  - draw: { player: each, from: deck, to: hand, count: 3 }",
        "  - draw: { player: each, from: deck, to: hand, count: 3 }\n  - set_state: { player: each, state: life_points, value: 3000 }\n  - add_state: { player: active, state: life_points, amount: 500 }",
    );
    let ruleset = ruleset_of(&yaml);
    let creation = created(&ruleset);

    // A `set_state` without a zone writes Player state, `each` in seat order; `add_state` is new in
    // a setup and writes Player state only.
    assert_eq!(
        state_changes(creation.opening.trace.events()),
        vec![
            changed(0, "life_points", int(3000)),
            changed(1, "life_points", int(3000)),
            changed(0, "life_points", int(3500)),
        ]
    );
    // The Empty GameState holds the initial values; the Events explain the Opening Position.
    assert_eq!(
        creation.empty.player_state(p(0)),
        Some(values(4000, false).as_slice())
    );
    assert_eq!(
        creation.opening.state.player_state(p(0)),
        Some(values(3500, false).as_slice())
    );
    assert_eq!(
        creation.opening.state.player_state(p(1)),
        Some(values(3000, false).as_slice())
    );
    assert_eq!(
        creation.empty.apply_trace(creation.opening.trace.events()),
        Ok(creation.opening.state.clone())
    );
}

#[test]
fn player_state_is_written_in_an_automatic_step() {
    let yaml = edit(
        RULESET,
        "    - id: start\n      automatic: []",
        "    - id: start\n      automatic:\n        - set_state: { player: active, state: summoned, value: true }\n        - add_state: { player: each, state: life_points, amount: 100 }",
    );
    let ruleset = ruleset_of(&yaml);
    let creation = created(&ruleset);

    // Turn 1 starts at creation: the Active Player is marked and every Player gains 100.
    assert_eq!(
        state_changes(creation.opening.trace.events()),
        vec![
            changed(0, "summoned", flag(true)),
            changed(0, "life_points", int(4100)),
            changed(1, "life_points", int(4100)),
        ]
    );
    let opening = creation.opening.state;
    assert_eq!(
        opening.player_state(p(0)),
        Some(values(4100, true).as_slice())
    );
    assert_eq!(
        opening.player_state(p(1)),
        Some(values(4100, false).as_slice())
    );

    // Turn 2 starts when seat 0 passes: the same steps run for seat 1, who is now Active.
    let next = take(&ruleset, &opening, &act(0, "pass_turn", &[]));
    assert_eq!(
        state_changes(next.trace.events()),
        vec![
            changed(1, "summoned", flag(true)),
            changed(0, "life_points", int(4200)),
            changed(1, "life_points", int(4200)),
        ]
    );
    assert_eq!(
        next.state.player_state(p(0)),
        Some(values(4200, true).as_slice())
    );
    assert_eq!(
        next.state.player_state(p(1)),
        Some(values(4200, true).as_slice())
    );
}

#[test]
fn player_state_is_written_by_an_action_cost() {
    let (ruleset, creation) = opening();
    let next = take(
        &ruleset,
        &creation.opening.state,
        &act(0, "pay_to_hurt", &[]),
    );
    // The cost is paid first, then the effect runs.
    assert_eq!(
        next.trace.events().to_vec(),
        vec![
            changed(0, "life_points", int(3000)),
            changed(1, "life_points", int(2000)),
        ]
    );
    assert_eq!(
        next.state.player_state(p(0)),
        Some(values(3000, false).as_slice())
    );
    assert_eq!(
        next.state.player_state(p(1)),
        Some(values(2000, false).as_slice())
    );
}

#[test]
fn player_state_is_written_by_a_triggered_card_effect() {
    let (ruleset, creation) = opening();
    // `tithe_unit` enters the field: its controller (seat 0) gains 500 Life Points and the
    // controller's opponent (seat 1) is marked. The action's own write comes first, the trigger
    // after the action's effects.
    let next = take(
        &ruleset,
        &creation.opening.state,
        &act(0, "summon", &[TITHE_0]),
    );
    assert_eq!(
        next.trace.events().to_vec(),
        vec![
            Event::CardMoved {
                card: TITHE_0,
                player: p(0),
                from: ZoneId::new("hand"),
                to: ZoneId::new("field"),
            },
            changed(0, "summoned", flag(true)),
            changed(0, "life_points", int(4500)),
            changed(1, "summoned", flag(true)),
        ]
    );
    assert_eq!(
        next.state.player_state(p(0)),
        Some(values(4500, true).as_slice())
    );
    assert_eq!(
        next.state.player_state(p(1)),
        Some(values(4000, true).as_slice())
    );
}

#[test]
fn an_unchanged_player_state_emits_no_event() {
    let (ruleset, creation) = opening();
    let input = creation.opening.state;

    // The value is the one the Player has, and an `add_state` of 0 adds nothing.
    let same = take(&ruleset, &input, &act(0, "keep_flag", &[]));
    assert!(same.trace.events().is_empty());
    assert_eq!(same.state, input);

    // A second `set_state` to the same value changes nothing either.
    let marked = take(&ruleset, &input, &act(0, "mark_opponent", &[]));
    assert_eq!(state_changes(marked.trace.events()).len(), 1);
    let again = take(&ruleset, &marked.state, &act(0, "mark_opponent", &[]));
    assert!(again.trace.events().is_empty());
    assert_eq!(again.state, marked.state);
}

// ---- AC2: apply

#[test]
fn applying_player_state_changed_reproduces_the_change() {
    let (ruleset, creation) = opening();
    let mut state = creation.opening.state.clone();
    let mut events = creation.opening.trace.events().to_vec();
    let script = [
        act(0, "summon", &[TITHE_0]),
        act(0, "pass_turn", &[]),
        act(1, "drain_all", &[]),
        act(1, "pay_to_hurt", &[]),
    ];
    for action in &script {
        let next = take(&ruleset, &state, action);
        events.extend(next.trace.events().iter().cloned());
        // Each trace applied to the state before it gives the state after it.
        assert_eq!(
            state.apply_trace(next.trace.events()),
            Ok(next.state.clone())
        );
        state = next.state;
    }
    assert!(
        state_changes(&events).len() >= 6,
        "the script changes Player state several times"
    );
    // The Empty GameState and the whole record give the final state.
    assert_eq!(creation.empty.apply_trace(&events), Ok(state.clone()));
    assert_eq!(
        state.player_state(p(0)),
        Some(values(2400, true).as_slice())
    );
    assert_eq!(
        state.player_state(p(1)),
        Some(values(2900, true).as_slice())
    );
}

#[test]
fn player_state_changed_of_an_unknown_state_or_wrong_type_does_not_apply() {
    let (_, creation) = opening();
    let mut state = creation.opening.state;
    let before = state.clone();

    assert_eq!(
        state.apply(&changed(0, "mana", int(1))),
        Err(ApplyError::UnknownPlayerState {
            player: p(0),
            state: state_id("mana"),
        })
    );
    assert_eq!(
        state.apply(&changed(1, "life_points", flag(true))),
        Err(ApplyError::PlayerStateTypeMismatch {
            player: p(1),
            state: state_id("life_points"),
            expected: FieldType::Int,
            found: FieldType::Bool,
        })
    );
    assert_eq!(
        state.apply(&changed(7, "life_points", int(1))),
        Err(ApplyError::UnknownPlayer { player: p(7) })
    );
    assert_eq!(state, before, "a rejected Event changes nothing");

    // A well-formed one applies.
    assert_eq!(state.apply(&changed(1, "life_points", int(1))), Ok(()));
    assert_eq!(state.player_state(p(1)), Some(values(1, false).as_slice()));
}

#[test]
fn an_add_state_overflowing_player_state_is_rejected() {
    let (ruleset, creation) = opening();
    let input = creation.opening.state;
    let before = input.clone();
    assert_eq!(
        ruleset.transition(&input, &act(0, "overflow", &[])),
        Err(Rejection::PlayerStateOverflow {
            player: p(0),
            state: state_id("life_points"),
        })
    );
    assert_eq!(input, before);
}

// ---- reads

#[test]
fn a_where_reading_player_state_decides_whether_an_action_is_legal() {
    let (ruleset, creation) = opening();
    let start = creation.opening.state;

    // Seat 0 has not summoned and its opponent has all its Life Points: `summon` is legal for every
    // card of the hand, `bully` for none.
    assert_eq!(
        legal_cards(&ruleset, &start, "summon"),
        vec![PLAIN_0, OTHER_TITHE_0, TITHE_0]
    );
    assert!(legal_cards(&ruleset, &start, "bully").is_empty());

    // The opponent loses Life Points: `bully` reads the opponent's state and is now legal.
    let drained = take(&ruleset, &start, &act(0, "drain", &[]));
    assert_eq!(
        legal_cards(&ruleset, &drained.state, "bully"),
        vec![PLAIN_0, OTHER_TITHE_0, TITHE_0]
    );

    // Once seat 0 has summoned, `summon` reads the flag and is illegal; the Rejection is the
    // `where`'s.
    let summoned = take(&ruleset, &drained.state, &act(0, "summon", &[PLAIN_0]));
    assert!(legal_cards(&ruleset, &summoned.state, "summon").is_empty());
    assert_eq!(
        legal_cards(&ruleset, &summoned.state, "bully"),
        vec![OTHER_TITHE_0, TITHE_0]
    );
    assert_eq!(
        ruleset.transition(&summoned.state, &act(0, "summon", &[TITHE_0])),
        Err(Rejection::WhereConditionNotMet {
            param: ParamName::new("card"),
            card: TITHE_0,
        })
    );
}

// ---- AC3: win conditions on Player state (ticket #96, step 2)
//
// A Player-state win condition is a `when` expression whose refs are Player Refs without a
// `player:` key: it holds for a Player when it is true reading that Player's state, in the GameState
// the transition produced. The tests edit the fixture: a smaller `init`, and a `win_conditions`
// section appended to the text.

/// One win condition entry: `lte: [{ ref: player.life_points }, threshold]`, loser `owner`.
fn life_condition(id: &str, threshold: i64) -> String {
    format!(
        "  - id: {id}\n    when: {{ lte: [{{ ref: player.life_points }}, {threshold}] }}\n    loser: owner\n"
    )
}

/// The fixture with these win conditions appended, and every Player starting at `init` life.
fn with_win_conditions(init: i64, conditions: &[String]) -> String {
    let smaller = edit(
        RULESET,
        "{ id: life_points, type: int, init: 4000 }",
        &format!("{{ id: life_points, type: int, init: {init} }}"),
    );
    format!("{smaller}\nwin_conditions:\n{}", conditions.concat())
}

/// A Game of the fixture whose Players start at `init` life and lose at 0 or less (`life_out`).
fn life_out_game(init: i64) -> (DeclarativeRuleset, GameCreation) {
    let ruleset = ruleset_of(&with_win_conditions(init, &[life_condition("life_out", 0)]));
    let creation = created(&ruleset);
    (ruleset, creation)
}

fn loss(player: u32, reason: &str) -> Loss {
    Loss {
        player: p(player),
        reason: WinConditionId::new(reason),
    }
}

#[test]
fn a_player_state_win_condition_ends_the_game_when_a_field_reaches_its_threshold() {
    let (ruleset, creation) = life_out_game(600);
    let start = creation.opening.state;
    assert!(start.outcome().is_none());

    // One transition short of 0: seat 1 has 300 left, and the Game goes on.
    let short = take(&ruleset, &start, &act(0, "drain", &[]));
    assert_eq!(
        short.trace.events().to_vec(),
        vec![changed(1, "life_points", int(300))]
    );
    assert!(short.state.outcome().is_none());

    // The transition that reaches exactly 0 ends it: `GameEnded` is its last Event, and the Player
    // whose state was read is the loser.
    let last = take(&ruleset, &short.state, &act(0, "drain", &[]));
    assert_eq!(
        last.trace.events().to_vec(),
        vec![
            changed(1, "life_points", int(0)),
            Event::GameEnded {
                losses: vec![loss(1, "life_out")],
            },
        ]
    );
    let outcome = last.state.outcome().expect("the Game has ended");
    assert_eq!(outcome.winners(), [p(0)].as_slice());
    assert_eq!(outcome.losses(), [loss(1, "life_out")].as_slice());
    assert!(!outcome.is_drawn_game());

    // No Action is accepted afterwards.
    assert_eq!(
        ruleset.transition(&last.state, &act(0, "drain", &[])),
        Err(Rejection::GameOver)
    );
}

#[test]
fn a_player_state_below_its_threshold_also_loses() {
    // 200 life, a drain of 300: from above 0 to -100.
    let (ruleset, creation) = life_out_game(200);
    let ended = take(&ruleset, &creation.opening.state, &act(0, "drain", &[]));
    assert_eq!(
        ended.trace.events().to_vec(),
        vec![
            changed(1, "life_points", int(-100)),
            Event::GameEnded {
                losses: vec![loss(1, "life_out")],
            },
        ]
    );
    let outcome = ended.state.outcome().expect("the Game has ended");
    assert_eq!(outcome.winners(), [p(0)].as_slice());
}

#[test]
fn both_players_reaching_the_threshold_in_one_transition_is_a_drawn_game() {
    let (ruleset, creation) = life_out_game(100);
    // Seat 1 is the Active Player, and the losses are still in seat order.
    let passed = take(&ruleset, &creation.opening.state, &act(0, "pass_turn", &[]));
    assert!(passed.state.outcome().is_none());
    assert_eq!(passed.state.active_player(), p(1));

    let drawn = take(&ruleset, &passed.state, &act(1, "drain_all", &[]));
    assert_eq!(
        drawn.trace.events().to_vec(),
        vec![
            changed(0, "life_points", int(0)),
            changed(1, "life_points", int(0)),
            Event::GameEnded {
                losses: vec![loss(0, "life_out"), loss(1, "life_out")],
            },
        ]
    );
    let outcome = drawn.state.outcome().expect("the Game has ended");
    assert!(outcome.is_drawn_game());
    assert!(outcome.winners().is_empty());
}

#[test]
fn a_player_state_win_condition_is_judged_at_the_end_of_creation() {
    // Both Players start at 0: the Opening Position already holds the Terminal Outcome.
    let (_, creation) = life_out_game(0);
    let outcome = creation
        .opening
        .state
        .outcome()
        .expect("the Game ends at creation");
    assert!(outcome.is_drawn_game());
    assert_eq!(
        outcome.losses(),
        [loss(0, "life_out"), loss(1, "life_out")].as_slice()
    );
    assert_eq!(
        creation.opening.trace.events().last(),
        Some(&Event::GameEnded {
            losses: vec![loss(0, "life_out"), loss(1, "life_out")],
        })
    );
    // The Empty GameState is not judged: it is only the starting point of the fold.
    assert!(creation.empty.outcome().is_none());
}

#[test]
fn the_first_win_condition_in_declaration_order_is_a_players_reason() {
    // A Player who meets both conditions in one transition gets the one declared first.
    let early_first = with_win_conditions(
        300,
        &[
            life_condition("early_out", 0),
            life_condition("life_out", 0),
        ],
    );
    let ruleset = ruleset_of(&early_first);
    let creation = created(&ruleset);
    let ended = take(&ruleset, &creation.opening.state, &act(0, "drain", &[]));
    assert_eq!(
        ended.state.outcome().expect("ended").losses(),
        [loss(1, "early_out")].as_slice()
    );

    let late_first = with_win_conditions(
        300,
        &[
            life_condition("life_out", 0),
            life_condition("early_out", 0),
        ],
    );
    let ruleset = ruleset_of(&late_first);
    let creation = created(&ruleset);
    let ended = take(&ruleset, &creation.opening.state, &act(0, "drain", &[]));
    assert_eq!(
        ended.state.outcome().expect("ended").losses(),
        [loss(1, "life_out")].as_slice()
    );
}

#[test]
fn applying_the_trace_of_a_player_state_loss_reproduces_the_outcome() {
    let (ruleset, creation) = life_out_game(100);
    let mut state = creation.opening.state.clone();
    let mut events = creation.opening.trace.events().to_vec();
    for action in [act(0, "pass_turn", &[]), act(1, "drain_all", &[])] {
        let next = take(&ruleset, &state, &action);
        events.extend(next.trace.events().iter().cloned());
        assert_eq!(
            state.apply_trace(next.trace.events()),
            Ok(next.state.clone())
        );
        state = next.state;
    }
    // The whole record folded over the Empty GameState gives the final state, Terminal Outcome
    // included.
    let folded = creation
        .empty
        .apply_trace(&events)
        .expect("the record applies");
    assert_eq!(folded, state);
    assert_eq!(folded.outcome(), state.outcome());
    assert!(folded.outcome().expect("ended").is_drawn_game());
    // Nothing applies after `GameEnded`.
    assert_eq!(
        state.clone().apply(&changed(0, "life_points", int(5))),
        Err(ApplyError::GameAlreadyEnded)
    );
}

#[test]
fn a_player_selector_in_a_win_condition_is_a_load_error() {
    let yaml = edit(
        &with_win_conditions(4000, &[life_condition("life_out", 0)]),
        "{ ref: player.life_points }, 0",
        "{ ref: player.life_points, player: active }, 0",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: DefinitionPath::root()
                .key("win_conditions")
                .id("life_out")
                .key("when")
                .key("lte")
                .index(0),
            kind: DefinitionErrorKind::PlayerSelectorInWinCondition,
        }
    );
}

#[test]
fn a_win_condition_mixing_card_and_player_refs_is_a_load_error() {
    let mixed = "  - id: life_out\n    when: { and: [{ lte: [{ ref: player.life_points }, 0] }, { lte: [{ ref: unit.power }, 0] }] }\n    loser: owner\n";
    let yaml = with_win_conditions(4000, &[mixed.to_string()]);
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: DefinitionPath::root()
                .key("win_conditions")
                .id("life_out")
                .key("when"),
            kind: DefinitionErrorKind::MixedWinConditionSubject,
        }
    );
}

#[test]
fn a_win_condition_on_an_undeclared_player_state_is_a_load_error() {
    let yaml = edit(
        &with_win_conditions(4000, &[life_condition("life_out", 0)]),
        "{ ref: player.life_points }, 0",
        "{ ref: player.mana }, 0",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: DefinitionPath::root()
                .key("win_conditions")
                .id("life_out")
                .key("when")
                .key("lte")
                .index(0),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::PlayerState,
                id: "mana".to_string(),
            },
        }
    );
}
