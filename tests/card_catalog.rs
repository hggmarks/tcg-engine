//! Acceptance tests for ticket #20: the typed `CardCatalog` and its validation against an already
//! loaded `RulesetDefinition` (stage 2 of ADR-0004's two-stage loading).
//!
//! Drafts are built in Rust, the way a code frontend would build them. Every error test asserts the
//! whole `CatalogError`: kind and path.

mod common;

use common::{card, catalog_meta, creature_id, path, small_catalog, small_definition, small_draft};
use tcg_engine::card_catalog::{
    CardCatalog, CardCatalogDraft, CardDraft, CardId, CatalogError, CatalogErrorKind, EffectDraft,
};
use tcg_engine::ruleset_definition::{
    CardTypeId, FieldName, FieldType, FieldValue, ReferenceTarget, RulesetDefinition, RulesetId,
};

fn int(value: i64) -> FieldValue {
    FieldValue::Int(value)
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

fn goblin_fields() -> Vec<(&'static str, FieldValue)> {
    vec![
        ("name", text("Goblin")),
        ("cost", int(1)),
        ("power", int(2)),
    ]
}

fn goblin() -> CardDraft {
    card("goblin", "creature", goblin_fields())
}

fn draft_with(cards: Vec<CardDraft>) -> CardCatalogDraft {
    CardCatalogDraft {
        meta: catalog_meta("small", 1),
        cards,
    }
}

fn error_of(ruleset: &RulesetDefinition, draft: CardCatalogDraft) -> CatalogError {
    match CardCatalog::validate(ruleset, draft) {
        Err(error) => error,
        Ok(catalog) => panic!("expected a validation error, got {catalog:?}"),
    }
}

/// The error for a draft holding only the given cards, validated against the small ruleset.
fn small_error_of(cards: Vec<CardDraft>) -> CatalogError {
    error_of(&small_definition(), draft_with(cards))
}

fn card_path(id: &str) -> tcg_engine::definition_path::DefinitionPath {
    path().key("cards").id(id)
}

// ---- the model

#[test]
fn card_catalog_accepts_the_small_catalog() {
    let catalog = small_catalog();

    assert_eq!(catalog.meta().ruleset(), &RulesetId::new("small"));
    assert_eq!(catalog.meta().ruleset_version().get(), 1);

    // 2 Leaders, 13 creatures and the mana card, in file order.
    let ids: Vec<&str> = catalog.cards().iter().map(|c| c.id().as_str()).collect();
    let mut expected = vec!["hero".to_string(), "second_hero".to_string()];
    expected.extend((1..=13).map(creature_id));
    expected.push("mana".to_string());
    assert_eq!(ids, expected);

    let hero = catalog.card(&CardId::new("hero")).expect("hero is listed");
    assert_eq!(hero.id(), &CardId::new("hero"));
    assert_eq!(hero.card_type(), &CardTypeId::new("leader"));

    // A card type without fields needs no fields.
    let mana = catalog.card(&CardId::new("mana")).expect("mana is listed");
    assert_eq!(mana.card_type(), &CardTypeId::new("mana"));
    assert!(mana.fields().is_empty());

    assert!(catalog.card(&CardId::new("ghost")).is_none());
}

#[test]
fn card_catalog_keeps_field_values_in_schema_order() {
    // The creature schema is name, cost, power; the card gives power, name, cost.
    let draft = draft_with(vec![card(
        "goblin",
        "creature",
        vec![
            ("power", int(2)),
            ("name", text("Goblin")),
            ("cost", int(1)),
        ],
    )]);
    let catalog = CardCatalog::validate(&small_definition(), draft).expect("valid catalog");

    let goblin = catalog
        .card(&CardId::new("goblin"))
        .expect("goblin is listed");
    assert_eq!(goblin.id(), &CardId::new("goblin"));
    assert_eq!(goblin.card_type(), &CardTypeId::new("creature"));
    assert_eq!(
        goblin.fields(),
        [
            (FieldName::new("name"), text("Goblin")),
            (FieldName::new("cost"), int(1)),
            (FieldName::new("power"), int(2)),
        ]
    );
    assert_eq!(goblin.field(&FieldName::new("cost")), Some(&int(1)));
    assert_eq!(goblin.field(&FieldName::new("life")), None);
}

#[test]
fn card_catalog_keeps_cards_in_file_order() {
    let draft = draft_with(vec![
        card("zebra", "creature", goblin_fields()),
        card("ant", "creature", goblin_fields()),
        card("mana", "mana", vec![]),
    ]);
    let catalog = CardCatalog::validate(&small_definition(), draft).expect("valid catalog");
    let ids: Vec<&str> = catalog.cards().iter().map(|c| c.id().as_str()).collect();
    assert_eq!(ids, vec!["zebra", "ant", "mana"]);
}

// ---- meta

#[test]
fn card_catalog_rejects_other_ruleset() {
    let definition = small_definition();

    let mut draft = draft_with(vec![goblin()]);
    draft.meta = catalog_meta("other", 1);
    assert_eq!(
        error_of(&definition, draft),
        CatalogError {
            path: path().key("meta").key("ruleset"),
            kind: CatalogErrorKind::RulesetIdMismatch {
                expected: RulesetId::new("small"),
                found: "other".to_string(),
            },
        }
    );

    let mut draft = draft_with(vec![goblin()]);
    draft.meta = catalog_meta("small", 2);
    assert_eq!(
        error_of(&definition, draft),
        CatalogError {
            path: path().key("meta").key("ruleset_version"),
            kind: CatalogErrorKind::RulesetVersionMismatch {
                expected: definition.meta().version(),
                found: 2,
            },
        }
    );

    // The id is checked before the version, and the meta before the cards.
    let mut draft = draft_with(vec![card("Bad", "ghost", vec![])]);
    draft.meta = catalog_meta("other", 2);
    assert_eq!(
        error_of(&definition, draft).kind,
        CatalogErrorKind::RulesetIdMismatch {
            expected: RulesetId::new("small"),
            found: "other".to_string(),
        }
    );
}

// ---- card ids and types

#[test]
fn card_catalog_rejects_invalid_card_id() {
    for bad in ["", "3", "Goblin", "a]b", "a-b"] {
        assert_eq!(
            small_error_of(vec![card(bad, "creature", goblin_fields())]),
            CatalogError {
                path: path().key("cards").index(0).key("id"),
                kind: CatalogErrorKind::InvalidId {
                    id: bad.to_string()
                },
            },
            "id {bad:?}"
        );
    }

    // The id is checked before anything else about the card, and the position is that in the list.
    assert_eq!(
        small_error_of(vec![goblin(), card("Bad", "ghost", vec![])]),
        CatalogError {
            path: path().key("cards").index(1).key("id"),
            kind: CatalogErrorKind::InvalidId {
                id: "Bad".to_string()
            },
        }
    );
}

#[test]
fn card_catalog_rejects_duplicate_card_id() {
    // The repeat is rejected before its type is looked at.
    assert_eq!(
        small_error_of(vec![
            goblin(),
            card("mana", "mana", vec![]),
            card("goblin", "ghost", vec![]),
        ]),
        CatalogError {
            path: path().key("cards").index(2),
            kind: CatalogErrorKind::DuplicateId {
                id: "goblin".to_string(),
                first: path().key("cards").index(0),
            },
        }
    );
}

#[test]
fn card_catalog_rejects_unknown_card_type() {
    assert_eq!(
        small_error_of(vec![card("fireball", "spell", vec![("cost", int(3))])]),
        CatalogError {
            path: card_path("fireball").key("type"),
            kind: CatalogErrorKind::DanglingReference {
                target: ReferenceTarget::CardType,
                id: "spell".to_string(),
            },
        }
    );
}

// ---- fields

#[test]
fn card_catalog_rejects_missing_field() {
    // Only `name` is given: the first missing field in schema order is `cost`.
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![("name", text("Goblin"))]
        )]),
        CatalogError {
            path: card_path("goblin").key("fields"),
            kind: CatalogErrorKind::MissingField {
                field: FieldName::new("cost"),
            },
        }
    );

    // A card type with fields cannot leave them all out.
    assert_eq!(
        small_error_of(vec![card("hero", "leader", vec![])]),
        CatalogError {
            path: card_path("hero").key("fields"),
            kind: CatalogErrorKind::MissingField {
                field: FieldName::new("name"),
            },
        }
    );
}

