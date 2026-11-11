//! Acceptance tests for ticket #25: combat against creatures and Leaders, on the whole real Small
//! Ruleset (`attack`, `leader_dead` and all), the real catalog and the real sample decks, played
//! from the Opening Position.
//!
//! A scenario is played with real Actions: seat 0 plays a creature on Turn 1, seat 1 plays one on
//! Turn 2 and the creatures can fight from Turn 3 on. A few tests edit the text of the catalog (the
//! life or the power of a Leader) or of the ruleset (one more action, or three Players), and say so.
//! Static validation of expressions, code-built drafts and the Display of the new errors are tested
//! in the engine crate (`tests/expressions.rs`).

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameState, Loss, Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionDraft, ActionId, CardTypeId, CardValue, EffectDraft, Expression, ExpressionDraft,
    FieldName, FieldValue, ParamAlternativeDraft, ParamCardsDraft, ParamDraft, ParamName,
    ParamOwner, RefDraft, StateName, WinConditionId, WinConditionWhen, WinConditionWhenDraft,
    ZoneId,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_ruleset_draft, LoadError,
    YamlError, YamlErrorKind,
};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// With this Seed seat 0 opens with, in hand order: `goblin_scout` (card 1, cost 1, power 1),
/// `river_otter` (13, cost 2, power 3) and `moss_sprite` (7, cost 1, power 2) among others; seat 1
/// holds `moss_sprite` (63, cost 1, power 2) and `river_otter` (72, cost 2, power 3). Each test
/// asserts the facts it relies on.
const SEED: u64 = 9;

/// Seat 0's creatures of power 2 and 3, and seat 1's.
const MOSS_0: CardInstanceId = CardInstanceId(7);
const OTTER_0: CardInstanceId = CardInstanceId(13);
const MOSS_1: CardInstanceId = CardInstanceId(63);
const OTTER_1: CardInstanceId = CardInstanceId(72);

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn path() -> DefinitionPath {
    DefinitionPath::root()
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

fn ruleset_from(ruleset_yaml: &str, catalog_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog_yaml).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The real Small Ruleset and the real catalog.
fn small() -> DeclarativeRuleset {
    ruleset_from(SMALL_RULESET, SMALL_CATALOG)
}

/// The real Small Ruleset with a catalog edited as text.
fn small_with_catalog(from: &str, to: &str) -> DeclarativeRuleset {
    ruleset_from(SMALL_RULESET, &edit(SMALL_CATALOG, from, to))
}

/// The Deck Lists of the seats, each one of the real sample decks.
fn decks(ruleset: &DeclarativeRuleset, yamls: &[&str]) -> Vec<DeckList> {
    yamls
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

/// Deck A for seat 0 and deck B for seat 1.
fn opening(ruleset: &DeclarativeRuleset) -> GameState {
    ruleset
        .create_game(&decks(ruleset, &[DECK_A, DECK_B]), Seed(SEED))
        .expect("the sample decks create a game")
        .opening
        .state
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn play(player: u32, card: CardInstanceId) -> Action {
    action(player, "play_creature", &[card])
}

fn pass(player: u32) -> Action {
    action(player, "pass_turn", &[])
}

fn attack(player: u32, attacker: CardInstanceId, target: CardInstanceId) -> Action {
    action(player, "attack", &[attacker, target])
}

fn accepted(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
) -> Transition<GameState, Event> {
    ruleset
        .transition(state, action)
        .expect("the Action is accepted")
}

/// Accepts every Action in turn, each on the state the previous one produced.
fn accept_all(ruleset: &DeclarativeRuleset, state: &GameState, actions: &[Action]) -> GameState {
    let mut state = state.clone();
    for action in actions {
        state = accepted(ruleset, &state, action).state;
    }
    state
}

/// Asserts the exact Rejection, and that the GameState is what it was before the call.
fn assert_rejected(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
    expected: Rejection,
) {
    let before = state.clone();
    assert_eq!(ruleset.transition(state, action), Err(expected));
    assert_eq!(state, &before, "a rejected Action changes nothing");
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

/// The Leader of a seat.
fn leader_of(state: &GameState, player: u32) -> CardInstanceId {
    *cards_in(state, player, "leader_zone")
        .first()
        .expect("the Leader Zone holds the Leader")
}

fn state_value(state: &GameState, card: CardInstanceId, name: &str) -> Option<FieldValue> {
    state
        .card(card)
        .and_then(|instance| instance.state_value(&StateName::new(name)))
        .cloned()
}

fn int_field(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    card: CardInstanceId,
    field: &str,
) -> i64 {
    let instance = state.card(card).expect("the card exists");
    let definition = ruleset
        .catalog()
        .card(instance.definition())
        .expect("the card is in the catalog");
    match definition.field(&FieldName::new(field)) {
        Some(FieldValue::Int(value)) => *value,
        other => panic!("expected an int `{field}`, got {other:?}"),
    }
}

/// Applies Events that set a scenario up; each must be accepted.
fn apply_all(state: &mut GameState, events: &[Event]) {
    for event in events {
        state.apply(event).expect("the scenario Event is accepted");
    }
}

/// The Event of a creature becoming tapped.
fn tapped(card: CardInstanceId) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new("tapped"),
        value: FieldValue::Bool(true),
    }
}

fn life(card: CardInstanceId, value: i64) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new("current_life"),
        value: FieldValue::Int(value),
    }
}

fn moved(card: CardInstanceId, player: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(player),
        from: zone(from),
        to: zone(to),
    }
}

fn leader_dead(seat: u32) -> Loss {
    Loss {
        player: p(seat),
        reason: WinConditionId::new("leader_dead"),
    }
}

/// Turn 1: seat 0 plays `a` and passes. Turn 2: seat 1 plays `t` and passes. The game waits at
/// Turn 3 for seat 0, whose `a` can attack, with `t` on the field of seat 1, untapped.
fn at_turn_3(ruleset: &DeclarativeRuleset, a: CardInstanceId, t: CardInstanceId) -> GameState {
    accept_all(
        ruleset,
        &opening(ruleset),
        &[play(0, a), pass(0), play(1, t), pass(1)],
    )
}

