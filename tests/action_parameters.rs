//! Acceptance tests for ticket #24 that need no YAML: action definitions built from code the way a
//! code frontend would build them, and the Display of the new Rejections.
//!
//! The same definitions read from the real Small Ruleset file, and playing creatures, are tested
//! in the loader crate (`crates/tcg-loader/tests/play_creature.rs`).

mod common;

use common::{path, small_draft};
use tcg_engine::declarative::{CardInstanceId, Rejection};
use tcg_engine::player::PlayerId;
use tcg_engine::ruleset_definition::{
    ActionDraft, ActionId, CardTypeId, CardValue, Cost, CostDraft, DefinitionError,
    DefinitionErrorKind, Effect, EffectDraft, FieldName, FieldTypeDraft, FieldValue,
    InitializerDraft, InstanceStateDraft, ParamDraft, ParamName, RefDraft, ReferenceTarget,
    RulesetDefinition, RulesetDraft, StateName, ZoneId,
};

/// The Small Ruleset structure whose creatures also have `recently_played`, with `play_creature`
/// built from drafts.
fn draft_with_play_creature(count: &str) -> RulesetDraft {
    let mut draft = small_draft();
    draft.card_types[1].instance_state.push(InstanceStateDraft {
        name: "recently_played".to_string(),
        state_type: FieldTypeDraft::Bool,
        init: InitializerDraft::Literal(FieldValue::Bool(false)),
    });
    draft.actions.push(ActionDraft {
        id: "play_creature".to_string(),
        params: vec![ParamDraft {
            name: "card".to_string(),
            zone: "hand".to_string(),
            card_type: "creature".to_string(),
        }],
        cost: vec![CostDraft::Tap {
            zone: "mana_zone".to_string(),
            count: RefDraft(count.to_string()),
        }],
        effects: vec![
            EffectDraft::Move {
                card: RefDraft("card".to_string()),
                to: "field".to_string(),
            },
            EffectDraft::SetState {
                card: RefDraft("card".to_string()),
                state: "recently_played".to_string(),
                value: FieldValue::Bool(true),
            },
        ],
    });
    draft
}

#[test]
fn a_code_frontend_builds_an_action_with_params_cost_and_effects() {
    let definition = RulesetDefinition::validate(draft_with_play_creature("card.cost"))
        .expect("the draft is valid");
    let play = definition
        .action(&ActionId::new("play_creature"))
        .expect("play_creature is declared");

    let [param] = play.params() else {
        panic!("one param expected, got {:?}", play.params());
    };
    assert_eq!(param.name(), &ParamName::new("card"));
    assert_eq!(param.zone(), &ZoneId::new("hand"));
    assert_eq!(param.card_type(), &CardTypeId::new("creature"));

    let [Cost::Tap { zone, count }] = play.costs() else {
        panic!("one tap cost expected, got {:?}", play.costs());
    };
    assert_eq!(zone, &ZoneId::new("mana_zone"));
    assert_eq!(count.param(), &ParamName::new("card"));
    // `cost` is a field of the creature, resolved when the definition is validated.
    assert_eq!(count.value(), &CardValue::Field(FieldName::new("cost")));

    let [Effect::Move { card, to }, Effect::SetState {
        card: marked,
        state,
        value,
    }] = play.effects()
    else {
        panic!("move then set_state expected, got {:?}", play.effects());
    };
    assert_eq!(card.param(), &ParamName::new("card"));
    assert_eq!(to, &ZoneId::new("field"));
    assert_eq!(marked.param(), &ParamName::new("card"));
    assert_eq!(state, &StateName::new("recently_played"));
    assert_eq!(value, &FieldValue::Bool(true));
}

#[test]
fn a_ref_to_an_instance_state_resolves_to_a_state() {
    // `price` is an int instance state of the creature and no field: the ref reads the state.
    let mut draft = draft_with_play_creature("card.price");
    draft.card_types[1].instance_state.push(InstanceStateDraft {
        name: "price".to_string(),
        state_type: FieldTypeDraft::Int,
        init: InitializerDraft::Literal(FieldValue::Int(1)),
    });
    let definition = RulesetDefinition::validate(draft).expect("the draft is valid");
    let play = definition
        .action(&ActionId::new("play_creature"))
        .expect("play_creature is declared");
    let [Cost::Tap { count, .. }] = play.costs() else {
        panic!("one tap cost expected, got {:?}", play.costs());
    };
    assert_eq!(count.value(), &CardValue::State(StateName::new("price")));
}

#[test]
fn a_code_frontend_gets_the_same_reference_errors_as_the_loader() {
    let error = RulesetDefinition::validate(draft_with_play_creature("spell.cost"))
        .expect_err("the param `spell` is not declared");
    assert_eq!(
        error,
        DefinitionError {
            path: path()
                .key("actions")
                .id("play_creature")
                .key("cost")
                .index(0)
                .key("tap")
                .key("count"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Param,
                id: "spell".to_string(),
            },
        }
    );

    let error = RulesetDefinition::validate(draft_with_play_creature("card.cost.x"))
        .expect_err("the ref is malformed");
    assert_eq!(
        error.kind,
        DefinitionErrorKind::InvalidReference {
            reference: "card.cost.x".to_string()
        }
    );
}

#[test]
fn the_new_rejections_name_what_went_wrong() {
    let param = ParamName::new("card");
    let card = CardInstanceId(7);
    let text = |rejection: Rejection| rejection.to_string();

    let shown = text(Rejection::ArgumentCountMismatch {
        expected: 1,
        found: 2,
    });
    assert!(shown.contains('1') && shown.contains('2'), "{shown}");

    let shown = text(Rejection::UnknownCard {
        param: param.clone(),
        card,
    });
    assert!(shown.contains("card") && shown.contains('7'), "{shown}");

    let shown = text(Rejection::NotCardOwner {
        param: param.clone(),
        card,
        owner: PlayerId(1),
    });
    assert!(
        shown.contains("card") && shown.contains('7') && shown.contains('1'),
        "{shown}"
    );

    let shown = text(Rejection::CardNotInZone {
        param: param.clone(),
        card,
        zone: ZoneId::new("hand"),
    });
    assert!(shown.contains("hand") && shown.contains('7'), "{shown}");

    let shown = text(Rejection::WrongCardType {
        param,
        card,
        expected: CardTypeId::new("creature"),
        found: CardTypeId::new("mana"),
    });
    assert!(
        shown.contains("creature") && shown.contains("mana"),
        "{shown}"
    );

    let shown = text(Rejection::NegativeCount { value: -3 });
    assert!(shown.contains("-3"), "{shown}");

    let shown = text(Rejection::NotEnoughUntappedCards {
        zone: ZoneId::new("mana_zone"),
        required: 4,
        available: 2,
    });
    assert!(
        shown.contains("mana_zone") && shown.contains('4') && shown.contains('2'),
        "{shown}"
    );

    let shown = text(Rejection::CardDataMismatch { card });
    assert!(shown.contains('7'), "{shown}");
}
