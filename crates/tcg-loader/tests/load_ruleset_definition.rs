//! Acceptance tests for ticket #19: the YAML loader (syntax only) in front of the engine's
//! `RulesetDefinition::validate`.

use std::num::NonZeroU32;

use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    CardTypeDraft, DeckInputDraft, DefinitionError, DefinitionErrorKind, FieldDraft, FieldType,
    FieldTypeDraft, FieldValue, InitializerDraft, InstanceStateDraft, MetaDraft, ParametersDraft,
    PlayerSelector, ReferenceTarget, RulesetDefinition, RulesetDraft, SetupStep, Visibility,
    ZoneDraft, ZoneId,
};
use tcg_loader::{
    load_ruleset_definition, parse_ruleset_draft, LoadError, Location, YamlError, YamlErrorKind,
    YamlFeature,
};

/// Lines 1-44 of `rulesets/small-ruleset/small-ruleset.yaml`: meta, parameters, zones, card types
/// and inputs, without the behavior sections.
const FIXTURE: &str = include_str!("fixtures/small-ruleset-structure.yaml");
const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");

/// The meta and parameters sections shared by the small documents below.
const PREFIX: &str =
    "meta: { id: small, version: 1 }\nparameters: { players: 2, starting_player: 0 }\n";

// ---- draft builders (a copy of the ones in the engine's tests)

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

/// The structural subset of the Small Ruleset, written by hand.
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
    }
}

// ---- helpers

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

fn yaml_error(yaml: &str) -> YamlError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Yaml(error)) => error,
        other => panic!("expected a YAML error, got {other:?}"),
    }
}

fn definition_error(yaml: &str) -> DefinitionError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Definition(error)) => error,
        other => panic!("expected a definition error, got {other:?}"),
    }
}

/// A document with one card type `c` that has the field `life: int` and one instance state `s`,
/// whose entries (after `name: s`) are given as flow-mapping content.
fn document_with_state(state_entries: &str) -> String {
    format!(
        "{PREFIX}zones: []\ncard_types:\n  - id: c\n    fields:\n      - {{ name: life, type: int }}\n    instance_state:\n      - {{ name: s, {state_entries} }}\n    effects: false\ninputs: []\n"
    )
}

fn state_init_path() -> DefinitionPath {
    path()
        .key("card_types")
        .index(0)
        .key("instance_state")
        .index(0)
        .key("init")
}

/// The same element as `state_init_path`, addressed by id as the engine reports it.
fn semantic_init_path() -> DefinitionPath {
    path()
        .key("card_types")
        .id("c")
        .key("instance_state")
        .id("s")
        .key("init")
}

// ---- the fixture

#[test]
fn small_ruleset_structure_fixture_loads() {
    let draft = parse_ruleset_draft(FIXTURE).expect("fixture parses");
    assert_eq!(draft, small_ruleset_draft());

    let loaded = load_ruleset_definition(FIXTURE).expect("fixture loads");
    let validated = RulesetDefinition::validate(small_ruleset_draft()).expect("valid draft");
    assert_eq!(loaded, validated);
}

#[test]
fn structure_fixture_is_a_prefix_of_the_small_ruleset() {
    assert!(SMALL_RULESET.starts_with(FIXTURE));
}

// ---- initializers

#[test]
fn init_keeps_the_yaml_scalar_type() {
    let init_of = |state_entries: &str| -> InitializerDraft {
        let draft = parse_ruleset_draft(&document_with_state(state_entries)).expect("parses");
        draft.card_types[0].instance_state[0].init.clone()
    };
    assert_eq!(
        init_of("type: bool, init: false"),
        InitializerDraft::Literal(FieldValue::Bool(false))
    );
    assert_eq!(
        init_of("type: int, init: 5"),
        InitializerDraft::Literal(FieldValue::Int(5))
    );
    assert_eq!(
        init_of("type: text, init: \"hello\""),
        InitializerDraft::Literal(FieldValue::Text("hello".to_string()))
    );
    assert_eq!(
        init_of("type: int, init: { from_field: life }"),
        InitializerDraft::FromField("life".to_string())
    );

    // The scalar is checked against the declared type without coercion.
    assert_eq!(
        definition_error(&document_with_state("type: text, init: 5")),
        DefinitionError {
            path: semantic_init_path(),
            kind: DefinitionErrorKind::InitializerTypeMismatch {
                declared: FieldType::Text,
                found: FieldType::Int
            },
        }
    );
    assert_eq!(
        definition_error(&document_with_state("type: bool, init: \"false\"")),
        DefinitionError {
            path: semantic_init_path(),
            kind: DefinitionErrorKind::InitializerTypeMismatch {
                declared: FieldType::Bool,
                found: FieldType::Text
            },
        }
    );

    // Only bool, integer and text scalars and the from_field map are initializers.
    for rejected in ["1.5", "~", "[1]"] {
        let error = yaml_error(&document_with_state(&format!(
            "type: int, init: {rejected}"
        )));
        assert_eq!(error.path, state_init_path(), "init: {rejected}");
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "init: {rejected} gave {:?}",
            error.kind
        );
    }
}