/// Turn 3: seat 0 attacks the Leader of seat 1 with `a`, which taps it, and passes. The game waits
/// at Turn 4 for seat 1, whose `t` can attack and whose enemy `a` is tapped.
fn at_turn_4(ruleset: &DeclarativeRuleset, a: CardInstanceId, t: CardInstanceId) -> GameState {
    let turn_3 = at_turn_3(ruleset, a, t);
    let leader = leader_of(&turn_3, 1);
    accept_all(ruleset, &turn_3, &[attack(0, a, leader), pass(0)])
}

fn yaml_error(yaml: &str) -> YamlError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Yaml(error)) => error,
        other => panic!("expected a YAML error, got {other:?}"),
    }
}

// ---- loading the real file

/// Asserts that `expression` is `<operator>(Value(<param>.<state>, read as an instance state),
/// Literal(<literal>))`, with `operator` `eq` or `lte`.
fn assert_state_against(
    expression: &Expression,
    operator: &str,
    param: &str,
    state: &str,
    literal: FieldValue,
) {
    let (left, right) = match (operator, expression) {
        ("eq", Expression::Eq(left, right)) | ("lte", Expression::Lte(left, right)) => {
            (left, right)
        }
        _ => panic!("expected `{operator}`, got {expression:?}"),
    };
    let Expression::Value(read) = left.as_ref() else {
        panic!("a value is expected, got {left:?}");
    };
    assert_eq!(read.param(), &ParamName::new(param));
    assert_eq!(read.value(), &CardValue::State(StateName::new(state)));
    assert_eq!(right.as_ref(), &Expression::Literal(literal));
}

#[test]
fn the_real_small_ruleset_loads_with_attack_and_leader_dead() {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the real file loads");

    let attack = definition
        .action(&ActionId::new("attack"))
        .expect("attack is declared");
    let [attacker, target] = attack.params() else {
        panic!("attack has two params, got {:?}", attack.params());
    };

    assert_eq!(attacker.name(), &ParamName::new("attacker"));
    let [only] = attacker.alternatives() else {
        panic!(
            "attacker has one alternative, got {:?}",
            attacker.alternatives()
        );
    };
    assert_eq!(only.zone(), &zone("field"));
    assert_eq!(only.card_type(), &CardTypeId::new("creature"));
    assert_eq!(only.owner(), ParamOwner::Actor);
    // `where: { and: [ untapped, not recently played ] }`, each side read as the instance state.
    let Some(Expression::And(operands)) = only.condition() else {
        panic!(
            "the attacker has an `and` condition, got {:?}",
            only.condition()
        );
    };
    assert_eq!(operands.len(), 2);
    for (operand, state) in operands.iter().zip(["tapped", "recently_played"]) {
        let Expression::Eq(left, right) = operand else {
            panic!("an `eq` is expected, got {operand:?}");
        };
        let Expression::Value(read) = left.as_ref() else {
            panic!("a value is expected, got {left:?}");
        };
        assert_eq!(read.param(), &ParamName::new("attacker"));
        assert_eq!(read.value(), &CardValue::State(StateName::new(state)));
        assert_eq!(
            right.as_ref(),
            &Expression::Literal(FieldValue::Bool(false))
        );
    }

    assert_eq!(target.name(), &ParamName::new("target"));
    let [creature, leader] = target.alternatives() else {
        panic!(
            "target has two alternatives, got {:?}",
            target.alternatives()
        );
    };
    assert_eq!(creature.zone(), &zone("field"));
    assert_eq!(creature.card_type(), &CardTypeId::new("creature"));
    assert_eq!(creature.owner(), ParamOwner::Opponent);
    // `where: { eq: [{ ref: target.tapped }, true] }`
    let Some(tapped_only) = creature.condition() else {
        panic!("the creature alternative has a condition");
    };
    assert_state_against(
        tapped_only,
        "eq",
        "target",
        "tapped",
        FieldValue::Bool(true),
    );
    assert_eq!(leader.zone(), &zone("leader_zone"));
    assert_eq!(leader.card_type(), &CardTypeId::new("leader"));
    assert_eq!(leader.owner(), ParamOwner::Opponent);
    assert_eq!(leader.condition(), None, "a Leader can always be targeted");

    let conditions = definition.win_conditions();
    assert_eq!(conditions.len(), 2);
    assert_eq!(conditions[0].id(), &WinConditionId::new("leader_dead"));
    // `when: { lte: [{ ref: leader.current_life }, 0] }`, about the card type `leader`.
    let WinConditionWhen::Expression { subject, condition } = conditions[0].when() else {
        panic!(
            "leader_dead is an expression, got {:?}",
            conditions[0].when()
        );
    };
    assert_eq!(subject, &CardTypeId::new("leader"));
    assert_state_against(
        condition,
        "lte",
        "leader",
        "current_life",
        FieldValue::Int(0),
    );
    assert_eq!(conditions[1].id(), &WinConditionId::new("deck_out"));
}

