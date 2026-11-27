//! Acceptance tests for ticket #97 (ADR-0042): several action phases, `advance_phase`, the `count`
//! expression and the `first_turn` expression.
//!
//! The ruleset, catalog and decks are the fixtures `phases-*.yaml`: two Players, a card type `unit`
//! and a Turn `first` (automatic), `main` (actions), `between` (automatic, draws a card), `battle`
//! (actions), `end` (automatic, adds 1 to `turns_ended`). Each load error is made by editing one
//! line of the ruleset text, as `edit` says. Each Player opens with three `plain_unit` in hand:
//! seat 0 holds the cards 5, 4, 3 and seat 1 the cards 11, 10, 9. The Duel Ruleset's own rules are
//! tested in `duel_ruleset.rs`.

use tcg_engine::card_catalog::{CatalogError, CatalogErrorKind};
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState,
    ObservedEvent, Rejection, Viewer,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, DefinitionError, DefinitionErrorKind, FieldValue, ParamName, PhaseId, PhaseKind,
    PlayerStateId, ReferenceTarget, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, CatalogLoadError, LoadError,
};

const RULESET: &str = include_str!("fixtures/phases-ruleset.yaml");
const CATALOG: &str = include_str!("fixtures/phases-catalog.yaml");
const DECK: &str = include_str!("fixtures/phases-deck.yaml");
const WATCHER_DECK: &str = include_str!("fixtures/phases-watcher-deck.yaml");

/// The hand of seat 0 is `[CARD_A, CARD_B, CARD_C]` and that of seat 1 `[OTHER_A, OTHER_B, OTHER_C]`.
const CARD_A: CardInstanceId = CardInstanceId(5);
const CARD_B: CardInstanceId = CardInstanceId(4);
const CARD_C: CardInstanceId = CardInstanceId(3);
const OTHER_A: CardInstanceId = CardInstanceId(11);

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn phase(id: &str) -> PhaseId {
    PhaseId::new(id)
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

fn act(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn game_with(deck: &str) -> (DeclarativeRuleset, GameCreation) {
    let definition = load_ruleset_definition(RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, CATALOG).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks: Vec<_> = (0..2)
        .map(|_| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), deck)
                .expect("the deck is legal")
        })
        .collect();
    let creation = ruleset
        .create_game(&decks, Seed(1))
        .expect("the decks create a game");
    (ruleset, creation)
}

