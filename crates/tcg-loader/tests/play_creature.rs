//! Acceptance tests for ticket #24: action parameters, costs and refs, and playing a creature by
//! paying mana, on the real Small Ruleset.
//!
//! The fixture is the real `small-ruleset.yaml` with `attack` cut: Main lists `play_creature` and
//! `pass_turn`, and `actions` holds the file's own `play_creature` and `pass_turn` blocks, sliced
//! from the file and not retyped. `attack` waits for ticket [11]. The other fixtures are text edits
//! of this one. Code-built drafts and the Display of the new Rejections are tested in the engine
//! crate (`tests/action_parameters.rs`).

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameState, Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionDraft, ActionId, CardTypeId, CardValue, Cost, CostDraft, DefinitionError,
    DefinitionErrorKind, Effect, EffectDraft, FieldName, FieldType, FieldValue, ParamDraft,
    ParamName, PhaseId, RefDraft, RefType, ReferenceTarget, StateName, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_ruleset_draft, LoadError,
    YamlError, YamlErrorKind,
};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// With this Seed Player 0 opens with, in hand order: `goblin_scout` (cost 1), `river_otter`
/// (cost 2), `moss_sprite` (cost 1), `thorn_boar` (cost 4) and `ash_drake` (cost 5); Player 1's
/// hand starts with `moss_sprite`. Each test asserts the costs it relies on.
const SEED: u64 = 9;

const MAIN_WITH_PLAY: &str = "actions: [play_creature, pass_turn]";

// ---- the fixture, sliced from the real file

/// The real file up to its `actions:` section, with Main listing `play_creature` and `pass_turn`
/// (the file lists `attack` too).
fn small_head(main: &str) -> String {
    let (head, _) = SMALL_RULESET
        .split_once("\nactions:")
        .expect("the Small Ruleset has an actions section");
    edit(head, "actions: [play_creature, attack, pass_turn]", main)
}

/// The `play_creature` block of the real file.
fn play_creature_block() -> &'static str {
    let from = SMALL_RULESET
        .find("  - id: play_creature")
        .expect("the file declares play_creature");
    let to = SMALL_RULESET
        .find("  - id: attack")
        .expect("the file declares attack");
    &SMALL_RULESET[from..to]
}

/// The `pass_turn` block of the real file.
fn pass_turn_block() -> &'static str {
    let from = SMALL_RULESET
        .find("  - id: pass_turn")
        .expect("the file declares pass_turn");
    let to = SMALL_RULESET
        .find("\nwin_conditions:")
        .expect("the file has win_conditions");
    &SMALL_RULESET[from..to]
}

/// The Small Ruleset without `attack`, as the document of this ticket.
fn small_cut() -> String {
    small_with(MAIN_WITH_PLAY, "")
}

/// The cut with Main listing `main` and `extra` (more action blocks) after the file's two.
fn small_with(main: &str, extra: &str) -> String {
    format!(
        "{}\nactions:\n{}{}\n{extra}",
        small_head(main),
        play_creature_block(),
        pass_turn_block()
    )
}

/// `yaml` with the one occurrence of `from` replaced by `to`.
fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(
        yaml.matches(from).count(),
        1,
        "`{from}` occurs once in the fixture"
    );
    yaml.replace(from, to)
}

/// The cut whose creatures also have an int instance state `price` that starts at 1, and whose
/// `play_creature` taps `card.price` mana.
fn priced_cut() -> String {
    let with_state = edit(
        &small_cut(),
        "      - { name: recently_played,  type: bool, init: false }\n",
        "      - { name: recently_played,  type: bool, init: false }\n      - { name: price,            type: int,  init: 1 }\n",
    );
    edit(
        &with_state,
        "count: { ref: card.cost }",
        "count: { ref: card.price }",
    )
}

/// Two more actions of the hand and the field.
const EXTRA_ACTIONS: &str = r"  - id: bounce
    params:
      - { name: card, zone: field, card_type: creature }
    effects:
      - move: { card: { ref: card }, to: hand }
  - id: calm
    params:
      - { name: card, zone: hand, card_type: creature }
    effects:
      - set_state: { card: { ref: card }, state: recently_played, value: false }
      - set_state: { card: { ref: card }, state: recently_played, value: true }
";

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

