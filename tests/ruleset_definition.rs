//! Acceptance tests for ticket #19: the typed `RulesetDefinition` and its semantic validation.
//!
//! Drafts are built in Rust, the way a code frontend would build them. Every error test asserts the
//! whole `DefinitionError`: kind and path.

use std::num::NonZeroU32;

use tcg_engine::definition_path::{DefinitionPath, PathSegment};
use tcg_engine::ruleset_definition::{
    ActionDraft, ActionId, AutomaticStep, AutomaticStepDraft, CardTypeDraft, CardTypeId, Condition,
    ConditionDraft, DeckInputDraft, DefinitionError, DefinitionErrorKind, Effect, EffectDraft,
    FieldDraft, FieldName, FieldType, FieldTypeDraft, FieldValue, Initializer, InitializerDraft,
    InstanceStateDraft, MetaDraft, ParametersDraft, PhaseDraft, PhaseKind, PlayerSelector,
    Position, ReferenceTarget, RulesetDefinition, RulesetDraft, SetupStep, SetupStepDraft,
    StateName, TurnDraft, Visibility, ZoneDraft, ZoneId,
};

// ---- draft builders

fn zone(id: &str, visibility: Visibility, ordered: bool) -> ZoneDraft {
    ZoneDraft {
        id: id.to_string(),
        visibility,
        ordered,
    }
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

fn tapped() -> InstanceStateDraft {
    state(
        "tapped",
        FieldTypeDraft::Bool,
        InitializerDraft::Literal(FieldValue::Bool(false)),
    )
}

fn card_type(
    id: &str,
    fields: Vec<FieldDraft>,
    instance_state: Vec<InstanceStateDraft>,
) -> CardTypeDraft {
    CardTypeDraft {
        id: id.to_string(),
        fields,
        instance_state,
        effects: false,
    }
}

fn input(
    id: &str,
    card_types: &[&str],
    size: u32,
    max_copies: Option<u32>,
    zone: &str,
) -> DeckInputDraft {
    DeckInputDraft {
        id: id.to_string(),
        card_types: card_types.iter().map(|c| c.to_string()).collect(),
        size,
        max_copies,
        zone: zone.to_string(),
    }
}

/// The structural subset of the Small Ruleset, written by hand. The loader tests hold a copy of this
/// function and compare it with what the fixture file parses to.
fn small_ruleset_draft() -> RulesetDraft {
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
            zone("deck", Visibility::Hidden, true),
            zone("hand", Visibility::Owner, false),
            zone("field", Visibility::Public, false),
            zone("trash", Visibility::Public, true),
            zone("mana_deck", Visibility::Hidden, true),
            zone("mana_zone", Visibility::Public, false),
            zone("leader_zone", Visibility::Public, false),
        ],
        card_types: vec![
            card_type(
                "leader",
                vec![
                    field("name", FieldTypeDraft::Text),
                    field("power", FieldTypeDraft::Int),
                    field("life", FieldTypeDraft::Int),
                ],
                vec![
                    state(
                        "current_life",
                        FieldTypeDraft::Int,
                        InitializerDraft::FromField("life".to_string()),
                    ),
                    tapped(),
                ],
            ),
            card_type(
                "creature",
                vec![
                    field("name", FieldTypeDraft::Text),
                    field("cost", FieldTypeDraft::Int),
                    field("power", FieldTypeDraft::Int),
                ],
                vec![
                    tapped(),
                    state(
                        "recently_played",
                        FieldTypeDraft::Bool,
                        InitializerDraft::Literal(FieldValue::Bool(false)),
                    ),
                ],
            ),
            card_type("mana", vec![], vec![tapped()]),
        ],
        inputs: vec![
            input("leader", &["leader"], 1, None, "leader_zone"),
            input("main_deck", &["creature"], 50, Some(4), "deck"),
            input("mana_deck", &["mana"], 10, None, "mana_deck"),
        ],
        setup: vec![],
        actions: vec![],
        turn: None,
        win_conditions: vec![],
    }
}

/// A draft with no zones and no inputs, holding only the given card types.
fn draft_with_card_types(card_types: Vec<CardTypeDraft>) -> RulesetDraft {
    RulesetDraft {
        meta: MetaDraft {
            id: "test".to_string(),
            version: 1,
        },
        parameters: ParametersDraft {
            players: 2,
            starting_player: 0,
        },
        zones: vec![],
        card_types,
        inputs: vec![],
        setup: vec![],
        actions: vec![],
        turn: None,
        win_conditions: vec![],
    }
}

fn error_of(draft: RulesetDraft) -> DefinitionError {
    match RulesetDefinition::validate(draft) {
        Err(error) => error,
        Ok(definition) => panic!("expected a validation error, got {definition:?}"),
    }
}

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

fn state_path(card_type: &str, state: &str) -> DefinitionPath {
    path()
        .key("card_types")
        .id(card_type)
        .key("instance_state")
        .id(state)
}

// ---- the model

/// A card type as (id, fields, instance state, allows effects).
type DescribedCardType<'a> = (
    &'a str,
    Vec<(&'a str, FieldType)>,
    Vec<(&'a str, FieldType, Initializer)>,
    bool,
);

/// A deck input as (id, card types, size, max copies, zone).
type DescribedInput<'a> = (&'a str, Vec<&'a str>, u32, Option<u32>, &'a str);

