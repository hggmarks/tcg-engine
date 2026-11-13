//! Acceptance tests for ticket #49: the Card Effect envelope on a Card Definition, validated against
//! an already loaded Ruleset Definition (ADR-0022).
//!
//! Drafts are built in Rust, the way a code frontend would build them. Every error test asserts the
//! whole `CatalogError`: kind and path. No Card Effect is supported yet, so every well-formed one
//! ends in an unsupported error naming its path, one variant per kind.

mod common;

use common::{card, catalog_meta, path, small_definition, small_draft};
use tcg_engine::card_catalog::{
    CardCatalog, CardCatalogDraft, CardDraft, CardEffectDraft, CatalogError, CatalogErrorKind,
    ModifyDraft, TriggerMatcherDraft,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    DefinitionErrorKind, EffectDraft, ExpressionDraft, FieldType, FieldValue, RefDraft,
    ReferenceTarget, RulesetDefinition,
};

fn int(value: i64) -> FieldValue {
    FieldValue::Int(value)
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

/// The Small Ruleset structure with creatures allowed to carry Card Effects.
fn effects_definition() -> RulesetDefinition {
    let mut draft = small_draft();
    draft.card_types[1].effects = true;
    RulesetDefinition::validate(draft).expect("the effects ruleset draft is valid")
}

fn goblin_with(effects: Vec<CardEffectDraft>) -> CardDraft {
    let mut goblin = card(
        "goblin",
        "creature",
        vec![
            ("name", text("Goblin")),
            ("cost", int(1)),
            ("power", int(2)),
        ],
    );
    goblin.effects = effects;
    goblin
}

fn catalog_of(cards: Vec<CardDraft>) -> CardCatalogDraft {
    CardCatalogDraft {
        meta: catalog_meta("small", 1),
        cards,
    }
}

fn error_of(definition: &RulesetDefinition, effects: Vec<CardEffectDraft>) -> CatalogError {
    match CardCatalog::validate(definition, catalog_of(vec![goblin_with(effects)])) {
        Err(error) => error,
        Ok(catalog) => panic!("expected a validation error, got {catalog:?}"),
    }
}

/// The error for the effects of one goblin, validated against the effects ruleset.
fn effects_error_of(effects: Vec<CardEffectDraft>) -> CatalogError {
    error_of(&effects_definition(), effects)
}

/// `cards[goblin].effects[0]`.
fn effect_path() -> DefinitionPath {
    path().key("cards").id("goblin").key("effects").index(0)
}

fn self_ref(name: &str) -> RefDraft {
    RefDraft(name.to_string())
}

fn move_self_to_trash() -> EffectDraft {
    EffectDraft::Move {
        card: self_ref("self"),
        to: "trash".to_string(),
    }
}

/// A well-formed triggered Card Effect: `on: { enter: field }` and `do: [move self to trash]`.
fn triggered() -> CardEffectDraft {
    CardEffectDraft {
        on: Some(TriggerMatcherDraft::Enter("field".to_string())),
        condition: None,
        body: Some(vec![move_self_to_trash()]),
        while_condition: None,
        modify: None,
    }
}

/// A well-formed continuous Card Effect: `while: true`, `modify: { field: power, amount: 1 }`.
fn continuous() -> CardEffectDraft {
    CardEffectDraft {
        on: None,
        condition: None,
        body: None,
        while_condition: Some(ExpressionDraft::Literal(FieldValue::Bool(true))),
        modify: Some(ModifyDraft {
            field: "power".to_string(),
            amount: 1,
        }),
    }
}

/// `{ and: [ { eq: [ self.power, "three" ] } ] }`: ill-typed at `and[0].eq[1]`.
fn ill_typed_deep_condition() -> ExpressionDraft {
    ExpressionDraft::And(vec![ExpressionDraft::Eq(
        Box::new(ExpressionDraft::Ref(self_ref("self.power"))),
        Box::new(ExpressionDraft::Literal(text("three"))),
    )])
}

fn invalid_card_effect(at: DefinitionPath, error: DefinitionErrorKind) -> CatalogError {
    CatalogError {
        path: at,
        kind: CatalogErrorKind::InvalidCardEffect {
            error: Box::new(error),
        },
    }
}

// ---- where Card Effects are allowed

#[test]
fn effects_on_card_type_without_effects_is_rejected() {
    // The Small Ruleset's creature has `effects: false`: even a well-formed Card Effect is refused,
    // at the list, before the element is read.
    assert_eq!(
        error_of(&small_definition(), vec![triggered()]),
        CatalogError {
            path: path().key("cards").id("goblin").key("effects"),
            kind: CatalogErrorKind::EffectsNotAllowed,
        }
    );
    assert_eq!(
        error_of(&small_definition(), vec![CardEffectDraft::default()]).kind,
        CatalogErrorKind::EffectsNotAllowed
    );

    // An empty list is the same as none, on a type that allows effects or not.
    for definition in [small_definition(), effects_definition()] {
        assert!(CardCatalog::validate(&definition, catalog_of(vec![goblin_with(vec![])])).is_ok());
    }
}

// ---- the shape

#[test]
fn mixed_card_effect_shapes_are_rejected() {
    // `on` with `while`, and each key of one shape with a key of the other.
    let mut on_and_while = triggered();
    on_and_while.while_condition = continuous().while_condition;
    let mut modify_on_trigger = triggered();
    modify_on_trigger.modify = continuous().modify;
    let mut do_under_while = continuous();
    do_under_while.body = triggered().body;
    let mut if_under_while = continuous();
    if_under_while.condition = Some(ExpressionDraft::Literal(FieldValue::Bool(true)));
    for (name, mixed) in [
        ("on + while", on_and_while),
        ("on + modify", modify_on_trigger),
        ("while + do", do_under_while),
        ("while + if", if_under_while),
    ] {
        assert_eq!(
            effects_error_of(vec![mixed]),
            CatalogError {
                path: effect_path(),
                kind: CatalogErrorKind::MixedCardEffectShapes,
            },
            "{name}"
        );
    }
}

#[test]
fn card_effect_without_a_shape_is_rejected() {
    // Neither `on` nor `while`: an empty element, or keys that belong to a shape that was not
    // opened.
    let mut only_do = CardEffectDraft::default();
    only_do.body = triggered().body;
    let mut only_modify = CardEffectDraft::default();
    only_modify.modify = continuous().modify;
    for (name, shapeless) in [
        ("empty", CardEffectDraft::default()),
        ("do only", only_do),
        ("modify only", only_modify),
    ] {
        assert_eq!(
            effects_error_of(vec![shapeless]),
            CatalogError {
                path: effect_path(),
                kind: CatalogErrorKind::MissingCardEffectShape,
            },
            "{name}"
        );
    }
}

#[test]
fn card_effect_with_empty_do_is_rejected() {
    let mut empty = triggered();
    empty.body = Some(vec![]);
    assert_eq!(
        effects_error_of(vec![empty]),
        CatalogError {
            path: effect_path().key("do"),
            kind: CatalogErrorKind::EmptyDo,
        }
    );

    // An absent `do` is a different error at the same place.
    let mut missing = triggered();
    missing.body = None;
    assert_eq!(
        effects_error_of(vec![missing]),
        CatalogError {
            path: effect_path().key("do"),
            kind: CatalogErrorKind::MissingDo,
        }
    );
}

#[test]
fn continuous_card_effect_without_modify_is_rejected() {
    let mut missing = continuous();
    missing.modify = None;
    assert_eq!(
        effects_error_of(vec![missing]),
        CatalogError {
            path: effect_path(),
            kind: CatalogErrorKind::MissingModify,
        }
    );
}

// ---- the trigger matcher

#[test]
fn unknown_trigger_matcher_is_rejected() {
    let mut unknown = triggered();
    unknown.on = Some(TriggerMatcherDraft::Other("leave".to_string()));
    assert_eq!(
        effects_error_of(vec![unknown]),
        CatalogError {
            path: effect_path().key("on"),
            kind: CatalogErrorKind::UnknownTriggerMatcher {
                matcher: "leave".to_string(),
            },
        }
    );
}

#[test]
fn enter_matcher_needs_a_declared_zone() {
    let mut dangling = triggered();
    dangling.on = Some(TriggerMatcherDraft::Enter("graveyard".to_string()));
    assert_eq!(
        effects_error_of(vec![dangling]),
        CatalogError {
            path: effect_path().key("on").key("enter"),
            kind: CatalogErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "graveyard".to_string(),
            },
        }
    );
}