/// The path of a part of `play_creature`.
fn in_play_creature() -> DefinitionPath {
    path().key("actions").id("play_creature")
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

/// Asserts that `yaml` fails validation with `kind` at `at`.
fn assert_definition_error(yaml: &str, at: DefinitionPath, kind: DefinitionErrorKind) {
    assert_eq!(definition_error(yaml), DefinitionError { path: at, kind });
}

fn ruleset_from(yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(yaml).expect("the fixture loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn small() -> DeclarativeRuleset {
    ruleset_from(&small_cut())
}

/// Deck A for seat 0 and deck B for seat 1.
fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    [DECK_A, DECK_B]
        .into_iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn opening(ruleset: &DeclarativeRuleset) -> GameState {
    ruleset
        .create_game(&decks(ruleset), Seed(SEED))
        .expect("the sample decks create a game")
        .opening
        .state
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action {
        player: p(player),
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

fn accepted(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
) -> Transition<GameState, Event> {
    ruleset
        .transition(state, action)
        .expect("the Action is accepted")
}

/// Asserts the exact Rejection, and that the GameState is what it was before the call.
fn assert_rejected(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
    expected: Rejection,
) {
    let before = state.clone();
    assert_eq!(ruleset.transition(state, action), Err(expected));
    assert_eq!(state, &before, "a rejected Action changes nothing");
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

/// The `n`-th card from the Top of a zone, 0 being the Top.
fn from_top(state: &GameState, player: u32, zone_id: &str, n: usize) -> CardInstanceId {
    *cards_in(state, player, zone_id)
        .iter()
        .rev()
        .nth(n)
        .expect("the zone has that many cards")
}

fn state_value(state: &GameState, card: CardInstanceId, name: &str) -> Option<FieldValue> {
    state
        .card(card)
        .and_then(|instance| instance.state_value(&StateName::new(name)))
        .cloned()
}

/// The `cost` field of a card's definition.
fn cost_of(ruleset: &DeclarativeRuleset, state: &GameState, card: CardInstanceId) -> i64 {
    let instance = state.card(card).expect("the card exists");
    let definition = ruleset
        .catalog()
        .card(instance.definition())
        .expect("the card is in the catalog");
    match definition.field(&FieldName::new("cost")) {
        Some(FieldValue::Int(cost)) => *cost,
        other => panic!("expected an int cost, got {other:?}"),
    }
}

/// The first card of a Player's hand whose cost is `cost`.
fn hand_card_with_cost(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    player: u32,
    cost: i64,
) -> CardInstanceId {
    cards_in(state, player, "hand")
        .into_iter()
        .find(|&card| cost_of(ruleset, state, card) == cost)
        .expect("the hand holds a card of that cost")
}

fn moved(card: CardInstanceId, player: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(player),
        from: zone(from),
        to: zone(to),
    }
}

fn changed(card: CardInstanceId, state: &str, value: bool) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new(state),
        value: FieldValue::Bool(value),
    }
}

fn price_set(card: CardInstanceId, value: i64) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new("price"),
        value: FieldValue::Int(value),
    }
}

/// Applies Events that set a scenario up; each must be accepted.
fn apply_all(state: &mut GameState, events: &[Event]) {
    for event in events {
        state.apply(event).expect("the scenario Event is accepted");
    }
}

fn card_param() -> ParamName {
    ParamName::new("card")
}

// ---- loading action definitions

#[test]
fn play_creature_loads_with_params_cost_and_effects() {
    let definition = load_ruleset_definition(&small_cut()).expect("the cut loads");
    let play = definition
        .action(&ActionId::new("play_creature"))
        .expect("play_creature is declared");

    let [param] = play.params() else {
        panic!("play_creature has one param, got {:?}", play.params());
    };
    assert_eq!(param.name(), &card_param());
    assert_eq!(param.zone(), &zone("hand"));
    assert_eq!(param.card_type(), &CardTypeId::new("creature"));

    let [Cost::Tap {
        zone: tapped,
        count,
    }] = play.costs()
    else {
        panic!("play_creature has one tap cost, got {:?}", play.costs());
    };
    assert_eq!(tapped, &zone("mana_zone"));
    assert_eq!(count.param(), &card_param());
    assert_eq!(count.value(), &CardValue::Field(FieldName::new("cost")));

    let [Effect::Move { card: moving, to }, Effect::SetState {
        card: marked,
        state,
        value,
    }] = play.effects()
    else {
        panic!(
            "play_creature has move then set_state, got {:?}",
            play.effects()
        );
    };
    assert_eq!(moving.param(), &card_param());
    assert_eq!(to, &zone("field"));
    assert_eq!(marked.param(), &card_param());
    assert_eq!(state, &StateName::new("recently_played"));
    assert_eq!(value, &FieldValue::Bool(true));

    let pass = definition
        .action(&ActionId::new("pass_turn"))
        .expect("pass_turn is declared");
    assert!(pass.params().is_empty());
    assert!(pass.costs().is_empty());
    assert_eq!(pass.effects(), [Effect::EndTurn]);
}

#[test]
fn the_loader_reads_params_cost_and_effects_as_plain_drafts() {
    // Syntax only: a ref is the text of the file, which the engine parses.
    let draft = parse_ruleset_draft(&small_cut()).expect("the cut parses");
    assert_eq!(
        draft.actions[0],
        ActionDraft {
            id: "play_creature".to_string(),
            params: vec![ParamDraft {
                name: "card".to_string(),
                zone: "hand".to_string(),
                card_type: "creature".to_string(),
            }],
            cost: vec![CostDraft::Tap {
                zone: "mana_zone".to_string(),
                count: RefDraft("card.cost".to_string()),
            }],
            effects: vec![
                EffectDraft::Move {
                    card: RefDraft("card".to_string()),
                    to: "field".to_string(),
                },
                EffectDraft::SetState {
                    card: RefDraft("card".to_string()),
                    state: "recently_played".to_string(),
                    value: FieldValue::Bool(true),
                },
            ],
        }
    );
    assert_eq!(
        draft.actions[1],
        ActionDraft {
            id: "pass_turn".to_string(),
            params: vec![],
            cost: vec![],
            effects: vec![EffectDraft::EndTurn],
        }
    );

    // A malformed ref is not a syntax error: the engine reports it.
    let malformed = edit(
        &small_cut(),
        "count: { ref: card.cost }",
        "count: { ref: \"\" }",
    );
    let draft = parse_ruleset_draft(&malformed).expect("the loader does not parse refs");
    assert_eq!(
        draft.actions[0].cost,
        vec![CostDraft::Tap {
            zone: "mana_zone".to_string(),
            count: RefDraft(String::new()),
        }]
    );
}