#[test]
fn small_ruleset_draft_validates_into_typed_definition() {
    let definition = RulesetDefinition::validate(small_ruleset_draft()).expect("valid draft");

    assert_eq!(definition.meta().id().as_str(), "small");
    assert_eq!(definition.meta().version().get(), 1);
    assert_eq!(definition.parameters().player_count().get(), 2);
    assert_eq!(definition.parameters().starting_player().get(), 0);

    let zones: Vec<(&str, Visibility, bool)> = definition
        .zones()
        .iter()
        .map(|z| (z.id().as_str(), z.visibility(), z.ordered()))
        .collect();
    assert_eq!(
        zones,
        vec![
            ("deck", Visibility::Hidden, true),
            ("hand", Visibility::Owner, false),
            ("field", Visibility::Public, false),
            ("trash", Visibility::Public, true),
            ("mana_deck", Visibility::Hidden, true),
            ("mana_zone", Visibility::Public, false),
            ("leader_zone", Visibility::Public, false),
        ]
    );

    let card_types: Vec<DescribedCardType> = definition
        .card_types()
        .iter()
        .map(|ct| {
            (
                ct.id().as_str(),
                ct.fields()
                    .iter()
                    .map(|f| (f.name().as_str(), f.field_type()))
                    .collect(),
                ct.instance_state()
                    .iter()
                    .map(|s| (s.name().as_str(), s.state_type(), s.init().clone()))
                    .collect(),
                ct.allows_effects(),
            )
        })
        .collect();
    let false_literal = || Initializer::Literal(FieldValue::Bool(false));
    assert_eq!(
        card_types,
        vec![
            (
                "leader",
                vec![
                    ("name", FieldType::Text),
                    ("power", FieldType::Int),
                    ("life", FieldType::Int),
                ],
                vec![
                    (
                        "current_life",
                        FieldType::Int,
                        Initializer::FromField(FieldName::new("life"))
                    ),
                    ("tapped", FieldType::Bool, false_literal()),
                ],
                false,
            ),
            (
                "creature",
                vec![
                    ("name", FieldType::Text),
                    ("cost", FieldType::Int),
                    ("power", FieldType::Int),
                ],
                vec![
                    ("tapped", FieldType::Bool, false_literal()),
                    ("recently_played", FieldType::Bool, false_literal()),
                ],
                false,
            ),
            (
                "mana",
                vec![],
                vec![("tapped", FieldType::Bool, false_literal())],
                false,
            ),
        ]
    );

    let inputs: Vec<DescribedInput> = definition
        .deck_inputs()
        .iter()
        .map(|i| {
            (
                i.id().as_str(),
                i.card_types().iter().map(CardTypeId::as_str).collect(),
                i.size(),
                i.max_copies(),
                i.zone().as_str(),
            )
        })
        .collect();
    assert_eq!(
        inputs,
        vec![
            ("leader", vec!["leader"], 1, None, "leader_zone"),
            ("main_deck", vec!["creature"], 50, Some(4), "deck"),
            ("mana_deck", vec!["mana"], 10, None, "mana_deck"),
        ]
    );

    // References are typed and resolve through the lookups.
    let main_deck = &definition.deck_inputs()[1];
    assert_eq!(main_deck.card_types()[0], CardTypeId::new("creature"));
    assert_eq!(main_deck.zone(), &ZoneId::new("deck"));
    assert!(definition.card_type(&main_deck.card_types()[0]).is_some());
    assert!(definition.zone(main_deck.zone()).is_some());

    let deck = definition.zone(&ZoneId::new("deck")).expect("deck zone");
    assert_eq!(deck.visibility(), Visibility::Hidden);
    assert!(deck.ordered());
    let creature = definition
        .card_type(&CardTypeId::new("creature"))
        .expect("creature card type");
    assert_eq!(creature.id(), &CardTypeId::new("creature"));
    assert!(definition.zone(&ZoneId::new("graveyard")).is_none());
    assert!(definition.card_type(&CardTypeId::new("spell")).is_none());
}

#[test]
fn int_bool_and_text_field_types_are_supported() {
    let draft = draft_with_card_types(vec![card_type(
        "c",
        vec![
            field("a", FieldTypeDraft::Int),
            field("b", FieldTypeDraft::Bool),
            field("c", FieldTypeDraft::Text),
        ],
        vec![
            state(
                "label",
                FieldTypeDraft::Text,
                InitializerDraft::Literal(FieldValue::Text("none".to_string())),
            ),
            state(
                "count",
                FieldTypeDraft::Int,
                InitializerDraft::Literal(FieldValue::Int(-3)),
            ),
        ],
    )]);
    let definition = RulesetDefinition::validate(draft).expect("valid draft");
    let c = &definition.card_types()[0];

    let fields: Vec<(&str, FieldType)> = c
        .fields()
        .iter()
        .map(|f| (f.name().as_str(), f.field_type()))
        .collect();
    assert_eq!(
        fields,
        vec![
            ("a", FieldType::Int),
            ("b", FieldType::Bool),
            ("c", FieldType::Text)
        ]
    );
    let states: Vec<(&str, FieldType, Initializer)> = c
        .instance_state()
        .iter()
        .map(|s| (s.name().as_str(), s.state_type(), s.init().clone()))
        .collect();
    assert_eq!(
        states,
        vec![
            (
                "label",
                FieldType::Text,
                Initializer::Literal(FieldValue::Text("none".to_string()))
            ),
            (
                "count",
                FieldType::Int,
                Initializer::Literal(FieldValue::Int(-3))
            ),
        ]
    );
}

#[test]
fn enum_and_enum_set_field_types_are_unsupported() {
    for unsupported in [FieldTypeDraft::Enum, FieldTypeDraft::EnumSet] {
        let on_field =
            draft_with_card_types(vec![card_type("c", vec![field("f", unsupported)], vec![])]);
        assert_eq!(
            error_of(on_field),
            DefinitionError {
                path: path()
                    .key("card_types")
                    .id("c")
                    .key("fields")
                    .id("f")
                    .key("type"),
                kind: DefinitionErrorKind::UnsupportedFieldType {
                    field_type: unsupported
                },
            }
        );

        // The type is checked before the initializer, so a well-formed literal does not matter.
        let on_state = draft_with_card_types(vec![card_type(
            "c",
            vec![],
            vec![state(
                "s",
                unsupported,
                InitializerDraft::Literal(FieldValue::Text("x".to_string())),
            )],
        )]);
        assert_eq!(
            error_of(on_state),
            DefinitionError {
                path: state_path("c", "s").key("type"),
                kind: DefinitionErrorKind::UnsupportedFieldType {
                    field_type: unsupported
                },
            }
        );
    }
}

// ---- parameters

#[test]
fn zero_players_is_an_invalid_player_count() {
    let mut draft = small_ruleset_draft();
    draft.parameters.players = 0;
    assert_eq!(
        error_of(draft.clone()),
        DefinitionError {
            path: path().key("parameters").key("players"),
            kind: DefinitionErrorKind::InvalidPlayerCount { players: 0 },
        }
    );

    // The number of Players is checked before the starting player.
    draft.parameters.starting_player = 5;
    assert_eq!(
        error_of(draft).kind,
        DefinitionErrorKind::InvalidPlayerCount { players: 0 }
    );
}

#[test]
fn starting_player_must_be_a_position_in_the_player_order() {
    let mut draft = small_ruleset_draft();
    draft.parameters.starting_player = 2;
    assert_eq!(
        error_of(draft.clone()),
        DefinitionError {
            path: path().key("parameters").key("starting_player"),
            kind: DefinitionErrorKind::InvalidStartingPlayer {
                starting_player: 2,
                players: 2
            },
        }
    );

    draft.parameters.starting_player = 1;
    let definition = RulesetDefinition::validate(draft).expect("position 1 of 2 is valid");
    assert_eq!(definition.parameters().starting_player().get(), 1);
}

// ---- ids

#[test]
fn duplicate_ids_are_rejected_within_each_list() {
    let duplicate = |id: &str, at: DefinitionPath, first: DefinitionPath| DefinitionError {
        path: at,
        kind: DefinitionErrorKind::DuplicateId {
            id: id.to_string(),
            first,
        },
    };

    // zone
    let mut draft = small_ruleset_draft();
    draft.zones.push(zone("deck", Visibility::Public, false));
    assert_eq!(
        error_of(draft),
        duplicate(
            "deck",
            path().key("zones").index(7),
            path().key("zones").index(0)
        )
    );

    // card type
    let mut draft = small_ruleset_draft();
    draft.card_types.push(card_type("mana", vec![], vec![]));
    assert_eq!(
        error_of(draft),
        duplicate(
            "mana",
            path().key("card_types").index(3),
            path().key("card_types").index(2)
        )
    );

    // field of one card type
    let draft = draft_with_card_types(vec![card_type(
        "c",
        vec![
            field("x", FieldTypeDraft::Int),
            field("y", FieldTypeDraft::Int),
            field("x", FieldTypeDraft::Text),
        ],
        vec![],
    )]);
    assert_eq!(
        error_of(draft),
        duplicate(
            "x",
            path().key("card_types").id("c").key("fields").index(2),
            path().key("card_types").id("c").key("fields").index(0)
        )
    );

    // instance state of one card type
    let draft = draft_with_card_types(vec![card_type(
        "c",
        vec![],
        vec![
            tapped(),
            state(
                "s",
                FieldTypeDraft::Int,
                InitializerDraft::Literal(FieldValue::Int(0)),
            ),
            tapped(),
        ],
    )]);
    assert_eq!(
        error_of(draft),
        duplicate(
            "tapped",
            path()
                .key("card_types")
                .id("c")
                .key("instance_state")
                .index(2),
            path()
                .key("card_types")
                .id("c")
                .key("instance_state")
                .index(0)
        )
    );

    // input
    let mut draft = small_ruleset_draft();
    draft
        .inputs
        .push(input("main_deck", &["creature"], 1, None, "deck"));
    assert_eq!(
        error_of(draft),
        duplicate(
            "main_deck",
            path().key("inputs").index(3),
            path().key("inputs").index(1)
        )
    );
}

