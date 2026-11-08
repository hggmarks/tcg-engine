//! Acceptance tests for ticket #20: the YAML loader (syntax only) for deck lists, in front of the
//! engine's `DeckList::validate`, and the starter data of the Small Ruleset.

use tcg_engine::card_catalog::{CardCatalog, CardId};
use tcg_engine::deck_list::{
    DeckEntryDraft, DeckError, DeckErrorKind, DeckInputCardsDraft, DeckList, DeckListDraft,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    CardTypeId, DeckInputId, FieldName, FieldValue, ReferenceTarget, RulesetDefinition,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_deck_list_draft,
    DeckLoadError, Location, YamlError, YamlErrorKind, YamlFeature,
};

/// The structure of the Small Ruleset: meta, parameters, zones, card types and inputs.
const FIXTURE: &str = include_str!("fixtures/small-ruleset-structure.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

fn ruleset() -> RulesetDefinition {
    load_ruleset_definition(FIXTURE).expect("the fixture is a valid ruleset")
}

fn catalog() -> CardCatalog {
    load_card_catalog(&ruleset(), SMALL_CATALOG).expect("the small catalog is valid")
}

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

fn yaml_error(yaml: &str) -> YamlError {
    match load_deck_list(&ruleset(), &catalog(), yaml) {
        Err(DeckLoadError::Yaml(error)) => error,
        other => panic!("expected a YAML error, got {other:?}"),
    }
}

fn deck_error(yaml: &str) -> DeckError {
    match load_deck_list(&ruleset(), &catalog(), yaml) {
        Err(DeckLoadError::Deck(error)) => error,
        other => panic!("expected a deck error, got {other:?}"),
    }
}

/// What a loaded deck holds for one input: its entries as (card, count), in order.
fn described(deck: &DeckList, input: &str) -> Vec<(String, u32)> {
    deck.input(&DeckInputId::new(input))
        .expect("the input is in the deck list")
        .entries()
        .iter()
        .map(|entry| (entry.card().as_str().to_string(), entry.count().get()))
        .collect()
}

// ---- the starter data

#[test]
fn small_catalog_and_sample_decks_load() {
    let ruleset = ruleset();
    let catalog = load_card_catalog(&ruleset, SMALL_CATALOG).expect("the small catalog loads");

    // The catalog targets the Small Ruleset and holds 3 Leaders, 16 creatures and the mana card.
    assert_eq!(catalog.meta().ruleset().as_str(), "small");
    assert_eq!(catalog.meta().ruleset_version().get(), 1);
    let count_of = |card_type: &str| {
        catalog
            .cards()
            .iter()
            .filter(|card| card.card_type() == &CardTypeId::new(card_type))
            .count()
    };
    assert_eq!(catalog.cards().len(), 20);
    assert_eq!(count_of("leader"), 3);
    assert_eq!(count_of("creature"), 16);
    assert_eq!(count_of("mana"), 1);

    // One creature and one Leader, with every field.
    let scout = catalog
        .card(&CardId::new("goblin_scout"))
        .expect("goblin_scout is in the catalog");
    assert_eq!(scout.card_type(), &CardTypeId::new("creature"));
    assert_eq!(
        scout.fields(),
        [
            (
                FieldName::new("name"),
                FieldValue::Text("Goblin Scout".to_string())
            ),
            (FieldName::new("cost"), FieldValue::Int(1)),
            (FieldName::new("power"), FieldValue::Int(1)),
        ]
    );
    let ardel = catalog
        .card(&CardId::new("captain_ardel"))
        .expect("captain_ardel is in the catalog");
    assert_eq!(ardel.card_type(), &CardTypeId::new("leader"));
    assert_eq!(
        ardel.fields(),
        [
            (
                FieldName::new("name"),
                FieldValue::Text("Captain Ardel".to_string())
            ),
            (FieldName::new("power"), FieldValue::Int(2)),
            (FieldName::new("life"), FieldValue::Int(5)),
        ]
    );
    assert!(catalog.card(&CardId::new("mana")).is_some());

    // Both sample decks are legal and hold 1 Leader, 50 creatures (at most 4 copies of each) and
    // 10 mana cards.
    let deck_a = load_deck_list(&ruleset, &catalog, DECK_A).expect("deck A is legal");
    let deck_b = load_deck_list(&ruleset, &catalog, DECK_B).expect("deck B is legal");
    for (name, deck, leader, distinct_creatures) in [
        ("deck A", &deck_a, "captain_ardel", 13),
        ("deck B", &deck_b, "warden_ilse", 15),
    ] {
        assert_eq!(
            described(deck, "leader"),
            vec![(leader.to_string(), 1)],
            "{name}"
        );
        assert_eq!(
            described(deck, "mana_deck"),
            vec![("mana".to_string(), 10)],
            "{name}"
        );
        let main = described(deck, "main_deck");
        assert_eq!(main.len(), distinct_creatures, "{name}");
        assert_eq!(
            main.iter().map(|(_, count)| count).sum::<u32>(),
            50,
            "{name}"
        );
        assert_eq!(
            main.iter().map(|(_, count)| *count).max(),
            Some(4),
            "{name}"
        );
    }
    assert_ne!(deck_a, deck_b);
}

// ---- parsing

#[test]
fn deck_list_draft_keeps_file_order() {
    let yaml = "inputs:\n  - input: mana_deck\n    cards:\n      - { card: mana, count: 10 }\n  - input: leader\n    cards:\n      - { card: zed, count: 1 }\n      - { card: abe, count: 0 }\n";
    assert_eq!(
        parse_deck_list_draft(yaml),
        Ok(DeckListDraft {
            inputs: vec![
                DeckInputCardsDraft {
                    input: "mana_deck".to_string(),
                    cards: vec![DeckEntryDraft {
                        card: "mana".to_string(),
                        count: 10,
                    }],
                },
                DeckInputCardsDraft {
                    input: "leader".to_string(),
                    cards: vec![
                        DeckEntryDraft {
                            card: "zed".to_string(),
                            count: 1,
                        },
                        DeckEntryDraft {
                            card: "abe".to_string(),
                            count: 0,
                        },
                    ],
                },
            ],
        })
    );
}

#[test]
fn deck_list_loads_in_ruleset_order() {
    // The inputs of deck A in the reverse order give the same deck list as the file.
    let mut draft = parse_deck_list_draft(DECK_A).expect("deck A parses");
    draft.inputs.reverse();
    let reversed = DeckList::validate(&ruleset(), &catalog(), draft).expect("legal deck");
    let from_file = load_deck_list(&ruleset(), &catalog(), DECK_A).expect("deck A is legal");
    assert_eq!(reversed, from_file);

    let inputs: Vec<&str> = from_file
        .inputs()
        .iter()
        .map(|input| input.input().as_str())
        .collect();
    assert_eq!(inputs, vec!["leader", "main_deck", "mana_deck"]);
}

#[test]
fn count_must_be_a_non_negative_whole_number() {
    for rejected in ["-1", "1.5", "~"] {
        let yaml = format!(
            "inputs:\n  - input: leader\n    cards:\n      - {{ card: captain_ardel, count: {rejected} }}\n"
        );
        let error = yaml_error(&yaml);
        assert_eq!(
            error.path,
            path()
                .key("inputs")
                .index(0)
                .key("cards")
                .index(0)
                .key("count"),
            "count: {rejected}"
        );
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "count: {rejected} gave {:?}",
            error.kind
        );
    }
}

