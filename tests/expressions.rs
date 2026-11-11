//! Acceptance tests for ticket #25 that need no YAML: params with alternatives, expressions, `if`
//! and `add_state` effects and expression win conditions, built from code the way a code frontend
//! would build them, with the exact errors of static validation, and the Display of the new
//! Rejections and errors.
//!
//! The same constructs read from the real Small Ruleset file, and combat played on it, are tested
//! in the loader crate (`crates/tcg-loader/tests/combat.rs`).

use tcg_engine::declarative::{CardInstanceId, GameCreationError, Rejection};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    ActionDraft, CardTypeDraft, CardTypeId, DefinitionError, DefinitionErrorKind, Effect,
    EffectDraft, Expression, ExpressionDraft, FieldDraft, FieldType, FieldTypeDraft, FieldValue,
    InitializerDraft, InstanceStateDraft, MetaDraft, ParamAlternativeDraft, ParamCardsDraft,
    ParamDraft, ParamName, ParametersDraft, RefDraft, RefType, ReferenceTarget, RulesetDefinition,
    RulesetDraft, StateName, Visibility, WinConditionDraft, WinConditionWhen,
    WinConditionWhenDraft, ZoneDraft,
};

// ---- builders

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

/// The path of the `attack` action.
fn in_attack() -> DefinitionPath {
    path().key("actions").id("attack")
}

fn attacker_params() -> DefinitionPath {
    in_attack().key("params").id("attacker")
}

fn target_params() -> DefinitionPath {
    in_attack().key("params").id("target")
}

fn field(name: &str, field_type: FieldTypeDraft) -> FieldDraft {
    FieldDraft {
        name: name.to_string(),
        field_type,
    }
}

fn state(name: &str, state_type: FieldTypeDraft, init: InitializerDraft) -> InstanceStateDraft {
    InstanceStateDraft {
        name: name.to_string(),
        state_type,
        init,
    }
}

fn flag(name: &str) -> InstanceStateDraft {
    state(
        name,
        FieldTypeDraft::Bool,
        InitializerDraft::Literal(FieldValue::Bool(false)),
    )
}

/// The structure of the Small Ruleset that `attack` needs: a hand, a field, a trash and a Leader
/// Zone; Leaders with `power` and `life` fields and the states `current_life` and `tapped`;
/// creatures with `cost` and `power` fields and the states `tapped` and `recently_played`; and the
/// `mana` card type.
fn base() -> RulesetDraft {
    let zone = |id: &str| ZoneDraft {
        id: id.to_string(),
        visibility: Visibility::Public,
        ordered: false,
    };
    RulesetDraft {
        meta: MetaDraft {
            id: "small".to_string(),
            version: 1,
        },
        parameters: ParametersDraft {
            players: 2,
            starting_player: 0,
        },
        zones: vec![
            zone("hand"),
            zone("field"),
            zone("trash"),
            zone("leader_zone"),
        ],
        card_types: vec![
            CardTypeDraft {
                id: "leader".to_string(),
                fields: vec![
                    field("name", FieldTypeDraft::Text),
                    field("power", FieldTypeDraft::Int),
                    field("life", FieldTypeDraft::Int),
                ],
                instance_state: vec![
                    state(
                        "current_life",
                        FieldTypeDraft::Int,
                        InitializerDraft::FromField("life".to_string()),
                    ),
                    flag("tapped"),
                ],
                effects: false,
            },
            CardTypeDraft {
                id: "creature".to_string(),
                fields: vec![
                    field("name", FieldTypeDraft::Text),
                    field("cost", FieldTypeDraft::Int),
                    field("power", FieldTypeDraft::Int),
                ],
                instance_state: vec![flag("tapped"), flag("recently_played")],
                effects: false,
            },
            CardTypeDraft {
                id: "mana".to_string(),
                fields: vec![],
                instance_state: vec![flag("tapped")],
                effects: false,
            },
        ],
        inputs: vec![],
        setup: vec![],
        actions: vec![],
        turn: None,
        win_conditions: vec![],
    }
}

fn reference(text: &str) -> RefDraft {
    RefDraft(text.to_string())
}

fn read(text: &str) -> ExpressionDraft {
    ExpressionDraft::Ref(reference(text))
}

fn boolean(value: bool) -> ExpressionDraft {
    ExpressionDraft::Literal(FieldValue::Bool(value))
}

fn int(value: i64) -> ExpressionDraft {
    ExpressionDraft::Literal(FieldValue::Int(value))
}

fn eq(left: ExpressionDraft, right: ExpressionDraft) -> ExpressionDraft {
    ExpressionDraft::Eq(Box::new(left), Box::new(right))
}

fn gte(left: ExpressionDraft, right: ExpressionDraft) -> ExpressionDraft {
    ExpressionDraft::Gte(Box::new(left), Box::new(right))
}

