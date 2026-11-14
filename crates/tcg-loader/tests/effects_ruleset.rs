//! Acceptance tests for ticket #49: the Effects Ruleset (derived from the Small Ruleset, which
//! freezes) and the loading of the Card Effect envelope (ADR-0022).
//!
//! The Effects Ruleset, its catalog and its two sample decks are loaded from the real files. Its
//! Golden Game is the Small Ruleset's Leader-kill Golden Game (same Seed, same decks, same Actions)
//! and must play identically in rules: the Event Traces are equal, and the Golden Snapshot is the
//! only thing that differs, by its name. The snapshot changes only by the ignored test
//! `regenerate_golden_snapshot`, never by a normal run (ADR-0017).

use tcg_engine::card_catalog::{
    CardEffectDraft, CatalogError, CatalogErrorKind, ModifyDraft, TriggerMatcherDraft,
};
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameState,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::Ruleset;
use tcg_engine::ruleset_definition::{
    ActionId, EffectDraft, ExpressionDraft, FieldValue, RefDraft, RulesetDefinition,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_card_catalog_draft,
    CatalogLoadError, YamlError, YamlErrorKind,
};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const SMALL_DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const SMALL_DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");
const EFFECTS_DECK_A: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-a.yaml");
const EFFECTS_DECK_B: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-b.yaml");

const LEADER_KILL_SNAPSHOT: &str = include_str!("golden/effects-leader-kill.trace");
const LEADER_KILL_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/effects-leader-kill.trace"
);
const REGENERATE_COMMAND: &str =
    "cargo test -p tcg-loader --test effects_ruleset -- --ignored regenerate_golden_snapshot";

/// The Seed and the cards of the Small Ruleset's Leader-kill Golden Game (see
/// `small_ruleset_end_to_end.rs`): the decks and the instance ids are the same here.
const LEADER_KILL_SEED: u64 = 9;
const LEADER_1: CardInstanceId = CardInstanceId(61);
const LEADER_0: CardInstanceId = CardInstanceId(0);
const GOBLIN_0: CardInstanceId = CardInstanceId(1);
const MOSS_0: CardInstanceId = CardInstanceId(7);
const OTTER_0: CardInstanceId = CardInstanceId(13);
const HAWK_0: CardInstanceId = CardInstanceId(31);
const BOAR_0: CardInstanceId = CardInstanceId(35);
const MOSS_1: CardInstanceId = CardInstanceId(63);
const OTTER_1: CardInstanceId = CardInstanceId(72);
const GOLEM_1: CardInstanceId = CardInstanceId(75);
const CAT_1: CardInstanceId = CardInstanceId(80);
const GUARD_1: CardInstanceId = CardInstanceId(85);

// ---- helpers

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

fn effects_definition() -> RulesetDefinition {
    load_ruleset_definition(EFFECTS_RULESET).expect("the Effects Ruleset loads")
}