#[test]
fn card_catalog_rejects_unknown_field() {
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![
                ("name", text("Goblin")),
                ("cost", int(1)),
                ("power", int(2)),
                ("rarity", text("common")),
            ]
        )]),
        CatalogError {
            path: card_path("goblin").key("fields").id("rarity"),
            kind: CatalogErrorKind::UnknownField {
                field: "rarity".to_string(),
            },
        }
    );

    // The fields given are checked in file order before the missing ones: here `cost` and `power`
    // are missing, yet the unknown field is reported.
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![("name", text("Goblin")), ("rarity", text("common"))]
        )])
        .kind,
        CatalogErrorKind::UnknownField {
            field: "rarity".to_string(),
        }
    );

    // A field of another card type is unknown here.
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![
                ("name", text("Goblin")),
                ("cost", int(1)),
                ("power", int(2)),
                ("life", int(5)),
            ]
        )])
        .path,
        card_path("goblin").key("fields").id("life")
    );
}

#[test]
fn card_catalog_rejects_field_type_mismatch() {
    let mismatch = |declared: FieldType, found: FieldType| CatalogErrorKind::FieldTypeMismatch {
        declared,
        found,
    };

    // A text where an int is declared.
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![
                ("name", text("Goblin")),
                ("cost", int(1)),
                ("power", text("5")),
            ]
        )]),
        CatalogError {
            path: card_path("goblin").key("fields").id("power"),
            kind: mismatch(FieldType::Int, FieldType::Text),
        }
    );

    // An int where a text is declared.
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![("name", int(3)), ("cost", int(1)), ("power", int(2))]
        )]),
        CatalogError {
            path: card_path("goblin").key("fields").id("name"),
            kind: mismatch(FieldType::Text, FieldType::Int),
        }
    );

    // A bool where an int is declared.
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![
                ("name", text("Goblin")),
                ("cost", FieldValue::Bool(true)),
                ("power", int(2)),
            ]
        )])
        .kind,
        mismatch(FieldType::Int, FieldType::Bool)
    );
}