// ---- conditions reuse Expression checking, with the deep path kept

#[test]
fn ill_typed_trigger_condition_is_rejected() {
    let mut ill_typed = triggered();
    ill_typed.condition = Some(ill_typed_deep_condition());
    assert_eq!(
        effects_error_of(vec![ill_typed]),
        invalid_card_effect(
            effect_path()
                .key("if")
                .key("and")
                .index(0)
                .key("eq")
                .index(1),
            DefinitionErrorKind::ExpressionTypeMismatch {
                expected: FieldType::Int,
                found: FieldType::Text,
            },
        )
    );

    // A condition that is not a bool fails at the `if` itself.
    let mut not_bool = triggered();
    not_bool.condition = Some(ExpressionDraft::Literal(int(1)));
    assert_eq!(
        effects_error_of(vec![not_bool]),
        invalid_card_effect(
            effect_path().key("if"),
            DefinitionErrorKind::ExpressionTypeMismatch {
                expected: FieldType::Bool,
                found: FieldType::Int,
            },
        )
    );
}

#[test]
fn ill_typed_continuous_condition_is_rejected() {
    let mut ill_typed = continuous();
    ill_typed.while_condition = Some(ill_typed_deep_condition());
    assert_eq!(
        effects_error_of(vec![ill_typed]),
        invalid_card_effect(
            effect_path()
                .key("while")
                .key("and")
                .index(0)
                .key("eq")
                .index(1),
            DefinitionErrorKind::ExpressionTypeMismatch {
                expected: FieldType::Int,
                found: FieldType::Text,
            },
        )
    );
}