#[test]
fn owner_where_and_one_of_are_still_unknown_keys_of_a_param() {
    // Ticket [11] adds them.
    let with_owner = edit(
        &small_cut(),
        "{ name: card, zone: hand, card_type: creature }",
        "{ name: card, zone: hand, card_type: creature, owner: opponent }",
    );
    let error = yaml_error(&with_owner);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "owner".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .index(0)
            .key("params")
            .index(0)
            .key("owner")
    );

    let with_one_of = edit(
        &small_cut(),
        "{ name: card, zone: hand, card_type: creature }",
        "{ name: card, one_of: [ { zone: hand } ] }",
    );
    let error = yaml_error(&with_one_of);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "one_of".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .index(0)
            .key("params")
            .index(0)
            .key("one_of")
    );
}

// ---- validating refs at load

#[test]
fn ref_to_an_undeclared_param_is_a_dangling_reference() {
    let dangling = DefinitionErrorKind::DanglingReference {
        target: ReferenceTarget::Param,
        id: "spell".to_string(),
    };

    let in_tap = edit(&small_cut(), "{ ref: card.cost }", "{ ref: spell.cost }");
    assert_definition_error(
        &in_tap,
        in_play_creature()
            .key("cost")
            .index(0)
            .key("tap")
            .key("count"),
        dangling.clone(),
    );

    let in_move = edit(
        &small_cut(),
        "- move: { card: { ref: card }, to: field }",
        "- move: { card: { ref: spell }, to: field }",
    );
    assert_definition_error(
        &in_move,
        in_play_creature()
            .key("effects")
            .index(0)
            .key("move")
            .key("card"),
        dangling.clone(),
    );

    let in_set_state = edit(
        &small_cut(),
        "- set_state: { card: { ref: card }, state: recently_played, value: true }",
        "- set_state: { card: { ref: spell }, state: recently_played, value: true }",
    );
    assert_definition_error(
        &in_set_state,
        in_play_creature()
            .key("effects")
            .index(1)
            .key("set_state")
            .key("card"),
        dangling,
    );
}

#[test]
fn ref_to_an_undeclared_field_or_state_is_a_dangling_reference() {
    let yaml = edit(&small_cut(), "{ ref: card.cost }", "{ ref: card.nothing }");
    assert_definition_error(
        &yaml,
        in_play_creature()
            .key("cost")
            .index(0)
            .key("tap")
            .key("count"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::FieldOrInstanceState,
            id: "nothing".to_string(),
        },
    );

    // A name another card type declares is not enough: the param's card type must declare it.
    let yaml = edit(&small_cut(), "{ ref: card.cost }", "{ ref: card.life }");
    assert_definition_error(
        &yaml,
        in_play_creature()
            .key("cost")
            .index(0)
            .key("tap")
            .key("count"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::FieldOrInstanceState,
            id: "life".to_string(),
        },
    );
}

#[test]
fn ref_of_the_wrong_type_is_rejected_at_load() {
    let tap_count = in_play_creature()
        .key("cost")
        .index(0)
        .key("tap")
        .key("count");
    let int = RefType::Value(FieldType::Int);

    // `tap.count` needs an int value: not a text field, not a bool state, not a card.
    for (reference, found) in [
        ("card.name", RefType::Value(FieldType::Text)),
        ("card.tapped", RefType::Value(FieldType::Bool)),
        ("card", RefType::Card),
    ] {
        let yaml = edit(
            &small_cut(),
            "{ ref: card.cost }",
            &format!("{{ ref: {reference} }}"),
        );
        assert_definition_error(
            &yaml,
            tap_count.clone(),
            DefinitionErrorKind::ReferenceTypeMismatch {
                expected: int,
                found,
            },
        );
    }

    // `move.card` and `set_state.card` need a card.
    let in_move = edit(
        &small_cut(),
        "- move: { card: { ref: card }, to: field }",
        "- move: { card: { ref: card.cost }, to: field }",
    );
    assert_definition_error(
        &in_move,
        in_play_creature()
            .key("effects")
            .index(0)
            .key("move")
            .key("card"),
        DefinitionErrorKind::ReferenceTypeMismatch {
            expected: RefType::Card,
            found: int,
        },
    );
    let in_set_state = edit(
        &small_cut(),
        "- set_state: { card: { ref: card }, state: recently_played, value: true }",
        "- set_state: { card: { ref: card.tapped }, state: recently_played, value: true }",
    );
    assert_definition_error(
        &in_set_state,
        in_play_creature()
            .key("effects")
            .index(1)
            .key("set_state")
            .key("card"),
        DefinitionErrorKind::ReferenceTypeMismatch {
            expected: RefType::Card,
            found: RefType::Value(FieldType::Bool),
        },
    );
}

