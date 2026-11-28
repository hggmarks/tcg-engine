//! Acceptance tests for ticket #49: the Effects Ruleset (derived from the Small Ruleset, which
//! freezes) and the loading of the Card Effect envelope (ADR-0022).
//!
//! The Effects Ruleset, its catalog and its two sample decks are loaded from the real files. Since
//! ticket #53 the sample decks play the effect cards, so the Effects Ruleset's Golden Game is its
//! own script, played with those decks (it includes a trigger resolution). The equivalence of the
//! two rulesets is kept on the Small Ruleset's cards: the Effects Ruleset plays the Small Ruleset's
//! Leader-kill script with the Small sample decks into the same Event Traces. The Effects snapshot
//! changes only by the ignored test `regenerate_golden_snapshot`, never by a normal run (ADR-0017).

use tcg_engine::card_catalog::{
    CardEffectDraft, CatalogError, CatalogErrorKind, ModifyDraft, ScopeDraft, TriggerMatcherDraft,
};
use tcg_engine::declarative::{
    Action, Answer, CardInstanceId, DecisionKind, DeclarativeRuleset, Event,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::Ruleset;
use tcg_engine::ruleset_definition::{
    EffectDraft, ExpressionDraft, FieldValue, RefDraft, RulesetDefinition,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_card_catalog_draft,
    CatalogLoadError, YamlError, YamlErrorKind,
};

mod common;

use common::golden::{
    action, assert_matches_snapshot, assert_replaying_the_record_reproduces, attack, decks,
    play_game, render, render_observed, GoldenGame, Played,
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

/// The Seed of both Leader-kill Golden Games.
const LEADER_KILL_SEED: u64 = 9;

/// The cards of the Small Ruleset's Leader-kill script, which the Small sample decks number.
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

/// The cards of the Effects Ruleset's own Leader-kill script, in the Opening Position of Seed 9
/// with the Effects sample decks (Deck A numbers its main deck from 1 in file order, Deck B from
/// 62: seat 1's Leader is 61).
const EFFECTS_LEADER_1: CardInstanceId = CardInstanceId(61);
const EFFECTS_GOBLIN_0: CardInstanceId = CardInstanceId(1);
const EFFECTS_MOSS_0: CardInstanceId = CardInstanceId(7);
const EFFECTS_OTTER_0: CardInstanceId = CardInstanceId(13);
const EFFECTS_CAT_0: CardInstanceId = CardInstanceId(22);
const EFFECTS_LAMP_0: CardInstanceId = CardInstanceId(31);
const EFFECTS_DAWN_0: CardInstanceId = CardInstanceId(34);
const EFFECTS_BANNER_0: CardInstanceId = CardInstanceId(48);

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

/// Ends the Turn of `player`.
fn pass(player: u32) -> Action {
    action(player, "pass_turn", &[])
}

fn play(player: u32, card: CardInstanceId) -> Action {
    action(player, "play_creature", &[card])
}

/// Seat 0 races seat 1's Leader (life 6) and seat 1 answers: the Small Ruleset's Leader-kill
/// script, literally.
fn small_leader_kill_script() -> Vec<Action> {
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

/// Seat 0 races seat 1's Leader (life 6) with the Effects sample decks while seat 1 only passes.
/// Playing the Lamp Keeper is a trigger resolution (its `enter` trigger draws a card), and the
/// Banner Bearer, an aura, is in the field when the last four attacks land.
fn effects_leader_kill_script() -> Vec<Action> {
    vec![
        play(0, EFFECTS_GOBLIN_0),
        play(0, EFFECTS_MOSS_0),
        pass(0),
        pass(1),
        play(0, EFFECTS_OTTER_0),
        play(0, EFFECTS_DAWN_0),
        attack(0, EFFECTS_GOBLIN_0, EFFECTS_LEADER_1),
        attack(0, EFFECTS_MOSS_0, EFFECTS_LEADER_1),
        pass(0),
        pass(1),
        play(0, EFFECTS_LAMP_0),
        play(0, EFFECTS_BANNER_0),
        play(0, EFFECTS_CAT_0),
        attack(0, EFFECTS_GOBLIN_0, EFFECTS_LEADER_1),
        attack(0, EFFECTS_MOSS_0, EFFECTS_LEADER_1),
        attack(0, EFFECTS_OTTER_0, EFFECTS_LEADER_1),
        attack(0, EFFECTS_DAWN_0, EFFECTS_LEADER_1),
    ]
}

fn answer(player: u32, answer: Answer) -> Action {
    Action::Answer {
        player: PlayerId(player),
        answer,
    }
}

/// The Seed of the Effects pausing Golden Game, found by a search over the Seeds that played a
/// scripted bot at the shipped Effects decks and kept the shortest Game that holds the three
/// Choices below and ends. The search is not committed: the script is the Game.
const PAUSING_SEED: u64 = 204;

/// The cards of the pausing Golden Game's script: their Card Instance Ids in the Opening Position of
/// Seed 204 with the shipped Effects decks (Deck A numbers its main deck from 1 in file order,
/// Deck B from 62, so seat 1's Leader is 61). The number in a name is the card's position among
/// the copies of its definition in its deck.
const PAUSING_LEADER_0: CardInstanceId = CardInstanceId(0);
const PAUSING_GOBLIN_0: CardInstanceId = CardInstanceId(1);
const PAUSING_GOBLIN_3: CardInstanceId = CardInstanceId(4);
const PAUSING_OWL_0: CardInstanceId = CardInstanceId(11);
const PAUSING_OTTER_0: CardInstanceId = CardInstanceId(13);
const PAUSING_GOLEM_3: CardInstanceId = CardInstanceId(20);
const PAUSING_CAT_1: CardInstanceId = CardInstanceId(22);
const PAUSING_DAWN_1: CardInstanceId = CardInstanceId(34);
const PAUSING_LEADER_1: CardInstanceId = CardInstanceId(61);
const PAUSING_HOUND_0: CardInstanceId = CardInstanceId(66);
const PAUSING_VINE_1: CardInstanceId = CardInstanceId(83);
const PAUSING_LEECH_1: CardInstanceId = CardInstanceId(85);

/// The pausing Golden Game: the Effects sample decks and Seed 204, 23 Actions, with the three kinds
/// of Choice in it, each answered through `Action::Answer`:
///
/// - `curious_owl`, an Option Choice of seat 0, answered with option 0 (draw);
/// - `mind_leech`, played by seat 1, a Card Choice decided by its opponent, seat 0, who moves its
///   own `goblin_scout` to its trash;
/// - `snare_vine`, played by seat 1 while seat 0 has creatures in the field, a Card Choice decided
///   by its controller, who taps seat 0's first `goblin_scout`.
///
/// Seat 0 then kills seat 1's Leader (life 6) with four attackers in the last Turn.
fn effects_pausing_script() -> Vec<Action> {
    vec![
        // Turn 1, seat 0: `curious_owl` pauses for its Option Choice.
        play(0, PAUSING_OWL_0),
        answer(0, Answer::Option(0)),
        play(0, PAUSING_GOBLIN_0),
        pass(0),
        // Turn 2, seat 1: `mind_leech` pauses for seat 0, the opponent, to choose a creature of
        // its own hand.
        play(1, PAUSING_LEECH_1),
        answer(0, Answer::Card(PAUSING_GOBLIN_3)),
        pass(1),
        // Turn 3, seat 0.
        play(0, PAUSING_OTTER_0),
        play(0, PAUSING_DAWN_1),
        attack(0, PAUSING_GOBLIN_0, PAUSING_LEADER_1),
        attack(0, PAUSING_OWL_0, PAUSING_LEADER_1),
        pass(0),
        // Turn 4, seat 1: `snare_vine` pauses for its controller, who taps a creature in seat 0's
        // field.
        play(1, PAUSING_VINE_1),
        answer(1, Answer::Card(PAUSING_GOBLIN_0)),
        play(1, PAUSING_HOUND_0),
        attack(1, PAUSING_LEECH_1, PAUSING_LEADER_0),
        pass(1),
        // Turn 5, seat 0: the last attacks land on seat 1's Leader.
        play(0, PAUSING_GOLEM_3),
        play(0, PAUSING_CAT_1),
        attack(0, PAUSING_GOBLIN_0, PAUSING_LEADER_1),
        attack(0, PAUSING_OWL_0, PAUSING_LEADER_1),
        attack(0, PAUSING_OTTER_0, PAUSING_LEADER_1),
        attack(0, PAUSING_DAWN_1, PAUSING_LEADER_1),
    ]
}

/// The Effects Leader-kill Golden Game: the Effects sample decks, Seed 9.
fn effects_leader_kill() -> GoldenGame {
    GoldenGame {
        name: "effects-leader-kill",
        seed: LEADER_KILL_SEED,
        script: effects_leader_kill_script(),
    }
}

/// The Small Ruleset's Leader-kill Golden Game: the Small sample decks, Seed 9.
fn small_leader_kill() -> GoldenGame {
    GoldenGame {
        name: "small-leader-kill",
        seed: LEADER_KILL_SEED,
        script: small_leader_kill_script(),
    }
}

/// The pausing Golden Game: the Effects sample decks, `PAUSING_SEED`.
fn effects_pausing() -> GoldenGame {
    GoldenGame {
        name: "effects-pausing",
        seed: PAUSING_SEED,
        script: effects_pausing_script(),
    }
}

fn play_effects_leader_kill() -> Played {
    let ruleset = effects();
    let decks = decks(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B]);
    play_game(&ruleset, &decks, &effects_leader_kill())
}

/// The Effects Ruleset playing the Small Ruleset's Leader-kill script with the Small sample decks.
fn play_effects_ruleset_small_leader_kill() -> Played {
    let ruleset = effects();
    let decks = decks(&ruleset, [SMALL_DECK_A, SMALL_DECK_B]);
    play_game(&ruleset, &decks, &small_leader_kill())
}

fn play_small_leader_kill() -> Played {
    let ruleset = small();
    let decks = decks(&ruleset, [SMALL_DECK_A, SMALL_DECK_B]);
    play_game(&ruleset, &decks, &small_leader_kill())
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
    // carries six triggered (tickets #50 and #53), two continuous (ticket #51) and three aura ones
    // (ticket #52): eleven carriers.
    let allows: Vec<(&str, bool)> = definition
        .card_types()
        .iter()
        .map(|card_type| (card_type.id().as_str(), card_type.allows_effects()))
        .collect();
    assert_eq!(
        allows,
        [("leader", false), ("creature", true), ("mana", false)]
    );
    // The creatures carrying a Card Effect are the ones that exist for them (effects-ruleset.md);
    // the cards of the Small catalog carry none.
    let carriers: Vec<&str> = ruleset
        .catalog()
        .cards()
        .iter()
        .filter(|card| !card.effects().is_empty())
        .map(|card| card.id().as_str())
        .collect();
    assert_eq!(
        carriers,
        [
            "lamp_keeper",
            "dawn_scout",
            "grave_jackal",
            "eager_recruit",
            "restless_shade",
            "pyre_imp",
            "curious_owl",
            "snare_vine",
            "mind_leech",
            "battle_veteran",
            "weary_ox",
            "banner_bearer",
            "dread_herald",
            "rally_captain",
        ]
    );
    let small_ruleset = small();
    let small_ids: Vec<&str> = small_ruleset
        .catalog()
        .cards()
        .iter()
        .map(|card| card.id().as_str())
        .collect();
    assert!(ruleset
        .catalog()
        .cards()
        .iter()
        .filter(|card| small_ids.contains(&card.id().as_str()))
        .all(|card| card.effects().is_empty()));
}

#[test]
fn the_effects_leader_kill_golden_game_matches_its_snapshot() {
    let played = play_effects_leader_kill();
    assert_eq!(played.steps.len(), effects_leader_kill_script().len());

    // A trigger resolution is in the snapshot: the Action that plays the Lamp Keeper moves the
    // card to the field, and the draw its `enter` trigger resolves follows in the same trace.
    let (_, events) = played
        .steps
        .iter()
        .find(|(action, _)| *action == play(0, EFFECTS_LAMP_0))
        .expect("the script plays the Lamp Keeper");
    let drawn_after_the_play = events
        .iter()
        .skip_while(|event| !matches!(event, Event::CardMoved { to, .. } if to.as_str() == "field"))
        .any(|event| {
            matches!(
                event,
                Event::CardMoved { player, from, to, .. }
                    if *player == PlayerId(0) && from.as_str() == "deck" && to.as_str() == "hand"
            )
        });
    assert!(
        drawn_after_the_play,
        "the Lamp Keeper's draw follows its play: {events:?}"
    );

    assert_matches_snapshot(
        "effects-leader-kill",
        LEADER_KILL_SNAPSHOT,
        &render(&played),
        REGENERATE_COMMAND,
    );
}

#[test]
fn the_effects_ruleset_plays_the_small_rulesets_cards_identically() {
    let small = play_small_leader_kill();
    let effects = play_effects_ruleset_small_leader_kill();
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
            "      - { on: { enter: field }, do: [ { discard: {} } ] }\n",
            "discard",
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
fn an_unknown_key_in_a_scope_is_a_syntax_error() {
    // Like `modify`'s, a scope knows its four keys and nothing else.
    let yaml = sage_yaml(
        "      - { while: true, scope: { zone: field, card_type: creature, range: 2 }, modify: { field: power, amount: 1 } }\n",
    );
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "range".to_string()
        }
    );
    assert_eq!(error.path, sage_syntax_path().key("scope").key("range"));
}

#[test]
fn a_scope_is_read_into_its_draft() {
    // The loader reads no meaning: the owner text and the flag reach the engine as written, and a
    // missing key stays absent.
    let yaml = sage_yaml(
        "      - while: true\n        scope: { zone: field, card_type: creature, owner: opponent, include_self: true }\n        modify: { field: power, amount: -1 }\n      - while: true\n        scope: { zone: field, card_type: creature }\n        modify: { field: power, amount: 1 }\n",
    );
    let draft = parse_card_catalog_draft(&yaml).expect("parses");
    assert_eq!(
        draft.cards[0]
            .effects
            .iter()
            .map(|effect| effect.scope.clone())
            .collect::<Vec<_>>(),
        vec![
            Some(ScopeDraft {
                zone: "field".to_string(),
                card_type: "creature".to_string(),
                owner: Some("opponent".to_string()),
                include_self: Some(true),
            }),
            Some(ScopeDraft {
                zone: "field".to_string(),
                card_type: "creature".to_string(),
                owner: None,
                include_self: None,
            }),
        ]
    );
}

#[test]
fn non_bool_include_self_is_a_syntax_error() {
    // A bool key of the format takes an unquoted `true` or `false` only (ADR-0009), like `ordered`
    // and `effects`: a quoted `"true"` is a text, and `yes`, `1` and `~` are not bools.
    for flag in ["\"true\"", "yes", "1", "~"] {
        let yaml = sage_yaml(&format!(
            "      - {{ while: true, scope: {{ zone: field, card_type: creature, include_self: {flag} }}, modify: {{ field: power, amount: 1 }} }}\n"
        ));
        let error = yaml_error(&yaml);
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "include_self {flag}: got {:?}",
            error.kind
        );
        assert_eq!(
            error.path,
            sage_syntax_path().key("scope").key("include_self"),
            "include_self {flag}"
        );
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
                scope: None,
                activated: None,
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
                scope: None,
                activated: None,
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
                scope: None,
                activated: None,
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
fn the_engine_accepts_a_loaded_card_effect_of_either_shape_and_reports_errors_with_a_path() {
    // A YAML frontend gets the engine's errors, with the same paths a code frontend gets.
    let triggered = sage_yaml(
        "      - { on: { enter: field }, if: { gte: [ { ref: self.power }, 1 ] }, do: [ { move: { card: { ref: self }, to: trash } } ] }\n",
    );
    let catalog = load_card_catalog(&effects_definition(), &triggered)
        .expect("a triggered Card Effect is supported (ADR-0023)");
    assert_eq!(catalog.cards()[0].effects().len(), 1);

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

// ---- the triggered Card Effects of the catalog (ADR-0023)

/// A deck of the shipped catalog that lists the five creatures with a Card Effect, which no
/// shipped deck does.
const EFFECT_CARDS_DECK: &str = "inputs:
  - input: leader
    cards:
      - { card: captain_ardel, count: 1 }
  - input: main_deck
    cards:
      - { card: lamp_keeper,     count: 4 }
      - { card: dawn_scout,      count: 4 }
      - { card: grave_jackal,    count: 4 }
      - { card: eager_recruit,   count: 4 }
      - { card: restless_shade,  count: 4 }
      - { card: goblin_scout,    count: 4 }
      - { card: moss_sprite,     count: 4 }
      - { card: ember_hound,     count: 4 }
      - { card: river_otter,     count: 4 }
      - { card: stone_golem,     count: 4 }
      - { card: shadow_cat,      count: 4 }
      - { card: iron_guard,      count: 4 }
      - { card: wind_hawk,       count: 2 }
  - input: mana_deck
    cards:
      - { card: mana, count: 10 }
";

/// The transition of Player 0 playing their `id` card, arranged into the hand, in Main of Turn 1
/// (the Start phase moved 2 mana cards, enough for every cost here).
fn play_catalog_card(id: &str) -> Result<Vec<Event>, tcg_engine::declarative::Rejection> {
    let ruleset = effects();
    let deck = load_deck_list(ruleset.definition(), ruleset.catalog(), EFFECT_CARDS_DECK)
        .expect("the deck is legal");
    let mut state = ruleset
        .create_game(&[deck.clone(), deck], Seed(1))
        .expect("the decks create a game")
        .opening
        .state;
    let card = state
        .cards()
        .iter()
        .find(|card| card.owner() == PlayerId(0) && card.definition().as_str() == id)
        .expect("the deck has the card")
        .id();
    let from = state.zone_of(card).expect("in a zone").clone();
    if from.as_str() != "hand" {
        state
            .apply(&Event::CardMoved {
                card,
                player: PlayerId(0),
                from,
                to: tcg_engine::ruleset_definition::ZoneId::new("hand"),
            })
            .expect("the arrangement is legal");
    }
    ruleset
        .transition(&state, &play(0, card))
        .map(|transition| transition.trace.into_events())
}

#[test]
fn the_catalog_creatures_with_a_card_effect_trigger_as_documented() {
    let ruleset = effects();
    let effects_of = |id: &str| {
        ruleset
            .catalog()
            .cards()
            .iter()
            .find(|card| card.id().as_str() == id)
            .map(|card| card.effects().len())
    };
    for (id, count) in [
        ("lamp_keeper", 1),
        ("dawn_scout", 1),
        ("grave_jackal", 1),
        ("eager_recruit", 1),
        ("restless_shade", 2),
        ("pyre_imp", 2),
        ("goblin_scout", 0),
    ] {
        assert_eq!(effects_of(id), Some(count), "{id}");
    }

    // Played, the two drawers draw a card for their controller: the last Event of the trace.
    for id in ["lamp_keeper", "eager_recruit"] {
        let events = play_catalog_card(id).expect("the play is accepted");
        assert!(
            matches!(
                events.last(),
                Some(Event::CardMoved { player, from, to, .. })
                    if *player == PlayerId(0) && from.as_str() == "deck" && to.as_str() == "hand"
            ),
            "{id}: {events:?}"
        );
    }

    // The Shade's two effects wake each other for ever: the cap rejects the play.
    assert_eq!(
        play_catalog_card("restless_shade"),
        Err(tcg_engine::declarative::Rejection::TriggerCapExceeded { cap: 100 })
    );
}

// ---- the Effects Leader-kill Golden Game, as each Player observed it (ticket #66)

/// The Golden Game as seat 0 and as seat 1 observed it. Each snapshot is in the format of
/// ADR-0017 with Observed Events in place of Events: a header, then `== creation` and one `Debug`
/// line per Observed Event of the creation trace, then for each accepted Action `== action N`, the
/// `Debug` line of its Taken Action and one `Debug` line per Observed Event of its transition,
/// then `== outcome` and the Terminal Outcome as the Viewer sees it. They change only by the ignored
/// test `regenerate_observed_golden_snapshots`, never by a normal run.
const OBSERVED_SNAPSHOTS: [&str; 2] = [
    include_str!("golden/effects-leader-kill.observed-by-player-0.trace"),
    include_str!("golden/effects-leader-kill.observed-by-player-1.trace"),
];
const OBSERVED_PATHS: [&str; 2] = [
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/effects-leader-kill.observed-by-player-0.trace"
    ),
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/effects-leader-kill.observed-by-player-1.trace"
    ),
];
const REGENERATE_OBSERVED_COMMAND: &str =
    "cargo test -p tcg-loader --test effects_ruleset -- --ignored regenerate_observed_golden_snapshots";

/// The Effects Golden Game `game` as seat `seat` observed it, played with the Effects sample decks.
fn render_effects_observed(game: &GoldenGame, seat: u32) -> String {
    let ruleset = effects();
    let decks = decks(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B]);
    render_observed(&ruleset, &decks, game, seat)
}