#[test]
fn enum_and_enum_set_type_names_are_unsupported() {
    for (spelling, field_type) in [
        ("enum", FieldTypeDraft::Enum),
        ("enum_set", FieldTypeDraft::EnumSet),
    ] {
        let yaml = format!(
            "{PREFIX}zones: []\ncard_types:\n  - id: c\n    fields:\n      - {{ name: f, type: {spelling} }}\n    instance_state: []\n    effects: false\ninputs: []\n"
        );
        assert_eq!(
            definition_error(&yaml),
            DefinitionError {
                path: path()
                    .key("card_types")
                    .id("c")
                    .key("fields")
                    .id("f")
                    .key("type"),
                kind: DefinitionErrorKind::UnsupportedFieldType { field_type },
            },
            "type: {spelling}"
        );
    }
}

// ---- syntax errors

#[test]
fn unknown_keys_are_rejected_at_every_level() {
    let unknown = |key: &str, at: DefinitionPath| {
        (
            YamlErrorKind::UnknownKey {
                key: key.to_string(),
            },
            at,
        )
    };
    let cases: Vec<(String, (YamlErrorKind, DefinitionPath))> = vec![
        (
            format!("{PREFIX}bogus: 1\n"),
            unknown("bogus", path().key("bogus")),
        ),
        (
            "meta: { id: small, version: 1 }\nparameters: { players: 2, starting_player: 0, extra: 1 }\n"
                .to_string(),
            unknown("extra", path().key("parameters").key("extra")),
        ),
        (
            format!(
                "{PREFIX}zones:\n  - {{ id: deck, visibility: hidden, ordered: true, extra: 1 }}\n"
            ),
            unknown("extra", path().key("zones").index(0).key("extra")),
        ),
        (
            format!(
                "{PREFIX}zones: []\ncard_types:\n  - id: c\n    fields:\n      - {{ name: a, type: int, extra: 1 }}\n"
            ),
            unknown(
                "extra",
                path()
                    .key("card_types")
                    .index(0)
                    .key("fields")
                    .index(0)
                    .key("extra"),
            ),
        ),
        (
            document_with_state("type: int, init: { from_field: life, bogus: 1 }"),
            unknown("bogus", state_init_path().key("bogus")),
        ),
        (
            format!(
                "{PREFIX}zones: []\ncard_types: []\ninputs:\n  - {{ id: i, card_types: [], size: 1, zone: z, extra: 1 }}\n"
            ),
            unknown("extra", path().key("inputs").index(0).key("extra")),
        ),
    ];
    for (yaml, (kind, at)) in cases {
        let error = yaml_error(&yaml);
        assert_eq!(error.kind, kind, "{yaml}");
        assert_eq!(error.path, at, "{yaml}");
    }

    // Line and column are 1-based and point at the unknown key.
    let error = yaml_error(&format!(
        "{PREFIX}zones:\n  - {{ id: deck, visibility: hidden, ordered: true, extra: 1 }}\n"
    ));
    assert_eq!(
        error.location,
        Some(Location {
            line: 4,
            column: 52
        })
    );
}

