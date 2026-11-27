//! Acceptance tests for ticket #98 (ADR-0043): the int Expressions `add` and `sub`, the strict
//! comparisons `gt` and `lt`, and the Amount that is an int Expression.
//!
//! The ruleset, catalog and deck are the fixtures `arithmetic-*.yaml`: two Players, a card type `unit`
//! (an int field `power` of 5, an int instance state `charge` of 7), the Player states `life_points`
//! (4000), `mana` (10) and `marked`, and the actions `probe` (adds an amount to the opponent's Life
//! Points), `gate` (a `where` whose truth the test reads from the Action's acceptance), `either` and
//! `neither` (two alternatives, the first of which always overflows). Each test edits one line of
//! the ruleset text, as `probe_game` and `gate_game` say. Seat 0 opens with the cards 5, 4 and 3 in
//! hand, all `alpha`. The Duel Ruleset's combat is tested in `duel_ruleset.rs`.

use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState, Loss,
    Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, CardTypeId, DefinitionError, DefinitionErrorKind, FieldType, FieldValue, ParamName,
    PlayerStateId, WinConditionId, WinConditionWhen,
};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition, LoadError};

const RULESET: &str = include_str!("fixtures/arithmetic-ruleset.yaml");
const CATALOG: &str = include_str!("fixtures/arithmetic-catalog.yaml");
const DECK: &str = include_str!("fixtures/arithmetic-deck.yaml");

/// The hand of seat 0 is `[CARD_A, CARD_B, CARD_C]`, every card an `alpha` of power 5.
const CARD_A: CardInstanceId = CardInstanceId(5);
const CARD_B: CardInstanceId = CardInstanceId(4);

const MAX: &str = "9223372036854775807";
const MIN: &str = "-9223372036854775808";

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
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

/// A Game of the ruleset text `ruleset_yaml` with the fixture catalog and deck for both Players.
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

/// The Game whose `probe` adds `amount`, the text of an Expression, to the opponent's Life Points.
fn probe_game(amount: &str) -> (DeclarativeRuleset, GameCreation) {
    game_of(&probe_text(amount))
}

fn probe_text(amount: &str) -> String {
    edit(
        RULESET,
        "amount: 0 }   # PROBE",
        &format!("amount: {amount} }}"),
    )
}

/// The Game whose `gate` has `condition`, the text of an Expression, as its `where`.
fn gate_game(condition: &str) -> (DeclarativeRuleset, GameCreation) {
    game_of(&gate_text(condition))
}