#[test]
fn card_catalog_rejects_duplicate_field() {
    // The second `name` is rejected as a repeat, before its type is compared with the schema, and
    // is never merged with the first.
    assert_eq!(
        small_error_of(vec![card(
            "goblin",
            "creature",
            vec![
                ("name", text("Goblin")),
                ("cost", int(1)),
                ("name", int(7)),
                ("power", int(2)),
            ]
        )]),
        CatalogError {
            path: card_path("goblin").key("fields").index(2),
            kind: CatalogErrorKind::DuplicateField {
                field: "name".to_string(),
                first: card_path("goblin").key("fields").index(0),
            },
        }
    );
}

// ---- effects

#[test]
fn card_catalog_rejects_effects_on_type_without_effects() {
    let mut card = goblin();
    card.effects = vec![EffectDraft {
        primitive: "draw".to_string(),
    }];
    assert_eq!(
        small_error_of(vec![card]),
        CatalogError {
            path: card_path("goblin").key("effects"),
            kind: CatalogErrorKind::EffectsNotAllowed,
        }
    );

    // An empty effect list is the same as none.
    let mut card = goblin();
    card.effects = vec![];
    assert!(CardCatalog::validate(&small_definition(), draft_with(vec![card])).is_ok());
}

#[test]
fn card_catalog_rejects_unsupported_effect() {
    // A ruleset whose creatures may have effects. No effect primitive exists yet, so every entry is
    // unsupported.
    let mut ruleset = small_draft();
    ruleset.card_types[1].effects = true;
    let definition = RulesetDefinition::validate(ruleset).expect("valid ruleset");

    let mut with_effects = goblin();
    with_effects.effects = vec![
        EffectDraft {
            primitive: "draw".to_string(),
        },
        EffectDraft {
            primitive: "damage".to_string(),
        },
    ];
    assert_eq!(
        error_of(&definition, draft_with(vec![with_effects])),
        CatalogError {
            path: card_path("goblin").key("effects").index(0),
            kind: CatalogErrorKind::UnsupportedEffect {
                primitive: "draw".to_string(),
            },
        }
    );

    // No effects, on a type that allows them, is valid.
    assert!(CardCatalog::validate(&definition, draft_with(vec![goblin()])).is_ok());

    // The fields of a card are checked before its effects.
    let mut leader = card("hero", "leader", vec![]);
    leader.effects = vec![EffectDraft {
        primitive: "draw".to_string(),
    }];
    assert_eq!(
        error_of(&definition, draft_with(vec![leader])).kind,
        CatalogErrorKind::MissingField {
            field: FieldName::new("name"),
        }
    );
}

// ---- first error wins

#[test]
fn card_catalog_reports_the_first_error_in_file_order() {
    let mut bad_effects = goblin();
    bad_effects.id = "later".to_string();
    bad_effects.effects = vec![EffectDraft {
        primitive: "draw".to_string(),
    }];
    assert_eq!(
        small_error_of(vec![
            card("first", "creature", vec![("name", text("First"))]),
            card("second", "spell", vec![]),
            bad_effects,
        ])
        .path,
        card_path("first").key("fields")
    );
}