fn small() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn effects() -> DeclarativeRuleset {
    let definition = effects_definition();
    let catalog = load_card_catalog(&definition, EFFECTS_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn decks(ruleset: &DeclarativeRuleset, yamls: [&str; 2]) -> Vec<DeckList> {
    yamls
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action {
        player: PlayerId(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn play(player: u32, card: CardInstanceId) -> Action {
    action(player, "play_creature", &[card])
}

fn pass(player: u32) -> Action {
    action(player, "pass_turn", &[])
}

fn attack(player: u32, attacker: CardInstanceId, target: CardInstanceId) -> Action {
    action(player, "attack", &[attacker, target])
}

/// Seat 0 races seat 1's Leader (life 6) and seat 1 answers: the Small Ruleset's Leader-kill
/// script, literally.
fn leader_kill_script() -> Vec<Action> {
    vec![
        play(0, MOSS_0),
        play(0, GOBLIN_0),
        pass(0),
        play(1, OTTER_1),
        pass(1),
        attack(0, MOSS_0, LEADER_1),
        attack(0, GOBLIN_0, LEADER_1),
        play(0, OTTER_0),
        pass(0),
        attack(1, OTTER_1, MOSS_0),
        play(1, GOLEM_1),
        play(1, MOSS_1),
        pass(1),
        attack(0, GOBLIN_0, OTTER_1),
        attack(0, OTTER_0, LEADER_1),
        play(0, BOAR_0),
        play(0, HAWK_0),
        pass(0),
        attack(1, OTTER_1, OTTER_0),
        attack(1, GOLEM_1, LEADER_0),
        attack(1, MOSS_1, LEADER_0),
        play(1, CAT_1),
        play(1, GUARD_1),
        pass(1),
        attack(0, GOBLIN_0, LEADER_1),
        attack(0, BOAR_0, LEADER_1),
        attack(0, HAWK_0, LEADER_1),
    ]
}

/// A Golden Game as it was played: the Event Trace of Game creation, each accepted Action with its
/// Event Trace, and the final GameState.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Played {
    name: &'static str,
    creation: Vec<Event>,
    steps: Vec<(Action, Vec<Event>)>,
    final_state: GameState,
}

/// Plays the literal script. Every Action must be accepted.
fn play_game(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    name: &'static str,
    script: &[Action],
) -> Played {
    let created = ruleset
        .create_game(decks, Seed(LEADER_KILL_SEED))
        .expect("the sample decks create a game");
    let creation = created.opening.trace.events().to_vec();
    let mut state = created.opening.state;
    let mut steps = Vec::new();
    for (index, action) in script.iter().enumerate() {
        let transition = ruleset
            .transition(&state, action)
            .unwrap_or_else(|rejection| {
                panic!(
                    "action {} of {name} ({action:?}) is rejected: {rejection:?}",
                    index + 1
                )
            });
        steps.push((action.clone(), transition.trace.events().to_vec()));
        state = transition.state;
    }
    Played {
        name,
        creation,
        steps,
        final_state: state,
    }
}

fn play_effects_leader_kill() -> Played {
    let ruleset = effects();
    let decks = decks(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B]);
    play_game(
        &ruleset,
        &decks,
        "effects-leader-kill",
        &leader_kill_script(),
    )
}

fn play_small_leader_kill() -> Played {
    let ruleset = small();
    let decks = decks(&ruleset, [SMALL_DECK_A, SMALL_DECK_B]);
    play_game(&ruleset, &decks, "small-leader-kill", &leader_kill_script())
}

/// The text of a Golden Snapshot (ADR-0017), in the format of the Small Ruleset's snapshots.
fn render(played: &Played) -> String {
    let mut text = format!(
        "# {} | seed {LEADER_KILL_SEED} | seat 0 deck-a, seat 1 deck-b\n== creation\n",
        played.name
    );
    for event in &played.creation {
        text.push_str(&format!("{event:?}\n"));
    }
    for (index, (action, events)) in played.steps.iter().enumerate() {
        let cards: Vec<String> = action
            .arguments
            .iter()
            .map(|argument| match argument {
                Argument::Card(card) => card.0.to_string(),
            })
            .collect();
        text.push_str(&format!(
            "== action {}: player {} {} [{}]\n",
            index + 1,
            action.player.0,
            action.action.as_str(),
            cards.join(", ")
        ));
        for event in events {
            text.push_str(&format!("{event:?}\n"));
        }
    }
    let outcome = played
        .final_state
        .outcome()
        .expect("a Golden Game ends the Game");
    text.push_str(&format!("== outcome\n{outcome:?}\n"));
    text
}

fn yaml_error(yaml: &str) -> YamlError {
    match parse_card_catalog_draft(yaml) {
        Err(error) => error,
        Ok(draft) => panic!("expected a YAML error, got {draft:?}"),
    }
}

/// A catalog for the Effects Ruleset with one creature, `sage`, whose `effects` list is the given
/// text (the items of the list, each starting with `      - `).
fn sage_yaml(effects: &str) -> String {
    format!(
        "meta: {{ ruleset: effects, ruleset_version: 1 }}\ncards:\n  - id: sage\n    type: creature\n    fields: {{ name: Sage, cost: 1, power: 1 }}\n    effects:\n{effects}"
    )
}

fn catalog_error(yaml: &str) -> CatalogError {
    match load_card_catalog(&effects_definition(), yaml) {
        Err(CatalogLoadError::Catalog(error)) => error,
        other => panic!("expected a catalog error, got {other:?}"),
    }
}

/// `cards[sage].effects[0]`: where the engine reports an error (declarations are addressed by id).
fn sage_effect_path() -> DefinitionPath {
    path().key("cards").id("sage").key("effects").index(0)
}

/// `cards[0].effects[0]`: where the loader reports a syntax error (positional, ADR-0008).
fn sage_syntax_path() -> DefinitionPath {
    path().key("cards").index(0).key("effects").index(0)
}

// ---- the Effects Ruleset

#[test]
fn effects_ruleset_loads_and_creates_a_game() {
    let ruleset = effects();
    let decks = decks(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B]);
    let created = ruleset
        .create_game(&decks, Seed(1))
        .expect("the sample decks create a game");
    // 1 Leader, 50 creatures and 10 mana cards for each of 2 seats.
    assert_eq!(created.opening.state.cards().len(), 122);

    // The Effects Ruleset is its own ruleset, and its catalog says so.
    let definition = ruleset.definition();
    assert_eq!(definition.meta().id().as_str(), "effects");
    assert_eq!(definition.meta().version().get(), 1);
    assert_eq!(ruleset.catalog().meta().ruleset().as_str(), "effects");

    // The one difference from the Small Ruleset: creatures may carry Card Effects. The catalog
    // carries two, both continuous (ticket #51).
    let allows: Vec<(&str, bool)> = definition
        .card_types()
        .iter()
        .map(|card_type| (card_type.id().as_str(), card_type.allows_effects()))
        .collect();
    assert_eq!(
        allows,
        [("leader", false), ("creature", true), ("mana", false)]
    );
    let carriers: Vec<&str> = ruleset
        .catalog()
        .cards()
        .iter()
        .filter(|card| !card.effects().is_empty())
        .map(|card| card.id().as_str())
        .collect();
    assert_eq!(carriers, ["battle_veteran", "weary_ox"]);
}

#[test]
fn golden_effects_leader_kill() {
    let played = play_effects_leader_kill();
    assert_eq!(played.steps.len(), leader_kill_script().len());
    let actual = render(&played);
    if actual != LEADER_KILL_SNAPSHOT {
        let mut expected_lines = LEADER_KILL_SNAPSHOT.lines();
        let mut actual_lines = actual.lines();
        for line_number in 1.. {
            let (expected, found) = (expected_lines.next(), actual_lines.next());
            assert!(
                expected == found,
                "the Golden Game effects-leader-kill differs from its snapshot at line \
                 {line_number}\n  expected: {}\n  actual:   {}\nIf the change is deliberate, \
                 rewrite the snapshot with:\n  {REGENERATE_COMMAND}",
                expected.unwrap_or("<end of the snapshot>"),
                found.unwrap_or("<end of the trace>"),
            );
            if expected.is_none() {
                break;
            }
        }
        panic!(
            "the snapshot differs in its line endings; rewrite it with:\n  {REGENERATE_COMMAND}"
        );
    }
    assert!(LEADER_KILL_SNAPSHOT.ends_with('\n') && !LEADER_KILL_SNAPSHOT.ends_with("\n\n"));
}

#[test]
fn effects_ruleset_golden_game_equals_small_ruleset() {
    let small = play_small_leader_kill();
    let effects = play_effects_leader_kill();
    assert!(!small.creation.is_empty());
    assert_eq!(effects.creation, small.creation, "creation traces");
    assert_eq!(effects.steps.len(), small.steps.len());
    for (index, (one, other)) in effects.steps.iter().zip(&small.steps).enumerate() {
        assert_eq!(one, other, "action {}", index + 1);
    }
    let outcome = effects.final_state.outcome().expect("the Game ended");
    assert_eq!(Some(outcome), small.final_state.outcome());
    assert_eq!(outcome.winners(), [PlayerId(0)]);
    // The whole final GameState is equal too: nothing in it names the ruleset.
    assert_eq!(effects.final_state, small.final_state);
}

/// Rewrites the Golden Snapshot. Run it on purpose, and read the diff before committing it.
#[test]
#[ignore = "deliberate update: rewrites the golden snapshot"]
fn regenerate_golden_snapshot() {
    let played = play_effects_leader_kill();
    std::fs::write(LEADER_KILL_PATH, render(&played)).expect("the snapshot is written");
}

// ---- the envelope, in the loader

#[test]
fn card_effect_unknown_envelope_key_is_a_syntax_error() {
    let yaml = sage_yaml("      - { on: { enter: field }, do: [ { end_turn: {} } ], bogus: 1 }\n");
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "bogus".to_string()
        }
    );
    assert_eq!(error.path, sage_syntax_path().key("bogus"));

    // The same for a continuous element, and the key is the one the format does not know.
    let yaml =
        sage_yaml("      - { while: true, modify: { field: power, amount: 1 }, until: true }\n");
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "until".to_string()
        }
    );
    assert_eq!(error.path, sage_syntax_path().key("until"));

    // An unknown key inside `modify` is unknown too.
    let yaml = sage_yaml("      - { while: true, modify: { field: power, amount: 1, by: 2 } }\n");
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "by".to_string()
        }
    );
    assert_eq!(error.path, sage_syntax_path().key("modify").key("by"));
}