#[test]
fn the_loader_reads_where_one_of_if_and_add_state_as_plain_drafts() {
    let draft = parse_ruleset_draft(SMALL_RULESET).expect("the real file parses");
    let reference = |text: &str| RefDraft(text.to_string());
    let read = |text: &str| Box::new(ExpressionDraft::Ref(reference(text)));
    let literal = |value: bool| Box::new(ExpressionDraft::Literal(FieldValue::Bool(value)));
    let power_check = |then: Vec<EffectDraft>| EffectDraft::If {
        condition: ExpressionDraft::Gte(read("attacker.power"), read("target.power")),
        then,
        otherwise: vec![],
    };

    assert_eq!(
        draft.actions[1],
        ActionDraft {
            id: "attack".to_string(),
            params: vec![
                ParamDraft {
                    name: "attacker".to_string(),
                    cards: ParamCardsDraft::Single(ParamAlternativeDraft {
                        zone: "field".to_string(),
                        card_type: "creature".to_string(),
                        owner: None,
                        condition: Some(ExpressionDraft::And(vec![
                            ExpressionDraft::Eq(read("attacker.tapped"), literal(false)),
                            ExpressionDraft::Eq(read("attacker.recently_played"), literal(false)),
                        ])),
                    }),
                },
                ParamDraft {
                    name: "target".to_string(),
                    cards: ParamCardsDraft::OneOf(vec![
                        ParamAlternativeDraft {
                            zone: "field".to_string(),
                            card_type: "creature".to_string(),
                            owner: Some("opponent".to_string()),
                            condition: Some(ExpressionDraft::Eq(
                                read("target.tapped"),
                                literal(true)
                            )),
                        },
                        ParamAlternativeDraft {
                            zone: "leader_zone".to_string(),
                            card_type: "leader".to_string(),
                            owner: Some("opponent".to_string()),
                            condition: None,
                        },
                    ]),
                },
            ],
            cost: vec![],
            effects: vec![
                EffectDraft::SetState {
                    card: reference("attacker"),
                    state: "tapped".to_string(),
                    value: FieldValue::Bool(true),
                },
                EffectDraft::If {
                    condition: ExpressionDraft::IsCardType {
                        card: reference("target"),
                        card_type: "creature".to_string(),
                    },
                    then: vec![power_check(vec![EffectDraft::Move {
                        card: reference("target"),
                        to: "trash".to_string(),
                    }])],
                    otherwise: vec![power_check(vec![EffectDraft::AddState {
                        card: reference("target"),
                        state: "current_life".to_string(),
                        amount: -1,
                    }])],
                },
            ],
        }
    );

    // `leader_dead` is an expression, `deck_out` is not.
    assert_eq!(draft.win_conditions[0].id, "leader_dead");
    assert_eq!(
        draft.win_conditions[0].when,
        WinConditionWhenDraft::Expression(ExpressionDraft::Lte(
            read("leader.current_life"),
            Box::new(ExpressionDraft::Literal(FieldValue::Int(0))),
        ))
    );
    assert_eq!(
        draft.win_conditions[1].when,
        WinConditionWhenDraft::DrawFromEmpty {
            zone: "deck".to_string()
        }
    );
}

#[test]
fn a_malformed_param_expression_or_if_effect_is_a_yaml_error() {
    let malformed = |yaml: String, at: DefinitionPath| {
        let error = yaml_error(&yaml);
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "expected a malformed error at `{at}`, got {error:?}"
        );
        assert_eq!(error.path, at);
    };
    let unknown_key = |yaml: String, key: &str, at: DefinitionPath| {
        let error = yaml_error(&yaml);
        assert_eq!(
            error.kind,
            YamlErrorKind::UnknownKey {
                key: key.to_string()
            }
        );
        assert_eq!(error.path, at);
    };
    let attacker = || path().key("actions").index(1).key("params").index(0);
    let target = || path().key("actions").index(1).key("params").index(1);
    let effects = || path().key("actions").index(1).key("effects");

    // A param with `one_of` and a key of the simple form.
    malformed(
        edit(
            SMALL_RULESET,
            "      - name: target\n        one_of:",
            "      - name: target\n        zone: field\n        one_of:",
        ),
        target(),
    );
    for key in ["card_type: creature", "owner: opponent", "where: true"] {
        malformed(
            edit(
                SMALL_RULESET,
                "      - name: target\n        one_of:",
                &format!("      - name: target\n        {key}\n        one_of:"),
            ),
            target(),
        );
    }
    // A param with neither form.
    malformed(
        edit(
            SMALL_RULESET,
            "{ name: card, zone: hand, card_type: creature }",
            "{ name: card }",
        ),
        path().key("actions").index(0).key("params").index(0),
    );
    malformed(
        edit(
            SMALL_RULESET,
            "{ name: card, zone: hand, card_type: creature }",
            "{ name: card, zone: hand }",
        ),
        path().key("actions").index(0).key("params").index(0),
    );

    // An operator with the wrong number of operands.
    let eq_at = attacker().key("where").key("and").index(0).key("eq");
    malformed(
        edit(
            SMALL_RULESET,
            "{ eq: [{ ref: attacker.tapped }, false] }",
            "{ eq: [{ ref: attacker.tapped }] }",
        ),
        eq_at.clone(),
    );
    malformed(
        edit(
            SMALL_RULESET,
            "{ eq: [{ ref: attacker.tapped }, false] }",
            "{ eq: [{ ref: attacker.tapped }, false, false] }",
        ),
        eq_at,
    );
    malformed(
        edit(
            SMALL_RULESET,
            "is_card_type: [{ ref: target }, creature]",
            "is_card_type: [{ ref: target }]",
        ),
        effects().index(1).key("if").key("is_card_type"),
    );

    // An unknown operator is an unknown key at the expression that holds it.
    unknown_key(
        edit(
            SMALL_RULESET,
            "{ eq: [{ ref: attacker.tapped }, false] }",
            "{ equals: [{ ref: attacker.tapped }, false] }",
        ),
        "equals",
        attacker().key("where").key("and").index(0),
    );
    unknown_key(
        edit(
            SMALL_RULESET,
            "when: { lte: [{ ref: leader.current_life }, 0] }",
            "when: { below: [{ ref: leader.current_life }, 0] }",
        ),
        "below",
        path().key("win_conditions").index(0).key("when"),
    );

    // An `if` effect needs `then`.
    malformed(
        edit(
            SMALL_RULESET,
            "            then: [ { move: { card: { ref: target }, to: trash } } ]\n",
            "",
        ),
        effects().index(1).key("then").index(0),
    );
    // `add_state` takes an unquoted integer amount.
    malformed(
        edit(SMALL_RULESET, "amount: -1", "amount: \"-1\""),
        effects()
            .index(1)
            .key("else")
            .index(0)
            .key("then")
            .index(0)
            .key("add_state")
            .key("amount"),
    );
}