#[test]
fn ambiguous_ref_is_rejected_at_load() {
    // The creature declares `cost` as a field and, here, also as an instance state. Card type
    // validation accepts that; a ref to the name cannot tell which one is meant.
    let both = edit(
        &small_cut(),
        "      - { name: recently_played,  type: bool, init: false }\n",
        "      - { name: recently_played,  type: bool, init: false }\n      - { name: cost, type: int, init: { from_field: cost } }\n",
    );
    assert!(load_ruleset_definition(&both.replace("card.cost", "card.power")).is_ok());
    assert_definition_error(
        &both,
        in_play_creature()
            .key("cost")
            .index(0)
            .key("tap")
            .key("count"),
        DefinitionErrorKind::AmbiguousReference {
            name: "cost".to_string(),
        },
    );
}

#[test]
fn malformed_ref_is_an_invalid_reference() {
    let tap_count = in_play_creature()
        .key("cost")
        .index(0)
        .key("tap")
        .key("count");
    for reference in ["card.cost.x", "Card", "", "card.", ".cost", "card cost"] {
        let yaml = edit(
            &small_cut(),
            "{ ref: card.cost }",
            &format!("{{ ref: \"{reference}\" }}"),
        );
        assert_definition_error(
            &yaml,
            tap_count.clone(),
            DefinitionErrorKind::InvalidReference {
                reference: reference.to_string(),
            },
        );
    }

    let in_move = edit(
        &small_cut(),
        "- move: { card: { ref: card }, to: field }",
        "- move: { card: { ref: \"card.cost.x\" }, to: field }",
    );
    assert_definition_error(
        &in_move,
        in_play_creature()
            .key("effects")
            .index(0)
            .key("move")
            .key("card"),
        DefinitionErrorKind::InvalidReference {
            reference: "card.cost.x".to_string(),
        },
    );
}

// ---- the other load checks

#[test]
fn a_param_must_name_a_declared_zone_and_card_type() {
    let param = "{ name: card, zone: hand, card_type: creature }";
    let yaml = edit(
        &small_cut(),
        param,
        "{ name: card, zone: nowhere, card_type: creature }",
    );
    assert_definition_error(
        &yaml,
        in_play_creature().key("params").id("card").key("zone"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Zone,
            id: "nowhere".to_string(),
        },
    );

    let yaml = edit(
        &small_cut(),
        param,
        "{ name: card, zone: hand, card_type: ghost }",
    );
    assert_definition_error(
        &yaml,
        in_play_creature().key("params").id("card").key("card_type"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::CardType,
            id: "ghost".to_string(),
        },
    );
}

#[test]
fn param_names_are_identifiers_and_unique_within_their_action() {
    let param = "      - { name: card, zone: hand, card_type: creature }\n";
    let duplicated = edit(
        &small_cut(),
        param,
        &format!("{param}      - {{ name: card, zone: field, card_type: creature }}\n"),
    );
    assert_definition_error(
        &duplicated,
        in_play_creature().key("params").index(1),
        DefinitionErrorKind::DuplicateId {
            id: "card".to_string(),
            first: in_play_creature().key("params").index(0),
        },
    );

    let invalid = edit(
        &small_cut(),
        "{ name: card, zone: hand, card_type: creature }",
        "{ name: Card, zone: hand, card_type: creature }",
    );
    assert_definition_error(
        &invalid,
        in_play_creature().key("params").index(0).key("name"),
        DefinitionErrorKind::InvalidId {
            id: "Card".to_string(),
        },
    );

    // The same name in another action is fine: params belong to their action.
    let reused = small_with(MAIN_WITH_PLAY, EXTRA_ACTIONS);
    assert!(load_ruleset_definition(&reused).is_ok());
}

#[test]
fn a_cost_and_the_effects_must_name_declared_zones_and_states() {
    let yaml = edit(
        &small_cut(),
        "tap: { zone: mana_zone,",
        "tap: { zone: nowhere,",
    );
    assert_definition_error(
        &yaml,
        in_play_creature()
            .key("cost")
            .index(0)
            .key("tap")
            .key("zone"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Zone,
            id: "nowhere".to_string(),
        },
    );

    let yaml = edit(&small_cut(), "to: field }", "to: nowhere }");
    assert_definition_error(
        &yaml,
        in_play_creature()
            .key("effects")
            .index(0)
            .key("move")
            .key("to"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Zone,
            id: "nowhere".to_string(),
        },
    );

    let set_state = "state: recently_played, value: true }";
    let yaml = edit(&small_cut(), set_state, "state: nonsense, value: true }");
    assert_definition_error(
        &yaml,
        in_play_creature()
            .key("effects")
            .index(1)
            .key("set_state")
            .key("state"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::InstanceState,
            id: "nonsense".to_string(),
        },
    );

    // A state that only another card type declares is not one of the param's card.
    let yaml = edit(&small_cut(), set_state, "state: current_life, value: 3 }");
    assert_definition_error(
        &yaml,
        in_play_creature()
            .key("effects")
            .index(1)
            .key("set_state")
            .key("state"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::InstanceState,
            id: "current_life".to_string(),
        },
    );
}

#[test]
fn a_set_state_value_must_have_the_type_of_the_state() {
    let yaml = edit(
        &small_cut(),
        "state: recently_played, value: true }",
        "state: recently_played, value: 5 }",
    );
    assert_definition_error(
        &yaml,
        in_play_creature()
            .key("effects")
            .index(1)
            .key("set_state")
            .key("value"),
        DefinitionErrorKind::StateValueTypeMismatch {
            state: "recently_played".to_string(),
            declared: FieldType::Bool,
            found: FieldType::Int,
        },
    );
}