#[test]
fn forbidden_yaml_features_are_rejected() {
    let forbidden = |feature: YamlFeature| YamlErrorKind::ForbiddenFeature { feature };
    let cases: Vec<(&str, String, YamlErrorKind, DefinitionPath)> = vec![
        (
            "anchor",
            "meta:\n  id: &x small\n  version: 1\n".to_string(),
            forbidden(YamlFeature::Anchor),
            path().key("meta").key("id"),
        ),
        (
            "alias",
            "meta:\n  id: *x\n  version: 1\n".to_string(),
            forbidden(YamlFeature::Alias),
            path().key("meta").key("id"),
        ),
        (
            "core tag",
            "meta:\n  id: !!str small\n  version: 1\n".to_string(),
            forbidden(YamlFeature::Tag),
            path().key("meta").key("id"),
        ),
        (
            "custom tag",
            "meta:\n  id: !custom small\n  version: 1\n".to_string(),
            forbidden(YamlFeature::Tag),
            path().key("meta").key("id"),
        ),
        (
            "merge key",
            "meta:\n  <<: { id: small }\n  version: 1\n".to_string(),
            forbidden(YamlFeature::MergeKey),
            path().key("meta").key("<<"),
        ),
        (
            "duplicate key",
            "meta:\n  id: small\n  id: other\n  version: 1\n".to_string(),
            forbidden(YamlFeature::DuplicateKey),
            path().key("meta").key("id"),
        ),
        (
            "second document",
            format!("{FIXTURE}---\nmeta: {{ id: other, version: 2 }}\n"),
            forbidden(YamlFeature::MultipleDocuments),
            path(),
        ),
        (
            "null for a text",
            format!(
                "{PREFIX}zones: []\ncard_types:\n  - id: c\n    fields:\n      - {{ name: ~, type: int }}\n"
            ),
            forbidden(YamlFeature::ImplicitTyping),
            path()
                .key("card_types")
                .index(0)
                .key("fields")
                .index(0)
                .key("name"),
        ),
    ];
    for (name, yaml, kind, at) in cases {
        let error = yaml_error(&yaml);
        assert_eq!(error.kind, kind, "{name}");
        assert_eq!(error.path, at, "{name}");
    }

    // A bool key accepts only a YAML bool scalar (ticket #20), so `yes` is a text there, quoted or
    // not, and is `Malformed` at the key rather than `ImplicitTyping`.
    for yes in ["yes", "\"yes\""] {
        let error = yaml_error(&format!(
            "{PREFIX}zones: []\ncard_types:\n  - id: c\n    fields: []\n    instance_state: []\n    effects: {yes}\n"
        ));
        assert_eq!(
            error.path,
            path().key("card_types").index(0).key("effects"),
            "effects: {yes}"
        );
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "effects: {yes} gave {:?}",
            error.kind
        );
    }

    // Line and column are 1-based and point at the anchored node.
    let error = yaml_error("meta:\n  id: &x small\n  version: 1\n");
    assert_eq!(
        error.location,
        Some(Location {
            line: 2,
            column: 10
        })
    );
}

#[test]
fn win_conditions_are_still_unsupported() {
    // `setup` (ticket #21), `turn` and `actions` (ticket #22) are loaded; `win_conditions` is only
    // noticed, until ticket [09].
    let error = yaml_error(&format!("{FIXTURE}win_conditions: []\n"));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnsupportedSection {
            section: "win_conditions".to_string()
        }
    );
    assert_eq!(error.path, path().key("win_conditions"));

    // A section with an empty value is still present.
    let error = yaml_error(&format!("{FIXTURE}win_conditions:\n"));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnsupportedSection {
            section: "win_conditions".to_string()
        }
    );

    // The turn and the actions are not unsupported any more: an empty turn is read, and the engine
    // is the one to reject it.
    let error = definition_error(&format!("{FIXTURE}turn:\n  phases: []\n"));
    assert_eq!(
        error.kind,
        DefinitionErrorKind::InvalidActionPhaseCount { found: 0 }
    );
}

#[test]
fn the_real_small_ruleset_still_waits_for_attack_conditions() {
    // `play_creature` loads (ticket [10]); `attack` has `where` and `one_of`, which ticket [11]
    // adds. The file loads up to the first of them: its `setup`, `turn` and `play_creature` are
    // read.
    let error = yaml_error(SMALL_RULESET);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "where".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .index(1)
            .key("params")
            .index(0)
            .key("where")
    );
}

// ---- the boundary between the loader and the engine

/// The fixture with one more zone, named like the first.
fn fixture_with_duplicate_zone() -> String {
    FIXTURE.replace(
        "\n\ncard_types:",
        "\n  - { id: deck, visibility: hidden, ordered: true }\n\ncard_types:",
    )
}