#[test]
fn ids_may_repeat_across_lists() {
    let draft = RulesetDraft {
        meta: MetaDraft {
            id: "namespaces".to_string(),
            version: 1,
        },
        parameters: ParametersDraft {
            players: 2,
            starting_player: 0,
        },
        zones: vec![zone("mana_deck", Visibility::Hidden, true)],
        card_types: vec![
            card_type(
                "leader",
                vec![field("power", FieldTypeDraft::Int)],
                vec![tapped()],
            ),
            card_type(
                "mana",
                vec![field("power", FieldTypeDraft::Int)],
                vec![tapped()],
            ),
        ],
        inputs: vec![
            input("mana_deck", &["mana"], 10, None, "mana_deck"),
            input("leader", &["leader"], 1, None, "mana_deck"),
        ],
        setup: vec![],
        actions: vec![],
        turn: None,
        win_conditions: vec![],
    };
    let definition = RulesetDefinition::validate(draft).expect("ids are scoped to their list");
    let zone_ids: Vec<&str> = definition.zones().iter().map(|z| z.id().as_str()).collect();
    assert_eq!(zone_ids, vec!["mana_deck"]);
    let card_type_ids: Vec<&str> = definition
        .card_types()
        .iter()
        .map(|c| c.id().as_str())
        .collect();
    assert_eq!(card_type_ids, vec!["leader", "mana"]);
    let input_ids: Vec<&str> = definition
        .deck_inputs()
        .iter()
        .map(|i| i.id().as_str())
        .collect();
    assert_eq!(input_ids, vec!["mana_deck", "leader"]);
}

#[test]
fn dangling_references_are_rejected() {
    let mut draft = small_ruleset_draft();
    draft.inputs[1].card_types = vec!["ghost".to_string()];
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: path()
                .key("inputs")
                .id("main_deck")
                .key("card_types")
                .index(0),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::CardType,
                id: "ghost".to_string()
            },
        }
    );

    // The position of the bad entry is reported.
    let mut draft = small_ruleset_draft();
    draft.inputs[1].card_types = vec!["creature".to_string(), "ghost".to_string()];
    assert_eq!(
        error_of(draft).path,
        path()
            .key("inputs")
            .id("main_deck")
            .key("card_types")
            .index(1)
    );

    let mut draft = small_ruleset_draft();
    draft.inputs[1].zone = "nowhere".to_string();
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: path().key("inputs").id("main_deck").key("zone"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "nowhere".to_string()
            },
        }
    );

    // Within an input, the card types are checked before the zone.
    let mut draft = small_ruleset_draft();
    draft.inputs[1].card_types = vec!["ghost".to_string()];
    draft.inputs[1].zone = "nowhere".to_string();
    assert_eq!(
        error_of(draft).kind,
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::CardType,
            id: "ghost".to_string()
        }
    );
}

// ---- initializers

#[test]
fn from_field_must_name_a_field_of_the_same_type() {
    let with_state = |declared: FieldTypeDraft, from_field: &str| {
        draft_with_card_types(vec![card_type(
            "c",
            vec![
                field("life", FieldTypeDraft::Int),
                field("name", FieldTypeDraft::Text),
            ],
            vec![state(
                "s",
                declared,
                InitializerDraft::FromField(from_field.to_string()),
            )],
        )])
    };

    assert_eq!(
        error_of(with_state(FieldTypeDraft::Int, "hp")),
        DefinitionError {
            path: state_path("c", "s").key("init").key("from_field"),
            kind: DefinitionErrorKind::FromFieldMissing {
                field: "hp".to_string()
            },
        }
    );
    assert_eq!(
        error_of(with_state(FieldTypeDraft::Int, "name")),
        DefinitionError {
            path: state_path("c", "s").key("init").key("from_field"),
            kind: DefinitionErrorKind::FromFieldTypeMismatch {
                field: "name".to_string(),
                declared: FieldType::Int,
                found: FieldType::Text
            },
        }
    );
    let definition = RulesetDefinition::validate(with_state(FieldTypeDraft::Int, "life"))
        .expect("life is an int field");
    assert_eq!(
        definition.card_types()[0].instance_state()[0].init(),
        &Initializer::FromField(FieldName::new("life"))
    );
}

#[test]
fn literal_initializer_must_match_the_declared_type() {
    let draft = draft_with_card_types(vec![card_type(
        "c",
        vec![],
        vec![state(
            "s",
            FieldTypeDraft::Bool,
            InitializerDraft::Literal(FieldValue::Int(5)),
        )],
    )]);
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: state_path("c", "s").key("init"),
            kind: DefinitionErrorKind::InitializerTypeMismatch {
                declared: FieldType::Bool,
                found: FieldType::Int
            },
        }
    );
}

// ---- paths and the crate boundary

#[test]
fn definition_path_displays_ids_in_brackets() {
    let field_path = path()
        .key("card_types")
        .id("creature")
        .key("fields")
        .id("cost");
    assert_eq!(field_path.to_string(), "card_types[creature].fields[cost]");
    assert_eq!(path().key("zones").index(3).to_string(), "zones[3]");
    assert_eq!(path().to_string(), "");
    assert_eq!(
        path().key("zones").index(3).segments(),
        &[PathSegment::Key("zones".to_string()), PathSegment::Index(3)]
    );
}

#[test]
fn engine_crate_has_no_yaml_or_serde_dependency() {
    let manifest = include_str!("../Cargo.toml");
    assert!(!manifest.contains("serde"));
    assert!(!manifest.contains("yaml"));
}

// ---- identifier grammar (ticket #20)

