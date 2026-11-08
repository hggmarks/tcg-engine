//! Acceptance tests for ticket #19: the typed `RulesetDefinition` and its semantic validation.
//!
//! Drafts are built in Rust, the way a code frontend would build them. Every error test asserts the
//! whole `DefinitionError`: kind and path.

use tcg_engine::definition_path::{DefinitionPath, PathSegment};
use tcg_engine::ruleset_definition::{
    CardTypeDraft, CardTypeId, DeckInputDraft, DefinitionError, DefinitionErrorKind, FieldDraft,
    FieldName, FieldType, FieldTypeDraft, FieldValue, Initializer, InitializerDraft,
    InstanceStateDraft, MetaDraft, ParametersDraft, ReferenceTarget, RulesetDefinition,
    RulesetDraft, Visibility, ZoneDraft, ZoneId,
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

    for bad in ["", "3", "Deck", "a]b", "a-b", "_x"] {
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
    for id in ["a", "a1", "a_b", "true", "x9_", "mana_deck_2"] {
        draft.zones.push(zone(id, Visibility::Public, false));
    }
    let definition = RulesetDefinition::validate(draft).expect("every id matches the grammar");
    assert_eq!(definition.zones().len(), 13);
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