/// The fixture whose first input points to a zone that does not exist.
fn fixture_with_dangling_zone() -> String {
    FIXTURE.replace("zone: leader_zone }", "zone: nowhere }")
}

fn draft_with_duplicate_zone() -> RulesetDraft {
    let mut draft = small_ruleset_draft();
    draft.zones.push(zone("deck", Visibility::Hidden, true));
    draft
}

fn draft_with_dangling_zone() -> RulesetDraft {
    let mut draft = small_ruleset_draft();
    draft.inputs[0].zone = "nowhere".to_string();
    draft
}

#[test]
fn yaml_and_code_frontends_get_identical_semantic_errors() {
    let from_yaml = definition_error(&fixture_with_duplicate_zone());
    let from_code = RulesetDefinition::validate(draft_with_duplicate_zone()).unwrap_err();
    assert_eq!(from_yaml, from_code);
    assert_eq!(
        from_yaml,
        DefinitionError {
            path: path().key("zones").index(7),
            kind: DefinitionErrorKind::DuplicateId {
                id: "deck".to_string(),
                first: path().key("zones").index(0)
            },
        }
    );

    let from_yaml = definition_error(&fixture_with_dangling_zone());
    let from_code = RulesetDefinition::validate(draft_with_dangling_zone()).unwrap_err();
    assert_eq!(from_yaml, from_code);
    assert_eq!(
        from_yaml,
        DefinitionError {
            path: path().key("inputs").id("leader").key("zone"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "nowhere".to_string()
            },
        }
    );
}

#[test]
fn loader_parses_semantically_invalid_files_into_drafts() {
    assert_eq!(
        parse_ruleset_draft(&fixture_with_duplicate_zone()),
        Ok(draft_with_duplicate_zone())
    );
    assert_eq!(
        parse_ruleset_draft(&fixture_with_dangling_zone()),
        Ok(draft_with_dangling_zone())
    );
}

#[test]
fn loader_inherits_the_workspace_guardrails() {
    let manifest = include_str!("../Cargo.toml");
    let lints = manifest
        .split("[lints]")
        .nth(1)
        .expect("the loader manifest has a [lints] section");
    let lints = lints.split("\n[").next().unwrap_or(lints);
    assert!(
        lints
            .lines()
            .any(|line| line.replace(' ', "") == "workspace=true"),
        "[lints] must set workspace = true"
    );

    let own_clippy_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("clippy.toml");
    assert!(
        !own_clippy_toml.exists(),
        "a crate-local clippy.toml would shadow the root one"
    );
}

// ---- ticket #20: identifier grammar and deck input constraints

#[test]
fn explicit_null_max_copies_is_rejected() {
    // An absent key means "no limit".
    let draft = parse_ruleset_draft(FIXTURE).expect("fixture parses");
    assert_eq!(draft.inputs[0].max_copies, None);
    assert_eq!(draft.inputs[1].max_copies, Some(4));

    // `max_copies: ~` is not the same thing: it is a syntax error at the key.
    let yaml = FIXTURE.replace("max_copies: 4", "max_copies: ~");
    assert_ne!(yaml, FIXTURE);
    let error = yaml_error(&yaml);
    assert_eq!(error.path, path().key("inputs").index(1).key("max_copies"));
    assert!(
        matches!(error.kind, YamlErrorKind::Malformed { .. }),
        "got {:?}",
        error.kind
    );
}

#[test]
fn numeric_ids_are_invalid_whether_quoted_or_not() {
    for id in ["3", "\"3\""] {
        let yaml = format!(
            "{PREFIX}zones:\n  - {{ id: {id}, visibility: hidden, ordered: true }}\ncard_types: []\ninputs: []\n"
        );
        assert_eq!(
            definition_error(&yaml),
            DefinitionError {
                path: path().key("zones").index(0).key("id"),
                kind: DefinitionErrorKind::InvalidId {
                    id: "3".to_string()
                },
            },
            "id: {id}"
        );
    }
}