// ---- attacking a creature

#[test]
fn attacking_a_tapped_creature_of_equal_or_lower_power_kills_it() {
    let ruleset = small();
    // Seat 1's creature attacks seat 0's, which is tapped because it attacked on Turn 3: with equal
    // power (moss against moss) and with higher power (otter against moss).
    for (attacker, target) in [(MOSS_1, MOSS_0), (OTTER_1, MOSS_0)] {
        let mut turn_4 = at_turn_4(&ruleset, target, attacker);
        // The trash of the target's owner already holds a card (from its deck, for the scenario),
        // so that the Top can be told from the bottom.
        let below = *cards_in(&turn_4, 0, "deck")
            .last()
            .expect("seat 0 has cards in its deck");
        apply_all(&mut turn_4, &[moved(below, 0, "deck", "trash")]);
        let attacker_power = int_field(&ruleset, &turn_4, attacker, "power");
        let target_power = int_field(&ruleset, &turn_4, target, "power");
        assert!(attacker_power >= target_power);
        assert_eq!(
            state_value(&turn_4, target, "tapped"),
            Some(FieldValue::Bool(true))
        );
        assert_eq!(cards_in(&turn_4, 1, "field"), [attacker]);

        let transition = accepted(&ruleset, &turn_4, &attack(1, attacker, target));
        // The attacker taps, then the target goes from its owner's field to its owner's trash.
        assert_eq!(
            transition.trace.events(),
            [tapped(attacker), moved(target, 0, "field", "trash")]
        );
        let next = &transition.state;
        // The target is the Top of its owner's trash, above the card that was there, and no longer
        // on the field.
        assert_eq!(cards_in(next, 0, "trash"), [below, target]);
        assert!(cards_in(next, 0, "field").is_empty());
        // The attacker never dies: it is still on its owner's field, tapped.
        assert_eq!(cards_in(next, 1, "field"), [attacker]);
        assert_eq!(
            state_value(next, attacker, "tapped"),
            Some(FieldValue::Bool(true))
        );
        assert_eq!(
            turn_4.apply_trace(transition.trace.events()),
            Ok(next.clone())
        );
    }
}

#[test]
fn attacking_a_creature_of_higher_power_only_taps_the_attacker() {
    let ruleset = small();
    // Seat 1's moss (power 2) attacks seat 0's tapped otter (power 3).
    let turn_4 = at_turn_4(&ruleset, OTTER_0, MOSS_1);
    assert_eq!(int_field(&ruleset, &turn_4, MOSS_1, "power"), 2);
    assert_eq!(int_field(&ruleset, &turn_4, OTTER_0, "power"), 3);
    assert_eq!(
        state_value(&turn_4, OTTER_0, "tapped"),
        Some(FieldValue::Bool(true))
    );

    let transition = accepted(&ruleset, &turn_4, &attack(1, MOSS_1, OTTER_0));
    // A weak attack is still an accepted Action: the attacker taps and nothing else happens.
    assert_eq!(transition.trace.events(), [tapped(MOSS_1)]);
    let next = &transition.state;
    assert_eq!(cards_in(next, 0, "field"), [OTTER_0]);
    assert!(cards_in(next, 0, "trash").is_empty());
    assert_eq!(cards_in(next, 1, "field"), [MOSS_1]);
    assert_eq!(
        state_value(next, MOSS_1, "tapped"),
        Some(FieldValue::Bool(true))
    );
}

// ---- attacking a Leader

#[test]
fn attacking_a_leader_of_equal_or_lower_power_takes_one_life() {
    // The Leader of seat 1 (`warden_ilse`) has power 1 and life 6; the attacker has power 2. Then
    // the same Leader with its power edited to 2, equal to the attacker's.
    for ruleset in [
        small(),
        small_with_catalog("power: 1, life: 6", "power: 2, life: 6"),
    ] {
        let turn_3 = at_turn_3(&ruleset, MOSS_0, MOSS_1);
        let leader = leader_of(&turn_3, 1);
        assert!(
            int_field(&ruleset, &turn_3, MOSS_0, "power")
                >= int_field(&ruleset, &turn_3, leader, "power")
        );
        assert_eq!(
            state_value(&turn_3, leader, "current_life"),
            Some(FieldValue::Int(6))
        );
        // Accepted while the opponent has an untapped creature on the field.
        assert_eq!(cards_in(&turn_3, 1, "field"), [MOSS_1]);
        assert_eq!(
            state_value(&turn_3, MOSS_1, "tapped"),
            Some(FieldValue::Bool(false))
        );

        let transition = accepted(&ruleset, &turn_3, &attack(0, MOSS_0, leader));
        assert_eq!(transition.trace.events(), [tapped(MOSS_0), life(leader, 5)]);
        let next = &transition.state;
        assert_eq!(
            state_value(next, leader, "current_life"),
            Some(FieldValue::Int(5))
        );
        assert_eq!(
            state_value(next, MOSS_0, "tapped"),
            Some(FieldValue::Bool(true))
        );
        assert_eq!(next.outcome(), None);
        assert_eq!(
            turn_3.apply_trace(transition.trace.events()),
            Ok(next.clone())
        );
    }
}

#[test]
fn attacking_a_leader_of_higher_power_only_taps_the_attacker() {
    // The Leader of seat 1 has its power edited to 4; the attacker's is 3.
    let ruleset = small_with_catalog("power: 1, life: 6", "power: 4, life: 6");
    let turn_3 = at_turn_3(&ruleset, OTTER_0, MOSS_1);
    let leader = leader_of(&turn_3, 1);
    assert_eq!(int_field(&ruleset, &turn_3, OTTER_0, "power"), 3);
    assert_eq!(int_field(&ruleset, &turn_3, leader, "power"), 4);

    let transition = accepted(&ruleset, &turn_3, &attack(0, OTTER_0, leader));
    assert_eq!(transition.trace.events(), [tapped(OTTER_0)]);
    assert_eq!(
        state_value(&transition.state, leader, "current_life"),
        Some(FieldValue::Int(6))
    );
}