#[test]
fn the_effects_leader_kill_golden_game_as_observed_by_each_player_matches_its_snapshot() {
    let game = effects_leader_kill();
    let observed: Vec<String> = (0..2)
        .map(|seat| render_effects_observed(&game, seat))
        .collect();
    for (seat, (actual, snapshot)) in observed.iter().zip(OBSERVED_SNAPSHOTS).enumerate() {
        assert_matches_snapshot(
            &format!("effects-leader-kill as observed by player {seat}"),
            snapshot,
            actual,
            REGENERATE_OBSERVED_COMMAND,
        );
    }
    // The two Players do not see the same Game: what each observed differs.
    assert_ne!(OBSERVED_SNAPSHOTS[0], OBSERVED_SNAPSHOTS[1]);
}

/// Rewrites the observed Golden Snapshots. Run it on purpose, and read the diff before committing
/// it. It does not touch `regenerate_golden_snapshot`'s files.
#[test]
#[ignore = "deliberate update: rewrites the observed golden snapshots"]
fn regenerate_observed_golden_snapshots() {
    for (seat, path) in OBSERVED_PATHS.iter().enumerate() {
        let seat = u32::try_from(seat).expect("a seat");
        std::fs::write(path, render_effects_observed(&effects_leader_kill(), seat))
            .expect("the observed snapshot is written");
    }
}