#[test]
fn deck_input_constraints_reach_the_loader_as_definition_errors() {
    let yaml = FIXTURE.replace("size: 50, max_copies: 4", "size: 3, max_copies: 4");
    assert_ne!(yaml, FIXTURE);
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: path().key("inputs").id("main_deck").key("max_copies"),
            kind: DefinitionErrorKind::InvalidMaxCopies {
                max_copies: 4,
                size: 3
            },
        }
    );

    let yaml = FIXTURE.replace("card_types: [mana],", "card_types: [],");
    assert_ne!(yaml, FIXTURE);
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: path().key("inputs").id("mana_deck").key("card_types"),
            kind: DefinitionErrorKind::NoCardTypes,
        }
    );
}

#[test]
fn reserved_words_are_rejected_as_ids_quoted_or_not() {
    let document = |id: &str| {
        format!(
            "{PREFIX}zones:\n  - {{ id: {id}, visibility: hidden, ordered: true }}\ncard_types: []\ninputs: []\n"
        )
    };

    // Quoted, the word is a text: the engine rejects it as an id.
    for word in ["true", "false", "null"] {
        assert_eq!(
            definition_error(&document(&format!("\"{word}\""))),
            DefinitionError {
                path: path().key("zones").index(0).key("id"),
                kind: DefinitionErrorKind::InvalidId {
                    id: word.to_string()
                },
            },
            "id: \"{word}\""
        );
    }

    // Unquoted, serde-saphyr reads `true` and `false` into a text as the words themselves, so the
    // engine rejects them exactly like the quoted spelling.
    for word in ["true", "false"] {
        assert_eq!(
            definition_error(&document(word)),
            DefinitionError {
                path: path().key("zones").index(0).key("id"),
                kind: DefinitionErrorKind::InvalidId {
                    id: word.to_string()
                },
            },
            "id: {word}"
        );
    }

    // An unquoted null is not read into a text at all: the loader rejects it as implicit typing.
    for word in ["null", "~"] {
        let error = yaml_error(&document(word));
        assert_eq!(
            error.kind,
            YamlErrorKind::ForbiddenFeature {
                feature: YamlFeature::ImplicitTyping
            },
            "id: {word}"
        );
        assert_eq!(
            error.path,
            path().key("zones").index(0).key("id"),
            "id: {word}"
        );
    }
}

const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");

#[test]
fn quoted_numbers_and_bools_are_rejected_in_format_keys() {
    #[derive(Clone, Copy)]
    enum File {
        Ruleset,
        Catalog,
        Deck,
    }
    use File::{Catalog, Deck, Ruleset};

    // (file, the text to quote, the quoted text, the path of the key). The first occurrence of the
    // text in the file is replaced.
    let cases: Vec<(File, &str, &str, DefinitionPath)> = vec![
        (
            Ruleset,
            "version: 1 }",
            "version: \"1\" }",
            path().key("meta").key("version"),
        ),
        (
            Ruleset,
            "players: 2",
            "players: \"2\"",
            path().key("parameters").key("players"),
        ),
        (
            Ruleset,
            "starting_player: 0",
            "starting_player: \"0\"",
            path().key("parameters").key("starting_player"),
        ),
        (
            Ruleset,
            "ordered: true }",
            "ordered: \"true\" }",
            path().key("zones").index(0).key("ordered"),
        ),
        (
            Ruleset,
            "effects: false",
            "effects: \"false\"",
            path().key("card_types").index(0).key("effects"),
        ),
        (
            Ruleset,
            "size: 1,",
            "size: \"1\",",
            path().key("inputs").index(0).key("size"),
        ),
        (
            Ruleset,
            "max_copies: 4",
            "max_copies: \"4\"",
            path().key("inputs").index(1).key("max_copies"),
        ),
        (
            Catalog,
            "ruleset_version: 1",
            "ruleset_version: \"1\"",
            path().key("meta").key("ruleset_version"),
        ),
        (
            Deck,
            "count: 1",
            "count: \"1\"",
            path()
                .key("inputs")
                .index(0)
                .key("cards")
                .index(0)
                .key("count"),
        ),
    ];

    let parse = |file: File, yaml: &str| -> Result<(), YamlError> {
        match file {
            Ruleset => parse_ruleset_draft(yaml).map(|_| ()),
            Catalog => tcg_loader::parse_card_catalog_draft(yaml).map(|_| ()),
            Deck => tcg_loader::parse_deck_list_draft(yaml).map(|_| ()),
        }
    };
    for (file, unquoted, quoted, at) in cases {
        let original = match file {
            Ruleset => FIXTURE,
            Catalog => SMALL_CATALOG,
            Deck => DECK_A,
        };
        assert!(original.contains(unquoted), "{unquoted}");
        assert_eq!(parse(file, original), Ok(()), "the unquoted file loads");

        let error = parse(file, &original.replacen(unquoted, quoted, 1)).expect_err(&format!(
            "{quoted}: a quoted value is a text, not a number or a bool"
        ));
        assert_eq!(error.path, at, "{quoted}");
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "{quoted} gave {:?}",
            error.kind
        );
    }
}