#[test]
fn ruleset_definition_rejects_invalid_id() {
    let invalid = |id: &str, at: DefinitionPath| DefinitionError {
        path: at,
        kind: DefinitionErrorKind::InvalidId { id: id.to_string() },
    };

    for bad in ["", "3", "Deck", "a]b", "a-b", "_x", "true", "false", "null"] {
        // ruleset id
        let mut draft = small_ruleset_draft();
        draft.meta.id = bad.to_string();
        assert_eq!(
            error_of(draft),
            invalid(bad, path().key("meta").key("id")),
            "ruleset id {bad:?}"
        );

        // zone
        let mut draft = small_ruleset_draft();
        draft.zones.push(zone(bad, Visibility::Public, false));
        assert_eq!(
            error_of(draft),
            invalid(bad, path().key("zones").index(7).key("id")),
            "zone {bad:?}"
        );

        // card type
        let mut draft = small_ruleset_draft();
        draft.card_types.push(card_type(bad, vec![], vec![]));
        assert_eq!(
            error_of(draft),
            invalid(bad, path().key("card_types").index(3).key("id")),
            "card type {bad:?}"
        );

        // field
        let mut draft = small_ruleset_draft();
        draft.card_types[1]
            .fields
            .push(field(bad, FieldTypeDraft::Int));
        assert_eq!(
            error_of(draft),
            invalid(
                bad,
                path()
                    .key("card_types")
                    .id("creature")
                    .key("fields")
                    .index(3)
                    .key("name")
            ),
            "field {bad:?}"
        );

        // instance state
        let mut draft = small_ruleset_draft();
        draft.card_types[1].instance_state.push(state(
            bad,
            FieldTypeDraft::Int,
            InitializerDraft::Literal(FieldValue::Int(0)),
        ));
        assert_eq!(
            error_of(draft),
            invalid(
                bad,
                path()
                    .key("card_types")
                    .id("creature")
                    .key("instance_state")
                    .index(2)
                    .key("name")
            ),
            "instance state {bad:?}"
        );

        // deck input
        let mut draft = small_ruleset_draft();
        draft
            .inputs
            .push(input(bad, &["creature"], 1, None, "deck"));
        assert_eq!(
            error_of(draft),
            invalid(bad, path().key("inputs").index(3).key("id")),
            "deck input {bad:?}"
        );
    }

    // The ruleset id comes first, before the parameters.
    let mut draft = small_ruleset_draft();
    draft.meta.id = "Small".to_string();
    draft.parameters.players = 0;
    assert_eq!(
        error_of(draft).kind,
        DefinitionErrorKind::InvalidId {
            id: "Small".to_string()
        }
    );

    // Declarations are checked in file order: a bad zone id wins over a bad card type id.
    let mut draft = small_ruleset_draft();
    draft.zones.push(zone("Z", Visibility::Public, false));
    draft.card_types.push(card_type("C", vec![], vec![]));
    assert_eq!(error_of(draft).path, path().key("zones").index(7).key("id"));
}

#[test]
fn identifier_grammar_accepts_lowercase_letters_digits_and_underscores() {
    let mut draft = small_ruleset_draft();
    for id in ["a", "a1", "a_b", "x9_", "mana_deck_2"] {
        draft.zones.push(zone(id, Visibility::Public, false));
    }
    let definition = RulesetDefinition::validate(draft).expect("every id matches the grammar");
    assert_eq!(definition.zones().len(), 12);
}

// ---- deck input constraints (ticket #20)

fn input_path(id: &str) -> DefinitionPath {
    path().key("inputs").id(id)
}

#[test]
fn deck_input_rejects_size_zero() {
    let mut draft = small_ruleset_draft();
    draft.inputs[1].size = 0;
    assert_eq!(
        error_of(draft.clone()),
        DefinitionError {
            path: input_path("main_deck").key("size"),
            kind: DefinitionErrorKind::InvalidDeckSize,
        }
    );

    // The size is checked before the copy limit and the zone.
    draft.inputs[1].zone = "nowhere".to_string();
    assert_eq!(error_of(draft).kind, DefinitionErrorKind::InvalidDeckSize);

    let mut draft = small_ruleset_draft();
    draft.inputs[1].size = 1;
    draft.inputs[1].max_copies = Some(1);
    assert!(
        RulesetDefinition::validate(draft).is_ok(),
        "size 1 is valid"
    );
}

#[test]
fn deck_input_rejects_max_copies_zero() {
    let mut draft = small_ruleset_draft();
    draft.inputs[1].max_copies = Some(0);
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: input_path("main_deck").key("max_copies"),
            kind: DefinitionErrorKind::InvalidMaxCopies {
                max_copies: 0,
                size: 50
            },
        }
    );
}

#[test]
fn deck_input_rejects_max_copies_above_size() {
    let mut draft = small_ruleset_draft();
    draft.inputs[1].size = 3;
    draft.inputs[1].max_copies = Some(4);
    assert_eq!(
        error_of(draft.clone()),
        DefinitionError {
            path: input_path("main_deck").key("max_copies"),
            kind: DefinitionErrorKind::InvalidMaxCopies {
                max_copies: 4,
                size: 3
            },
        }
    );

    // max_copies equal to the size is accepted.
    draft.inputs[1].max_copies = Some(3);
    let definition = RulesetDefinition::validate(draft).expect("max_copies == size is valid");
    assert_eq!(definition.deck_inputs()[1].max_copies(), Some(3));
}

#[test]
fn deck_input_rejects_empty_card_types() {
    let mut draft = small_ruleset_draft();
    draft.inputs[1].card_types = vec![];
    assert_eq!(
        error_of(draft.clone()),
        DefinitionError {
            path: input_path("main_deck").key("card_types"),
            kind: DefinitionErrorKind::NoCardTypes,
        }
    );

    // Card types are checked first, before the size and the zone.
    draft.inputs[1].size = 0;
    draft.inputs[1].zone = "nowhere".to_string();
    assert_eq!(error_of(draft).kind, DefinitionErrorKind::NoCardTypes);
}

#[test]
fn deck_input_rejects_repeated_card_type() {
    let mut draft = small_ruleset_draft();
    draft.inputs[1].card_types = vec!["creature".to_string(), "creature".to_string()];
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: input_path("main_deck").key("card_types").index(1),
            kind: DefinitionErrorKind::DuplicateId {
                id: "creature".to_string(),
                first: input_path("main_deck").key("card_types").index(0),
            },
        }
    );
}

// ---- setup (ticket #21)

fn shuffle(player: &str, zone: &str) -> SetupStepDraft {
    SetupStepDraft::Shuffle {
        player: player.to_string(),
        zone: zone.to_string(),
    }
}

fn draw(player: &str, from: &str, to: &str, count: u32) -> SetupStepDraft {
    SetupStepDraft::Draw {
        player: player.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        count,
    }
}

/// The Small Ruleset structure with the given setup steps.
fn small_ruleset_with_setup(setup: Vec<SetupStepDraft>) -> RulesetDraft {
    let mut draft = small_ruleset_draft();
    draft.setup = setup;
    draft
}

fn setup_path(step: usize, primitive: &str, key: &str) -> DefinitionPath {
    path().key("setup").index(step).key(primitive).key(key)
}

#[test]
fn valid_setup_is_kept_in_order() {
    let definition = RulesetDefinition::validate(small_ruleset_with_setup(vec![
        shuffle("each", "deck"),
        draw("each", "deck", "hand", 5),
    ]))
    .expect("a valid setup");
    assert_eq!(
        definition.setup(),
        [
            SetupStep::Shuffle {
                player: PlayerSelector::Each,
                zone: ZoneId::new("deck"),
            },
            SetupStep::Draw {
                player: PlayerSelector::Each,
                from: ZoneId::new("deck"),
                to: ZoneId::new("hand"),
                count: NonZeroU32::new(5).expect("5 is not zero"),
            },
        ]
    );

    // No steps is a valid setup.
    let definition = RulesetDefinition::validate(small_ruleset_with_setup(vec![]))
        .expect("an empty setup is valid");
    assert_eq!(definition.setup(), []);
}