// ---- who can attack, and whom

#[test]
fn a_tapped_attacker_is_rejected() {
    let ruleset = small();
    let turn_3 = at_turn_3(&ruleset, MOSS_0, MOSS_1);
    let leader = leader_of(&turn_3, 1);
    // The first attack taps the attacker; it cannot attack again this Turn.
    let attacked = accepted(&ruleset, &turn_3, &attack(0, MOSS_0, leader)).state;
    assert_eq!(
        state_value(&attacked, MOSS_0, "tapped"),
        Some(FieldValue::Bool(true))
    );
    assert_rejected(
        &ruleset,
        &attacked,
        &attack(0, MOSS_0, leader),
        Rejection::WhereConditionNotMet {
            param: ParamName::new("attacker"),
            card: MOSS_0,
        },
    );
}

#[test]
fn a_recently_played_attacker_is_rejected() {
    let ruleset = small();
    let played = accept_all(&ruleset, &opening(&ruleset), &[play(0, MOSS_0)]);
    assert_eq!(
        state_value(&played, MOSS_0, "recently_played"),
        Some(FieldValue::Bool(true))
    );
    let leader = leader_of(&played, 1);
    assert_rejected(
        &ruleset,
        &played,
        &attack(0, MOSS_0, leader),
        Rejection::WhereConditionNotMet {
            param: ParamName::new("attacker"),
            card: MOSS_0,
        },
    );
}

#[test]
fn an_untapped_enemy_creature_cannot_be_targeted() {
    let ruleset = small();
    let turn_3 = at_turn_3(&ruleset, MOSS_0, MOSS_1);
    assert_eq!(
        state_value(&turn_3, MOSS_1, "tapped"),
        Some(FieldValue::Bool(false))
    );
    let rejection = Rejection::NoAlternativeMatched {
        param: ParamName::new("target"),
        card: MOSS_1,
    };
    assert_rejected(&ruleset, &turn_3, &attack(0, MOSS_0, MOSS_1), rejection);

    // Once it is tapped, the same attack is accepted: equal power, so the target dies.
    let mut tapped_enemy = turn_3.clone();
    apply_all(&mut tapped_enemy, &[tapped(MOSS_1)]);
    let transition = accepted(&ruleset, &tapped_enemy, &attack(0, MOSS_0, MOSS_1));
    assert_eq!(
        transition.trace.events(),
        [tapped(MOSS_0), moved(MOSS_1, 1, "field", "trash")]
    );
}

#[test]
fn an_own_card_cannot_be_targeted() {
    let ruleset = small();
    // Seat 0 has a second creature on the field, and a Leader of its own.
    let mut turn_3 = accept_all(
        &ruleset,
        &at_turn_3(&ruleset, MOSS_0, MOSS_1),
        &[play(0, OTTER_0)],
    );
    let own_leader = leader_of(&turn_3, 0);
    let rejection = |card| Rejection::NoAlternativeMatched {
        param: ParamName::new("target"),
        card,
    };
    assert_rejected(
        &ruleset,
        &turn_3,
        &attack(0, MOSS_0, OTTER_0),
        rejection(OTTER_0),
    );
    // Not even when it is tapped.
    apply_all(&mut turn_3, &[tapped(OTTER_0)]);
    assert_rejected(
        &ruleset,
        &turn_3,
        &attack(0, MOSS_0, OTTER_0),
        rejection(OTTER_0),
    );
    assert_rejected(
        &ruleset,
        &turn_3,
        &attack(0, MOSS_0, own_leader),
        rejection(own_leader),
    );
}

#[test]
fn a_tapped_leader_can_be_targeted() {
    // A Leader can always be targeted, tapped or not.
    let ruleset = small();
    let mut turn_3 = at_turn_3(&ruleset, MOSS_0, MOSS_1);
    let leader = leader_of(&turn_3, 1);
    apply_all(&mut turn_3, &[tapped(leader)]);
    assert_eq!(
        state_value(&turn_3, leader, "tapped"),
        Some(FieldValue::Bool(true))
    );
    let transition = accepted(&ruleset, &turn_3, &attack(0, MOSS_0, leader));
    assert_eq!(transition.trace.events(), [tapped(MOSS_0), life(leader, 5)]);
}

// ---- the Rejections of a param with one alternative, and Opponents

/// The real file with one more action in Main: `strike` takes one tapped creature of the opponent,
/// a param of a single alternative, written as `param` (the simple form or a `one_of` of one).
fn with_strike(param: &str) -> String {
    let main = edit(
        SMALL_RULESET,
        "actions: [play_creature, attack, pass_turn]",
        "actions: [play_creature, attack, strike, pass_turn]",
    );
    edit(
        &main,
        "\nwin_conditions:",
        &format!(
            r"
  - id: strike
    params:
      - {param}
    effects:
      - set_state: {{ card: {{ ref: victim }}, state: tapped, value: false }}

win_conditions:"
        ),
    )
}

const STRIKE_SIMPLE: &str = "{ name: victim, zone: field, card_type: creature, owner: opponent, where: { eq: [{ ref: victim.tapped }, true] } }";
const STRIKE_ONE_OF: &str = "{ name: victim, one_of: [ { zone: field, card_type: creature, owner: opponent, where: { eq: [{ ref: victim.tapped }, true] } } ] }";

#[test]
fn a_single_alternative_reports_its_first_failing_check() {
    reports_the_first_failing_check(STRIKE_SIMPLE);
}