#[test]
fn empty_and_condition_is_a_valid_bool() {
    // `and: []` holds, so it passes expression checking; the Card Effect then fails only as
    // unsupported.
    let mut trigger = triggered();
    trigger.condition = Some(ExpressionDraft::And(vec![]));
    assert_eq!(
        effects_error_of(vec![trigger]).kind,
        CatalogErrorKind::UnsupportedTriggeredCardEffect
    );
    let mut continuous = continuous();
    continuous.while_condition = Some(ExpressionDraft::And(vec![]));
    assert_eq!(
        effects_error_of(vec![continuous]).kind,
        CatalogErrorKind::UnsupportedContinuousCardEffect
    );
}

// ---- the self ref

#[test]
fn self_ref_resolves_against_the_carrying_card_type() {
    // `self.power` in `if` and `{ ref: self }` in `do` resolve: the Card Effect fails only as
    // unsupported.
    let mut using_self = triggered();
    using_self.condition = Some(ExpressionDraft::Gte(
        Box::new(ExpressionDraft::Ref(self_ref("self.power"))),
        Box::new(ExpressionDraft::Literal(int(1))),
    ));
    assert_eq!(
        effects_error_of(vec![using_self]),
        CatalogError {
            path: effect_path(),
            kind: CatalogErrorKind::UnsupportedTriggeredCardEffect,
        }
    );

    // Self is typed as the card's own type: a field that creatures lack (a Leader's `life`) does
    // not resolve.
    let mut unknown_name = triggered();
    unknown_name.condition = Some(ExpressionDraft::Gte(
        Box::new(ExpressionDraft::Ref(self_ref("self.life"))),
        Box::new(ExpressionDraft::Literal(int(1))),
    ));
    assert_eq!(
        effects_error_of(vec![unknown_name]),
        invalid_card_effect(
            effect_path().key("if").key("gte").index(0),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::FieldOrInstanceState,
                id: "life".to_string(),
            },
        )
    );

    // The only name in scope is `self`: any other starts a dangling reference to a param.
    let mut other = triggered();
    other.body = Some(vec![EffectDraft::Move {
        card: self_ref("other"),
        to: "trash".to_string(),
    }]);
    assert_eq!(
        effects_error_of(vec![other]),
        invalid_card_effect(
            effect_path().key("do").index(0).key("move").key("card"),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Param,
                id: "other".to_string(),
            },
        )
    );

    // An effect of `do` is checked in the same scope: `set_state` of an instance state the card
    // type declares resolves, one it lacks does not.
    let mut set_declared = triggered();
    set_declared.body = Some(vec![EffectDraft::SetState {
        card: self_ref("self"),
        state: "tapped".to_string(),
        value: FieldValue::Bool(true),
    }]);
    assert_eq!(
        effects_error_of(vec![set_declared]).kind,
        CatalogErrorKind::UnsupportedTriggeredCardEffect
    );
    let mut set_lacking = triggered();
    set_lacking.body = Some(vec![EffectDraft::SetState {
        card: self_ref("self"),
        state: "frozen".to_string(),
        value: FieldValue::Bool(true),
    }]);
    assert_eq!(
        effects_error_of(vec![set_lacking]),
        invalid_card_effect(
            effect_path()
                .key("do")
                .index(0)
                .key("set_state")
                .key("state"),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::InstanceState,
                id: "frozen".to_string(),
            },
        )
    );
}

// ---- end_turn