fn lte(left: ExpressionDraft, right: ExpressionDraft) -> ExpressionDraft {
    ExpressionDraft::Lte(Box::new(left), Box::new(right))
}

fn is_card_type(card: &str, card_type: &str) -> ExpressionDraft {
    ExpressionDraft::IsCardType {
        card: reference(card),
        card_type: card_type.to_string(),
    }
}

fn alternative(zone: &str, card_type: &str) -> ParamAlternativeDraft {
    ParamAlternativeDraft {
        zone: zone.to_string(),
        card_type: card_type.to_string(),
        owner: None,
        condition: None,
    }
}

fn opponent(mut alternative: ParamAlternativeDraft) -> ParamAlternativeDraft {
    alternative.owner = Some("opponent".to_string());
    alternative
}

fn when(
    mut alternative: ParamAlternativeDraft,
    condition: ExpressionDraft,
) -> ParamAlternativeDraft {
    alternative.condition = Some(condition);
    alternative
}

fn single(name: &str, alternative: ParamAlternativeDraft) -> ParamDraft {
    ParamDraft {
        name: name.to_string(),
        cards: ParamCardsDraft::Single(alternative),
    }
}

fn one_of(name: &str, alternatives: Vec<ParamAlternativeDraft>) -> ParamDraft {
    ParamDraft {
        name: name.to_string(),
        cards: ParamCardsDraft::OneOf(alternatives),
    }
}

/// The `attacker` param of the real `attack`: an untapped creature of the acting Player that was
/// not recently played.
fn attacker() -> ParamDraft {
    single(
        "attacker",
        when(
            alternative("field", "creature"),
            ExpressionDraft::And(vec![
                eq(read("attacker.tapped"), boolean(false)),
                eq(read("attacker.recently_played"), boolean(false)),
            ]),
        ),
    )
}

/// The `target` param of the real `attack`: a tapped creature or a Leader, of an opponent.
fn target() -> ParamDraft {
    one_of(
        "target",
        vec![
            when(
                opponent(alternative("field", "creature")),
                eq(read("target.tapped"), boolean(true)),
            ),
            opponent(alternative("leader_zone", "leader")),
        ],
    )
}

fn set_state(card: &str, name: &str, value: bool) -> EffectDraft {
    EffectDraft::SetState {
        card: reference(card),
        state: name.to_string(),
        value: FieldValue::Bool(value),
    }
}

fn add_state(card: &str, name: &str, amount: i64) -> EffectDraft {
    EffectDraft::AddState {
        card: reference(card),
        state: name.to_string(),
        amount,
    }
}

fn if_then(
    condition: ExpressionDraft,
    then: Vec<EffectDraft>,
    otherwise: Vec<EffectDraft>,
) -> EffectDraft {
    EffectDraft::If {
        condition,
        then,
        otherwise,
    }
}

/// The base with an `attack` action of these params and effects.
fn draft_with(params: Vec<ParamDraft>, effects: Vec<EffectDraft>) -> RulesetDraft {
    let mut draft = base();
    draft.actions.push(ActionDraft {
        id: "attack".to_string(),
        params,
        cost: vec![],
        effects,
    });
    draft
}

/// The base with the real `attack`, but with these effects.
fn with_effects(effects: Vec<EffectDraft>) -> RulesetDraft {
    draft_with(vec![attacker(), target()], effects)
}

/// The base with the real `attack` params, but with `attacker` replaced.
fn with_attacker(attacker: ParamDraft) -> RulesetDraft {
    draft_with(vec![attacker, target()], vec![])
}

fn loaded(draft: RulesetDraft) -> RulesetDefinition {
    RulesetDefinition::validate(draft).expect("the draft is valid")
}

fn error_of(draft: RulesetDraft) -> DefinitionError {
    RulesetDefinition::validate(draft).expect_err("the draft is invalid")
}

/// Asserts that the draft is invalid with `kind` at `at`.
fn assert_invalid(draft: RulesetDraft, at: DefinitionPath, kind: DefinitionErrorKind) {
    assert_eq!(error_of(draft), DefinitionError { path: at, kind });
}

fn dangling(target: ReferenceTarget, id: &str) -> DefinitionErrorKind {
    DefinitionErrorKind::DanglingReference {
        target,
        id: id.to_string(),
    }
}

fn mismatch(expected: FieldType, found: FieldType) -> DefinitionErrorKind {
    DefinitionErrorKind::ExpressionTypeMismatch { expected, found }
}

// ---- the real attack, built from code