#[test]
fn setup_accepts_the_active_player_selector() {
    let definition = RulesetDefinition::validate(small_ruleset_with_setup(vec![
        shuffle("active", "deck"),
        draw("active", "deck", "hand", 5),
    ]))
    .expect("`active` is a Player Selector");
    assert_eq!(
        definition.setup(),
        [
            SetupStep::Shuffle {
                player: PlayerSelector::Active,
                zone: ZoneId::new("deck"),
            },
            SetupStep::Draw {
                player: PlayerSelector::Active,
                from: ZoneId::new("deck"),
                to: ZoneId::new("hand"),
                count: NonZeroU32::new(5).expect("5 is not zero"),
            },
        ]
    );
}

#[test]
fn setup_shuffle_and_draw_need_an_ordered_zone() {
    // ADR-0013: `hand` is declared `ordered: false`.
    let not_ordered = DefinitionErrorKind::ZoneNotOrdered {
        zone: "hand".to_string(),
    };
    assert_eq!(
        error_of(small_ruleset_with_setup(vec![shuffle("each", "hand")])),
        DefinitionError {
            path: setup_path(0, "shuffle", "zone"),
            kind: not_ordered.clone(),
        }
    );
    assert_eq!(
        error_of(small_ruleset_with_setup(vec![
            shuffle("each", "deck"),
            draw("each", "hand", "deck", 1),
        ])),
        DefinitionError {
            path: setup_path(1, "draw", "from"),
            kind: not_ordered,
        }
    );

    // The destination of a draw may be unordered, like the hand.
    RulesetDefinition::validate(small_ruleset_with_setup(vec![draw(
        "each", "deck", "hand", 1,
    )]))
    .expect("only the source of a draw needs to be ordered");

    // A dangling reference is reported before the zone is looked at for its order.
    assert_eq!(
        error_of(small_ruleset_with_setup(vec![shuffle("each", "nowhere")])).kind,
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Zone,
            id: "nowhere".to_string()
        }
    );
}

#[test]
fn setup_rejects_unknown_player_selector() {
    // Only `each` and `active` exist (`active` arrived with the Turn cycle, ADR-0012).
    let error = error_of(small_ruleset_with_setup(vec![
        shuffle("each", "deck"),
        shuffle("opponent", "deck"),
    ]));
    assert_eq!(
        error,
        DefinitionError {
            path: setup_path(1, "shuffle", "player"),
            kind: DefinitionErrorKind::UnknownPlayerSelector {
                selector: "opponent".to_string()
            },
        }
    );

    let error = error_of(small_ruleset_with_setup(vec![draw(
        "who", "deck", "hand", 1,
    )]));
    assert_eq!(
        error,
        DefinitionError {
            path: setup_path(0, "draw", "player"),
            kind: DefinitionErrorKind::UnknownPlayerSelector {
                selector: "who".to_string()
            },
        }
    );
}

#[test]
fn setup_rejects_dangling_zone() {
    let dangling = |id: &str| DefinitionErrorKind::DanglingReference {
        target: ReferenceTarget::Zone,
        id: id.to_string(),
    };

    // shuffle.zone
    let error = error_of(small_ruleset_with_setup(vec![shuffle("each", "nowhere")]));
    assert_eq!(
        error,
        DefinitionError {
            path: setup_path(0, "shuffle", "zone"),
            kind: dangling("nowhere"),
        }
    );

    // draw.from
    let error = error_of(small_ruleset_with_setup(vec![
        shuffle("each", "deck"),
        draw("each", "nowhere", "hand", 5),
    ]));
    assert_eq!(
        error,
        DefinitionError {
            path: setup_path(1, "draw", "from"),
            kind: dangling("nowhere"),
        }
    );

    // draw.to
    let error = error_of(small_ruleset_with_setup(vec![draw(
        "each",
        "deck",
        "elsewhere",
        5,
    )]));
    assert_eq!(
        error,
        DefinitionError {
            path: setup_path(0, "draw", "to"),
            kind: dangling("elsewhere"),
        }
    );

    // A zone id that breaks the identifier grammar is a dangling reference too (ADR-0009).
    let error = error_of(small_ruleset_with_setup(vec![shuffle("each", "Deck")]));
    assert_eq!(
        error,
        DefinitionError {
            path: setup_path(0, "shuffle", "zone"),
            kind: dangling("Deck"),
        }
    );
}

#[test]
fn setup_rejects_zero_draw_count() {
    let error = error_of(small_ruleset_with_setup(vec![draw(
        "each", "deck", "hand", 0,
    )]));
    assert_eq!(
        error,
        DefinitionError {
            path: setup_path(0, "draw", "count"),
            kind: DefinitionErrorKind::InvalidCount,
        }
    );
}

#[test]
fn setup_is_checked_after_inputs() {
    // A bad input zone and a bad setup step: the input is reported, whatever the step.
    let mut draft = small_ruleset_with_setup(vec![shuffle("nobody", "nowhere")]);
    draft.inputs[0].zone = "missing".to_string();
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: path().key("inputs").id("leader").key("zone"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "missing".to_string()
            },
        }
    );
}

#[test]
fn setup_steps_are_checked_in_order_and_each_step_in_key_order() {
    let dangling = |id: &str| DefinitionErrorKind::DanglingReference {
        target: ReferenceTarget::Zone,
        id: id.to_string(),
    };
    let unknown_selector = |selector: &str| DefinitionErrorKind::UnknownPlayerSelector {
        selector: selector.to_string(),
    };
    let expect = |steps: Vec<SetupStepDraft>, path: DefinitionPath, kind: DefinitionErrorKind| {
        assert_eq!(
            error_of(small_ruleset_with_setup(steps)),
            DefinitionError { path, kind }
        );
    };

    // The first bad step wins, even when a later one is also bad.
    expect(
        vec![shuffle("each", "nowhere"), draw("who", "deck", "hand", 0)],
        setup_path(0, "shuffle", "zone"),
        dangling("nowhere"),
    );

    // Within a draw: player, then from, then to, then count.
    expect(
        vec![draw("who", "a", "b", 0)],
        setup_path(0, "draw", "player"),
        unknown_selector("who"),
    );
    expect(
        vec![draw("each", "a", "b", 0)],
        setup_path(0, "draw", "from"),
        dangling("a"),
    );
    expect(
        vec![draw("each", "deck", "b", 0)],
        setup_path(0, "draw", "to"),
        dangling("b"),
    );
    expect(
        vec![draw("each", "deck", "hand", 0)],
        setup_path(0, "draw", "count"),
        DefinitionErrorKind::InvalidCount,
    );

    // Within a shuffle: player, then zone.
    expect(
        vec![shuffle("who", "nowhere")],
        setup_path(0, "shuffle", "player"),
        unknown_selector("who"),
    );
}

// ---- actions and the turn (ticket #22)

fn pass_turn_action() -> ActionDraft {
    ActionDraft {
        id: "pass_turn".to_string(),
        params: vec![],
        cost: vec![],
        effects: vec![EffectDraft::EndTurn],
    }
}

fn automatic_phase(id: &str, steps: Vec<AutomaticStepDraft>) -> PhaseDraft {
    PhaseDraft {
        id: id.to_string(),
        automatic: Some(steps),
        actions: None,
    }
}