#[test]
fn unknown_primitive_or_operator_in_a_card_effect_is_an_unknown_key() {
    // The action effects and expressions of a Card Effect are those of the ruleset file, so an
    // unknown primitive or operator is an unknown key at the map that names it.
    let cases = [
        (
            "      - { on: { enter: field }, do: [ { draw: {} } ] }\n",
            "draw",
            sage_syntax_path().key("do").index(0),
        ),
        (
            "      - { on: { enter: field }, if: { not: [ true ] }, do: [ { end_turn: {} } ] }\n",
            "not",
            sage_syntax_path().key("if"),
        ),
        (
            "      - { while: { or: [ true ] }, modify: { field: power, amount: 1 } }\n",
            "or",
            sage_syntax_path().key("while"),
        ),
    ];
    for (effects, key, at) in cases {
        let error = yaml_error(&sage_yaml(effects));
        assert_eq!(
            error.kind,
            YamlErrorKind::UnknownKey {
                key: key.to_string()
            },
            "{effects}"
        );
        assert_eq!(error.path, at, "{effects}");
    }
}

#[test]
fn non_integer_modify_amount_is_a_syntax_error() {
    for amount in ["\"1\"", "1.5", "true", "~"] {
        let yaml = sage_yaml(&format!(
            "      - {{ while: true, modify: {{ field: power, amount: {amount} }} }}\n"
        ));
        let error = yaml_error(&yaml);
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "amount {amount}: got {:?}",
            error.kind
        );
        assert_eq!(
            error.path,
            sage_syntax_path().key("modify").key("amount"),
            "amount {amount}"
        );
    }
    // A negative integer is an integer.
    let yaml = sage_yaml("      - { while: true, modify: { field: power, amount: -2 } }\n");
    let draft = parse_card_catalog_draft(&yaml).expect("parses");
    assert_eq!(
        draft.cards[0].effects[0].modify,
        Some(ModifyDraft {
            field: "power".to_string(),
            amount: -2,
        })
    );
}