/// The fixture ruleset and its Opening Position: Turn 1, seat 0, waiting in Main.
fn opening() -> (DeclarativeRuleset, GameCreation) {
    let (ruleset, creation) = game_with(DECK);
    let state = &creation.opening.state;
    assert_eq!(state.phase(), Some(&phase("main")));
    assert_eq!(
        state.zone(p(0), &zone("hand")),
        Some([CARD_A, CARD_B, CARD_C].as_slice())
    );
    assert_eq!(state.zone(p(1), &zone("hand")).map(<[_]>::len), Some(3));
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

/// Takes each Action in turn and returns the last state.
fn play(ruleset: &DeclarativeRuleset, state: &GameState, actions: &[Action]) -> GameState {
    let mut state = state.clone();
    for action in actions {
        state = take(ruleset, &state, action).state;
    }
    state
}

fn is_legal(ruleset: &DeclarativeRuleset, state: &GameState, action: &Action) -> bool {
    ruleset.legal_actions(state).contains(action)
}

/// Whether some Legal Action is the declared action `id`, whatever its arguments.
fn offers(ruleset: &DeclarativeRuleset, state: &GameState, id: &str) -> bool {
    ruleset
        .legal_actions(state)
        .iter()
        .any(|action| matches!(action, Action::Declared { action, .. } if action.as_str() == id))
}

fn phase_began(id: &str) -> Event {
    Event::PhaseBegan { phase: phase(id) }
}

fn turn_began(player: u32, turn: u32) -> Event {
    Event::TurnBegan {
        player: p(player),
        turn: TurnNumber::new(turn).expect("a Turn Number is at least 1"),
    }
}

/// The draw of the `between` phase: the Top card of the deck of `player` goes to their hand.
fn drawn(state: &GameState, player: u32) -> Event {
    let card = *state
        .zone(p(player), &zone("deck"))
        .and_then(<[_]>::last)
        .expect("the deck has a card");
    Event::CardMoved {
        card,
        player: p(player),
        from: zone("deck"),
        to: zone("hand"),
    }
}

fn turn_ended(player: u32, count: i64) -> Event {
    Event::PlayerStateChanged {
        player: p(player),
        state: PlayerStateId::new("turns_ended"),
        value: FieldValue::Int(count),
    }
}

fn life(state: &GameState, seat: u32) -> i64 {
    match state.player_state_value(p(seat), &PlayerStateId::new("life_points")) {
        Some(FieldValue::Int(value)) => *value,
        other => panic!("life_points is {other:?}"),
    }
}

fn where_not_met(card: CardInstanceId) -> Rejection {
    Rejection::WhereConditionNotMet {
        param: ParamName::new("card"),
        card,
    }
}

fn not_in_phase(action: &str) -> Rejection {
    Rejection::ActionNotInCurrentPhase {
        action: ActionId::new(action),
    }
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

/// The path of the first operand of the `lte` of the `where` of `summon`, which holds the `count`.
fn summon_count() -> DefinitionPath {
    at_action("summon")
        .key("params")
        .id("card")
        .key("where")
        .key("lte")
        .index(0)
}

fn catalog_error(extra: &str) -> CatalogError {
    let definition = load_ruleset_definition(RULESET).expect("the ruleset loads");
    let catalog = format!("{CATALOG}{extra}");
    match load_card_catalog(&definition, &catalog) {
        Err(CatalogLoadError::Catalog(error)) => error,
        other => panic!("expected a catalog error, got {other:?}"),
    }
}

// ---- several action phases

#[test]
fn a_turn_with_two_action_phases_loads() {
    let definition = load_ruleset_definition(RULESET).expect("two action phases load");
    let phases = definition.turn().expect("the ruleset has a turn").phases();
    let ids: Vec<&str> = phases.iter().map(|phase| phase.id().as_str()).collect();
    assert_eq!(ids, ["first", "main", "between", "battle", "end"]);
    let listed: Vec<Vec<&str>> = phases
        .iter()
        .filter_map(|phase| match phase.kind() {
            PhaseKind::Actions(actions) => Some(actions.iter().map(ActionId::as_str).collect()),
            PhaseKind::Automatic(_) => None,
        })
        .collect();
    assert_eq!(
        listed,
        [
            vec!["summon", "to_battle", "end_turn"],
            vec!["strike", "raid", "end_turn"]
        ]
    );
}

#[test]
fn advance_phase_runs_the_automatic_phases_between_and_waits_in_the_next_action_phase() {
    let (ruleset, creation) = opening();
    let input = creation.opening.state;
    let before = input.clone();

    let moved = take(&ruleset, &input, &act(0, "to_battle", &[]));
    assert_eq!(
        moved.trace.events().to_vec(),
        vec![
            phase_began("between"),
            drawn(&input, 0),
            phase_began("battle"),
        ]
    );
    assert_eq!(moved.state.phase(), Some(&phase("battle")));
    // The Turn goes on: the same Active Player, the same Turn Number.
    assert_eq!(moved.state.active_player(), p(0));
    assert_eq!(moved.state.turn_number(), TurnNumber::new(1));
    assert_eq!(
        moved.state.zone(p(0), &zone("hand")).map(<[_]>::len),
        Some(4)
    );
    assert_eq!(input, before);
}

#[test]
fn end_turn_from_the_first_action_phase_passes_over_the_second() {
    let (ruleset, creation) = opening();
    let input = creation.opening.state;

    let ended = take(&ruleset, &input, &act(0, "end_turn", &[]));
    // The automatic phases after Main still run; Battle is passed over without beginning.
    assert_eq!(
        ended.trace.events().to_vec(),
        vec![
            phase_began("between"),
            drawn(&input, 0),
            phase_began("end"),
            turn_ended(0, 1),
            turn_began(1, 2),
            phase_began("first"),
            phase_began("main"),
        ]
    );
    assert!(!ended.trace.events().contains(&phase_began("battle")));
    assert_eq!(ended.state.active_player(), p(1));
    assert_eq!(ended.state.turn_number(), TurnNumber::new(2));
    assert_eq!(ended.state.phase(), Some(&phase("main")));
}

#[test]
fn end_turn_from_the_second_action_phase_ends_the_turn() {
    let (ruleset, creation) = opening();
    let in_battle = take(&ruleset, &creation.opening.state, &act(0, "to_battle", &[])).state;

    let ended = take(&ruleset, &in_battle, &act(0, "end_turn", &[]));
    assert_eq!(
        ended.trace.events().to_vec(),
        vec![
            phase_began("end"),
            turn_ended(0, 1),
            turn_began(1, 2),
            phase_began("first"),
            phase_began("main"),
        ]
    );
    assert_eq!(ended.state.active_player(), p(1));
    assert_eq!(ended.state.turn_number(), TurnNumber::new(2));
    assert_eq!(ended.state.phase(), Some(&phase("main")));
}

#[test]
fn an_action_of_one_action_phase_is_rejected_in_the_other() {
    let (ruleset, creation) = opening();
    // One card in the field, so that Battle has a card to act with.
    let in_main = take(
        &ruleset,
        &creation.opening.state,
        &act(0, "summon", &[CARD_A]),
    )
    .state;

    // Main: Battle's actions are not legal.
    for id in ["strike", "raid"] {
        let input = in_main.clone();
        assert_eq!(
            ruleset.transition(&input, &act(0, id, &[CARD_A])),
            Err(not_in_phase(id))
        );
        assert_eq!(input, in_main, "the input is unchanged");
    }
    assert!(offers(&ruleset, &in_main, "summon"));
    assert!(offers(&ruleset, &in_main, "to_battle"));
    assert!(offers(&ruleset, &in_main, "end_turn"));
    assert!(!offers(&ruleset, &in_main, "strike"));
    assert!(!offers(&ruleset, &in_main, "raid"));

    // Battle: Main's actions are not legal, `to_battle` included (there is no way back, and no
    // way forward from the last action phase).
    let in_battle = take(&ruleset, &in_main, &act(0, "to_battle", &[])).state;
    for (id, cards) in [("summon", vec![CARD_B]), ("to_battle", vec![])] {
        let input = in_battle.clone();
        assert_eq!(
            ruleset.transition(&input, &act(0, id, &cards)),
            Err(not_in_phase(id))
        );
        assert_eq!(input, in_battle, "the input is unchanged");
    }
    assert!(!offers(&ruleset, &in_battle, "summon"));
    assert!(!offers(&ruleset, &in_battle, "to_battle"));
    assert!(offers(&ruleset, &in_battle, "end_turn"));
    // The card is in the field and the opponent has none: `raid` is legal; `strike` is not on
    // Turn 1.
    assert!(is_legal(&ruleset, &in_battle, &act(0, "raid", &[CARD_A])));
}

// ---- load errors of the phases

#[test]
fn a_turn_without_an_action_phase_is_rejected() {
    // Both action phases made automatic: the Game would never wait for a Player.
    let one = edit(
        RULESET,
        "actions: [summon, to_battle, end_turn]",
        "automatic: []",
    );
    let none = edit(&one, "actions: [strike, raid, end_turn]", "automatic: []");
    assert_eq!(
        definition_error(&none),
        DefinitionError {
            path: DefinitionPath::root().key("turn").key("phases"),
            kind: DefinitionErrorKind::NoActionPhase,
        }
    );
}

#[test]
fn advance_phase_from_the_last_action_phase_is_rejected_at_load() {
    // `to_battle` listed by Battle, the last action phase: nowhere to go.
    let yaml = edit(
        RULESET,
        "actions: [strike, raid, end_turn]",
        "actions: [strike, to_battle, raid, end_turn]",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: DefinitionPath::root()
                .key("turn")
                .key("phases")
                .id("battle"),
            kind: DefinitionErrorKind::AdvancePhaseFromLastActionPhase {
                action: "to_battle".to_string()
            },
        }
    );
    // Listed by Main, which has a later action phase, it loads.
    assert!(load_ruleset_definition(RULESET).is_ok());
}

#[test]
fn a_misplaced_advance_phase_is_rejected_at_its_path() {
    let to_battle = "effects: [ { advance_phase: {} } ]";

    // Not last: another effect follows it.
    let not_last = edit(
        RULESET,
        to_battle,
        "effects: [ { advance_phase: {} }, { add_state: { player: active, state: life_points, amount: 1 } } ]",
    );
    assert_eq!(
        definition_error(&not_last),
        DefinitionError {
            path: at_action("to_battle").key("effects").index(0),
            kind: DefinitionErrorKind::AdvancePhaseNotLast,
        }
    );

    // Inside an `if`, at any depth.
    let in_if = edit(
        RULESET,
        to_battle,
        "effects: [ { if: true, then: [ { if: true, then: [ { advance_phase: {} } ] } ] } ]",
    );
    assert_eq!(
        definition_error(&in_if),
        DefinitionError {
            path: at_action("to_battle")
                .key("effects")
                .index(0)
                .key("then")
                .index(0)
                .key("then")
                .index(0),
            kind: DefinitionErrorKind::AdvancePhaseInIfEffect,
        }
    );

    // In a pushed action: one a Response Window opens.
    let pushed =
        format!("{RULESET}\nwindows:\n  - {{ id: reply, opened_by: [to_battle], actions: [] }}\n");
    assert_eq!(
        definition_error(&pushed),
        DefinitionError {
            path: at_action("to_battle").key("effects").index(0),
            kind: DefinitionErrorKind::AdvancePhaseInPushedAction,
        }
    );
}

#[test]
fn advance_phase_in_a_card_effect_is_rejected() {
    let effects = "  - id: skipper\n    type: unit\n    fields: { name: \"Skipper\", power: 1 }\n    effects:\n      - { on: { enter: field }, do: [ { advance_phase: {} } ] }\n";
    assert_eq!(
        catalog_error(effects),
        CatalogError {
            path: DefinitionPath::root()
                .key("cards")
                .id("skipper")
                .key("effects")
                .index(0)
                .key("do")
                .index(0),
            kind: CatalogErrorKind::AdvancePhaseInCardEffect,
        }
    );
    // Nested in an `if` of the `do`: the same error, at the `advance_phase`.
    let nested = "  - id: skipper\n    type: unit\n    fields: { name: \"Skipper\", power: 1 }\n    effects:\n      - { on: { enter: field }, do: [ { if: true, then: [ { advance_phase: {} } ] } ] }\n";
    assert_eq!(
        catalog_error(nested).kind,
        CatalogErrorKind::AdvancePhaseInCardEffect
    );
}

// ---- count

#[test]
fn count_reads_the_cards_in_a_players_zone() {
    let (ruleset, creation) = opening();
    let opening = creation.opening.state;

    // `summon` needs at most 1 card in the field. With 0: every card of the hand can be summoned.
    for card in [CARD_A, CARD_B, CARD_C] {
        assert!(is_legal(&ruleset, &opening, &act(0, "summon", &[card])));
    }
    // With 1 (the boundary): still legal.
    let one = play(&ruleset, &opening, &[act(0, "summon", &[CARD_A])]);
    assert!(is_legal(&ruleset, &one, &act(0, "summon", &[CARD_B])));
    // With 2: not legal, and the rejection is the `where`.
    let two = play(&ruleset, &one, &[act(0, "summon", &[CARD_B])]);
    let before = two.clone();
    assert_eq!(
        ruleset.transition(&two, &act(0, "summon", &[CARD_C])),
        Err(where_not_met(CARD_C))
    );
    assert_eq!(two, before);
    assert!(!offers(&ruleset, &two, "summon"));

    // `raid` reads the opponent's zone: it is empty now, so `raid` is legal, and `count` is never
    // the acting Player's own zone there.
    let in_battle = take(&ruleset, &two, &act(0, "to_battle", &[])).state;
    assert!(is_legal(&ruleset, &in_battle, &act(0, "raid", &[CARD_A])));
    assert!(is_legal(&ruleset, &in_battle, &act(0, "raid", &[CARD_B])));
}

#[test]
fn count_changes_as_cards_move() {
    let (ruleset, creation) = opening();
    // Turn 1: seat 0 summons and goes to Battle. Seat 1's field is empty: `raid` is legal.
    let t1_battle = play(
        &ruleset,
        &creation.opening.state,
        &[act(0, "summon", &[CARD_A]), act(0, "to_battle", &[])],
    );
    assert!(is_legal(&ruleset, &t1_battle, &act(0, "raid", &[CARD_A])));

    // Turn 2: seat 1 summons into its own field; for seat 1 the opponent is seat 0, who has one
    // card, so seat 1's `raid` is not legal.
    let t2_battle = play(
        &ruleset,
        &t1_battle,
        &[
            act(0, "end_turn", &[]),
            act(1, "summon", &[OTHER_A]),
            act(1, "to_battle", &[]),
        ],
    );
    let before = t2_battle.clone();
    assert_eq!(
        ruleset.transition(&t2_battle, &act(1, "raid", &[OTHER_A])),
        Err(where_not_met(OTHER_A))
    );
    assert_eq!(t2_battle, before);

    // Turn 3: seat 0's `raid` was legal before seat 1's card arrived and is not now.
    let t3_battle = play(
        &ruleset,
        &t2_battle,
        &[act(1, "end_turn", &[]), act(0, "to_battle", &[])],
    );
    assert_eq!(
        ruleset.transition(&t3_battle, &act(0, "raid", &[CARD_A])),
        Err(where_not_met(CARD_A))
    );
    assert!(!offers(&ruleset, &t3_battle, "raid"));
}

#[test]
fn count_in_a_card_effect_is_read_for_the_controller_and_the_controllers_opponent() {
    // Every card is a `watcher`. Turn 1: seat 0 summons its first card: `controller` count is 1,
    // the opponent has none. Turn 2: seat 1 summons its first card, which also costs the opponent
    // (seat 0, one card) 50. Turn 3: seat 0 summons a second card: its own count is 2, so it gains
    // nothing, and seat 1 (one card) loses 50.
    let (ruleset, creation) = game_with(WATCHER_DECK);
    let after_first = take(
        &ruleset,
        &creation.opening.state,
        &act(0, "summon", &[CARD_A]),
    )
    .state;
    assert_eq!((life(&after_first, 0), life(&after_first, 1)), (4500, 4000));

    let turn_2 = play(&ruleset, &after_first, &[act(0, "end_turn", &[])]);
    let after_second = take(&ruleset, &turn_2, &act(1, "summon", &[OTHER_A])).state;
    assert_eq!(
        (life(&after_second, 0), life(&after_second, 1)),
        (4450, 4500)
    );

    let turn_3 = play(&ruleset, &after_second, &[act(1, "end_turn", &[])]);
    let after_third = take(&ruleset, &turn_3, &act(0, "summon", &[CARD_B])).state;
    assert_eq!((life(&after_third, 0), life(&after_third, 1)), (4450, 4450));
}

#[test]
fn count_of_an_unknown_zone_is_rejected() {
    let yaml = edit(
        RULESET,
        "{ count: { zone: field, player: active } }",
        "{ count: { zone: nowhere, player: active } }",
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: summon_count().key("count").key("zone"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "nowhere".to_string(),
            },
        }
    );
}