#[test]
fn end_turn_in_card_effect_is_rejected() {
    let mut direct = triggered();
    direct.body = Some(vec![move_self_to_trash(), EffectDraft::EndTurn]);
    assert_eq!(
        effects_error_of(vec![direct]),
        CatalogError {
            path: effect_path().key("do").index(1),
            kind: CatalogErrorKind::EndTurnInCardEffect,
        }
    );

    // Nested in the `then` or the `else` of an `if` effect, at the end_turn's own path.
    let nested = |then: Vec<EffectDraft>, otherwise: Vec<EffectDraft>| {
        let mut effect = triggered();
        effect.body = Some(vec![EffectDraft::If {
            condition: ExpressionDraft::Literal(FieldValue::Bool(true)),
            then,
            otherwise,
        }]);
        effect
    };
    assert_eq!(
        effects_error_of(vec![nested(
            vec![move_self_to_trash(), EffectDraft::EndTurn],
            vec![]
        )]),
        CatalogError {
            path: effect_path().key("do").index(0).key("then").index(1),
            kind: CatalogErrorKind::EndTurnInCardEffect,
        }
    );
    assert_eq!(
        effects_error_of(vec![nested(vec![], vec![EffectDraft::EndTurn])]),
        CatalogError {
            path: effect_path().key("do").index(0).key("else").index(0),
            kind: CatalogErrorKind::EndTurnInCardEffect,
        }
    );
}

// ---- modify

#[test]
fn modify_of_undeclared_field_is_rejected() {
    let modifying = |field: &str| {
        let mut effect = continuous();
        effect.modify = Some(ModifyDraft {
            field: field.to_string(),
            amount: 1,
        });
        effect
    };
    // A name the card type does not declare, and an instance state (a Card Effect modifies
    // definition fields, not instance state): the same error.
    for field in ["rarity", "tapped"] {
        assert_eq!(
            effects_error_of(vec![modifying(field)]),
            CatalogError {
                path: effect_path().key("modify").key("field"),
                kind: CatalogErrorKind::UnknownModifyField {
                    field: field.to_string(),
                },
            },
            "{field}"
        );
    }
}

#[test]
fn modify_of_non_int_field_is_rejected() {
    let mut effect = continuous();
    effect.modify = Some(ModifyDraft {
        field: "name".to_string(),
        amount: 1,
    });
    assert_eq!(
        effects_error_of(vec![effect]),
        CatalogError {
            path: effect_path().key("modify").key("field"),
            kind: CatalogErrorKind::ModifyFieldNotInt {
                field: "name".to_string(),
                found: FieldType::Text,
            },
        }
    );
}

// ---- unsupported until implemented

#[test]
fn loaded_triggered_card_effect_is_unsupported() {
    // The first element, and a later one after a well-formed first.
    assert_eq!(
        effects_error_of(vec![triggered()]),
        CatalogError {
            path: effect_path(),
            kind: CatalogErrorKind::UnsupportedTriggeredCardEffect,
        }
    );
    let mut with_condition = triggered();
    with_condition.condition = Some(ExpressionDraft::And(vec![]));
    assert_eq!(
        effects_error_of(vec![with_condition]).kind,
        CatalogErrorKind::UnsupportedTriggeredCardEffect
    );
    // `on: { turn_start }` is a triggered Card Effect too.
    let mut turn_start = triggered();
    turn_start.on = Some(TriggerMatcherDraft::TurnStart);
    assert_eq!(
        effects_error_of(vec![turn_start]).kind,
        CatalogErrorKind::UnsupportedTriggeredCardEffect
    );
}

#[test]
fn loaded_continuous_card_effect_is_unsupported() {
    assert_eq!(
        effects_error_of(vec![continuous()]),
        CatalogError {
            path: effect_path(),
            kind: CatalogErrorKind::UnsupportedContinuousCardEffect,
        }
    );
}

#[test]
fn card_effects_are_checked_in_order_and_the_first_error_wins() {
    // Element 0 is ill-formed, element 1 is unsupported: element 0 is reported. Swapped, the
    // unsupported element 0 is reported, never the ill-formed element 1.
    let mut empty_do = triggered();
    empty_do.body = Some(vec![]);
    assert_eq!(
        effects_error_of(vec![empty_do.clone(), continuous()]).path,
        effect_path().key("do")
    );
    assert_eq!(
        effects_error_of(vec![continuous(), empty_do]),
        CatalogError {
            path: effect_path(),
            kind: CatalogErrorKind::UnsupportedContinuousCardEffect,
        }
    );
}