#[test]
fn a_one_of_with_one_alternative_reports_its_first_failing_check() {
    // The same Rejections, not `NoAlternativeMatched`: one alternative, whichever way it is written.
    reports_the_first_failing_check(STRIKE_ONE_OF);
}

fn reports_the_first_failing_check(param: &str) {
    let ruleset = ruleset_from(&with_strike(param), SMALL_CATALOG);
    let turn_3 = at_turn_3(&ruleset, MOSS_0, MOSS_1);
    let victim = ParamName::new("victim");
    let strike = |card| action(0, "strike", &[card]);

    // Owner first: seat 0's own creature, which is on the right zone and of the right type.
    assert_rejected(
        &ruleset,
        &turn_3,
        &strike(MOSS_0),
        Rejection::NotOpponentCard {
            param: victim.clone(),
            card: MOSS_0,
        },
    );
    // Then the zone of its owner: a card in the hand of seat 1.
    let in_hand = *cards_in(&turn_3, 1, "hand")
        .first()
        .expect("seat 1 has cards in hand");
    assert_rejected(
        &ruleset,
        &turn_3,
        &strike(in_hand),
        Rejection::CardNotInZone {
            param: victim.clone(),
            card: in_hand,
            zone: zone("field"),
        },
    );
    // Then the card type: the Leader of seat 1, placed on its field for the scenario.
    let enemy_leader = leader_of(&turn_3, 1);
    let mut leader_on_field = turn_3.clone();
    apply_all(
        &mut leader_on_field,
        &[moved(enemy_leader, 1, "leader_zone", "field")],
    );
    assert_rejected(
        &ruleset,
        &leader_on_field,
        &strike(enemy_leader),
        Rejection::WrongCardType {
            param: victim.clone(),
            card: enemy_leader,
            expected: CardTypeId::new("creature"),
            found: CardTypeId::new("leader"),
        },
    );
    // Then the `where` condition: an untapped creature of seat 1.
    assert_rejected(
        &ruleset,
        &turn_3,
        &strike(MOSS_1),
        Rejection::WhereConditionNotMet {
            param: victim,
            card: MOSS_1,
        },
    );
    // A card that passes every check binds.
    let mut tapped_enemy = turn_3.clone();
    apply_all(&mut tapped_enemy, &[tapped(MOSS_1)]);
    let transition = accepted(&ruleset, &tapped_enemy, &strike(MOSS_1));
    assert_eq!(
        transition.trace.events(),
        [Event::CardStateChanged {
            card: MOSS_1,
            state: StateName::new("tapped"),
            value: FieldValue::Bool(false),
        }]
    );
}

#[test]
fn an_empty_and_holds() {
    // The attacker's condition is `and` with no operand: it holds for a tapped creature too.
    let ruleset = ruleset_from(
        &edit(
            SMALL_RULESET,
            "          and:\n            - { eq: [{ ref: attacker.tapped }, false] }\n            - { eq: [{ ref: attacker.recently_played }, false] }\n",
            "          and: []\n",
        ),
        SMALL_CATALOG,
    );
    // The creature is recently played and attacks at once.
    let played = accept_all(&ruleset, &opening(&ruleset), &[play(0, MOSS_0)]);
    let leader = leader_of(&played, 1);
    // The Leader of seat 1 has 6 life and power 1, the attacker power 2.
    assert_eq!(
        state_value(&played, leader, "current_life"),
        Some(FieldValue::Int(6))
    );
    let attacked = accepted(&ruleset, &played, &attack(0, MOSS_0, leader));
    assert_eq!(attacked.trace.events(), [tapped(MOSS_0), life(leader, 5)]);
    // And again, though it is now tapped: it is not tapped again, and the Leader loses another life.
    let again = accepted(&ruleset, &attacked.state, &attack(0, MOSS_0, leader));
    assert_eq!(again.trace.events(), [life(leader, 4)]);
}

#[test]
fn opponent_is_any_player_other_than_the_actor() {
    // The real file with three Players, and the sample decks A, B and A.
    let ruleset = ruleset_from(
        &edit(SMALL_RULESET, "  players: 2", "  players: 3"),
        SMALL_CATALOG,
    );
    let creation = ruleset
        .create_game(&decks(&ruleset, &[DECK_A, DECK_B, DECK_A]), Seed(SEED))
        .expect("three decks create a three-Player game");
    // Seat 0 plays its moss (cost 1, power 2) on Turn 1; seats 1 and 2 pass, and Turn 4 is seat
    // 0's. The power of the attacker is at least the power of both Leaders (1 and 2).
    let turn_1 = creation.opening.state;
    let creature = MOSS_0;
    assert_eq!(int_field(&ruleset, &turn_1, creature, "cost"), 1);
    assert_eq!(int_field(&ruleset, &turn_1, creature, "power"), 2);
    let turn_4 = accept_all(
        &ruleset,
        &turn_1,
        &[play(0, creature), pass(0), pass(1), pass(2)],
    );
    assert_eq!(turn_4.active_player(), p(0));

    // Both enemy Leaders can be attacked, each from the same state.
    for seat in [1, 2] {
        let leader = leader_of(&turn_4, seat);
        let Some(FieldValue::Int(before)) = state_value(&turn_4, leader, "current_life") else {
            panic!("the Leader of seat {seat} has an int life");
        };
        assert!(
            int_field(&ruleset, &turn_4, creature, "power")
                >= int_field(&ruleset, &turn_4, leader, "power")
        );
        let transition = accepted(&ruleset, &turn_4, &attack(0, creature, leader));
        assert_eq!(
            transition.trace.events(),
            [tapped(creature), life(leader, before - 1)],
            "the Leader of seat {seat} loses a life"
        );
    }

    // A tapped creature of seat 2 can be attacked too.
    // It has no more power than the attacker, so it dies.
    let enemy = *cards_in(&turn_4, 2, "hand")
        .iter()
        .find(|&&card| int_field(&ruleset, &turn_4, card, "power") <= 2)
        .expect("seat 2 holds a creature of power 2 or less");
    let mut with_enemy = turn_4.clone();
    apply_all(
        &mut with_enemy,
        &[moved(enemy, 2, "hand", "field"), tapped(enemy)],
    );
    let transition = accepted(&ruleset, &with_enemy, &attack(0, creature, enemy));
    assert!(
        int_field(&ruleset, &with_enemy, creature, "power")
            >= int_field(&ruleset, &with_enemy, enemy, "power")
    );
    assert_eq!(
        transition.trace.events(),
        [tapped(creature), moved(enemy, 2, "field", "trash")]
    );
    // The actor's own Leader is nobody's opponent.
    let own_leader = leader_of(&turn_4, 0);
    assert_rejected(
        &ruleset,
        &turn_4,
        &attack(0, creature, own_leader),
        Rejection::NoAlternativeMatched {
            param: ParamName::new("target"),
            card: own_leader,
        },
    );
}