// ---- the pausing Golden Game (ticket #77)

const PAUSING_SNAPSHOT: &str = include_str!("golden/effects-pausing.trace");
const PAUSING_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/effects-pausing.trace"
);
const PAUSING_OBSERVED_SNAPSHOTS: [&str; 2] = [
    include_str!("golden/effects-pausing.observed-by-player-0.trace"),
    include_str!("golden/effects-pausing.observed-by-player-1.trace"),
];
const PAUSING_OBSERVED_PATHS: [&str; 2] = [
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/effects-pausing.observed-by-player-0.trace"
    ),
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/effects-pausing.observed-by-player-1.trace"
    ),
];
const REGENERATE_PAUSING_COMMAND: &str =
    "cargo test -p tcg-loader --test effects_ruleset -- --ignored regenerate_pausing_golden_snapshots";

fn play_effects_pausing() -> Played {
    let ruleset = effects();
    let decks = decks(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B]);
    play_game(&ruleset, &decks, &effects_pausing())
}

#[test]
fn the_pausing_golden_game_matches_its_snapshot() {
    let played = play_effects_pausing();
    assert_eq!(played.steps.len(), effects_pausing_script().len());

    // The script plays the Game to its end, caught here and not only by the snapshot: seat 0 wins.
    let outcome = played
        .final_state
        .outcome()
        .expect("the pausing Golden Game ends the Game");
    assert_eq!(outcome.winners(), [PlayerId(0)]);

    // The Game holds a Card Choice decided by the controller of the carrying card, one decided by
    // its opponent and an Option Choice: each is a `DecisionPending` in the Event Traces.
    let pending: Vec<(PlayerId, DecisionKind, PlayerId)> = played
        .steps
        .iter()
        .flat_map(|(_, events)| events)
        .filter_map(|event| match event {
            Event::DecisionPending {
                decider,
                kind,
                paused,
            } => {
                let controller = played
                    .final_state
                    .card(paused.trigger.card)
                    .expect("the carrying card exists")
                    .owner();
                Some((*decider, *kind, controller))
            }
            _ => None,
        })
        .collect();
    assert!(
        pending
            .iter()
            .any(|(decider, kind, controller)| *kind == DecisionKind::Card && decider == controller),
        "a Card Choice decided by the controller: {pending:?}"
    );
    assert!(
        pending
            .iter()
            .any(|(decider, kind, controller)| *kind == DecisionKind::Card && decider != controller),
        "a Card Choice decided by the opponent: {pending:?}"
    );
    assert!(
        pending
            .iter()
            .any(|(_, kind, _)| *kind == DecisionKind::Option),
        "an Option Choice: {pending:?}"
    );

    assert_matches_snapshot(
        "effects-pausing",
        PAUSING_SNAPSHOT,
        &render(&played),
        REGENERATE_PAUSING_COMMAND,
    );
}

