//! Acceptance tests for ticket #20: the YAML loader (syntax only) for card catalogs, in front of
//! the engine's `CardCatalog::validate`.

use tcg_engine::card_catalog::{
    CardCatalog, CardCatalogDraft, CardDraft, CardId, CatalogError, CatalogErrorKind,
    CatalogMetaDraft, EffectDraft,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    CardTypeId, FieldName, FieldType, FieldValue, RulesetDefinition,
};
use tcg_loader::{
    load_card_catalog, load_ruleset_definition, parse_card_catalog_draft, CatalogLoadError,
    Location, YamlError, YamlErrorKind, YamlFeature,
};

/// The structure of the Small Ruleset: meta, parameters, zones, card types and inputs.
const FIXTURE: &str = include_str!("fixtures/small-ruleset-structure.yaml");

fn ruleset() -> RulesetDefinition {
    load_ruleset_definition(FIXTURE).expect("the fixture is a valid ruleset")
}

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

fn int(value: i64) -> FieldValue {
    FieldValue::Int(value)
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

const META: &str = "meta: { ruleset: small, ruleset_version: 1 }\n";

/// A catalog document holding the given card entries (each a list item, one per line).
fn catalog_yaml(entries: &str) -> String {
    format!("{META}cards:\n{entries}")
}

/// One creature `goblin` whose `fields` mapping holds the given flow-mapping content.
fn goblin_yaml(fields: &str) -> String {
    catalog_yaml(&format!(
        "  - {{ id: goblin, type: creature, fields: {{ {fields} }} }}\n"
    ))
}

fn yaml_error(yaml: &str) -> YamlError {
    match load_card_catalog(&ruleset(), yaml) {
        Err(CatalogLoadError::Yaml(error)) => error,
        other => panic!("expected a YAML error, got {other:?}"),
    }
}

fn catalog_error(yaml: &str) -> CatalogError {
    match load_card_catalog(&ruleset(), yaml) {
        Err(CatalogLoadError::Catalog(error)) => error,
        other => panic!("expected a catalog error, got {other:?}"),
    }
}

fn goblin_draft_card() -> CardDraft {
    CardDraft {
        id: "goblin".to_string(),
        card_type: "creature".to_string(),
        fields: vec![
            ("power".to_string(), int(2)),
            ("name".to_string(), text("Goblin")),
            ("cost".to_string(), int(1)),
        ],
        effects: vec![],
    }
}

// ---- parsing

#[test]
fn catalog_draft_keeps_file_order_and_scalar_kinds() {
    let yaml = catalog_yaml(
        "  - { id: goblin, type: creature, fields: { power: 2, name: Goblin, cost: 1 } }\n  - { id: mana, type: mana, fields: {} }\n",
    );
    assert_eq!(
        parse_card_catalog_draft(&yaml),
        Ok(CardCatalogDraft {
            meta: CatalogMetaDraft {
                ruleset: "small".to_string(),
                ruleset_version: 1,
            },
            cards: vec![
                goblin_draft_card(),
                CardDraft {
                    id: "mana".to_string(),
                    card_type: "mana".to_string(),
                    fields: vec![],
                    effects: vec![],
                },
            ],
        })
    );

    // Bool, int and text scalars each keep their own kind.
    let draft = parse_card_catalog_draft(&goblin_yaml("a: true, b: -4, c: \"x\", d: hello"))
        .expect("parses");
    assert_eq!(
        draft.cards[0].fields,
        vec![
            ("a".to_string(), FieldValue::Bool(true)),
            ("b".to_string(), int(-4)),
            ("c".to_string(), text("x")),
            ("d".to_string(), text("hello")),
        ]
    );
}

#[test]
fn catalog_draft_keeps_only_the_effect_primitive_names() {
    let yaml = catalog_yaml(
        "  - id: sage\n    type: creature\n    fields: { name: Sage, cost: 1, power: 1 }\n    effects:\n      - draw: { count: 1 }\n      - damage: { amount: 2 }\n      - end_turn: {}\n",
    );
    let draft = parse_card_catalog_draft(&yaml).expect("parses");
    assert_eq!(
        draft.cards[0].effects,
        vec![
            EffectDraft {
                primitive: "draw".to_string()
            },
            EffectDraft {
                primitive: "damage".to_string()
            },
            EffectDraft {
                primitive: "end_turn".to_string()
            },
        ]
    );

    // An empty list and an absent key both mean no effects.
    let yaml = catalog_yaml(
        "  - { id: goblin, type: creature, fields: { name: G, cost: 1, power: 1 }, effects: [] }\n",
    );
    assert_eq!(
        parse_card_catalog_draft(&yaml).expect("parses").cards[0].effects,
        vec![]
    );

    // An effect entry names exactly one primitive.
    let yaml = catalog_yaml(
        "  - id: sage\n    type: creature\n    fields: { name: Sage, cost: 1, power: 1 }\n    effects:\n      - { draw: {}, damage: {} }\n",
    );
    let error = yaml_error(&yaml);
    assert_eq!(
        error.path,
        path().key("cards").index(0).key("effects").index(0)
    );
    assert!(
        matches!(error.kind, YamlErrorKind::Malformed { .. }),
        "got {:?}",
        error.kind
    );
}

#[test]
fn card_catalog_loads_against_the_ruleset() {
    let yaml = catalog_yaml(
        "  - { id: goblin, type: creature, fields: { power: 2, name: Goblin, cost: 1 } }\n  - { id: mana, type: mana, fields: {} }\n",
    );
    let catalog = load_card_catalog(&ruleset(), &yaml).expect("valid catalog");

    let from_code = CardCatalog::validate(
        &ruleset(),
        CardCatalogDraft {
            meta: CatalogMetaDraft {
                ruleset: "small".to_string(),
                ruleset_version: 1,
            },
            cards: vec![
                goblin_draft_card(),
                CardDraft {
                    id: "mana".to_string(),
                    card_type: "mana".to_string(),
                    fields: vec![],
                    effects: vec![],
                },
            ],
        },
    )
    .expect("valid catalog");
    assert_eq!(catalog, from_code);

    let goblin = catalog.card(&CardId::new("goblin")).expect("goblin");
    assert_eq!(goblin.card_type(), &CardTypeId::new("creature"));
    assert_eq!(
        goblin.fields(),
        [
            (FieldName::new("name"), text("Goblin")),
            (FieldName::new("cost"), int(1)),
            (FieldName::new("power"), int(2)),
        ]
    );
}

// ---- scalar typing

#[test]
fn quoted_number_is_text_field_value() {
    // `power: "5"` is a text, so the engine reports a type mismatch; the loader does not coerce.
    let draft =
        parse_card_catalog_draft(&goblin_yaml("name: G, cost: 1, power: \"5\"")).expect("parses");
    assert_eq!(draft.cards[0].fields[2], ("power".to_string(), text("5")));
    assert_eq!(
        catalog_error(&goblin_yaml("name: G, cost: 1, power: \"5\"")),
        CatalogError {
            path: path().key("cards").id("goblin").key("fields").id("power"),
            kind: CatalogErrorKind::FieldTypeMismatch {
                declared: FieldType::Int,
                found: FieldType::Text,
            },
        }
    );

    // `name: 3` is an int, so the text field `name` is a mismatch.
    let draft =
        parse_card_catalog_draft(&goblin_yaml("name: 3, cost: 1, power: 1")).expect("parses");
    assert_eq!(draft.cards[0].fields[0], ("name".to_string(), int(3)));
    assert_eq!(
        catalog_error(&goblin_yaml("name: 3, cost: 1, power: 1")),
        CatalogError {
            path: path().key("cards").id("goblin").key("fields").id("name"),
            kind: CatalogErrorKind::FieldTypeMismatch {
                declared: FieldType::Text,
                found: FieldType::Int,
            },
        }
    );

    // A quoted number as a card id is a text like any other; the grammar rejects it either way.
    for id in ["3", "\"3\""] {
        let yaml = catalog_yaml(&format!("  - {{ id: {id}, type: mana, fields: {{}} }}\n"));
        assert_eq!(
            catalog_error(&yaml),
            CatalogError {
                path: path().key("cards").index(0).key("id"),
                kind: CatalogErrorKind::InvalidId {
                    id: "3".to_string()
                },
            },
            "id: {id}"
        );
    }
}

#[test]
fn only_bool_int_and_text_scalars_are_field_values() {
    for rejected in ["1.5", "~", "[1]", "{ a: 1 }"] {
        let error = yaml_error(&goblin_yaml(&format!(
            "name: G, cost: 1, power: {rejected}"
        )));
        assert_eq!(
            error.path,
            path().key("cards").index(0).key("fields").key("power"),
            "power: {rejected}"
        );
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "power: {rejected} gave {:?}",
            error.kind
        );
    }
}