#[test]
fn a_code_frontend_builds_the_attack_action_with_alternatives_expressions_and_effects() {
    let definition = loaded(with_effects(vec![
        set_state("attacker", "tapped", true),
        if_then(
            is_card_type("target", "creature"),
            vec![if_then(
                gte(read("attacker.power"), read("target.power")),
                vec![EffectDraft::Move {
                    card: reference("target"),
                    to: "trash".to_string(),
                }],
                vec![],
            )],
            vec![if_then(
                gte(read("attacker.power"), read("target.power")),
                vec![add_state("target", "current_life", -1)],
                vec![],
            )],
        ),
    ]));
    let attack = definition
        .action(&tcg_engine::ruleset_definition::ActionId::new("attack"))
        .expect("attack is declared");
    let [_, target] = attack.params() else {
        panic!("two params expected, got {:?}", attack.params());
    };
    assert_eq!(target.alternatives().len(), 2);

    let [Effect::SetState { .. }, Effect::If {
        condition,
        then,
        otherwise,
    }] = attack.effects()
    else {
        panic!(
            "a set_state then an if expected, got {:?}",
            attack.effects()
        );
    };
    assert!(
        matches!(
            condition,
            Expression::IsCardType { card, card_type }
                if card.param() == &ParamName::new("target")
                    && card_type == &CardTypeId::new("creature")
        ),
        "the condition is is_card_type, got {condition:?}"
    );
    assert!(matches!(then.as_slice(), [Effect::If { .. }]));
    let [Effect::If {
        then: damage,
        otherwise: nothing,
        ..
    }] = otherwise.as_slice()
    else {
        panic!("an inner if expected, got {otherwise:?}");
    };
    assert!(nothing.is_empty());
    assert!(
        matches!(
            damage.as_slice(),
            [Effect::AddState { state, amount: -1, .. }]
                if state == &StateName::new("current_life")
        ),
        "add_state of -1 expected, got {damage:?}"
    );
}

// ---- refs against every alternative

#[test]
fn a_ref_must_resolve_on_every_one_of_alternative() {
    // `power` is a field of both card types `target` may be: it loads.
    loaded(with_effects(vec![if_then(
        gte(read("attacker.power"), read("target.power")),
        vec![],
        vec![],
    )]));
    // `tapped` is an instance state of both.
    loaded(with_effects(vec![if_then(
        eq(read("target.tapped"), boolean(true)),
        vec![],
        vec![],
    )]));

    // `cost` is a field of creatures only: the Leader alternative lacks it, and the error is at the
    // slot of the ref.
    let at = in_attack()
        .key("effects")
        .index(0)
        .key("if")
        .key("gte")
        .index(1);
    assert_invalid(
        with_effects(vec![if_then(
            gte(read("attacker.power"), read("target.cost")),
            vec![],
            vec![],
        )]),
        at,
        dangling(ReferenceTarget::FieldOrInstanceState, "cost"),
    );
    // And `life`, a field of Leaders only, is missing on the first card type.
    let at = in_attack()
        .key("effects")
        .index(0)
        .key("if")
        .key("gte")
        .index(1);
    assert_invalid(
        with_effects(vec![if_then(
            gte(read("attacker.power"), read("target.life")),
            vec![],
            vec![],
        )]),
        at,
        dangling(ReferenceTarget::FieldOrInstanceState, "life"),
    );
}

#[test]
fn a_ref_that_differs_between_alternatives_is_inconsistent() {
    let inconsistent = |name: &str, other: &str| DefinitionErrorKind::InconsistentReference {
        name: name.to_string(),
        card_type: "creature".to_string(),
        other: other.to_string(),
    };
    let at = in_attack()
        .key("effects")
        .index(0)
        .key("if")
        .key("eq")
        .index(0);
    let effects = |name: &str| {
        vec![if_then(
            eq(read(&format!("target.{name}")), int(1)),
            vec![],
            vec![],
        )]
    };

    // A field of the creature and an instance state of the Leader.
    let mut draft = with_effects(effects("rank"));
    draft.card_types[1]
        .fields
        .push(field("rank", FieldTypeDraft::Int));
    draft.card_types[0].instance_state.push(state(
        "rank",
        FieldTypeDraft::Int,
        InitializerDraft::Literal(FieldValue::Int(1)),
    ));
    assert_invalid(draft, at.clone(), inconsistent("rank", "leader"));

    // An int field of the creature and a bool field of the Leader.
    let mut draft = with_effects(effects("badge"));
    draft.card_types[1]
        .fields
        .push(field("badge", FieldTypeDraft::Int));
    draft.card_types[0]
        .fields
        .push(field("badge", FieldTypeDraft::Bool));
    assert_invalid(draft, at.clone(), inconsistent("badge", "leader"));

    // The same kind and type on both loads.
    let mut draft = with_effects(effects("badge"));
    draft.card_types[1]
        .fields
        .push(field("badge", FieldTypeDraft::Int));
    draft.card_types[0]
        .fields
        .push(field("badge", FieldTypeDraft::Int));
    loaded(draft);

    // A name that one card type declares as both a field and a state is ambiguous, on any of them.
    let mut draft = with_effects(effects("badge"));
    draft.card_types[1]
        .fields
        .push(field("badge", FieldTypeDraft::Int));
    draft.card_types[0]
        .fields
        .push(field("badge", FieldTypeDraft::Int));
    draft.card_types[0].instance_state.push(state(
        "badge",
        FieldTypeDraft::Int,
        InitializerDraft::Literal(FieldValue::Int(1)),
    ));
    assert_invalid(
        draft,
        at,
        DefinitionErrorKind::AmbiguousReference {
            name: "badge".to_string(),
        },
    );
}