// ---- setup (ticket #21)

/// The real Small Ruleset up to, and not including, its `turn:` line: the structure and `setup`.
fn small_ruleset_through_setup() -> String {
    let (before_turn, _) = SMALL_RULESET
        .split_once("\nturn:")
        .expect("the Small Ruleset has a turn section");
    format!("{before_turn}\n")
}

#[test]
fn setup_is_loaded_from_yaml() {
    let definition = load_ruleset_definition(&small_ruleset_through_setup())
        .expect("the real file, without its turn section, loads");
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
}

#[test]
fn setup_is_optional_and_means_no_steps() {
    let definition = load_ruleset_definition(FIXTURE).expect("the fixture has no setup");
    assert_eq!(definition.setup(), []);
}

#[test]
fn setup_rejects_unknown_step() {
    // An unknown primitive is an unknown key of its list element.
    let error = yaml_error(&format!(
        "{FIXTURE}setup:\n  - shuffle: {{ player: each, zone: deck }}\n  - untap: {{}}\n"
    ));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "untap".to_string()
        }
    );
    assert_eq!(error.path, path().key("setup").index(1));

    // An unknown key inside a step is reported inside it, positionally.
    let error = yaml_error(&format!(
        "{FIXTURE}setup:\n  - shuffle: {{ player: each, zone: deck, extra: 1 }}\n"
    ));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "extra".to_string()
        }
    );
    assert_eq!(
        error.path,
        path().key("setup").index(0).key("shuffle").key("extra")
    );
}

#[test]
fn setup_rejects_quoted_count() {
    let error = yaml_error(&format!(
        "{FIXTURE}setup:\n  - shuffle: {{ player: each, zone: deck }}\n  - draw: {{ player: each, from: deck, to: hand, count: \"5\" }}\n"
    ));
    assert_eq!(
        error.path,
        path().key("setup").index(1).key("draw").key("count")
    );
    assert!(
        matches!(error.kind, YamlErrorKind::Malformed { .. }),
        "got {:?}",
        error.kind
    );
}

#[test]
fn setup_meaning_is_checked_by_the_engine_with_positional_step_paths() {
    // The loader does not know the player selectors or the zones: the engine reports them.
    let error = definition_error(&format!(
        "{FIXTURE}setup:\n  - shuffle: {{ player: each, zone: deck }}\n  - draw: {{ player: each, from: deck, to: nowhere, count: 5 }}\n"
    ));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("setup").index(1).key("draw").key("to"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "nowhere".to_string()
            },
        }
    );
}

#[test]
fn unknown_enum_values_outside_setup_stay_malformed() {
    // Only an unknown Setup Step primitive is an unknown key; other unknown values are `Malformed`.
    let error = yaml_error(&format!(
        "{PREFIX}zones:\n  - {{ id: deck, visibility: secret, ordered: true }}\ncard_types: []\ninputs: []\n"
    ));
    assert_eq!(error.path, path().key("zones").index(0).key("visibility"));
    assert!(
        matches!(error.kind, YamlErrorKind::Malformed { .. }),
        "visibility: secret gave {:?}",
        error.kind
    );

    let error = yaml_error(&format!(
        "{PREFIX}zones: []\ncard_types:\n  - id: c\n    fields:\n      - {{ name: f, type: float }}\n    instance_state: []\n    effects: false\ninputs: []\n"
    ));
    assert_eq!(
        error.path,
        path()
            .key("card_types")
            .index(0)
            .key("fields")
            .index(0)
            .key("type")
    );
    assert!(
        matches!(error.kind, YamlErrorKind::Malformed { .. }),
        "type: float gave {:?}",
        error.kind
    );
}