// ---- the restricted YAML subset

#[test]
fn deck_list_rejects_anchor() {
    let yaml = "inputs:\n  - input: &i leader\n    cards: []\n";
    let error = yaml_error(yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::ForbiddenFeature {
            feature: YamlFeature::Anchor
        }
    );
    assert_eq!(error.path, path().key("inputs").index(0).key("input"));
    // Line and column are 1-based and point at the anchored node.
    assert_eq!(
        error.location,
        Some(Location {
            line: 2,
            column: 15
        })
    );
}

#[test]
fn deck_list_rejects_the_other_forbidden_yaml_features() {
    let forbidden = |feature: YamlFeature| YamlErrorKind::ForbiddenFeature { feature };
    let cases = [
        (
            "alias",
            "inputs:\n  - input: *i\n    cards: []\n".to_string(),
            forbidden(YamlFeature::Alias),
            path().key("inputs").index(0).key("input"),
        ),
        (
            "tag",
            "inputs:\n  - input: !!str leader\n    cards: []\n".to_string(),
            forbidden(YamlFeature::Tag),
            path().key("inputs").index(0).key("input"),
        ),
        (
            "duplicate key",
            "inputs:\n  - input: leader\n    input: mana_deck\n    cards: []\n".to_string(),
            forbidden(YamlFeature::DuplicateKey),
            path().key("inputs").index(0).key("input"),
        ),
        (
            "second document",
            "inputs: []\n---\ninputs: []\n".to_string(),
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
fn deck_list_rejects_unknown_keys() {
    let unknown = |key: &str| YamlErrorKind::UnknownKey {
        key: key.to_string(),
    };
    let cases = [
        (
            "inputs: []\nbogus: 1\n",
            unknown("bogus"),
            path().key("bogus"),
        ),
        (
            "inputs:\n  - { input: leader, cards: [], extra: 1 }\n",
            unknown("extra"),
            path().key("inputs").index(0).key("extra"),
        ),
        (
            "inputs:\n  - { input: leader, cards: [{ card: captain_ardel, count: 1, extra: 1 }] }\n",
            unknown("extra"),
            path()
                .key("inputs")
                .index(0)
                .key("cards")
                .index(0)
                .key("extra"),
        ),
    ];
    for (yaml, kind, at) in cases {
        let error = yaml_error(yaml);
        assert_eq!(error.kind, kind, "{yaml}");
        assert_eq!(error.path, at, "{yaml}");
    }
}

// ---- the boundary between the loader and the engine

#[test]
fn yaml_and_code_frontends_get_identical_deck_errors() {
    let yaml = "inputs:\n  - input: leader\n    cards:\n      - { card: captain_ardel, count: 1 }\n  - input: main_deck\n    cards:\n      - { card: goblin_scout, count: 4 }\n      - { card: ghost, count: 1 }\n";
    let from_yaml = deck_error(yaml);

    let draft = DeckListDraft {
        inputs: vec![
            DeckInputCardsDraft {
                input: "leader".to_string(),
                cards: vec![DeckEntryDraft {
                    card: "captain_ardel".to_string(),
                    count: 1,
                }],
            },
            DeckInputCardsDraft {
                input: "main_deck".to_string(),
                cards: vec![
                    DeckEntryDraft {
                        card: "goblin_scout".to_string(),
                        count: 4,
                    },
                    DeckEntryDraft {
                        card: "ghost".to_string(),
                        count: 1,
                    },
                ],
            },
        ],
    };
    assert_eq!(parse_deck_list_draft(yaml), Ok(draft.clone()));
    let from_code = DeckList::validate(&ruleset(), &catalog(), draft).unwrap_err();
    assert_eq!(from_yaml, from_code);
    assert_eq!(
        from_yaml,
        DeckError {
            path: path()
                .key("inputs")
                .index(1)
                .key("cards")
                .index(1)
                .key("card"),
            kind: DeckErrorKind::DanglingReference {
                target: ReferenceTarget::Card,
                id: "ghost".to_string(),
            },
        }
    );
}

#[test]
fn loader_does_not_check_the_meaning_of_a_deck_list() {
    // A zero count, an unknown input and an unknown card all parse into a draft.
    let yaml = "inputs:\n  - { input: sideboard, cards: [{ card: ghost, count: 0 }] }\n";
    let draft = parse_deck_list_draft(yaml).expect("syntactically valid");
    assert_eq!(draft.inputs[0].input, "sideboard");
    assert_eq!(draft.inputs[0].cards[0].card, "ghost");
    assert_eq!(draft.inputs[0].cards[0].count, 0);

    // The engine then rejects the zero count, the first problem of the entry.
    assert_eq!(
        deck_error("inputs:\n  - { input: leader, cards: [{ card: ghost, count: 0 }] }\n"),
        DeckError {
            path: path()
                .key("inputs")
                .index(0)
                .key("cards")
                .index(0)
                .key("count"),
            kind: DeckErrorKind::ZeroCount,
        }
    );
}