// ---- the end of the Game

#[test]
fn a_leader_kill_ends_the_game() {
    // The Leader of seat 1 has 1 life.
    let ruleset = small_with_catalog("power: 1, life: 6", "power: 1, life: 1");
    let turn_3 = accept_all(
        &ruleset,
        &opening(&ruleset),
        &[play(0, MOSS_0), pass(0), pass(1)],
    );
    let leader = leader_of(&turn_3, 1);
    assert_eq!(
        state_value(&turn_3, leader, "current_life"),
        Some(FieldValue::Int(1))
    );

    let transition = accepted(&ruleset, &turn_3, &attack(0, MOSS_0, leader));
    assert_eq!(
        transition.trace.events(),
        [
            tapped(MOSS_0),
            life(leader, 0),
            Event::GameEnded {
                losses: vec![leader_dead(1)]
            },
        ]
    );
    let ended = &transition.state;
    let outcome = ended.outcome().expect("the Game ended");
    // The winner is the attacker's Player; the loser is the Leader's owner, for `leader_dead`.
    assert_eq!(outcome.winners(), [p(0)]);
    assert_eq!(outcome.losses(), [leader_dead(1)]);
    assert!(!outcome.is_drawn_game());
    assert_eq!(
        turn_3.apply_trace(transition.trace.events()),
        Ok(ended.clone())
    );

    // No Action is accepted afterwards.
    for rejected in [pass(1), pass(0), attack(0, MOSS_0, leader)] {
        assert_rejected(&ruleset, ended, &rejected, Rejection::GameOver);
    }
}

#[test]
fn leader_dead_is_the_reason_when_deck_out_also_holds() {
    // Seat 1 has a Leader at 0 life and an empty deck. Seat 0 passes, so Turn 2 begins for seat 1,
    // whose draw finds the deck empty: both conditions hold for seat 1 in the one transition, and
    // `leader_dead` is declared first.
    let ruleset = small();
    let mut state = opening(&ruleset);
    let leader = leader_of(&state, 1);
    let deck = cards_in(&state, 1, "deck");
    let mut scenario: Vec<Event> = deck
        .into_iter()
        .map(|card| moved(card, 1, "deck", "trash"))
        .collect();
    scenario.push(life(leader, 0));
    apply_all(&mut state, &scenario);
    assert_eq!(state.outcome(), None, "the Game goes on until a transition");

    let transition = accepted(&ruleset, &state, &pass(0));
    let events = transition.trace.events();
    assert!(events.contains(&Event::DrawFromEmptyZone {
        player: p(1),
        zone: zone("deck"),
    }));
    assert_eq!(
        events.last(),
        Some(&Event::GameEnded {
            losses: vec![leader_dead(1)]
        })
    );
    let outcome = transition.state.outcome().expect("the Game ended");
    assert_eq!(outcome.losses(), [leader_dead(1)]);
    assert_eq!(outcome.winners(), [p(0)]);
}

#[test]
fn a_leader_at_zero_or_negative_life_loses_at_game_creation() {
    let created = |ruleset: &DeclarativeRuleset| {
        ruleset
            .create_game(&decks(ruleset, &[DECK_A, DECK_B]), Seed(SEED))
            .expect("the sample decks create a game")
    };

    // `captain_ardel` belongs to seat 0 and `warden_ilse` to seat 1.
    for (ruleset, loser, winner) in [
        (
            small_with_catalog("power: 2, life: 5", "power: 2, life: 0"),
            0,
            1,
        ),
        (
            small_with_catalog("power: 1, life: 6", "power: 1, life: -1"),
            1,
            0,
        ),
    ] {
        let creation = created(&ruleset);
        let state = &creation.opening.state;
        let events = creation.opening.trace.events();
        // The Opening Position holds the outcome, and the trace ends with it.
        let outcome = state.outcome().expect("the Game ended at creation");
        assert_eq!(outcome.losses(), [leader_dead(loser)]);
        assert_eq!(outcome.winners(), [p(winner)]);
        assert_eq!(
            events.last(),
            Some(&Event::GameEnded {
                losses: vec![leader_dead(loser)]
            })
        );
        assert_eq!(creation.empty.apply_trace(events), Ok(state.clone()));
        assert_rejected(&ruleset, state, &pass(0), Rejection::GameOver);
    }

    // Both Leaders at 0: both lose at once, in seat order, and the game is drawn.
    let both = ruleset_from(
        SMALL_RULESET,
        &SMALL_CATALOG
            .replace("power: 2, life: 5", "power: 2, life: 0")
            .replace("power: 1, life: 6", "power: 1, life: 0"),
    );
    let creation = created(&both);
    let outcome = creation
        .opening
        .state
        .outcome()
        .expect("the Game ended at creation");
    assert_eq!(outcome.losses(), [leader_dead(0), leader_dead(1)]);
    assert!(outcome.winners().is_empty());
    assert!(outcome.is_drawn_game());
}

// ---- add_state, with checked arithmetic