#[test]
fn a_tap_cost_needs_a_card_type_that_declares_tapped() {
    // No card type declares `tapped` (the cut's `untap` would fail too, but actions are checked
    // before the turn).
    let yaml = small_cut().replace("name: tapped,", "name: exhausted,");
    assert_definition_error(
        &yaml,
        in_play_creature().key("cost").index(0).key("tap"),
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::InstanceState,
            id: "tapped".to_string(),
        },
    );
}

#[test]
fn an_action_is_validated_params_then_cost_then_effects() {
    // Errors in all three: the param's, then (once fixed) the cost's, then the effects'.
    let all_wrong = edit(
        &small_cut(),
        "{ name: card, zone: hand, card_type: creature }",
        "{ name: card, zone: nowhere, card_type: creature }",
    )
    .replace("{ ref: card.cost }", "{ ref: spell.cost }")
    .replace("to: field }", "to: nowhere }");
    assert_eq!(
        definition_error(&all_wrong).path,
        in_play_creature().key("params").id("card").key("zone")
    );

    let cost_and_effects = all_wrong.replace("zone: nowhere, card_type", "zone: hand, card_type");
    assert_eq!(
        definition_error(&cost_and_effects).path,
        in_play_creature()
            .key("cost")
            .index(0)
            .key("tap")
            .key("count")
    );

    let effects_only = cost_and_effects.replace("spell.cost", "card.cost");
    assert_eq!(
        definition_error(&effects_only).path,
        in_play_creature()
            .key("effects")
            .index(0)
            .key("move")
            .key("to")
    );
}

// ---- playing a creature

#[test]
fn playing_a_creature_taps_mana_moves_it_to_the_field_and_marks_it_recently_played() {
    let ruleset = small();
    let state = opening(&ruleset);
    let hand = cards_in(&state, 0, "hand");
    let creature = hand_card_with_cost(&ruleset, &state, 0, 2);
    assert_eq!(cost_of(&ruleset, &state, creature), 2);
    let mana = cards_in(&state, 0, "mana_zone");
    assert_eq!(mana.len(), 2, "Player 0 has 2 mana on the first Turn");
    let before = state.clone();

    let transition = accepted(&ruleset, &state, &play(0, creature));

    // One tap per point of cost, on the first mana cards, then the move, then the mark.
    assert_eq!(
        transition.trace.events(),
        [
            changed(mana[0], "tapped", true),
            changed(mana[1], "tapped", true),
            moved(creature, 0, "hand", "field"),
            changed(creature, "recently_played", true),
        ]
    );
    let next = &transition.state;
    assert_eq!(
        cards_in(next, 0, "hand"),
        hand.iter()
            .copied()
            .filter(|&card| card != creature)
            .collect::<Vec<_>>()
    );
    assert_eq!(cards_in(next, 0, "field"), [creature]);
    assert_eq!(
        cards_in(next, 0, "mana_zone"),
        mana,
        "mana stays in the zone"
    );
    for &card in &mana {
        assert_eq!(
            state_value(next, card, "tapped"),
            Some(FieldValue::Bool(true))
        );
    }
    assert_eq!(
        state_value(next, creature, "recently_played"),
        Some(FieldValue::Bool(true))
    );
    assert_eq!(
        state_value(next, creature, "tapped"),
        Some(FieldValue::Bool(false))
    );
    assert_eq!(next.active_player(), p(0));
    assert_eq!(next.turn_number(), TurnNumber::new(1));
    assert_eq!(next.phase(), Some(&PhaseId::new("main")));
    assert_eq!(cards_in(next, 1, "hand"), cards_in(&state, 1, "hand"));

    // The input is untouched, and the trace reproduces the next state (ADR-0010).
    assert_eq!(state, before);
    assert_eq!(
        state.apply_trace(transition.trace.events()),
        Ok(next.clone())
    );
}

#[test]
fn playing_a_creature_of_cost_one_taps_one_mana() {
    let ruleset = small();
    let state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 1);
    assert_eq!(cost_of(&ruleset, &state, creature), 1);
    let mana = cards_in(&state, 0, "mana_zone");

    let transition = accepted(&ruleset, &state, &play(0, creature));
    assert_eq!(
        transition.trace.events(),
        [
            changed(mana[0], "tapped", true),
            moved(creature, 0, "hand", "field"),
            changed(creature, "recently_played", true),
        ]
    );
    assert_eq!(
        state_value(&transition.state, mana[1], "tapped"),
        Some(FieldValue::Bool(false))
    );
}