fn action_phase(id: &str, actions: &[&str]) -> PhaseDraft {
    PhaseDraft {
        id: id.to_string(),
        automatic: None,
        actions: Some(actions.iter().map(|action| action.to_string()).collect()),
    }
}

fn step_untap(player: &str, zones: &[&str]) -> AutomaticStepDraft {
    AutomaticStepDraft::Untap {
        player: player.to_string(),
        zones: zones.iter().map(|zone| zone.to_string()).collect(),
    }
}

fn step_set_state(player: &str, zone: &str, state: &str, value: FieldValue) -> AutomaticStepDraft {
    AutomaticStepDraft::SetState {
        player: player.to_string(),
        zone: zone.to_string(),
        state: state.to_string(),
        value,
    }
}

fn step_draw(player: &str, from: &str, to: &str, count: u32) -> AutomaticStepDraft {
    AutomaticStepDraft::Draw {
        player: player.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        count,
    }
}

fn step_move(player: &str, from: &str, to: &str, count: u32, position: &str) -> AutomaticStepDraft {
    AutomaticStepDraft::Move {
        player: player.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        count,
        position: position.to_string(),
    }
}

fn step_if(
    then: Vec<AutomaticStepDraft>,
    otherwise: Vec<AutomaticStepDraft>,
) -> AutomaticStepDraft {
    AutomaticStepDraft::If {
        condition: ConditionDraft::FirstTurn,
        then,
        otherwise,
    }
}

/// The Small Ruleset structure with the action `pass_turn` and the given phases.
fn small_ruleset_with_phases(phases: Vec<PhaseDraft>) -> RulesetDraft {
    let mut draft = small_ruleset_draft();
    draft.actions = vec![pass_turn_action()];
    draft.turn = Some(TurnDraft { phases });
    draft
}

/// The Small Ruleset with a Start phase of the given steps, then Main waiting for `pass_turn`.
fn small_ruleset_with_start(steps: Vec<AutomaticStepDraft>) -> RulesetDraft {
    small_ruleset_with_phases(vec![
        automatic_phase("start", steps),
        action_phase("main", &["pass_turn"]),
    ])
}

fn phase_path(id: &str) -> DefinitionPath {
    path().key("turn").key("phases").id(id)
}

/// The path of step `index` of the automatic phase `phase`.
fn step_path(phase: &str, index: usize) -> DefinitionPath {
    phase_path(phase).key("automatic").index(index)
}

fn zone_not_ordered(zone: &str) -> DefinitionErrorKind {
    DefinitionErrorKind::ZoneNotOrdered {
        zone: zone.to_string(),
    }
}

fn dangling(target: ReferenceTarget, id: &str) -> DefinitionErrorKind {
    DefinitionErrorKind::DanglingReference {
        target,
        id: id.to_string(),
    }
}

/// The Start phase of the Small Ruleset.
fn small_start_steps() -> Vec<AutomaticStepDraft> {
    vec![
        step_untap("active", &["mana_zone", "field"]),
        step_set_state(
            "active",
            "field",
            "recently_played",
            FieldValue::Bool(false),
        ),
        step_if(vec![], vec![step_draw("active", "deck", "hand", 1)]),
        step_move("active", "mana_deck", "mana_zone", 2, "top"),
    ]
}

#[test]
fn valid_actions_and_turn_are_kept_in_order() {
    let definition = RulesetDefinition::validate(small_ruleset_with_phases(vec![
        automatic_phase("start", small_start_steps()),
        action_phase("main", &["pass_turn"]),
        automatic_phase("end", vec![]),
    ]))
    .expect("a valid turn");

    let actions = definition.actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id(), &ActionId::new("pass_turn"));
    assert_eq!(actions[0].effects(), [Effect::EndTurn]);
    assert_eq!(
        definition.action(&ActionId::new("pass_turn")),
        Some(&actions[0])
    );
    assert_eq!(definition.action(&ActionId::new("nothing")), None);

    let start = PhaseKind::Automatic(vec![
        AutomaticStep::Untap {
            player: PlayerSelector::Active,
            zones: vec![ZoneId::new("mana_zone"), ZoneId::new("field")],
        },
        AutomaticStep::SetState {
            player: PlayerSelector::Active,
            zone: ZoneId::new("field"),
            state: StateName::new("recently_played"),
            value: FieldValue::Bool(false),
        },
        AutomaticStep::If {
            condition: Condition::FirstTurn,
            then: vec![],
            otherwise: vec![AutomaticStep::Draw {
                player: PlayerSelector::Active,
                from: ZoneId::new("deck"),
                to: ZoneId::new("hand"),
                count: NonZeroU32::new(1).expect("1 is not zero"),
            }],
        },
        AutomaticStep::Move {
            player: PlayerSelector::Active,
            from: ZoneId::new("mana_deck"),
            to: ZoneId::new("mana_zone"),
            count: NonZeroU32::new(2).expect("2 is not zero"),
            position: Position::Top,
        },
    ]);
    let main = PhaseKind::Actions(vec![ActionId::new("pass_turn")]);
    let end = PhaseKind::Automatic(vec![]);
    let turn = definition.turn().expect("the ruleset has a turn");
    let phases: Vec<(&str, &PhaseKind)> = turn
        .phases()
        .iter()
        .map(|phase| (phase.id().as_str(), phase.kind()))
        .collect();
    assert_eq!(
        phases,
        vec![("start", &start), ("main", &main), ("end", &end)]
    );
}

#[test]
fn a_ruleset_without_actions_or_turn_has_none() {
    let definition =
        RulesetDefinition::validate(small_ruleset_draft()).expect("a ruleset without a turn");
    assert_eq!(definition.actions(), []);
    assert_eq!(definition.turn(), None);
}

#[test]
fn turn_steps_accept_the_each_and_active_selectors() {
    let definition = RulesetDefinition::validate(small_ruleset_with_start(vec![
        step_untap("each", &["field"]),
        step_untap("active", &["field"]),
    ]))
    .expect("both selectors are valid in a step");
    let turn = definition.turn().expect("a turn");
    let selectors: Vec<PlayerSelector> = turn
        .phases()
        .iter()
        .filter_map(|phase| match phase.kind() {
            PhaseKind::Automatic(steps) => Some(steps),
            PhaseKind::Actions(_) => None,
        })
        .flatten()
        .filter_map(|step| match step {
            AutomaticStep::Untap { player, .. } => Some(*player),
            _ => None,
        })
        .collect();
    assert_eq!(selectors, [PlayerSelector::Each, PlayerSelector::Active]);
}

#[test]
fn actions_reject_invalid_and_duplicate_ids() {
    let mut draft = small_ruleset_with_start(vec![]);
    draft.actions.push(ActionDraft {
        id: "Bad".to_string(),
        params: vec![],
        cost: vec![],
        effects: vec![],
    });
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: path().key("actions").index(1).key("id"),
            kind: DefinitionErrorKind::InvalidId {
                id: "Bad".to_string()
            },
        }
    );

    let mut draft = small_ruleset_with_start(vec![]);
    draft.actions.push(ActionDraft {
        id: "idle".to_string(),
        params: vec![],
        cost: vec![],
        effects: vec![],
    });
    draft.actions.push(pass_turn_action());
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: path().key("actions").index(2),
            kind: DefinitionErrorKind::DuplicateId {
                id: "pass_turn".to_string(),
                first: path().key("actions").index(0),
            },
        }
    );
}