// ---- narrowing

#[test]
fn is_card_type_narrows_the_possible_card_types_of_an_if_effect() {
    let creature_or_leader = |then: Vec<EffectDraft>, otherwise: Vec<EffectDraft>| {
        vec![if_then(is_card_type("target", "creature"), then, otherwise)]
    };

    // `current_life` is the state of Leaders only: valid in the `else` of the creature test.
    loaded(with_effects(creature_or_leader(
        vec![],
        vec![add_state("target", "current_life", -1)],
    )));
    // ... and a field that only creatures have is valid in the `then`.
    loaded(with_effects(creature_or_leader(
        vec![if_then(gte(read("target.cost"), int(1)), vec![], vec![])],
        vec![],
    )));

    // In the `then`, `target` is only a creature.
    assert_invalid(
        with_effects(creature_or_leader(
            vec![add_state("target", "current_life", -1)],
            vec![],
        )),
        in_attack()
            .key("effects")
            .index(0)
            .key("then")
            .index(0)
            .key("add_state")
            .key("state"),
        dangling(ReferenceTarget::InstanceState, "current_life"),
    );
    // Outside the `if`, it is still both.
    assert_invalid(
        with_effects(vec![add_state("target", "current_life", -1)]),
        in_attack()
            .key("effects")
            .index(0)
            .key("add_state")
            .key("state"),
        dangling(ReferenceTarget::InstanceState, "current_life"),
    );
    // ... also after the `if`.
    assert_invalid(
        with_effects(vec![
            if_then(is_card_type("target", "creature"), vec![], vec![]),
            add_state("target", "current_life", -1),
        ]),
        in_attack()
            .key("effects")
            .index(1)
            .key("add_state")
            .key("state"),
        dangling(ReferenceTarget::InstanceState, "current_life"),
    );

    // Only a condition that is exactly `is_card_type` narrows: not through `and`.
    assert_invalid(
        with_effects(vec![if_then(
            ExpressionDraft::And(vec![is_card_type("target", "creature")]),
            vec![],
            vec![add_state("target", "current_life", -1)],
        )]),
        in_attack()
            .key("effects")
            .index(0)
            .key("else")
            .index(0)
            .key("add_state")
            .key("state"),
        dangling(ReferenceTarget::InstanceState, "current_life"),
    );

    // The narrowing nests: inside the `else` target is a Leader, so a creature-only field is not
    // there any more.
    assert_invalid(
        with_effects(creature_or_leader(
            vec![],
            vec![if_then(gte(read("target.cost"), int(1)), vec![], vec![])],
        )),
        in_attack()
            .key("effects")
            .index(0)
            .key("else")
            .index(0)
            .key("if")
            .key("gte")
            .index(0),
        dangling(ReferenceTarget::FieldOrInstanceState, "cost"),
    );

    // When the test would leave no card type for the `else`, the card types are unchanged:
    // `attacker` can only be a creature, so its `else` still sees a creature.
    loaded(with_effects(vec![if_then(
        is_card_type("attacker", "creature"),
        vec![],
        vec![set_state("attacker", "recently_played", true)],
    )]));
}

// ---- types of expressions