/// The real file with `raise`, `lower` and `keep`, which add 1, -1 and 0 to the life of the acting
/// Player's Leader, and without `leader_dead`, so that a Leader may start at any life.
fn with_life_actions() -> String {
    let main = edit(
        SMALL_RULESET,
        "actions: [play_creature, attack, pass_turn]",
        "actions: [play_creature, attack, raise, lower, keep, pass_turn]",
    );
    let without_leader_dead = edit(
        &main,
        "  - id: leader_dead\n    when: { lte: [{ ref: leader.current_life }, 0] }\n    loser: owner\n",
        "",
    );
    let life_action = |id: &str, amount: i64| {
        format!(
            "  - id: {id}\n    params:\n      - {{ name: leader, zone: leader_zone, card_type: leader }}\n    effects:\n      - add_state: {{ card: {{ ref: leader }}, state: current_life, amount: {amount} }}\n"
        )
    };
    edit(
        &without_leader_dead,
        "\nwin_conditions:",
        &format!(
            "\n{}{}{}\nwin_conditions:",
            life_action("raise", 1),
            life_action("lower", -1),
            life_action("keep", 0)
        ),
    )
}

#[test]
fn add_state_overflow_is_a_state_overflow_rejection() {
    let at = |life: &str| {
        let ruleset = ruleset_from(
            &with_life_actions(),
            &edit(
                SMALL_CATALOG,
                "power: 2, life: 5",
                &format!("power: 2, life: {life}"),
            ),
        );
        let state = opening(&ruleset);
        (ruleset, state)
    };
    let current_life = StateName::new("current_life");

    // Above i64::MAX.
    let (ruleset, state) = at("9223372036854775807");
    let leader = leader_of(&state, 0);
    assert_eq!(
        state_value(&state, leader, "current_life"),
        Some(FieldValue::Int(i64::MAX))
    );
    assert_rejected(
        &ruleset,
        &state,
        &action(0, "raise", &[leader]),
        Rejection::StateOverflow {
            card: leader,
            state: current_life.clone(),
        },
    );
    // One less is fine, and an amount of 0 emits no Event.
    let lowered = accepted(&ruleset, &state, &action(0, "lower", &[leader]));
    assert_eq!(lowered.trace.events(), [life(leader, i64::MAX - 1)]);
    let kept = accepted(&ruleset, &state, &action(0, "keep", &[leader]));
    assert!(kept.trace.events().is_empty());
    assert_eq!(kept.state, state);

    // Below i64::MIN.
    let (ruleset, state) = at("-9223372036854775808");
    let leader = leader_of(&state, 0);
    assert_eq!(
        state_value(&state, leader, "current_life"),
        Some(FieldValue::Int(i64::MIN))
    );
    assert_rejected(
        &ruleset,
        &state,
        &action(0, "lower", &[leader]),
        Rejection::StateOverflow {
            card: leader,
            state: current_life,
        },
    );
    let raised = accepted(&ruleset, &state, &action(0, "raise", &[leader]));
    assert_eq!(raised.trace.events(), [life(leader, i64::MIN + 1)]);
    let kept = accepted(&ruleset, &state, &action(0, "keep", &[leader]));
    assert!(kept.trace.events().is_empty());
}

// ---- determinism

/// A whole Game: seat 0 plays moss and attacks the Leader of seat 1, whose own moss then kills it,
/// seat 0 plays an otter, and on Turn 7 the otter takes the last life of the Leader of seat 1 (it
/// has 2). Returns the Opening Position and every Transition after it.
fn play_a_game() -> (
    DeclarativeRuleset,
    Transition<GameState, Event>,
    Vec<Transition<GameState, Event>>,
) {
    let ruleset = small_with_catalog("power: 1, life: 6", "power: 1, life: 2");
    let opening = ruleset
        .create_game(&decks(&ruleset, &[DECK_A, DECK_B]), Seed(SEED))
        .expect("the sample decks create a game")
        .opening;
    let leader_0 = leader_of(&opening.state, 0);
    let leader_1 = leader_of(&opening.state, 1);
    let script = [
        play(0, MOSS_0),
        pass(0),
        play(1, MOSS_1),
        pass(1),
        attack(0, MOSS_0, leader_1),
        pass(0),
        attack(1, MOSS_1, MOSS_0),
        pass(1),
        play(0, OTTER_0),
        pass(0),
        attack(1, MOSS_1, leader_0),
        pass(1),
        attack(0, OTTER_0, leader_1),
    ];
    let mut transitions: Vec<Transition<GameState, Event>> = Vec::new();
    let mut state = opening.state.clone();
    for next in &script {
        let transition = accepted(&ruleset, &state, next);
        state = transition.state.clone();
        transitions.push(transition);
    }
    (ruleset, opening, transitions)
}

#[test]
fn combat_is_deterministic() {
    let (ruleset, opening, first) = play_a_game();
    let (_, second_opening, second) = play_a_game();
    assert_eq!(opening, second_opening);
    assert_eq!(
        first, second,
        "same Seed and Actions, same states and Event Traces"
    );

    // Applying each trace to the prior state gives the next one.
    let mut prior = opening.state.clone();
    for transition in &first {
        assert_eq!(
            prior.apply_trace(transition.trace.events()),
            Ok(transition.state.clone())
        );
        prior = transition.state.clone();
    }

    // The game played what it should: moss died on seat 0's trash at the Top, and the otter ended
    // the Game on Turn 7.
    let after_kill = &first[6].state;
    assert_eq!(cards_in(after_kill, 0, "trash").last(), Some(&MOSS_0));
    let ended = &first[first.len() - 1].state;
    let outcome = ended.outcome().expect("the Game ended");
    assert_eq!(outcome.winners(), [p(0)]);
    assert_eq!(outcome.losses(), [leader_dead(1)]);
    assert_rejected(&ruleset, ended, &pass(1), Rejection::GameOver);
}