#[test]
fn phases_reject_invalid_and_duplicate_ids() {
    let error = error_of(small_ruleset_with_phases(vec![
        automatic_phase("start", vec![]),
        action_phase("Main", &["pass_turn"]),
    ]));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("turn").key("phases").index(1).key("id"),
            kind: DefinitionErrorKind::InvalidId {
                id: "Main".to_string()
            },
        }
    );

    let error = error_of(small_ruleset_with_phases(vec![
        automatic_phase("start", vec![]),
        action_phase("main", &["pass_turn"]),
        automatic_phase("start", vec![]),
    ]));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("turn").key("phases").index(2),
            kind: DefinitionErrorKind::DuplicateId {
                id: "start".to_string(),
                first: path().key("turn").key("phases").index(0),
            },
        }
    );
}

#[test]
fn a_phase_is_automatic_or_waits_for_actions_and_never_both() {
    let neither = PhaseDraft {
        id: "main".to_string(),
        automatic: None,
        actions: None,
    };
    assert_eq!(
        error_of(small_ruleset_with_phases(vec![
            automatic_phase("start", vec![]),
            neither
        ])),
        DefinitionError {
            path: phase_path("main"),
            kind: DefinitionErrorKind::InvalidPhaseKind,
        }
    );

    let both = PhaseDraft {
        id: "main".to_string(),
        automatic: Some(vec![]),
        actions: Some(vec!["pass_turn".to_string()]),
    };
    assert_eq!(
        error_of(small_ruleset_with_phases(vec![both])),
        DefinitionError {
            path: phase_path("main"),
            kind: DefinitionErrorKind::InvalidPhaseKind,
        }
    );
}

#[test]
fn a_turn_has_exactly_one_phase_with_actions() {
    let turn_path = path().key("turn").key("phases");
    for (phases, found) in [
        (vec![], 0),
        (vec![automatic_phase("start", vec![])], 0),
        (
            vec![
                action_phase("main", &["pass_turn"]),
                action_phase("second", &["pass_turn"]),
            ],
            2,
        ),
    ] {
        assert_eq!(
            error_of(small_ruleset_with_phases(phases)),
            DefinitionError {
                path: turn_path.clone(),
                kind: DefinitionErrorKind::InvalidActionPhaseCount { found },
            }
        );
    }

    // The count is checked after the content of every phase.
    assert_eq!(
        error_of(small_ruleset_with_phases(vec![
            action_phase("main", &["pass_turn"]),
            action_phase("second", &["nowhere"]),
        ])),
        DefinitionError {
            path: phase_path("second").key("actions").index(0),
            kind: dangling(ReferenceTarget::Action, "nowhere"),
        }
    );
}

#[test]
fn a_phase_lists_declared_actions_once() {
    assert_eq!(
        error_of(small_ruleset_with_phases(vec![action_phase(
            "main",
            &["pass_turn", "attack"]
        )])),
        DefinitionError {
            path: phase_path("main").key("actions").index(1),
            kind: dangling(ReferenceTarget::Action, "attack"),
        }
    );
    assert_eq!(
        error_of(small_ruleset_with_phases(vec![action_phase(
            "main",
            &["pass_turn", "pass_turn"]
        )])),
        DefinitionError {
            path: phase_path("main").key("actions").index(1),
            kind: DefinitionErrorKind::DuplicateId {
                id: "pass_turn".to_string(),
                first: phase_path("main").key("actions").index(0),
            },
        }
    );
}

#[test]
fn turn_steps_reject_an_unknown_player_selector() {
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_untap(
            "opponent",
            &["field"]
        )])),
        DefinitionError {
            path: step_path("start", 0).key("untap").key("player"),
            kind: DefinitionErrorKind::UnknownPlayerSelector {
                selector: "opponent".to_string()
            },
        }
    );
}

#[test]
fn turn_steps_reject_dangling_zones() {
    let nowhere = dangling(ReferenceTarget::Zone, "nowhere");
    // untap.zones[i]
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_untap(
            "active",
            &["field", "nowhere"]
        )])),
        DefinitionError {
            path: step_path("start", 0).key("untap").key("zones").index(1),
            kind: nowhere.clone(),
        }
    );
    // set_state.zone
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_set_state(
            "active",
            "nowhere",
            "recently_played",
            FieldValue::Bool(false)
        )])),
        DefinitionError {
            path: step_path("start", 0).key("set_state").key("zone"),
            kind: nowhere.clone(),
        }
    );
    // draw.from, draw.to
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_draw(
            "active", "nowhere", "hand", 1
        )])),
        DefinitionError {
            path: step_path("start", 0).key("draw").key("from"),
            kind: nowhere.clone(),
        }
    );
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_draw(
            "active", "deck", "nowhere", 1
        )])),
        DefinitionError {
            path: step_path("start", 0).key("draw").key("to"),
            kind: nowhere.clone(),
        }
    );
    // move.from, move.to
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_move(
            "active",
            "nowhere",
            "mana_zone",
            1,
            "top"
        )])),
        DefinitionError {
            path: step_path("start", 0).key("move").key("from"),
            kind: nowhere.clone(),
        }
    );
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_move(
            "active",
            "mana_deck",
            "nowhere",
            1,
            "top"
        )])),
        DefinitionError {
            path: step_path("start", 0).key("move").key("to"),
            kind: nowhere,
        }
    );
}

#[test]
fn untap_rejects_a_zone_listed_twice() {
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_untap(
            "active",
            &["field", "mana_zone", "field"]
        )])),
        DefinitionError {
            path: step_path("start", 0).key("untap").key("zones").index(2),
            kind: DefinitionErrorKind::DuplicateId {
                id: "field".to_string(),
                first: step_path("start", 0).key("untap").key("zones").index(0),
            },
        }
    );
}

#[test]
fn turn_draw_and_move_need_an_ordered_source_zone() {
    // ADR-0013. `mana_zone` and `hand` are declared `ordered: false`.
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_draw(
            "active", "hand", "deck", 1
        )])),
        DefinitionError {
            path: step_path("start", 0).key("draw").key("from"),
            kind: zone_not_ordered("hand"),
        }
    );
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_move(
            "active",
            "mana_zone",
            "mana_deck",
            1,
            "top"
        )])),
        DefinitionError {
            path: step_path("start", 0).key("move").key("from"),
            kind: zone_not_ordered("mana_zone"),
        }
    );

    // A destination may be unordered.
    RulesetDefinition::validate(small_ruleset_with_start(vec![
        step_draw("active", "deck", "hand", 1),
        step_move("active", "mana_deck", "mana_zone", 1, "top"),
    ]))
    .expect("only the source needs to be ordered");

    // The same check reaches the steps nested in an `if`, with `then` and `else` as path keys.
    assert_eq!(
        error_of(small_ruleset_with_start(vec![
            step_untap("active", &["field"]),
            step_set_state("active", "field", "tapped", FieldValue::Bool(false)),
            step_if(vec![], vec![step_draw("active", "hand", "deck", 1)]),
        ])),
        DefinitionError {
            path: step_path("start", 2)
                .key("else")
                .index(0)
                .key("draw")
                .key("from"),
            kind: zone_not_ordered("hand"),
        }
    );
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_if(
            vec![
                step_untap("active", &["field"]),
                step_move("active", "mana_zone", "mana_deck", 1, "top"),
            ],
            vec![]
        )])),
        DefinitionError {
            path: step_path("start", 0)
                .key("then")
                .index(1)
                .key("move")
                .key("from"),
            kind: zone_not_ordered("mana_zone"),
        }
    );
}