#[test]
fn expression_operands_must_have_consistent_types() {
    let where_at = || attacker_params().key("where");
    let with_where = |condition: ExpressionDraft| {
        with_attacker(single(
            "attacker",
            when(alternative("field", "creature"), condition),
        ))
    };

    // `eq` compares two values of one type; the second operand is the one reported, and `expected`
    // is the type of the first.
    assert_invalid(
        with_where(eq(read("attacker.tapped"), int(5))),
        where_at().key("eq").index(1),
        mismatch(FieldType::Bool, FieldType::Int),
    );
    assert_invalid(
        with_where(eq(read("attacker.power"), read("attacker.tapped"))),
        where_at().key("eq").index(1),
        mismatch(FieldType::Int, FieldType::Bool),
    );
    // Text compares with text.
    loaded(with_where(eq(
        read("attacker.name"),
        ExpressionDraft::Literal(FieldValue::Text("Goblin".to_string())),
    )));

    // `gte` and `lte` need ints: the first operand that is not one is reported.
    assert_invalid(
        with_where(gte(read("attacker.tapped"), int(1))),
        where_at().key("gte").index(0),
        mismatch(FieldType::Int, FieldType::Bool),
    );
    assert_invalid(
        with_where(gte(int(1), boolean(true))),
        where_at().key("gte").index(1),
        mismatch(FieldType::Int, FieldType::Bool),
    );
    assert_invalid(
        with_where(lte(read("attacker.name"), int(1))),
        where_at().key("lte").index(0),
        mismatch(FieldType::Int, FieldType::Text),
    );
    assert_invalid(
        with_where(lte(int(1), read("attacker.name"))),
        where_at().key("lte").index(1),
        mismatch(FieldType::Int, FieldType::Text),
    );

    // `and` needs bools, and an `and` of nothing holds.
    assert_invalid(
        with_where(ExpressionDraft::And(vec![
            eq(read("attacker.tapped"), boolean(false)),
            int(5),
        ])),
        where_at().key("and").index(1),
        mismatch(FieldType::Bool, FieldType::Int),
    );
    assert_invalid(
        with_where(ExpressionDraft::And(vec![read("attacker.power")])),
        where_at().key("and").index(0),
        mismatch(FieldType::Bool, FieldType::Int),
    );
    loaded(with_where(ExpressionDraft::And(vec![])));

    // A `where`, an `if` and a nested `and` operand are bool slots: the error is at the slot.
    assert_invalid(
        with_where(int(5)),
        where_at(),
        mismatch(FieldType::Bool, FieldType::Int),
    );
    assert_invalid(
        with_where(read("attacker.power")),
        where_at(),
        mismatch(FieldType::Bool, FieldType::Int),
    );
    assert_invalid(
        with_effects(vec![if_then(read("attacker.power"), vec![], vec![])]),
        in_attack().key("effects").index(0).key("if"),
        mismatch(FieldType::Bool, FieldType::Int),
    );
    assert_invalid(
        with_where(ExpressionDraft::And(vec![ExpressionDraft::And(vec![int(
            1,
        )])])),
        where_at().key("and").index(0).key("and").index(0),
        mismatch(FieldType::Bool, FieldType::Int),
    );

    // Depth-first: the error of an operand comes before the type rule of its operator. Here both
    // operands of `gte` break it, but the second one is dangling.
    assert_invalid(
        with_where(gte(read("attacker.tapped"), read("ghost.power"))),
        where_at().key("gte").index(1),
        dangling(ReferenceTarget::Param, "ghost"),
    );
    // ... and an operand that is itself wrong is reported before the operator it sits in.
    assert_invalid(
        with_where(eq(int(1), gte(read("attacker.tapped"), int(1)))),
        where_at().key("eq").index(1).key("gte").index(0),
        mismatch(FieldType::Int, FieldType::Bool),
    );
}

#[test]
fn a_card_ref_is_not_a_value_of_an_expression() {
    let where_at = || attacker_params().key("where");
    let with_where = |condition: ExpressionDraft| {
        with_attacker(single(
            "attacker",
            when(alternative("field", "creature"), condition),
        ))
    };
    let card_ref = DefinitionErrorKind::CardRefNotAValue {
        reference: "attacker".to_string(),
    };

    assert_invalid(
        with_where(eq(read("attacker"), read("attacker"))),
        where_at().key("eq").index(0),
        card_ref.clone(),
    );
    assert_invalid(
        with_where(gte(int(1), read("attacker"))),
        where_at().key("gte").index(1),
        card_ref.clone(),
    );
    assert_invalid(
        with_where(ExpressionDraft::And(vec![read("attacker")])),
        where_at().key("and").index(0),
        card_ref.clone(),
    );
    // As the whole slot too.
    assert_invalid(with_where(read("attacker")), where_at(), card_ref.clone());
    assert_invalid(
        with_effects(vec![if_then(read("target"), vec![], vec![])]),
        in_attack().key("effects").index(0).key("if"),
        DefinitionErrorKind::CardRefNotAValue {
            reference: "target".to_string(),
        },
    );

    // `is_card_type` takes a card ref: a value ref is the existing slot mismatch, and the card
    // type must exist.
    assert_invalid(
        with_where(is_card_type("attacker.power", "creature")),
        where_at().key("is_card_type").index(0),
        DefinitionErrorKind::ReferenceTypeMismatch {
            expected: RefType::Card,
            found: RefType::Value(FieldType::Int),
        },
    );
    assert_invalid(
        with_where(is_card_type("attacker", "ghost")),
        where_at().key("is_card_type").index(1),
        dangling(ReferenceTarget::CardType, "ghost"),
    );
    loaded(with_where(is_card_type("attacker", "creature")));
}