#[test]
fn count_with_an_unknown_or_missing_selector_is_rejected() {
    let unknown = edit(
        RULESET,
        "{ count: { zone: field, player: active } }",
        "{ count: { zone: field, player: nobody } }",
    );
    assert_eq!(
        definition_error(&unknown),
        DefinitionError {
            path: summon_count().key("count").key("player"),
            kind: DefinitionErrorKind::UnknownPlayerSelector {
                selector: "nobody".to_string()
            },
        }
    );

    let missing = edit(
        RULESET,
        "{ count: { zone: field, player: active } }",
        "{ count: { zone: field } }",
    );
    assert_eq!(
        definition_error(&missing),
        DefinitionError {
            path: summon_count().key("count"),
            kind: DefinitionErrorKind::MissingPlayerSelector,
        }
    );

    // `controller` is the owner of `self`: a Card Effect has one, an action does not.
    let controller = edit(
        RULESET,
        "{ count: { zone: field, player: active } }",
        "{ count: { zone: field, player: controller } }",
    );
    assert_eq!(
        definition_error(&controller),
        DefinitionError {
            path: summon_count().key("count").key("player"),
            kind: DefinitionErrorKind::ControllerOutsideCardEffect,
        }
    );
}

#[test]
fn count_and_first_turn_in_a_win_condition_are_rejected() {
    let win = |when: &str| {
        format!("{RULESET}\nwin_conditions:\n  - id: lost\n    when: {when}\n    loser: owner\n")
    };
    let at_when = DefinitionPath::root()
        .key("win_conditions")
        .id("lost")
        .key("when");

    let count = win("{ eq: [{ count: { zone: field, player: active } }, 0] }");
    assert_eq!(
        definition_error(&count),
        DefinitionError {
            path: at_when.clone().key("eq").index(0),
            kind: DefinitionErrorKind::OperatorNotInWinCondition {
                operator: "count".to_string()
            },
        }
    );

    let first_turn = win("{ eq: [{ first_turn: {} }, true] }");
    assert_eq!(
        definition_error(&first_turn),
        DefinitionError {
            path: at_when.key("eq").index(0),
            kind: DefinitionErrorKind::OperatorNotInWinCondition {
                operator: "first_turn".to_string()
            },
        }
    );
}