#[test]
fn card_effect_envelope_loads_into_the_draft() {
    let yaml = sage_yaml(
        "      - on: { enter: field }\n        if: { gte: [ { ref: self.power }, 2 ] }\n        do:\n          - move: { card: { ref: self }, to: trash }\n      - on: { turn_start: {} }\n        do: [ { set_state: { card: { ref: self }, state: tapped, value: false } } ]\n      - while: { and: [] }\n        modify: { field: power, amount: 1 }\n",
    );
    let draft = parse_card_catalog_draft(&yaml).expect("parses");
    assert_eq!(
        draft.cards[0].effects,
        vec![
            CardEffectDraft {
                on: Some(TriggerMatcherDraft::Enter("field".to_string())),
                condition: Some(ExpressionDraft::Gte(
                    Box::new(ExpressionDraft::Ref(RefDraft("self.power".to_string()))),
                    Box::new(ExpressionDraft::Literal(FieldValue::Int(2))),
                )),
                body: Some(vec![EffectDraft::Move {
                    card: RefDraft("self".to_string()),
                    to: "trash".to_string(),
                }]),
                while_condition: None,
                modify: None,
            },
            CardEffectDraft {
                on: Some(TriggerMatcherDraft::TurnStart),
                condition: None,
                body: Some(vec![EffectDraft::SetState {
                    card: RefDraft("self".to_string()),
                    state: "tapped".to_string(),
                    value: FieldValue::Bool(false),
                }]),
                while_condition: None,
                modify: None,
            },
            CardEffectDraft {
                on: None,
                condition: None,
                body: None,
                while_condition: Some(ExpressionDraft::And(vec![])),
                modify: Some(ModifyDraft {
                    field: "power".to_string(),
                    amount: 1,
                }),
            },
        ]
    );

    // The loader does not read meaning: an unknown matcher reaches the engine as `Other`, the key
    // kept, so a code frontend would get the same engine error. A card with no `effects` key and
    // one with `effects: []` both have none.
    let yaml = sage_yaml("      - { on: { leave: field }, do: [] }\n");
    let draft = parse_card_catalog_draft(&yaml).expect("parses");
    assert_eq!(
        draft.cards[0].effects[0].on,
        Some(TriggerMatcherDraft::Other("leave".to_string()))
    );
    let yaml = sage_yaml("      []\n");
    assert!(parse_card_catalog_draft(&yaml).is_ok());
}

