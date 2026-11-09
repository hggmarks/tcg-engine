//! Fixtures shared by the catalog and deck list tests: the Small Ruleset structure and a small
//! card catalog for it, both built in Rust the way a code frontend would build them.

use tcg_engine::card_catalog::{CardCatalog, CardCatalogDraft, CardDraft, CatalogMetaDraft};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    CardTypeDraft, DeckInputDraft, FieldDraft, FieldTypeDraft, FieldValue, InitializerDraft,
    InstanceStateDraft, MetaDraft, ParametersDraft, RulesetDefinition, RulesetDraft,
    SetupStepDraft, Visibility, ZoneDraft,
};

pub fn path() -> DefinitionPath {
    DefinitionPath::root()
}

fn int(value: i64) -> FieldValue {
    FieldValue::Int(value)
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

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

fn tapped() -> InstanceStateDraft {
    InstanceStateDraft {
        name: "tapped".to_string(),
        state_type: FieldTypeDraft::Bool,
        init: InitializerDraft::Literal(FieldValue::Bool(false)),
    }
}

fn card_type(id: &str, fields: Vec<FieldDraft>) -> CardTypeDraft {
    CardTypeDraft {
        id: id.to_string(),
        fields,
        instance_state: vec![tapped()],
        effects: false,
    }
}

fn input(
    id: &str,
    card_type: &str,
    size: u32,
    max_copies: Option<u32>,
    zone: &str,
) -> DeckInputDraft {
    DeckInputDraft {
        id: id.to_string(),
        card_types: vec![card_type.to_string()],
        size,
        max_copies,
        zone: zone.to_string(),
    }
}

/// The Small Ruleset (id `small`, version 1): its structure and its setup, without instance-state
/// detail.
pub fn small_draft() -> RulesetDraft {
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
            ),
            card_type(
                "creature",
                vec![
                    field("name", FieldTypeDraft::Text),
                    field("cost", FieldTypeDraft::Int),
                    field("power", FieldTypeDraft::Int),
                ],
            ),
            card_type("mana", vec![]),
        ],
        inputs: vec![
            input("leader", "leader", 1, None, "leader_zone"),
            input("main_deck", "creature", 50, Some(4), "deck"),
            input("mana_deck", "mana", 10, None, "mana_deck"),
        ],
        setup: vec![
            SetupStepDraft::Shuffle {
                player: "each".to_string(),
                zone: "deck".to_string(),
            },
            SetupStepDraft::Draw {
                player: "each".to_string(),
                from: "deck".to_string(),
                to: "hand".to_string(),
                count: 5,
            },
        ],
        actions: vec![],
        turn: None,
    }
}

pub fn small_definition() -> RulesetDefinition {
    RulesetDefinition::validate(small_draft()).expect("the small ruleset draft is valid")
}

pub fn card(id: &str, card_type: &str, fields: Vec<(&str, FieldValue)>) -> CardDraft {
    CardDraft {
        id: id.to_string(),
        card_type: card_type.to_string(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect(),
        effects: vec![],
    }
}

/// The id of the n-th creature of the test catalog: `creature_01` to `creature_13`.
pub fn creature_id(n: i64) -> String {
    format!("creature_{n:02}")
}

pub fn catalog_meta(ruleset: &str, ruleset_version: u32) -> CatalogMetaDraft {
    CatalogMetaDraft {
        ruleset: ruleset.to_string(),
        ruleset_version,
    }
}

/// Two Leaders (`hero`, `second_hero`), 13 creatures and the mana card, in that file order.
pub fn small_catalog_draft() -> CardCatalogDraft {
    let mut cards = vec![
        card(
            "hero",
            "leader",
            vec![("name", text("Hero")), ("power", int(3)), ("life", int(20))],
        ),
        card(
            "second_hero",
            "leader",
            vec![
                ("name", text("Second Hero")),
                ("power", int(2)),
                ("life", int(25)),
            ],
        ),
    ];
    for n in 1..=13 {
        cards.push(card(
            &creature_id(n),
            "creature",
            vec![
                ("name", text(&format!("Creature {n}"))),
                ("cost", int(n % 5 + 1)),
                ("power", int(n)),
            ],
        ));
    }
    cards.push(card("mana", "mana", vec![]));
    CardCatalogDraft {
        meta: catalog_meta("small", 1),
        cards,
    }
}

pub fn small_catalog() -> CardCatalog {
    CardCatalog::validate(&small_definition(), small_catalog_draft())
        .expect("the small catalog draft is valid")
}