// ---- Turn 1

#[test]
fn a_where_can_forbid_an_action_on_turn_one() {
    let (ruleset, creation) = opening();
    // Turn 1: seat 0 has a card in the field and is in Battle, the opponent's field is empty and
    // the card has not acted. Only the Turn explains why `strike` is not legal.
    let t1_battle = play(
        &ruleset,
        &creation.opening.state,
        &[act(0, "summon", &[CARD_A]), act(0, "to_battle", &[])],
    );
    let before = t1_battle.clone();
    assert_eq!(
        ruleset.transition(&t1_battle, &act(0, "strike", &[CARD_A])),
        Err(where_not_met(CARD_A))
    );
    assert_eq!(t1_battle, before, "the input is unchanged");
    assert!(!offers(&ruleset, &t1_battle, "strike"));

    // Turn 2: the same `strike` is legal, and does what it says.
    let t2_battle = play(
        &ruleset,
        &t1_battle,
        &[
            act(0, "end_turn", &[]),
            act(1, "summon", &[OTHER_A]),
            act(1, "to_battle", &[]),
        ],
    );
    assert_eq!(t2_battle.turn_number(), TurnNumber::new(2));
    assert!(is_legal(
        &ruleset,
        &t2_battle,
        &act(1, "strike", &[OTHER_A])
    ));
    let struck = take(&ruleset, &t2_battle, &act(1, "strike", &[OTHER_A]));
    assert_eq!(
        struck.trace.events().to_vec(),
        vec![Event::PlayerStateChanged {
            player: p(0),
            state: PlayerStateId::new("life_points"),
            value: FieldValue::Int(3900),
        }]
    );
}