// ---- the restricted YAML subset

#[test]
fn card_catalog_rejects_anchor() {
    let yaml = catalog_yaml("  - id: &g goblin\n    type: creature\n    fields: {}\n");
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::ForbiddenFeature {
            feature: YamlFeature::Anchor
        }
    );
    assert_eq!(error.path, path().key("cards").index(0).key("id"));
    // Line and column are 1-based and point at the anchored node.
    assert_eq!(
        error.location,
        Some(Location {
            line: 3,
            column: 12
        })
    );
}

#[test]
fn card_catalog_rejects_the_other_forbidden_yaml_features() {
    let forbidden = |feature: YamlFeature| YamlErrorKind::ForbiddenFeature { feature };
    let cases = [
        (
            "alias",
            catalog_yaml("  - id: *g\n    type: creature\n    fields: {}\n"),
            forbidden(YamlFeature::Alias),
            path().key("cards").index(0).key("id"),
        ),
        (
            "tag",
            catalog_yaml("  - id: !!str goblin\n    type: creature\n    fields: {}\n"),
            forbidden(YamlFeature::Tag),
            path().key("cards").index(0).key("id"),
        ),
        (
            "duplicate field key",
            goblin_yaml("name: A, name: B, cost: 1, power: 1"),
            forbidden(YamlFeature::DuplicateKey),
            path().key("cards").index(0).key("fields").key("name"),
        ),
        (
            "second document",
            format!(
                "{}---\nmeta: {{ ruleset: x, ruleset_version: 1 }}\n",
                goblin_yaml("")
            ),
            forbidden(YamlFeature::MultipleDocuments),
            path(),
        ),
    ];
    for (name, yaml, kind, at) in cases {
        let error = yaml_error(&yaml);
        assert_eq!(error.kind, kind, "{name}");
        assert_eq!(error.path, at, "{name}");
    }
}