#[test]
fn tap_cost_taps_the_first_untapped_cards_in_zone_order() {
    let ruleset = small();
    let mut state = opening(&ruleset);
    // Two more mana cards for Player 0, as a later Turn gives them: the Mana Zone is
    // [m0, m1, m2, m3], and m1 is already tapped.
    let (first, second) = (
        from_top(&state, 0, "mana_deck", 0),
        from_top(&state, 0, "mana_deck", 1),
    );
    apply_all(
        &mut state,
        &[
            moved(first, 0, "mana_deck", "mana_zone"),
            moved(second, 0, "mana_deck", "mana_zone"),
        ],
    );
    let mana = cards_in(&state, 0, "mana_zone");
    assert_eq!(mana.len(), 4);
    apply_all(&mut state, &[changed(mana[1], "tapped", true)]);

    // Cost 2: the first two untapped cards in list order, skipping the tapped m1 (not sorted, not
    // the last ones).
    let creature = hand_card_with_cost(&ruleset, &state, 0, 2);
    let transition = accepted(&ruleset, &state, &play(0, creature));
    assert_eq!(
        transition.trace.events(),
        [
            changed(mana[0], "tapped", true),
            changed(mana[2], "tapped", true),
            moved(creature, 0, "hand", "field"),
            changed(creature, "recently_played", true),
        ]
    );
    assert_eq!(
        state_value(&transition.state, mana[3], "tapped"),
        Some(FieldValue::Bool(false))
    );

    // Cost 1 next: the only untapped card is left.
    let another = hand_card_with_cost(&ruleset, &transition.state, 0, 1);
    let next = accepted(&ruleset, &transition.state, &play(0, another));
    assert_eq!(
        next.trace.events(),
        [
            changed(mana[3], "tapped", true),
            moved(another, 0, "hand", "field"),
            changed(another, "recently_played", true),
        ]
    );
    assert_eq!(cards_in(&next.state, 0, "field"), [creature, another]);
}

#[test]
fn ref_resolves_an_instance_state_value() {
    // `play_creature` taps `card.price`, an int instance state that starts at 1 and differs from
    // the card's `cost` field.
    let ruleset = ruleset_from(&priced_cut());
    let state = opening(&ruleset);
    let boar = hand_card_with_cost(&ruleset, &state, 0, 4);
    assert_eq!(cost_of(&ruleset, &state, boar), 4);
    let mana = cards_in(&state, 0, "mana_zone");

    // The state is 1, so one mana is tapped, though the field says 4.
    let transition = accepted(&ruleset, &state, &play(0, boar));
    assert_eq!(
        transition.trace.events(),
        [
            changed(mana[0], "tapped", true),
            moved(boar, 0, "hand", "field"),
            changed(boar, "recently_played", true),
        ]
    );

    // The value is read when the Action runs: after the state changes, the cost follows it.
    let mut raised = state.clone();
    apply_all(&mut raised, &[price_set(boar, 2)]);
    let transition = accepted(&ruleset, &raised, &play(0, boar));
    assert_eq!(
        transition.trace.events(),
        [
            changed(mana[0], "tapped", true),
            changed(mana[1], "tapped", true),
            moved(boar, 0, "hand", "field"),
            changed(boar, "recently_played", true),
        ]
    );
}

#[test]
fn a_cost_of_zero_taps_nothing() {
    let ruleset = ruleset_from(&priced_cut());
    let mut state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 4);
    apply_all(&mut state, &[price_set(creature, 0)]);

    let transition = accepted(&ruleset, &state, &play(0, creature));
    assert_eq!(
        transition.trace.events(),
        [
            moved(creature, 0, "hand", "field"),
            changed(creature, "recently_played", true),
        ]
    );
    for mana in cards_in(&transition.state, 0, "mana_zone") {
        assert_eq!(
            state_value(&transition.state, mana, "tapped"),
            Some(FieldValue::Bool(false))
        );
    }
}

#[test]
fn a_negative_count_is_rejected() {
    let ruleset = ruleset_from(&priced_cut());
    let mut state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 4);
    apply_all(&mut state, &[price_set(creature, -1)]);

    assert_rejected(
        &ruleset,
        &state,
        &play(0, creature),
        Rejection::NegativeCount { value: -1 },
    );
}

#[test]
fn move_reports_the_zone_the_card_is_in() {
    let ruleset = ruleset_from(&small_with(
        "actions: [play_creature, bounce, calm, pass_turn]",
        EXTRA_ACTIONS,
    ));
    let state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 1);
    let played = accepted(&ruleset, &state, &play(0, creature)).state;

    // `bounce` takes a card from the field back to the hand: `from` is the field, not the hand.
    let transition = accepted(&ruleset, &played, &action(0, "bounce", &[creature]));
    assert_eq!(
        transition.trace.events(),
        [moved(creature, 0, "field", "hand")]
    );
    assert!(cards_in(&transition.state, 0, "field").is_empty());
    assert!(cards_in(&transition.state, 0, "hand").contains(&creature));
}

#[test]
fn set_state_emits_an_event_only_when_the_value_changes() {
    let ruleset = ruleset_from(&small_with(
        "actions: [play_creature, bounce, calm, pass_turn]",
        EXTRA_ACTIONS,
    ));
    let state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 1);

    // `calm` sets `recently_played` to false (it already is: no Event), then to true (an Event).
    let transition = accepted(&ruleset, &state, &action(0, "calm", &[creature]));
    assert_eq!(
        transition.trace.events(),
        [changed(creature, "recently_played", true)]
    );
}