#[test]
fn the_engine_rejects_every_loaded_triggered_card_effect_with_a_path() {
    // A YAML frontend gets the engine's errors, with the same paths a code frontend gets.
    let triggered = sage_yaml(
        "      - { on: { enter: field }, if: { gte: [ { ref: self.power }, 1 ] }, do: [ { move: { card: { ref: self }, to: trash } } ] }\n",
    );
    assert_eq!(
        catalog_error(&triggered),
        CatalogError {
            path: sage_effect_path(),
            kind: CatalogErrorKind::UnsupportedTriggeredCardEffect,
        }
    );

    // A continuous Card Effect is supported (ticket #51): the same text loads.
    let continuous = sage_yaml("      - { while: true, modify: { field: power, amount: 1 } }\n");
    let loaded = load_card_catalog(&effects_definition(), &continuous).expect("it loads");
    assert_eq!(loaded.cards()[0].effects().len(), 1);

    let unknown_matcher = sage_yaml("      - { on: { leave: field }, do: [ { end_turn: {} } ] }\n");
    assert_eq!(
        catalog_error(&unknown_matcher),
        CatalogError {
            path: sage_effect_path().key("on"),
            kind: CatalogErrorKind::UnknownTriggerMatcher {
                matcher: "leave".to_string(),
            },
        }
    );

    // The Small Ruleset's creatures do not allow effects: the same catalog is refused there.
    let small_definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let yaml = continuous.replace("ruleset: effects", "ruleset: small");
    match load_card_catalog(&small_definition, &yaml) {
        Err(CatalogLoadError::Catalog(error)) => assert_eq!(
            error,
            CatalogError {
                path: path().key("cards").id("sage").key("effects"),
                kind: CatalogErrorKind::EffectsNotAllowed,
            }
        ),
        other => panic!("expected EffectsNotAllowed, got {other:?}"),
    }
}