#[test]
fn card_catalog_rejects_unknown_keys() {
    let unknown = |key: &str| YamlErrorKind::UnknownKey {
        key: key.to_string(),
    };
    let cases = [
        (
            format!("{META}cards: []\nbogus: 1\n"),
            unknown("bogus"),
            path().key("bogus"),
        ),
        (
            "meta: { ruleset: small, ruleset_version: 1, extra: 1 }\ncards: []\n".to_string(),
            unknown("extra"),
            path().key("meta").key("extra"),
        ),
        (
            catalog_yaml("  - { id: g, type: mana, fields: {}, extra: 1 }\n"),
            unknown("extra"),
            path().key("cards").index(0).key("extra"),
        ),
    ];
    for (yaml, kind, at) in cases {
        let error = yaml_error(&yaml);
        assert_eq!(error.kind, kind, "{yaml}");
        assert_eq!(error.path, at, "{yaml}");
    }
}

// ---- the boundary between the loader and the engine

#[test]
fn yaml_and_code_frontends_get_identical_catalog_errors() {
    let yaml = catalog_yaml(
        "  - { id: goblin, type: creature, fields: { name: G, cost: 1, power: 1 } }\n  - { id: goblin, type: mana, fields: {} }\n",
    );
    let from_yaml = catalog_error(&yaml);

    let draft = CardCatalogDraft {
        meta: CatalogMetaDraft {
            ruleset: "small".to_string(),
            ruleset_version: 1,
        },
        cards: vec![
            CardDraft {
                id: "goblin".to_string(),
                card_type: "creature".to_string(),
                fields: vec![
                    ("name".to_string(), text("G")),
                    ("cost".to_string(), int(1)),
                    ("power".to_string(), int(1)),
                ],
                effects: vec![],
            },
            CardDraft {
                id: "goblin".to_string(),
                card_type: "mana".to_string(),
                fields: vec![],
                effects: vec![],
            },
        ],
    };
    assert_eq!(parse_card_catalog_draft(&yaml), Ok(draft.clone()));
    let from_code = CardCatalog::validate(&ruleset(), draft).unwrap_err();
    assert_eq!(from_yaml, from_code);
    assert_eq!(
        from_yaml,
        CatalogError {
            path: path().key("cards").index(1),
            kind: CatalogErrorKind::DuplicateId {
                id: "goblin".to_string(),
                first: path().key("cards").index(0),
            },
        }
    );
}

#[test]
fn loader_does_not_check_the_meaning_of_a_catalog() {
    // Wrong ruleset, unknown type, unknown field: the loader parses them all into a draft.
    let yaml = "meta: { ruleset: other, ruleset_version: 9 }\ncards:\n  - { id: Spell, type: spell, fields: { rarity: common } }\n";
    let draft = parse_card_catalog_draft(yaml).expect("syntactically valid");
    assert_eq!(draft.meta.ruleset, "other");
    assert_eq!(draft.meta.ruleset_version, 9);
    assert_eq!(draft.cards[0].id, "Spell");
    assert_eq!(draft.cards[0].card_type, "spell");
    assert_eq!(
        draft.cards[0].fields,
        vec![("rarity".to_string(), text("common"))]
    );
}