#[test]
fn zone_of_is_derived_from_the_owners_zones() {
    let ruleset = small();
    let state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 1);
    assert_eq!(state.zone_of(creature), Some(&zone("hand")));
    assert_eq!(
        state.zone_of(from_top(&state, 1, "deck", 0)),
        Some(&zone("deck"))
    );
    assert_eq!(state.zone_of(CardInstanceId(9_999)), None);

    let played = accepted(&ruleset, &state, &play(0, creature)).state;
    assert_eq!(played.zone_of(creature), Some(&zone("field")));
}

// ---- rejections

#[test]
fn playing_a_card_not_in_hand_is_rejected() {
    let ruleset = small();
    let state = opening(&ruleset);
    let in_deck = from_top(&state, 0, "deck", 0);
    assert_rejected(
        &ruleset,
        &state,
        &play(0, in_deck),
        Rejection::CardNotInZone {
            param: card_param(),
            card: in_deck,
            zone: zone("hand"),
        },
    );

    // A creature that was played is on the field, no longer in hand.
    let creature = hand_card_with_cost(&ruleset, &state, 0, 1);
    let played = accepted(&ruleset, &state, &play(0, creature)).state;
    assert_rejected(
        &ruleset,
        &played,
        &play(0, creature),
        Rejection::CardNotInZone {
            param: card_param(),
            card: creature,
            zone: zone("hand"),
        },
    );
}

#[test]
fn playing_a_card_of_another_card_type_is_rejected() {
    let ruleset = small();
    let mut state = opening(&ruleset);
    // A mana card drawn to the hand.
    let mana = from_top(&state, 0, "mana_deck", 0);
    apply_all(&mut state, &[moved(mana, 0, "mana_deck", "hand")]);

    assert_rejected(
        &ruleset,
        &state,
        &play(0, mana),
        Rejection::WrongCardType {
            param: card_param(),
            card: mana,
            expected: CardTypeId::new("creature"),
            found: CardTypeId::new("mana"),
        },
    );
}

#[test]
fn a_card_in_the_wrong_zone_is_reported_before_its_card_type() {
    let ruleset = small();
    let state = opening(&ruleset);
    // A mana card in the mana deck is in the wrong zone and of the wrong type: the zone is checked
    // first.
    let mana = from_top(&state, 0, "mana_deck", 0);
    assert_rejected(
        &ruleset,
        &state,
        &play(0, mana),
        Rejection::CardNotInZone {
            param: card_param(),
            card: mana,
            zone: zone("hand"),
        },
    );
}

#[test]
fn playing_an_opponents_card_is_rejected() {
    let ruleset = small();
    let state = opening(&ruleset);
    // The card is in the opponent's hand, so it is not in the actor's hand either; the owner is
    // checked first.
    let theirs = hand_card_with_cost(&ruleset, &state, 1, 1);
    assert_rejected(
        &ruleset,
        &state,
        &play(0, theirs),
        Rejection::NotCardOwner {
            param: card_param(),
            card: theirs,
            owner: p(1),
        },
    );
}

#[test]
fn playing_without_enough_mana_is_rejected() {
    let ruleset = small();
    let state = opening(&ruleset);
    let boar = hand_card_with_cost(&ruleset, &state, 0, 4);
    assert_eq!(cost_of(&ruleset, &state, boar), 4);
    assert_rejected(
        &ruleset,
        &state,
        &play(0, boar),
        Rejection::NotEnoughUntappedCards {
            zone: zone("mana_zone"),
            required: 4,
            available: 2,
        },
    );

    // Only the actor's untapped mana counts: Player 1 has mana too, and Player 0 has tapped one.
    let mut state = state;
    let theirs = (
        from_top(&state, 1, "mana_deck", 0),
        from_top(&state, 1, "mana_deck", 1),
    );
    let own = cards_in(&state, 0, "mana_zone");
    apply_all(
        &mut state,
        &[
            moved(theirs.0, 1, "mana_deck", "mana_zone"),
            moved(theirs.1, 1, "mana_deck", "mana_zone"),
            changed(own[0], "tapped", true),
        ],
    );
    let otter = hand_card_with_cost(&ruleset, &state, 0, 2);
    assert_rejected(
        &ruleset,
        &state,
        &play(0, otter),
        Rejection::NotEnoughUntappedCards {
            zone: zone("mana_zone"),
            required: 2,
            available: 1,
        },
    );
}

#[test]
fn playing_outside_a_phase_that_lists_it_is_rejected() {
    // `play_creature` is declared but Main lists only `pass_turn`.
    let ruleset = ruleset_from(&small_with("actions: [pass_turn]", ""));
    let state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 1);
    let not_in_phase = Rejection::ActionNotInCurrentPhase {
        action: ActionId::new("play_creature"),
    };
    assert_rejected(&ruleset, &state, &play(0, creature), not_in_phase.clone());
    // The phase is checked before the arguments.
    assert_rejected(
        &ruleset,
        &state,
        &action(0, "play_creature", &[]),
        not_in_phase,
    );
}

#[test]
fn playing_on_the_opponents_turn_is_rejected() {
    let ruleset = small();
    let state = opening(&ruleset);
    let theirs = hand_card_with_cost(&ruleset, &state, 1, 1);
    let not_active = Rejection::NotActivePlayer {
        player: p(1),
        active: p(0),
    };
    assert_rejected(&ruleset, &state, &play(1, theirs), not_active.clone());
    // The Player is checked before the arguments.
    assert_rejected(
        &ruleset,
        &state,
        &action(1, "play_creature", &[]),
        not_active,
    );
}

