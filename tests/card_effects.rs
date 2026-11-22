//! Acceptance tests for ticket #49: the Card Effect envelope on a Card Definition, validated against
//! an already loaded Ruleset Definition (ADR-0022).
//!
//! Drafts are built in Rust, the way a code frontend would build them. Every error test asserts the
//! whole `CatalogError`: kind and path. A well-formed Card Effect of either shape is accepted and
//! kept by its Card Definition: triggered since ticket #50 (ADR-0023), continuous since ticket #51
//! (ADR-0024).

mod common;

use common::{
    card, catalog_meta, creature_id, path, small_catalog, small_catalog_draft, small_definition,
    small_draft,
};
use std::num::NonZeroU32;

use tcg_engine::card_catalog::{
    CardCatalog, CardCatalogDraft, CardDraft, CardEffect, CardEffectDraft, CardId, CatalogError,
    CatalogErrorKind, ModifyDraft, ScopeDraft, ScopeOwner, TriggerMatcherDraft,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    ActionDraft, DefinitionError, DefinitionErrorKind, Effect, EffectDraft, EffectPlayer,
    EffectsDraft, Expression, ExpressionDraft, FieldName, FieldType, FieldValue, RefDraft,
    ReferenceTarget, RulesetDefinition, ZoneId,
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

/// Validates one goblin with these effects against the effects ruleset: they must be accepted.
fn assert_accepted(effects: Vec<CardEffectDraft>) {
    if let Err(error) = CardCatalog::validate(
        &effects_definition(),
        catalog_of(vec![goblin_with(effects)]),
    ) {
        panic!("expected the Card Effects to be accepted, got {error:?}");
    }
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
        scope: None,
        activated: None,
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
        scope: None,
        activated: None,
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
    let only_do = CardEffectDraft {
        body: triggered().body,
        ..CardEffectDraft::default()
    };
    let only_modify = CardEffectDraft {
        modify: continuous().modify,
        ..CardEffectDraft::default()
    };
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
            path: effect_path().key("modify"),
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
    // `and: []` holds, so it passes expression checking and a Card Effect of either shape is
    // accepted.
    let mut trigger = triggered();
    trigger.condition = Some(ExpressionDraft::And(vec![]));
    assert_accepted(vec![trigger]);
    let mut continuous = continuous();
    continuous.while_condition = Some(ExpressionDraft::And(vec![]));
    assert!(CardCatalog::validate(
        &effects_definition(),
        catalog_of(vec![goblin_with(vec![continuous])])
    )
    .is_ok());
}

// ---- the self ref

#[test]
fn self_ref_resolves_against_the_carrying_card_type() {
    // `self.power` in `if` and `{ ref: self }` in `do` resolve: the Card Effect is accepted.
    let mut using_self = triggered();
    using_self.condition = Some(ExpressionDraft::Gte(
        Box::new(ExpressionDraft::Ref(self_ref("self.power"))),
        Box::new(ExpressionDraft::Literal(int(1))),
    ));
    assert_accepted(vec![using_self]);

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
    assert_accepted(vec![set_declared]);
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

#[test]
fn narrowing_self_to_a_card_type_the_card_can_never_have_is_a_load_error() {
    // A goblin is a creature: `self` can never be a leader, so the test is an error at its own path
    // and not a retype of `self`.
    let mut effect = triggered();
    effect.body = Some(vec![EffectDraft::If {
        condition: ExpressionDraft::IsCardType {
            card: self_ref("self"),
            card_type: "leader".to_string(),
        },
        then: vec![move_self_to_trash()],
        otherwise: vec![],
    }]);
    assert_eq!(
        effects_error_of(vec![effect]),
        invalid_card_effect(
            effect_path()
                .key("do")
                .index(0)
                .key("if")
                .key("is_card_type"),
            DefinitionErrorKind::ImpossibleCardType {
                param: "self".to_string(),
                card_type: "leader".to_string(),
            },
        )
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

// ---- unsupported until implemented (continuous only; triggered was lifted by ADR-0023)

#[test]
fn loaded_triggered_card_effect_is_accepted() {
    // ADR-0023 lifted the rejection of triggered Card Effects (ADR-0022).
    assert_accepted(vec![triggered()]);
    let mut with_condition = triggered();
    with_condition.condition = Some(ExpressionDraft::And(vec![]));
    assert_accepted(vec![with_condition]);
    // `on: { turn_start }` is a triggered Card Effect too.
    let mut turn_start = triggered();
    turn_start.on = Some(TriggerMatcherDraft::TurnStart);
    assert_accepted(vec![turn_start]);
}

#[test]
fn continuous_card_effects_load_into_the_card_definition() {
    // A continuous Card Effect is accepted on the carrying card and is visible in `effects()`, in
    // file order, with its `while` and its `modify`.
    let mut second = continuous();
    second.modify = Some(ModifyDraft {
        field: "power".to_string(),
        amount: -3,
    });
    let catalog = CardCatalog::validate(
        &effects_definition(),
        catalog_of(vec![goblin_with(vec![continuous(), second])]),
    )
    .expect("continuous Card Effects are supported");
    let goblin = catalog
        .card(&CardId::new("goblin"))
        .expect("the card exists");
    let [CardEffect::Continuous(first), CardEffect::Continuous(second)] = goblin.effects() else {
        panic!(
            "expected two continuous Card Effects, got {:?}",
            goblin.effects()
        );
    };
    assert_eq!(
        first.condition(),
        &Expression::Literal(FieldValue::Bool(true))
    );
    assert_eq!(first.modify().field(), &FieldName::new("power"));
    assert_eq!(first.modify().amount(), 1);
    assert_eq!(second.modify().amount(), -3);
}

#[test]
fn card_effects_are_checked_in_order_and_the_first_error_wins() {
    // Element 0 is ill-formed and element 1 is well-formed: element 0 is reported, at its own
    // path.
    let mut empty_do = triggered();
    empty_do.body = Some(vec![]);
    assert_eq!(
        effects_error_of(vec![empty_do.clone(), triggered()]).path,
        effect_path().key("do")
    );
    // A well-formed element of either shape is accepted, so the ill-formed one after it is the one
    // reported, at its own index.
    let second = path()
        .key("cards")
        .id("goblin")
        .key("effects")
        .index(1)
        .key("do");
    assert_eq!(
        effects_error_of(vec![triggered(), empty_do.clone()]).path,
        second
    );
    assert_eq!(effects_error_of(vec![continuous(), empty_do]).path, second);
}

// ---- in a full catalog

#[test]
fn card_effect_of_a_card_in_a_full_catalog_is_addressed_by_its_card_id() {
    // The catalog without effects is valid against the effects ruleset too: nothing carries a Card
    // Effect.
    assert!(small_catalog()
        .cards()
        .iter()
        .all(|definition| definition.effects().is_empty()));
    assert!(CardCatalog::validate(&effects_definition(), small_catalog_draft()).is_ok());

    // An ill-formed effect on the third creature is addressed by that card's id, after the cards
    // before it and the well-formed effect before it were accepted.
    let mut draft = small_catalog_draft();
    let id = creature_id(3);
    let Some(third) = draft.cards.iter_mut().find(|card| card.id == id) else {
        panic!("the catalog has {id}");
    };
    let mut empty_do = triggered();
    empty_do.body = Some(vec![]);
    third.effects = vec![continuous(), empty_do];
    let Err(error) = CardCatalog::validate(&effects_definition(), draft) else {
        panic!("expected a validation error");
    };
    assert_eq!(
        error,
        CatalogError {
            path: path()
                .key("cards")
                .id(&id)
                .key("effects")
                .index(1)
                .key("do"),
            kind: CatalogErrorKind::EmptyDo,
        }
    );
}

// ---- ticket #50: triggered Card Effects are accepted, continuous ones still reject (ADR-0023)

fn draw_for_controller(count: u32) -> EffectDraft {
    EffectDraft::Draw {
        player: "controller".to_string(),
        from: "deck".to_string(),
        to: "hand".to_string(),
        count,
    }
}

#[test]
fn card_effects_of_both_shapes_are_accepted_and_kept_in_file_order() {
    // Both unsupported rejections of ADR-0022 are lifted: triggered by ADR-0023, continuous by
    // ADR-0024. A card may mix the shapes, and keeps every element in file order.
    let catalog = CardCatalog::validate(
        &effects_definition(),
        catalog_of(vec![goblin_with(vec![
            triggered(),
            continuous(),
            triggered(),
        ])]),
    )
    .expect("Card Effects of either shape are supported");
    let goblin = catalog
        .card(&CardId::new("goblin"))
        .expect("the catalog has the goblin");
    let [CardEffect::Triggered(_), CardEffect::Continuous(_), CardEffect::Triggered(_)] =
        goblin.effects()
    else {
        panic!(
            "expected a triggered, a continuous and a triggered Card Effect, got {:?}",
            goblin.effects()
        );
    };
}

#[test]
fn draw_effect_for_the_controller_validates_in_a_card_effect() {
    let mut trigger = triggered();
    trigger.body = Some(vec![draw_for_controller(2)]);
    let catalog = CardCatalog::validate(
        &effects_definition(),
        catalog_of(vec![goblin_with(vec![trigger])]),
    )
    .expect("a draw for the controller is a Card Effect");
    let goblin = catalog
        .card(&CardId::new("goblin"))
        .expect("the catalog has the goblin");
    let CardEffect::Triggered(effect) = &goblin.effects()[0] else {
        panic!("expected a triggered Card Effect");
    };
    assert_eq!(
        effect.body(),
        [Effect::Draw {
            player: EffectPlayer::Controller,
            from: ZoneId::new("deck"),
            to: ZoneId::new("hand"),
            count: NonZeroU32::new(2).expect("2 is not 0"),
        }]
    );
}

#[test]
fn draw_effect_in_a_card_effect_checks_its_zones_count_and_player() {
    let draw_path = || effect_path().key("do").index(0).key("draw");
    let with = |draw: EffectDraft| {
        let mut trigger = triggered();
        trigger.body = Some(vec![draw]);
        effects_error_of(vec![trigger])
    };
    let draw_of = |player: &str, from: &str, count: u32| EffectDraft::Draw {
        player: player.to_string(),
        from: from.to_string(),
        to: "hand".to_string(),
        count,
    };

    assert_eq!(
        with(draw_of("controller", "nowhere", 1)),
        invalid_card_effect(
            draw_path().key("from"),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "nowhere".to_string(),
            },
        )
    );
    assert_eq!(
        with(EffectDraft::Draw {
            player: "controller".to_string(),
            from: "deck".to_string(),
            to: "nowhere".to_string(),
            count: 1,
        }),
        invalid_card_effect(
            draw_path().key("to"),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "nowhere".to_string(),
            },
        )
    );
    // The source of a draw is an ordered zone, like the source of a draw step.
    assert_eq!(
        with(draw_of("controller", "hand", 1)),
        invalid_card_effect(
            draw_path().key("from"),
            DefinitionErrorKind::ZoneNotOrdered {
                zone: "hand".to_string(),
            },
        )
    );
    assert_eq!(
        with(draw_of("controller", "deck", 0)),
        invalid_card_effect(draw_path().key("count"), DefinitionErrorKind::InvalidCount)
    );
    // `controller` is the only player an effect acts for.
    assert_eq!(
        with(draw_of("active", "deck", 1)),
        invalid_card_effect(
            draw_path().key("player"),
            DefinitionErrorKind::UnknownEffectPlayer {
                player: "active".to_string(),
            },
        )
    );
}

#[test]
fn controller_outside_a_card_effect_is_rejected_in_an_action() {
    let mut draft = small_draft();
    draft.actions.push(ActionDraft {
        id: "study".to_string(),
        params: vec![],
        cost: vec![],
        effects: vec![draw_for_controller(1)],
    });
    assert_eq!(
        RulesetDefinition::validate(draft),
        Err(DefinitionError {
            path: path()
                .key("actions")
                .id("study")
                .key("effects")
                .index(0)
                .key("draw")
                .key("player"),
            kind: DefinitionErrorKind::ControllerOutsideCardEffect,
        })
    );
}

#[test]
fn the_active_zone_of_card_effects_must_be_a_declared_zone() {
    let mut draft = small_draft();
    assert_eq!(
        RulesetDefinition::validate(draft.clone())
            .expect("valid")
            .effects_active_zone(),
        None
    );
    draft.effects = Some(EffectsDraft {
        active_zone: "field".to_string(),
    });
    assert_eq!(
        RulesetDefinition::validate(draft.clone())
            .expect("valid")
            .effects_active_zone(),
        Some(&ZoneId::new("field"))
    );
    draft.effects = Some(EffectsDraft {
        active_zone: "battlefield".to_string(),
    });
    assert_eq!(
        RulesetDefinition::validate(draft),
        Err(DefinitionError {
            path: path().key("effects").key("active_zone"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "battlefield".to_string(),
            },
        })
    );
}

// ---- ticket #52: the Modifier Scope of a continuous Card Effect (ADR-0025)

/// A continuous Card Effect with a scope over the creatures of `field`:
/// `while: true`, `scope: { zone: field, card_type: creature }`, `modify: { field: power, amount: 1 }`.
fn scoped() -> CardEffectDraft {
    let mut effect = continuous();
    effect.scope = Some(ScopeDraft {
        zone: "field".to_string(),
        card_type: "creature".to_string(),
        owner: None,
        include_self: None,
    });
    effect
}

fn scoped_with(change: impl FnOnce(&mut ScopeDraft)) -> CardEffectDraft {
    let mut effect = scoped();
    if let Some(scope) = effect.scope.as_mut() {
        change(scope);
    }
    effect
}

#[test]
fn a_scope_loads_into_the_card_definition() {
    // The absent `owner` is the controller and the absent `include_self` is false; `owner:
    // opponent` and `include_self: true` are kept. A continuous Card Effect without a scope has
    // none.
    let catalog = CardCatalog::validate(
        &effects_definition(),
        catalog_of(vec![goblin_with(vec![
            scoped(),
            scoped_with(|scope| {
                scope.owner = Some("opponent".to_string());
                scope.include_self = Some(true);
                scope.zone = "hand".to_string();
                scope.card_type = "leader".to_string();
            }),
            continuous(),
        ])]),
    )
    .expect("scoped continuous Card Effects are supported");
    let goblin = catalog
        .card(&CardId::new("goblin"))
        .expect("the card exists");
    let [CardEffect::Continuous(controller), CardEffect::Continuous(opponent), CardEffect::Continuous(own)] =
        goblin.effects()
    else {
        panic!(
            "expected three continuous Card Effects, got {:?}",
            goblin.effects()
        );
    };
    let controller = controller.scope().expect("the first has a scope");
    assert_eq!(controller.zone(), &ZoneId::new("field"));
    assert_eq!(controller.card_type().as_str(), "creature");
    assert_eq!(controller.owner(), ScopeOwner::Controller);
    assert!(!controller.include_self());
    let opponent = opponent.scope().expect("the second has a scope");
    assert_eq!(opponent.zone(), &ZoneId::new("hand"));
    assert_eq!(opponent.card_type().as_str(), "leader");
    assert_eq!(opponent.owner(), ScopeOwner::Opponent);
    assert!(opponent.include_self());
    assert_eq!(own.scope(), None);
}

#[test]
fn a_scope_zone_must_be_a_declared_zone() {
    assert_eq!(
        effects_error_of(vec![scoped_with(
            |scope| scope.zone = "graveyard".to_string()
        )]),
        CatalogError {
            path: effect_path().key("scope").key("zone"),
            kind: CatalogErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "graveyard".to_string(),
            },
        }
    );
}

#[test]
fn a_scope_card_type_must_be_declared() {
    assert_eq!(
        effects_error_of(vec![scoped_with(
            |scope| scope.card_type = "dragon".to_string()
        )]),
        CatalogError {
            path: effect_path().key("scope").key("card_type"),
            kind: CatalogErrorKind::DanglingReference {
                target: ReferenceTarget::CardType,
                id: "dragon".to_string(),
            },
        }
    );
    // The scope's card type does not need `effects: true`: its targets carry nothing (`leader` is
    // declared with `effects: false`).
    assert_accepted(vec![scoped_with(|scope| {
        scope.card_type = "leader".to_string();
    })]);
}

#[test]
fn an_unknown_scope_owner_is_rejected() {
    // Only `opponent` exists; the controller is the absent key.
    assert_eq!(
        effects_error_of(vec![scoped_with(
            |scope| scope.owner = Some("controller".to_string())
        )]),
        invalid_card_effect(
            effect_path().key("scope").key("owner"),
            DefinitionErrorKind::UnknownOwner {
                owner: "controller".to_string(),
            },
        )
    );
}

#[test]
fn modify_of_a_field_the_scope_card_type_does_not_declare_is_rejected() {
    // `mana` declares no field, and the carrier (a creature) declares `power`: the field is checked
    // against the scope's card type.
    let mut effect = scoped_with(|scope| scope.card_type = "mana".to_string());
    effect.modify = Some(ModifyDraft {
        field: "power".to_string(),
        amount: 1,
    });
    assert_eq!(
        effects_error_of(vec![effect]),
        CatalogError {
            path: effect_path().key("modify").key("field"),
            kind: CatalogErrorKind::UnknownModifyField {
                field: "power".to_string(),
            },
        }
    );

    // The other way round: `life` is a field of the scope's card type (`leader`) and not of the
    // carrier's, and an aura must be able to grant it.
    let mut effect = scoped_with(|scope| scope.card_type = "leader".to_string());
    effect.modify = Some(ModifyDraft {
        field: "life".to_string(),
        amount: 1,
    });
    assert_accepted(vec![effect]);

    // With no scope the carrier's card type is checked, as before.
    let mut own = continuous();
    own.modify = Some(ModifyDraft {
        field: "life".to_string(),
        amount: 1,
    });
    assert_eq!(
        effects_error_of(vec![own]),
        CatalogError {
            path: effect_path().key("modify").key("field"),
            kind: CatalogErrorKind::UnknownModifyField {
                field: "life".to_string(),
            },
        }
    );
}

#[test]
fn modify_of_a_non_int_field_of_the_scope_card_type_is_rejected() {
    let mut effect = scoped_with(|scope| scope.card_type = "leader".to_string());
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

#[test]
fn a_scope_without_a_while_has_no_shape() {
    // `while` stays required: a scope and a modify with no `while` is not a continuous Card Effect.
    let mut effect = scoped();
    effect.while_condition = None;
    assert_eq!(
        effects_error_of(vec![effect]),
        CatalogError {
            path: effect_path(),
            kind: CatalogErrorKind::MissingCardEffectShape,
        }
    );
}

#[test]
fn a_scope_beside_on_is_a_mixed_shape() {
    let mut effect = triggered();
    effect.scope = scoped().scope;
    assert_eq!(
        effects_error_of(vec![effect]),
        CatalogError {
            path: effect_path(),
            kind: CatalogErrorKind::MixedCardEffectShapes,
        }
    );
}

#[test]
fn a_scope_is_checked_after_while_and_before_modify() {
    // An ill-typed `while` is reported before an undeclared scope zone.
    let mut effect = scoped_with(|scope| scope.zone = "graveyard".to_string());
    effect.while_condition = Some(ill_typed_deep_condition());
    assert_eq!(
        effects_error_of(vec![effect]).path,
        effect_path()
            .key("while")
            .key("and")
            .index(0)
            .key("eq")
            .index(1)
    );
    // An undeclared scope zone is reported before a missing `modify`, and the zone before the card
    // type before the owner.
    let mut effect = scoped_with(|scope| {
        scope.zone = "graveyard".to_string();
        scope.card_type = "dragon".to_string();
        scope.owner = Some("nobody".to_string());
    });
    effect.modify = None;
    assert_eq!(
        effects_error_of(vec![effect.clone()]).path,
        effect_path().key("scope").key("zone")
    );
    if let Some(scope) = effect.scope.as_mut() {
        scope.zone = "field".to_string();
    }
    assert_eq!(
        effects_error_of(vec![effect.clone()]).path,
        effect_path().key("scope").key("card_type")
    );
    if let Some(scope) = effect.scope.as_mut() {
        scope.card_type = "creature".to_string();
    }
    assert_eq!(
        effects_error_of(vec![effect.clone()]).path,
        effect_path().key("scope").key("owner")
    );
    if let Some(scope) = effect.scope.as_mut() {
        scope.owner = None;
    }
    assert_eq!(
        effects_error_of(vec![effect]),
        CatalogError {
            path: effect_path().key("modify"),
            kind: CatalogErrorKind::MissingModify,
        }
    );
}