// ---- observations

#[test]
fn an_observation_shows_the_phase_advance_phase_moved_to() {
    let (ruleset, creation) = opening();
    let input = creation.opening.state.clone();
    let action = act(0, "to_battle", &[]);
    let transition = take(&ruleset, &input, &action);
    assert_eq!(transition.state.phase(), Some(&phase("battle")));

    for seat in 0..2 {
        let mut observer = ruleset
            .observer(
                Viewer::Player(p(seat)),
                &creation.empty,
                creation.opening.trace.events(),
            )
            .expect("the Observer is built");
        let followed = ruleset
            .follow_transition(&mut observer, &input, &action, &transition)
            .expect("the transition is followed");
        // The phase of the Observed GameState, and what `observe` says of the new GameState.
        assert_eq!(
            followed.state.phase(),
            Some(&phase("battle")),
            "viewer {seat}"
        );
        assert_eq!(
            ruleset.observe(&observer, &transition.state),
            Ok(followed.state.clone())
        );
        // The phase changes are observed as they happened, for both Viewers.
        let phases: Vec<ObservedEvent> = followed
            .events
            .iter()
            .filter(|event| matches!(event, ObservedEvent::PhaseBegan { .. }))
            .cloned()
            .collect();
        assert_eq!(
            phases,
            vec![
                ObservedEvent::PhaseBegan {
                    phase: phase("between")
                },
                ObservedEvent::PhaseBegan {
                    phase: phase("battle")
                },
            ],
            "viewer {seat}"
        );
    }
}