#[test]
fn an_unknown_card_is_rejected() {
    let ruleset = small();
    let state = opening(&ruleset);
    let nobody = CardInstanceId(9_999);
    assert_rejected(
        &ruleset,
        &state,
        &play(0, nobody),
        Rejection::UnknownCard {
            param: card_param(),
            card: nobody,
        },
    );
}

#[test]
fn wrong_argument_count_is_rejected() {
    let ruleset = small();
    let state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 1);
    let nobody = CardInstanceId(9_999);

    assert_rejected(
        &ruleset,
        &state,
        &action(0, "play_creature", &[]),
        Rejection::ArgumentCountMismatch {
            expected: 1,
            found: 0,
        },
    );
    // The count is checked before any card: the second card is unknown too.
    assert_rejected(
        &ruleset,
        &state,
        &action(0, "play_creature", &[creature, nobody]),
        Rejection::ArgumentCountMismatch {
            expected: 1,
            found: 2,
        },
    );
    assert_rejected(
        &ruleset,
        &state,
        &action(0, "pass_turn", &[creature]),
        Rejection::ArgumentCountMismatch {
            expected: 0,
            found: 1,
        },
    );
}

// ---- scenarios

#[test]
fn a_played_creature_is_ready_on_its_owners_next_turn() {
    let ruleset = small();
    let state = opening(&ruleset);
    let creature = hand_card_with_cost(&ruleset, &state, 0, 2);
    let mana = cards_in(&state, 0, "mana_zone");

    // Turn 1: Player 0 plays a creature and passes.
    let played = accepted(&ruleset, &state, &play(0, creature)).state;
    let turn_2 = accepted(&ruleset, &played, &pass(0)).state;
    assert_eq!(turn_2.turn_number(), TurnNumber::new(2));

    // Turn 2 is Player 1's: Player 0's mana stays tapped and the creature stays recently played.
    for &card in &mana {
        assert_eq!(
            state_value(&turn_2, card, "tapped"),
            Some(FieldValue::Bool(true))
        );
    }
    assert_eq!(
        state_value(&turn_2, creature, "recently_played"),
        Some(FieldValue::Bool(true))
    );

    // Turn 3: Player 0's Start phase untaps the mana, and clears `recently_played`.
    let transition = accepted(&ruleset, &turn_2, &pass(1));
    let events = transition.trace.events();
    let position = |event: &Event| {
        events
            .iter()
            .position(|found| found == event)
            .expect("the Start phase emits the Event")
    };
    assert!(
        position(&changed(mana[0], "tapped", false))
            < position(&changed(creature, "recently_played", false)),
        "the mana is untapped, then the field is cleared"
    );
    assert!(events.contains(&changed(mana[1], "tapped", false)));

    let turn_3 = transition.state;
    assert_eq!(turn_3.turn_number(), TurnNumber::new(3));
    assert_eq!(turn_3.active_player(), p(0));
    for &card in &mana {
        assert_eq!(
            state_value(&turn_3, card, "tapped"),
            Some(FieldValue::Bool(false))
        );
    }
    assert_eq!(
        state_value(&turn_3, creature, "recently_played"),
        Some(FieldValue::Bool(false))
    );
    assert_eq!(cards_in(&turn_3, 0, "field"), [creature]);
    assert_eq!(cards_in(&turn_3, 0, "mana_zone").len(), 4);
}

/// One full run: the active Player plays, in hand order, every creature they can pay for, then
/// passes; for 8 Turns. Returns each accepted transition: the state after it and its trace.
fn run() -> Vec<(GameState, Vec<Event>)> {
    let ruleset = small();
    let mut state = opening(&ruleset);
    let mut steps = Vec::new();
    for _ in 0..8 {
        let player = state.active_player();
        loop {
            let untapped = cards_in(&state, player.0, "mana_zone")
                .into_iter()
                .filter(|&card| {
                    state_value(&state, card, "tapped") == Some(FieldValue::Bool(false))
                })
                .count();
            let payable = cards_in(&state, player.0, "hand")
                .into_iter()
                .find(|&card| {
                    usize::try_from(cost_of(&ruleset, &state, card))
                        .is_ok_and(|cost| cost <= untapped)
                });
            let Some(card) = payable else { break };
            let transition = accepted(&ruleset, &state, &play(player.0, card));
            steps.push((transition.state.clone(), transition.trace.into_events()));
            state = transition.state;
        }
        let transition = accepted(&ruleset, &state, &pass(player.0));
        steps.push((transition.state.clone(), transition.trace.into_events()));
        state = transition.state;
    }
    steps
}

#[test]
fn playing_creatures_is_deterministic() {
    let first = run();
    let second = run();
    assert_eq!(first, second, "every state and every trace is equal");

    // The run did play creatures: the last state has some on the fields.
    let (last, _) = first.last().expect("the run has steps");
    let on_fields = cards_in(last, 0, "field").len() + cards_in(last, 1, "field").len();
    assert!(on_fields > 0, "creatures were played");

    // Each trace reproduces the state it came with.
    let ruleset = small();
    let mut previous = opening(&ruleset);
    for (next, trace) in &first {
        assert_eq!(previous.apply_trace(trace).as_ref(), Ok(next));
        previous = next.clone();
    }
}