// ---- params

#[test]
fn a_param_alternative_names_a_declared_zone_card_type_and_owner() {
    let broken = |zone: &str, card_type: &str, owner: Option<&str>| {
        let mut alternative = alternative(zone, card_type);
        alternative.owner = owner.map(str::to_string);
        alternative
    };

    // The simple form: the keys are at the param.
    let simple = |alternative| draft_with(vec![single("attacker", alternative)], vec![]);
    assert_invalid(
        simple(broken("nowhere", "ghost", Some("enemy"))),
        attacker_params().key("zone"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );
    assert_invalid(
        simple(broken("field", "ghost", Some("enemy"))),
        attacker_params().key("card_type"),
        dangling(ReferenceTarget::CardType, "ghost"),
    );
    assert_invalid(
        simple(broken("field", "creature", Some("enemy"))),
        attacker_params().key("owner"),
        DefinitionErrorKind::UnknownOwner {
            owner: "enemy".to_string(),
        },
    );
    // The owner is checked before the `where`.
    let mut owned_and_wrong = broken("field", "creature", Some("enemy"));
    owned_and_wrong.condition = Some(int(5));
    assert_invalid(
        simple(owned_and_wrong),
        attacker_params().key("owner"),
        DefinitionErrorKind::UnknownOwner {
            owner: "enemy".to_string(),
        },
    );
    loaded(simple(broken("field", "creature", Some("opponent"))));
    loaded(simple(broken("field", "creature", None)));

    // `one_of`: the keys are at the alternative, each checked in turn.
    let many = |second| {
        draft_with(
            vec![one_of(
                "target",
                vec![opponent(alternative("field", "creature")), second],
            )],
            vec![],
        )
    };
    assert_invalid(
        many(broken("nowhere", "ghost", Some("enemy"))),
        target_params().key("one_of").index(1).key("zone"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );
    assert_invalid(
        many(broken("leader_zone", "ghost", Some("enemy"))),
        target_params().key("one_of").index(1).key("card_type"),
        dangling(ReferenceTarget::CardType, "ghost"),
    );
    assert_invalid(
        many(broken("leader_zone", "leader", Some("enemy"))),
        target_params().key("one_of").index(1).key("owner"),
        DefinitionErrorKind::UnknownOwner {
            owner: "enemy".to_string(),
        },
    );
    // An earlier alternative is checked first.
    let mut first_broken = opponent(alternative("nowhere", "creature"));
    first_broken.owner = Some("enemy".to_string());
    assert_invalid(
        draft_with(
            vec![one_of(
                "target",
                vec![first_broken, broken("nowhere", "ghost", None)],
            )],
            vec![],
        ),
        target_params().key("one_of").index(0).key("zone"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );

    // The model keeps the alternatives in order, with their owner.
    let definition = loaded(draft_with(vec![attacker(), target()], vec![]));
    let attack = definition
        .action(&tcg_engine::ruleset_definition::ActionId::new("attack"))
        .expect("attack is declared");
    let owners: Vec<_> = attack
        .params()
        .iter()
        .flat_map(|param| param.alternatives())
        .map(|alternative| {
            (
                alternative.zone().as_str().to_string(),
                alternative.card_type().as_str().to_string(),
                alternative.owner(),
                alternative.condition().is_some(),
            )
        })
        .collect();
    use tcg_engine::ruleset_definition::ParamOwner::{Actor, Opponent};
    assert_eq!(
        owners,
        [
            ("field".to_string(), "creature".to_string(), Actor, true),
            ("field".to_string(), "creature".to_string(), Opponent, true),
            (
                "leader_zone".to_string(),
                "leader".to_string(),
                Opponent,
                false
            ),
        ]
    );
}

#[test]
fn a_one_of_needs_an_alternative() {
    assert_invalid(
        draft_with(vec![one_of("target", vec![])], vec![]),
        target_params().key("one_of"),
        DefinitionErrorKind::NoAlternatives,
    );
    // One alternative is enough.
    loaded(draft_with(
        vec![one_of("target", vec![alternative("field", "creature")])],
        vec![],
    ));
}

#[test]
fn a_where_condition_reads_its_own_and_earlier_params_only() {
    // A param reads itself, typed as the alternative's card type alone.
    loaded(with_attacker(single(
        "attacker",
        when(
            alternative("field", "creature"),
            eq(read("attacker.tapped"), boolean(false)),
        ),
    )));
    assert_invalid(
        with_attacker(single(
            "attacker",
            when(
                alternative("field", "creature"),
                eq(read("attacker.life"), int(1)),
            ),
        )),
        attacker_params().key("where").key("eq").index(0),
        dangling(ReferenceTarget::FieldOrInstanceState, "life"),
    );

    // It does not read a param declared after it.
    assert_invalid(
        with_attacker(single(
            "attacker",
            when(
                alternative("field", "creature"),
                eq(read("target.tapped"), boolean(true)),
            ),
        )),
        attacker_params().key("where").key("eq").index(0),
        dangling(ReferenceTarget::Param, "target"),
    );

    // It reads an earlier param, with every card type it may take.
    let reads_attacker = |condition| {
        draft_with(
            vec![
                attacker(),
                one_of(
                    "target",
                    vec![
                        when(opponent(alternative("field", "creature")), condition),
                        opponent(alternative("leader_zone", "leader")),
                    ],
                ),
            ],
            vec![],
        )
    };
    loaded(reads_attacker(gte(
        read("attacker.power"),
        read("target.power"),
    )));
    // Inside the creature alternative `target` is a creature alone: `life` is not there ...
    assert_invalid(
        reads_attacker(gte(read("attacker.power"), read("target.life"))),
        target_params()
            .key("one_of")
            .index(0)
            .key("where")
            .key("gte")
            .index(1),
        dangling(ReferenceTarget::FieldOrInstanceState, "life"),
    );
    // ... and a name no earlier param has is dangling.
    assert_invalid(
        reads_attacker(gte(read("striker.power"), int(1))),
        target_params()
            .key("one_of")
            .index(0)
            .key("where")
            .key("gte")
            .index(0),
        dangling(ReferenceTarget::Param, "striker"),
    );
}

// ---- add_state

#[test]
fn add_state_needs_an_int_state_on_every_possible_card_type() {
    let at_effect = || in_attack().key("effects").index(0).key("add_state");

    // `target` may be a creature, which has no `current_life`.
    assert_invalid(
        with_effects(vec![add_state("target", "current_life", -1)]),
        at_effect().key("state"),
        dangling(ReferenceTarget::InstanceState, "current_life"),
    );
    // A state that no card type declares.
    assert_invalid(
        with_effects(vec![add_state("attacker", "mana_cost", 1)]),
        at_effect().key("state"),
        dangling(ReferenceTarget::InstanceState, "mana_cost"),
    );
    // `tapped` is a bool state: the amount is an int.
    assert_invalid(
        with_effects(vec![add_state("attacker", "tapped", 1)]),
        at_effect().key("amount"),
        DefinitionErrorKind::StateValueTypeMismatch {
            state: "tapped".to_string(),
            declared: FieldType::Bool,
            found: FieldType::Int,
        },
    );
    // The card is checked before the state.
    assert_invalid(
        with_effects(vec![add_state("ghost", "current_life", 1)]),
        at_effect().key("card"),
        dangling(ReferenceTarget::Param, "ghost"),
    );
    assert_invalid(
        with_effects(vec![add_state("attacker.power", "current_life", 1)]),
        at_effect().key("card"),
        DefinitionErrorKind::ReferenceTypeMismatch {
            expected: RefType::Card,
            found: RefType::Value(FieldType::Int),
        },
    );

    // An int state of a Leader-only param is valid, and the model keeps the amount.
    let definition = loaded(draft_with(
        vec![single("leader", alternative("leader_zone", "leader"))],
        vec![add_state("leader", "current_life", -3)],
    ));
    let attack = definition
        .action(&tcg_engine::ruleset_definition::ActionId::new("attack"))
        .expect("attack is declared");
    assert!(
        matches!(
            attack.effects(),
            [Effect::AddState { card, state, amount: -3 }]
                if card.param() == &ParamName::new("leader")
                    && state == &StateName::new("current_life")
        ),
        "add_state expected, got {:?}",
        attack.effects()
    );

    // `set_state` is held to the same rule on every possible card type.
    loaded(with_effects(vec![set_state("target", "tapped", true)]));
    assert_invalid(
        with_effects(vec![set_state("target", "recently_played", true)]),
        in_attack()
            .key("effects")
            .index(0)
            .key("set_state")
            .key("state"),
        dangling(ReferenceTarget::InstanceState, "recently_played"),
    );
    let mut draft = with_effects(vec![set_state("target", "tapped", true)]);
    draft.card_types[0].instance_state[1] = state(
        "tapped",
        FieldTypeDraft::Int,
        InitializerDraft::Literal(FieldValue::Int(0)),
    );
    assert_invalid(
        draft,
        in_attack()
            .key("effects")
            .index(0)
            .key("set_state")
            .key("value"),
        DefinitionErrorKind::StateValueTypeMismatch {
            state: "tapped".to_string(),
            declared: FieldType::Int,
            found: FieldType::Bool,
        },
    );
}

// ---- win conditions

fn win_condition(when: WinConditionWhenDraft) -> RulesetDraft {
    let mut draft = base();
    draft.win_conditions.push(WinConditionDraft {
        id: "leader_dead".to_string(),
        when,
        loser: "owner".to_string(),
    });
    draft
}

fn expression_when(expression: ExpressionDraft) -> RulesetDraft {
    win_condition(WinConditionWhenDraft::Expression(expression))
}

fn in_when() -> DefinitionPath {
    path().key("win_conditions").id("leader_dead").key("when")
}

#[test]
fn an_expression_win_condition_reads_exactly_one_card_type() {
    // The Small Ruleset's `leader_dead`: the Subject is the card type its refs start with.
    let definition = loaded(expression_when(lte(read("leader.current_life"), int(0))));
    let [condition] = definition.win_conditions() else {
        panic!("one win condition expected");
    };
    assert!(
        matches!(
            condition.when(),
            WinConditionWhen::Expression { subject, condition }
                if subject == &CardTypeId::new("leader")
                    && matches!(condition, Expression::Lte(..))
        ),
        "an expression about leaders, got {:?}",
        condition.when()
    );
    // A card ref counts too, and the same card type twice is one Subject.
    loaded(expression_when(ExpressionDraft::And(vec![
        is_card_type("leader", "leader"),
        lte(read("leader.current_life"), int(0)),
    ])));

    // No card type.
    assert_invalid(
        expression_when(eq(int(1), int(1))),
        in_when(),
        DefinitionErrorKind::InvalidSubjectCount { found: 0 },
    );
    // Two.
    assert_invalid(
        expression_when(ExpressionDraft::And(vec![
            lte(read("leader.current_life"), int(0)),
            lte(read("creature.power"), int(0)),
        ])),
        in_when(),
        DefinitionErrorKind::InvalidSubjectCount { found: 2 },
    );

    // The first name of a ref must be a card type.
    assert_invalid(
        expression_when(lte(read("ghost.current_life"), int(0))),
        in_when().key("lte").index(0),
        dangling(ReferenceTarget::CardType, "ghost"),
    );
    // The name after it must exist on that card type.
    assert_invalid(
        expression_when(lte(read("leader.cost"), int(0))),
        in_when().key("lte").index(0),
        dangling(ReferenceTarget::FieldOrInstanceState, "cost"),
    );
    // The `when` must be bool.
    assert_invalid(
        expression_when(read("leader.current_life")),
        in_when(),
        mismatch(FieldType::Bool, FieldType::Int),
    );
    assert_invalid(
        expression_when(lte(read("leader.name"), int(0))),
        in_when().key("lte").index(0),
        mismatch(FieldType::Int, FieldType::Text),
    );
}

// ---- Display

#[test]
fn the_new_rejections_and_definition_errors_have_a_display() {
    let shown_contains = |shown: String, needles: &[&str]| {
        for needle in needles {
            assert!(shown.contains(needle), "`{shown}` names `{needle}`");
        }
    };
    let param = ParamName::new("target");
    let card = CardInstanceId(7);

    shown_contains(
        Rejection::NotOpponentCard {
            param: param.clone(),
            card,
        }
        .to_string(),
        &["target", "7", "opponent"],
    );
    shown_contains(
        Rejection::WhereConditionNotMet {
            param: param.clone(),
            card,
        }
        .to_string(),
        &["target", "7", "where"],
    );
    shown_contains(
        Rejection::NoAlternativeMatched { param, card }.to_string(),
        &["target", "7", "alternative"],
    );
    shown_contains(
        Rejection::StateOverflow {
            card,
            state: StateName::new("current_life"),
        }
        .to_string(),
        &["current_life", "7", "overflow"],
    );
    shown_contains(
        Rejection::ExpressionTypeMismatch.to_string(),
        &["expression"],
    );
    shown_contains(
        GameCreationError::WinConditionRejected(Rejection::ExpressionTypeMismatch).to_string(),
        &["win condition", "expression"],
    );

    shown_contains(
        DefinitionErrorKind::UnknownOwner {
            owner: "enemy".to_string(),
        }
        .to_string(),
        &["enemy", "opponent"],
    );
    shown_contains(
        DefinitionErrorKind::NoAlternatives.to_string(),
        &["alternative"],
    );
    shown_contains(
        mismatch(FieldType::Bool, FieldType::Int).to_string(),
        &["Bool", "Int"],
    );
    shown_contains(
        DefinitionErrorKind::CardRefNotAValue {
            reference: "attacker".to_string(),
        }
        .to_string(),
        &["attacker", "card"],
    );
    shown_contains(
        DefinitionErrorKind::InconsistentReference {
            name: "rank".to_string(),
            card_type: "creature".to_string(),
            other: "leader".to_string(),
        }
        .to_string(),
        &["rank", "creature", "leader"],
    );
    shown_contains(
        DefinitionErrorKind::InvalidSubjectCount { found: 2 }.to_string(),
        &["one", "2"],
    );
}