#[test]
fn the_pausing_golden_game_as_observed_by_each_player_matches_its_snapshot() {
    let game = effects_pausing();
    let observed: Vec<String> = (0..2)
        .map(|seat| render_effects_observed(&game, seat))
        .collect();
    for (seat, (actual, snapshot)) in observed.iter().zip(PAUSING_OBSERVED_SNAPSHOTS).enumerate() {
        assert_matches_snapshot(
            &format!("effects-pausing as observed by player {seat}"),
            snapshot,
            actual,
            REGENERATE_PAUSING_COMMAND,
        );
    }
    // The two Players do not see the same Game: what each observed differs.
    assert_ne!(PAUSING_OBSERVED_SNAPSHOTS[0], PAUSING_OBSERVED_SNAPSHOTS[1]);
}

/// Rewrites the three snapshots of the pausing Golden Game: the Event Trace and the two observed
/// ones. Run it on purpose, and read the diff before committing it. It does not touch the
/// Leader-kill files.
#[test]
#[ignore = "deliberate update: rewrites the pausing golden snapshots"]
fn regenerate_pausing_golden_snapshots() {
    std::fs::write(PAUSING_PATH, render(&play_effects_pausing()))
        .expect("the pausing snapshot is written");
    for (seat, path) in PAUSING_OBSERVED_PATHS.iter().enumerate() {
        let seat = u32::try_from(seat).expect("a seat");
        std::fs::write(path, render_effects_observed(&effects_pausing(), seat))
            .expect("the observed pausing snapshot is written");
    }
}

// ---- replay (ticket #77)

#[test]
fn replaying_the_record_of_effects_leader_kill_reproduces_its_game() {
    let ruleset = effects();
    let decks = decks(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B]);
    assert_replaying_the_record_reproduces(&ruleset, &decks, &effects_leader_kill());
}

#[test]
fn replaying_the_record_of_effects_pausing_reproduces_its_game() {
    let ruleset = effects();
    let decks = decks(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B]);
    assert_replaying_the_record_reproduces(&ruleset, &decks, &effects_pausing());
}