fn gate_text(condition: &str) -> String {
    edit(
        RULESET,
        "where: { eq: [true, true] }   # GATE",
        &format!("where: {condition}"),
    )
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

fn life_changed(seat: u32, value: i64) -> Event {
    Event::PlayerStateChanged {
        player: p(seat),
        state: PlayerStateId::new("life_points"),
        value: FieldValue::Int(value),
    }
}

fn marked() -> Event {
    Event::PlayerStateChanged {
        player: p(0),
        state: PlayerStateId::new("marked"),
        value: FieldValue::Bool(true),
    }
}

/// What `probe` adds to the opponent's Life Points when its amount is `amount`, after seat 0 has
/// summoned the card 5, so that its field holds one card and the opponent's none.
fn added_by(amount: &str) -> i64 {
    let (ruleset, creation) = probe_game(amount);
    let summoned = take(
        &ruleset,
        &creation.opening.state,
        &act(0, "summon", &[CARD_A]),
    )
    .state;
    let probed = take(&ruleset, &summoned, &act(0, "probe", &[CARD_B]));
    match probed.trace.events() {
        [Event::PlayerStateChanged {
            player,
            state,
            value: FieldValue::Int(value),
        }] if *player == p(1) && state.as_str() == "life_points" => *value - 4000,
        other => panic!("`{amount}`: expected one Life Points change, got {other:?}"),
    }
}

/// Whether the `where` `condition` of `gate` holds for the card 5.
fn holds(condition: &str) -> bool {
    let (ruleset, creation) = gate_game(condition);
    let input = creation.opening.state;
    let before = input.clone();
    let gate = act(0, "gate", &[CARD_A]);
    let result = ruleset.transition(&input, &gate);
    assert_eq!(input, before, "`{condition}`: the input is untouched");
    match result {
        Ok(accepted) => {
            assert_eq!(accepted.trace.events().to_vec(), vec![marked()]);
            assert!(ruleset.legal_actions(&input).contains(&gate));
            true
        }
        Err(Rejection::WhereConditionNotMet { param, card }) => {
            assert_eq!((param, card), (ParamName::new("card"), CARD_A));
            assert!(!ruleset.legal_actions(&input).contains(&gate));
            false
        }
        Err(other) => panic!("`{condition}`: unexpected Rejection {other:?}"),
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

/// The path of the `where` of the param `card` of `gate`.
fn gate_where() -> DefinitionPath {
    at_action("gate").key("params").id("card").key("where")
}

/// The path of the amount of the first effect of `probe`.
fn probe_amount() -> DefinitionPath {
    at_action("probe")
        .key("effects")
        .index(0)
        .key("add_state")
        .key("amount")
}

fn mismatch(expected: FieldType, found: FieldType) -> DefinitionErrorKind {
    DefinitionErrorKind::ExpressionTypeMismatch { expected, found }
}

// ---- add and sub evaluate

#[test]
fn add_and_sub_evaluate_over_literals_card_fields_card_state_player_state_and_count() {
    // Seat 0 has one card on its field and seat 1 none. The card 4 has power 5 and charge 7, seat 0
    // has 10 mana and seat 1 has 4000 Life Points. Each amount is the exact int below.
    let rows: &[(&str, i64)] = &[
        // Literals, and the order of the operands of `sub`.
        ("{ add: [3, 4] }", 7),
        ("{ sub: [3, 4] }", -1),
        ("{ sub: [4, 3] }", 1),
        ("{ add: [-3, -4] }", -7),
        // A card field.
        ("{ add: [{ ref: card.power }, 1] }", 6),
        ("{ sub: [{ ref: card.power }, 1] }", 4),
        ("{ sub: [1, { ref: card.power }] }", -4),
        // A card's instance state.
        ("{ sub: [{ ref: card.charge }, 3] }", 4),
        ("{ add: [{ ref: card.charge }, { ref: card.power }] }", 12),
        // Player state, of either Player.
        ("{ add: [{ ref: player.mana, player: active }, 1] }", 11),
        (
            "{ sub: [{ ref: player.life_points, player: opponent }, 1] }",
            3999,
        ),
        // `count`, of either Player's zone.
        (
            "{ add: [{ count: { zone: field, player: active } }, 10] }",
            11,
        ),
        (
            "{ sub: [{ count: { zone: field, player: opponent } }, 3] }",
            -3,
        ),
    ];
    for (amount, expected) in rows {
        assert_eq!(added_by(amount), *expected, "amount `{amount}`");
    }
}

#[test]
fn nested_add_and_sub_evaluate() {
    let rows: &[(&str, i64)] = &[
        // (5 + 7) - (2 + (10 - 9)).
        (
            "{ sub: [{ add: [{ ref: card.power }, { ref: card.charge }] }, { add: [2, { sub: [10, 9] }] }] }",
            9,
        ),
        // (1 - 2) + (5 - (1 + 1)).
        (
            "{ add: [{ sub: [1, 2] }, { sub: [5, { add: [1, 1] }] }] }",
            2,
        ),
        // The second operand of `sub` is negated whole: 0 - (3 - 10).
        ("{ sub: [0, { sub: [3, 10] }] }", 7),
    ];
    for (amount, expected) in rows {
        assert_eq!(added_by(amount), *expected, "amount `{amount}`");
    }

    // A nested operator at the boundary of a `where`: 5 + 2 is 7, not above 7.
    assert!(holds(
        "{ gte: [{ add: [{ add: [{ ref: card.power }, 1] }, 1] }, 7] }"
    ));
    assert!(!holds(
        "{ gt: [{ add: [{ add: [{ ref: card.power }, 1] }, 1] }, 7] }"
    ));
}

#[test]
fn an_amount_may_be_any_int_expression_including_count() {
    // The amount that is a ref keeps its meaning.
    assert_eq!(added_by("{ ref: card.power }"), 5);
    // `count` is an Expression, so it is an Amount (ADR-0043 amends ADR-0042's "not an amount").
    assert_eq!(added_by("{ count: { zone: field, player: active } }"), 1);
    assert_eq!(added_by("{ ref: player.mana, player: active }"), 10);
    assert_eq!(added_by("-30"), -30);
}

// ---- overflow

#[test]
fn an_overflowing_add_is_rejected_with_the_game_state_unchanged() {
    // In an amount: `i64::MAX + 1`, `i64::MIN + -1`, an overflowing read operand, and an
    // overflow nested in an operand that fits otherwise.
    for amount in [
        format!("{{ add: [{MAX}, 1] }}"),
        format!("{{ add: [{MIN}, -1] }}"),
        format!("{{ add: [{{ ref: card.power }}, {MAX}] }}"),
        format!("{{ sub: [0, {{ add: [{MAX}, 1] }}] }}"),
    ] {
        let (ruleset, creation) = probe_game(&amount);
        let input = creation.opening.state;
        let before = input.clone();
        assert_eq!(
            ruleset.transition(&input, &act(0, "probe", &[CARD_B])),
            Err(Rejection::ArithmeticOverflow),
            "amount `{amount}`"
        );
        assert_eq!(input, before, "amount `{amount}`: the input is untouched");
    }

    // In the `where` of a param with one alternative: the overflow is the Rejection, and the card
    // is not a Legal Action.
    let (ruleset, creation) = gate_game(&format!(
        "{{ gt: [{{ add: [{MAX}, {{ ref: card.power }}] }}, 0] }}"
    ));
    let input = creation.opening.state;
    let before = input.clone();
    let gate = act(0, "gate", &[CARD_A]);
    assert_eq!(
        ruleset.transition(&input, &gate),
        Err(Rejection::ArithmeticOverflow)
    );
    assert_eq!(input, before);
    assert!(!ruleset.legal_actions(&input).contains(&gate));

    // The boundary fits: `i64::MAX - 1 + 1` is `i64::MAX`.
    assert!(holds(&format!(
        "{{ eq: [{{ add: [9223372036854775806, 1] }}, {MAX}] }}"
    )));
}

#[test]
fn an_overflowing_sub_is_rejected_with_the_game_state_unchanged() {
    // In a lone-alternative `where`: `5 - i64::MIN` does not fit. The Rejection is the overflow,
    // the input is untouched and the card is not a Legal Action.
    let (ruleset, creation) = gate_game(&format!(
        "{{ gt: [{{ sub: [{{ ref: card.power }}, {MIN}] }}, 0] }}"
    ));
    let input = creation.opening.state;
    let before = input.clone();
    let gate = act(0, "gate", &[CARD_A]);
    assert_eq!(
        ruleset.transition(&input, &gate),
        Err(Rejection::ArithmeticOverflow)
    );
    assert_eq!(input, before);
    assert!(!ruleset.legal_actions(&input).contains(&gate));

    // In an amount: `i64::MIN - 1` and `0 - i64::MIN`.
    for amount in [
        format!("{{ sub: [{MIN}, 1] }}"),
        format!("{{ sub: [0, {MIN}] }}"),
    ] {
        let (ruleset, creation) = probe_game(&amount);
        let input = creation.opening.state;
        let before = input.clone();
        assert_eq!(
            ruleset.transition(&input, &act(0, "probe", &[CARD_B])),
            Err(Rejection::ArithmeticOverflow),
            "amount `{amount}`"
        );
        assert_eq!(input, before, "amount `{amount}`: the input is untouched");
    }

    // The boundary fits: `i64::MIN - -1` is `i64::MIN + 1`.
    assert!(holds(&format!(
        "{{ eq: [{{ sub: [{MIN}, -1] }}, -9223372036854775807] }}"
    )));
}

#[test]
fn an_alternative_whose_where_overflows_does_not_fit() {
    let (ruleset, creation) = game_of(RULESET);
    let input = creation.opening.state;
    let before = input.clone();

    // `either`: the first alternative's `where` overflows `add`, so it does not fit and the second,
    // which holds, binds the card (ADR-0032, extended to `ArithmeticOverflow`).
    let either = act(0, "either", &[CARD_A]);
    let accepted = ruleset
        .transition(&input, &either)
        .expect("the second alternative fits");
    assert_eq!(accepted.trace.events().to_vec(), vec![marked()]);
    assert!(ruleset.legal_actions(&input).contains(&either));

    // `neither`: the first overflows and the second does not hold: no alternative matched, and
    // the card is not listed.
    let neither = act(0, "neither", &[CARD_A]);
    assert_eq!(
        ruleset.transition(&input, &neither),
        Err(Rejection::NoAlternativeMatched {
            param: ParamName::new("card"),
            card: CARD_A,
        })
    );
    assert!(!ruleset.legal_actions(&input).contains(&neither));
    assert_eq!(input, before);
}

// ---- gt and lt

#[test]
fn gt_holds_exactly_when_strictly_greater() {
    // The card has power 5: less, equal and greater than the literal.
    for (condition, expected) in [
        ("{ gt: [{ ref: card.power }, 6] }", false),
        ("{ gt: [{ ref: card.power }, 5] }", false),
        ("{ gt: [{ ref: card.power }, 4] }", true),
        ("{ gt: [5, {ref: card.power}] }", false),
        ("{ gt: [6, {ref: card.power}] }", true),
        ("{ gt: [-1, -2] }", true),
        ("{ gt: [-2, -1] }", false),
        ("{ gt: [0, 0] }", false),
    ] {
        assert_eq!(holds(condition), expected, "`{condition}`");
    }
}

#[test]
fn lt_holds_exactly_when_strictly_less() {
    for (condition, expected) in [
        ("{ lt: [{ ref: card.power }, 6] }", true),
        ("{ lt: [{ ref: card.power }, 5] }", false),
        ("{ lt: [{ ref: card.power }, 4] }", false),
        ("{ lt: [5, {ref: card.power}] }", false),
        ("{ lt: [4, {ref: card.power}] }", true),
        ("{ lt: [-2, -1] }", true),
        ("{ lt: [-1, -2] }", false),
        ("{ lt: [0, 0] }", false),
    ] {
        assert_eq!(holds(condition), expected, "`{condition}`");
    }
}

#[test]
fn add_and_sub_are_comparison_operands() {
    for (condition, expected) in [
        // `eq` takes an int Expression like any other value.
        ("{ eq: [{ add: [2, 3] }, 5] }", true),
        ("{ eq: [{ add: [2, 3] }, 6] }", false),
        ("{ eq: [{ sub: [2, 3] }, -1] }", true),
        ("{ eq: [5, { sub: [{ ref: card.power }, 0] }] }", true),
        (
            "{ eq: [{ add: [5, 5] }, { ref: player.mana, player: active }] }",
            true,
        ),
        // 5 + 7 = 12 against 11 and 12; 5 - 7 = -2 against 0 and -3.
        (
            "{ gt: [{ add: [{ ref: card.power }, { ref: card.charge }] }, 11] }",
            true,
        ),
        (
            "{ gt: [{ add: [{ ref: card.power }, { ref: card.charge }] }, 12] }",
            false,
        ),
        (
            "{ lt: [{ sub: [{ ref: card.power }, { ref: card.charge }] }, 0] }",
            true,
        ),
        (
            "{ lt: [{ sub: [{ ref: card.power }, { ref: card.charge }] }, -2] }",
            false,
        ),
        // `gte` and `lte` take them too.
        (
            "{ gte: [{ sub: [{ ref: card.power }, { ref: card.charge }] }, -2] }",
            true,
        ),
        (
            "{ lte: [{ sub: [{ ref: card.power }, { ref: card.charge }] }, -3] }",
            false,
        ),
        // The operand of `add` is a `count`.
        (
            "{ gt: [{ add: [{ count: { zone: hand, player: active } }, 0] }, 2] }",
            true,
        ),
    ] {
        assert_eq!(holds(condition), expected, "`{condition}`");
    }
}

// ---- load errors

#[test]
fn a_non_int_operand_of_add_or_sub_is_a_load_error_at_its_definition_path() {
    // In a `where`: a text field, a bool Player state and a bool expression, each at its operand.
    for (condition, at, found) in [
        (
            "{ gte: [{ add: [1, { ref: card.name }] }, 0] }",
            gate_where().key("gte").index(0).key("add").index(1),
            FieldType::Text,
        ),
        (
            "{ gte: [{ sub: [{ ref: player.marked, player: active }, 1] }, 0] }",
            gate_where().key("gte").index(0).key("sub").index(0),
            FieldType::Bool,
        ),
        (
            "{ gte: [{ add: [{ eq: [1, 1] }, 1] }, 0] }",
            gate_where().key("gte").index(0).key("add").index(0),
            FieldType::Bool,
        ),
        // A literal that is not an int.
        (
            "{ gt: [{ sub: [1, true] }, 0] }",
            gate_where().key("gt").index(0).key("sub").index(1),
            FieldType::Bool,
        ),
        // Nested, the innermost operand is the one reported.
        (
            "{ gt: [{ add: [1, { sub: [2, \"three\"] }] }, 0] }",
            gate_where()
                .key("gt")
                .index(0)
                .key("add")
                .index(1)
                .key("sub")
                .index(1),
            FieldType::Text,
        ),
    ] {
        assert_eq!(
            definition_error(&gate_text(condition)),
            DefinitionError {
                path: at,
                kind: mismatch(FieldType::Int, found),
            },
            "`{condition}`"
        );
    }

    // In an amount.
    let in_amount = definition_error(&probe_text("{ sub: [{ ref: card.name }, 1] }"));
    assert_eq!(
        in_amount,
        DefinitionError {
            path: probe_amount().key("sub").index(0),
            kind: mismatch(FieldType::Int, FieldType::Text),
        }
    );

    // The first operand that is not an int is the one reported, after each operand's own errors.
    let both = definition_error(&probe_text(
        "{ add: [{ ref: card.name }, { ref: player.marked, player: active }] }",
    ));
    assert_eq!(
        both,
        DefinitionError {
            path: probe_amount().key("add").index(0),
            kind: mismatch(FieldType::Int, FieldType::Text),
        }
    );
}

#[test]
fn gt_and_lt_take_two_int_operands() {
    for (condition, at, found) in [
        (
            "{ gt: [{ ref: card.name }, 1] }",
            gate_where().key("gt").index(0),
            FieldType::Text,
        ),
        (
            "{ lt: [1, true] }",
            gate_where().key("lt").index(1),
            FieldType::Bool,
        ),
    ] {
        assert_eq!(
            definition_error(&gate_text(condition)),
            DefinitionError {
                path: at,
                kind: mismatch(FieldType::Int, found),
            },
            "`{condition}`"
        );
    }

    // `eq` still compares two values of one type: an int against a bool is a mismatch at the second.
    assert_eq!(
        definition_error(&gate_text("{ eq: [{ add: [1, 2] }, true] }")),
        DefinitionError {
            path: gate_where().key("eq").index(1),
            kind: mismatch(FieldType::Int, FieldType::Bool),
        }
    );
}

#[test]
fn add_and_sub_take_exactly_two_operands() {
    // Two operands load.
    load_ruleset_definition(&probe_text("{ add: [1, 2] }")).expect("two operands load");
    for amount in [
        "{ add: [1] }",
        "{ add: [1, 2, 3] }",
        "{ sub: [] }",
        "{ sub: [1, 2, 3] }",
        "{ add: 1 }",
    ] {
        assert!(
            matches!(
                load_ruleset_definition(&probe_text(amount)),
                Err(LoadError::Yaml(_))
            ),
            "`{amount}` is a syntax error"
        );
    }
}

#[test]
fn a_non_int_amount_is_a_load_error_at_the_amount() {
    // Whatever the Expression, an amount of another type than int is `ExpressionTypeMismatch` at
    // the amount: a text field, a bool Player state, a comparison, a bool and a text literal.
    for (amount, found) in [
        ("{ ref: card.name }", FieldType::Text),
        ("{ ref: player.marked, player: active }", FieldType::Bool),
        ("{ eq: [1, 1] }", FieldType::Bool),
        ("{ gt: [1, 2] }", FieldType::Bool),
        ("true", FieldType::Bool),
        ("\"five\"", FieldType::Text),
    ] {
        assert_eq!(
            definition_error(&probe_text(amount)),
            DefinitionError {
                path: probe_amount(),
                kind: mismatch(FieldType::Int, found),
            },
            "`{amount}`"
        );
    }

    // The ref errors an amount had are unchanged: a card ref is not a value, and a name nothing
    // declares is dangling.
    assert_eq!(
        definition_error(&probe_text("{ ref: card }")),
        DefinitionError {
            path: probe_amount(),
            kind: DefinitionErrorKind::CardRefNotAValue {
                reference: "card".to_string(),
            },
        }
    );
}

#[test]
fn an_add_amount_in_a_setup_or_automatic_step_is_invalid() {
    // Nothing binds a card or names a Player to read in a setup or an automatic step, so the amount
    // stays an int literal: any other Expression is `InvalidAmount`, whatever its type.
    for amount in [
        "{ add: [1, 2] }",
        "{ sub: [1, 2] }",
        "{ count: { zone: field, player: active } }",
        "true",
    ] {
        let setup = edit(
            RULESET,
            "  - draw: { player: each, from: deck, to: hand, count: 3 }",
            &format!("  - draw: {{ player: each, from: deck, to: hand, count: 3 }}\n  - add_state: {{ player: each, state: life_points, amount: {amount} }}"),
        );
        assert_eq!(
            definition_error(&setup),
            DefinitionError {
                path: DefinitionPath::root()
                    .key("setup")
                    .index(1)
                    .key("add_state")
                    .key("amount"),
                kind: DefinitionErrorKind::InvalidAmount,
            },
            "setup amount `{amount}`"
        );

        let automatic = edit(
            RULESET,
            "    - id: end\n      automatic: []",
            &format!("    - id: end\n      automatic:\n        - add_state: {{ player: active, state: life_points, amount: {amount} }}"),
        );
        assert_eq!(
            definition_error(&automatic),
            DefinitionError {
                path: DefinitionPath::root()
                    .key("turn")
                    .key("phases")
                    .id("end")
                    .key("automatic")
                    .index(0)
                    .key("add_state")
                    .key("amount"),
                kind: DefinitionErrorKind::InvalidAmount,
            },
            "automatic amount `{amount}`"
        );
    }

    // An int literal is still an amount there.
    let setup = edit(
        RULESET,
        "  - draw: { player: each, from: deck, to: hand, count: 3 }",
        "  - draw: { player: each, from: deck, to: hand, count: 3 }\n  - add_state: { player: each, state: life_points, amount: -500 }",
    );
    let (_, creation) = game_of(&setup);
    for seat in 0..2 {
        assert_eq!(
            creation
                .opening
                .state
                .player_state_value(p(seat), &PlayerStateId::new("life_points")),
            Some(&FieldValue::Int(3500))
        );
    }
}

// ---- win conditions

/// The ruleset text with one win condition appended.
fn with_win_condition(line: &str) -> String {
    format!("{RULESET}\nwin_conditions:\n  - {line}\n")
}

/// The ruleset text whose `probe` adds `amount`, with one win condition appended.
fn probe_with_win_condition(amount: &str, line: &str) -> String {
    format!("{}\nwin_conditions:\n  - {line}\n", probe_text(amount))
}

fn win_at(id: &str) -> DefinitionPath {
    DefinitionPath::root()
        .key("win_conditions")
        .id(id)
        .key("when")
}

#[test]
fn a_win_condition_may_use_add_sub_gt_and_lt() {
    // Player state through `sub` and `lt`: a Player loses when their Life Points minus 100 are not
    // above 0, that is at 100 or fewer.
    const BROKE: &str = "{ id: broke, when: { lte: [{ sub: [{ ref: player.life_points }, 100] }, 0] }, loser: owner }";
    // 4000 - 3899 = 101 is above 100: the Game goes on.
    let (ruleset, creation) = game_of(&probe_with_win_condition("-3899", BROKE));
    let safe = take(
        &ruleset,
        &creation.opening.state,
        &act(0, "probe", &[CARD_B]),
    );
    assert_eq!(safe.trace.events().to_vec(), vec![life_changed(1, 101)]);
    assert!(safe.state.outcome().is_none());
    // 4000 - 3900 = 100 is not: seat 1 loses.
    let (ruleset, creation) = game_of(&probe_with_win_condition("-3900", BROKE));
    let lost = take(
        &ruleset,
        &creation.opening.state,
        &act(0, "probe", &[CARD_B]),
    );
    assert_eq!(
        lost.trace.events().to_vec(),
        vec![
            life_changed(1, 100),
            Event::GameEnded {
                losses: vec![Loss {
                    player: p(1),
                    reason: WinConditionId::new("broke"),
                }],
            },
        ]
    );

    // `gt` and `add` over a Player state load as a Player-state condition.
    let definition = load_ruleset_definition(&with_win_condition(
        "{ id: rich, when: { gt: [{ add: [{ ref: player.life_points }, 1] }, 9000] }, loser: owner }",
    ))
    .expect("`add` and `gt` are allowed in a win condition");
    assert!(matches!(
        definition.win_conditions()[0].when(),
        WinConditionWhen::PlayerState(_)
    ));

    // Over a card: its Subject is the card type of the refs inside `add`.
    let definition = load_ruleset_definition(&with_win_condition(
        "{ id: charged, when: { lt: [{ add: [{ ref: unit.power }, { ref: unit.charge }] }, 0] }, loser: owner }",
    ))
    .expect("`add` and `lt` are allowed in a win condition");
    assert!(matches!(
        definition.win_conditions()[0].when(),
        WinConditionWhen::Expression { subject, .. } if subject == &CardTypeId::new("unit")
    ));

    // A Subject and Player state in one condition stay mixed, whatever the operators between them.
    assert_eq!(
        definition_error(&with_win_condition(
            "{ id: mixed, when: { gt: [{ add: [{ ref: unit.power }, { ref: player.mana }] }, 0] }, loser: owner }"
        )),
        DefinitionError {
            path: DefinitionPath::root()
                .key("win_conditions")
                .id("mixed")
                .key("when"),
            kind: DefinitionErrorKind::MixedWinConditionSubject,
        }
    );
}

#[test]
fn count_and_first_turn_stay_forbidden_in_a_win_condition_at_any_depth() {
    // Under `add`, and under a `sub` nested in `gt`: the operator is rejected at its own path.
    assert_eq!(
        definition_error(&with_win_condition(
            "{ id: lost, when: { gt: [{ add: [{ count: { zone: field, player: active } }, 1] }, 0] }, loser: owner }"
        )),
        DefinitionError {
            path: win_at("lost").key("gt").index(0).key("add").index(0),
            kind: DefinitionErrorKind::OperatorNotInWinCondition {
                operator: "count".to_string(),
            },
        }
    );
    assert_eq!(
        definition_error(&with_win_condition(
            "{ id: lost, when: { lt: [1, { sub: [2, { add: [3, { count: { zone: hand, player: active } }] }] }] }, loser: owner }"
        )),
        DefinitionError {
            path: win_at("lost")
                .key("lt")
                .index(1)
                .key("sub")
                .index(1)
                .key("add")
                .index(1),
            kind: DefinitionErrorKind::OperatorNotInWinCondition {
                operator: "count".to_string(),
            },
        }
    );
    assert_eq!(
        definition_error(&with_win_condition(
            "{ id: lost, when: { eq: [{ first_turn: {} }, false] }, loser: owner }"
        )),
        DefinitionError {
            path: win_at("lost").key("eq").index(0),
            kind: DefinitionErrorKind::OperatorNotInWinCondition {
                operator: "first_turn".to_string(),
            },
        }
    );
}