#[test]
fn set_state_names_a_state_some_card_type_declares() {
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_set_state(
            "active",
            "field",
            "charge",
            FieldValue::Int(1)
        )])),
        DefinitionError {
            path: step_path("start", 0).key("set_state").key("state"),
            kind: dangling(ReferenceTarget::InstanceState, "charge"),
        }
    );
}

#[test]
fn set_state_value_has_the_declared_type() {
    // `recently_played` is a bool.
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_set_state(
            "active",
            "field",
            "recently_played",
            FieldValue::Int(1)
        )])),
        DefinitionError {
            path: step_path("start", 0).key("set_state").key("value"),
            kind: DefinitionErrorKind::StateValueTypeMismatch {
                state: "recently_played".to_string(),
                declared: FieldType::Bool,
                found: FieldType::Int,
            },
        }
    );

    // `current_life` is an int, and only the Leader declares it.
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_set_state(
            "active",
            "leader_zone",
            "current_life",
            FieldValue::Text("full".to_string())
        )])),
        DefinitionError {
            path: step_path("start", 0).key("set_state").key("value"),
            kind: DefinitionErrorKind::StateValueTypeMismatch {
                state: "current_life".to_string(),
                declared: FieldType::Int,
                found: FieldType::Text,
            },
        }
    );
}

#[test]
fn untap_needs_a_bool_state_named_tapped() {
    // No card type declares `tapped`.
    let mut draft = small_ruleset_with_start(vec![step_untap("active", &["field"])]);
    for card_type in &mut draft.card_types {
        card_type
            .instance_state
            .retain(|declared| declared.name != "tapped");
    }
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: step_path("start", 0).key("untap"),
            kind: dangling(ReferenceTarget::InstanceState, "tapped"),
        }
    );

    // A card type declares `tapped` as an int.
    let mut draft = small_ruleset_with_start(vec![step_untap("active", &["field"])]);
    draft.card_types[2].instance_state = vec![state(
        "tapped",
        FieldTypeDraft::Int,
        InitializerDraft::Literal(FieldValue::Int(0)),
    )];
    assert_eq!(
        error_of(draft),
        DefinitionError {
            path: step_path("start", 0).key("untap"),
            kind: DefinitionErrorKind::StateValueTypeMismatch {
                state: "tapped".to_string(),
                declared: FieldType::Int,
                found: FieldType::Bool,
            },
        }
    );
}

#[test]
fn turn_counts_are_at_least_one() {
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_draw(
            "active", "deck", "hand", 0
        )])),
        DefinitionError {
            path: step_path("start", 0).key("draw").key("count"),
            kind: DefinitionErrorKind::InvalidCount,
        }
    );
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_move(
            "active",
            "mana_deck",
            "mana_zone",
            0,
            "top"
        )])),
        DefinitionError {
            path: step_path("start", 0).key("move").key("count"),
            kind: DefinitionErrorKind::InvalidCount,
        }
    );
}

#[test]
fn move_position_is_top() {
    assert_eq!(
        error_of(small_ruleset_with_start(vec![step_move(
            "active",
            "mana_deck",
            "mana_zone",
            1,
            "bottom"
        )])),
        DefinitionError {
            path: step_path("start", 0).key("move").key("position"),
            kind: DefinitionErrorKind::UnknownPosition {
                position: "bottom".to_string()
            },
        }
    );
}

#[test]
fn turn_steps_are_checked_in_key_order_and_in_order() {
    let expect = |steps: Vec<AutomaticStepDraft>, at: DefinitionPath, kind: DefinitionErrorKind| {
        assert_eq!(
            error_of(small_ruleset_with_start(steps)),
            DefinitionError { path: at, kind }
        );
    };
    let unknown_selector = DefinitionErrorKind::UnknownPlayerSelector {
        selector: "who".to_string(),
    };

    // A move: player, from (dangling, then order), to, count, position.
    let move_at = |key: &'static str| step_path("start", 0).key("move").key(key);
    expect(
        vec![step_move("who", "nowhere", "nowhere", 0, "bottom")],
        move_at("player"),
        unknown_selector.clone(),
    );
    expect(
        vec![step_move("active", "nowhere", "nowhere", 0, "bottom")],
        move_at("from"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );
    expect(
        vec![step_move("active", "hand", "nowhere", 0, "bottom")],
        move_at("from"),
        zone_not_ordered("hand"),
    );
    expect(
        vec![step_move("active", "mana_deck", "nowhere", 0, "bottom")],
        move_at("to"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );
    expect(
        vec![step_move("active", "mana_deck", "hand", 0, "bottom")],
        move_at("count"),
        DefinitionErrorKind::InvalidCount,
    );

    // A set_state: player, zone, state, value.
    let set_state_at = |key: &'static str| step_path("start", 0).key("set_state").key(key);
    expect(
        vec![step_set_state(
            "who",
            "nowhere",
            "charge",
            FieldValue::Int(1),
        )],
        set_state_at("player"),
        unknown_selector,
    );
    expect(
        vec![step_set_state(
            "active",
            "nowhere",
            "charge",
            FieldValue::Int(1),
        )],
        set_state_at("zone"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );
    expect(
        vec![step_set_state(
            "active",
            "field",
            "charge",
            FieldValue::Int(1),
        )],
        set_state_at("state"),
        dangling(ReferenceTarget::InstanceState, "charge"),
    );

    // Steps in order; inside an `if`, the then-steps before the else-steps.
    expect(
        vec![
            step_untap("active", &["field"]),
            step_draw("active", "deck", "nowhere", 1),
            step_draw("active", "deck", "elsewhere", 1),
        ],
        step_path("start", 1).key("draw").key("to"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );
    expect(
        vec![step_if(
            vec![step_draw("active", "deck", "nowhere", 1)],
            vec![step_draw("active", "hand", "deck", 1)],
        )],
        step_path("start", 0)
            .key("then")
            .index(0)
            .key("draw")
            .key("to"),
        dangling(ReferenceTarget::Zone, "nowhere"),
    );
}

#[test]
fn actions_are_checked_after_setup_and_before_the_turn() {
    // A bad setup step, a bad action and a bad turn: the setup is reported.
    let mut draft = small_ruleset_with_start(vec![step_draw("who", "deck", "hand", 1)]);
    draft.setup = vec![shuffle("nobody", "deck")];
    draft.actions[0].id = "Bad".to_string();
    assert_eq!(
        error_of(draft).path,
        setup_path(0, "shuffle", "player"),
        "setup first"
    );

    // Without the setup error, the action is reported before the turn.
    let mut draft = small_ruleset_with_start(vec![step_draw("who", "deck", "hand", 1)]);
    draft.actions[0].id = "Bad".to_string();
    assert_eq!(
        error_of(draft).path,
        path().key("actions").index(0).key("id"),
        "actions before the turn"
    );
}
