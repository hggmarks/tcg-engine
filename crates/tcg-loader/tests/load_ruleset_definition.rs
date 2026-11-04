//! Acceptance tests for ticket #19: the YAML loader (syntax only) in front of the engine's
//! `RulesetDefinition::validate`.

use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    CardTypeDraft, DeckInputDraft, DefinitionError, DefinitionErrorKind, FieldDraft, FieldType,
    FieldTypeDraft, FieldValue, InitializerDraft, InstanceStateDraft, MetaDraft, ParametersDraft,
    ReferenceTarget, RulesetDefinition, RulesetDraft, Visibility, ZoneDraft,
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
            "yes for a bool",
            format!(
                "{PREFIX}zones: []\ncard_types:\n  - id: c\n    fields: []\n    instance_state: []\n    effects: yes\n"
            ),
            forbidden(YamlFeature::ImplicitTyping),
            path().key("card_types").index(0).key("effects"),
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
fn behavior_sections_are_unsupported() {
    let sections = [
        ("setup", "setup:\n  - draw: { player: each, count: 5 }\n"),
        ("turn", "turn:\n  phases: []\n"),
        ("actions", "actions:\n  - id: attack\n"),
        ("win_conditions", "win_conditions: []\n"),
    ];
    for (section, yaml) in sections {
        let error = yaml_error(&format!("{FIXTURE}{yaml}"));
        assert_eq!(
            error.kind,
            YamlErrorKind::UnsupportedSection {
                section: section.to_string()
            },
            "{section}"
        );
        assert_eq!(error.path, path().key(section), "{section}");
    }

    // A section with an empty value is still present.
    let error = yaml_error(&format!("{FIXTURE}setup:\n"));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnsupportedSection {
            section: "setup".to_string()
        }
    );

    // With several present, the first in the fixed order setup, turn, actions, win_conditions wins.
    let error = yaml_error(&format!("{FIXTURE}win_conditions: []\nturn: {{}}\n"));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnsupportedSection {
            section: "turn".to_string()
        }
    );

    // The real Small Ruleset has them all.
    let error = yaml_error(SMALL_RULESET);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnsupportedSection {
            section: "setup".to_string()
        }
    );
    assert_eq!(error.path, path().key("setup"));
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
