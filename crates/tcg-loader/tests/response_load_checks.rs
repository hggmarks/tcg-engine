//! Acceptance tests for ticket #85: the load checks of Response Windows (ADR-0037), on the real
//! Response Ruleset edited as text, one edit per error. Each check is a typed error at its
//! Definition Path, and the real file must load.
//!
//! `windows` is a top-level list `{ id, opened_by, actions }`. An action named in `opened_by` is
//! pushed when taken outside a window, and every action taken in an open window is pushed; a pushed
//! action keeps its card params public, ends no Turn and asks no Choice.

use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    ActionId, Cost, DefinitionError, DefinitionErrorKind, ParamName, ReferenceTarget, WindowId,
};
use tcg_loader::{load_ruleset_definition, LoadError, YamlError, YamlErrorKind};

const RESPONSE_RULESET: &str =
    include_str!("../../../rulesets/response-ruleset/response-ruleset.yaml");

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(
        yaml.matches(from).count(),
        1,
        "`{from}` occurs once in the fixture"
    );
    yaml.replace(from, to)
}

/// The error the engine reports for `yaml`.
fn definition_error(yaml: &str) -> DefinitionError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Definition(error)) => error,
        other => panic!("expected a definition error, got {other:?}"),
    }
}

fn yaml_error(yaml: &str) -> YamlError {
    match load_ruleset_definition(yaml) {
        Err(LoadError::Yaml(error)) => error,
        other => panic!("expected a YAML error, got {other:?}"),
    }
}

fn window(id: &str, opened_by: &str, actions: &str) -> String {
    format!("  - {{ id: {id}, opened_by: [{opened_by}], actions: [{actions}] }}\n")
}

#[test]
fn the_real_response_ruleset_declares_its_window_and_its_costs() {
    let definition = load_ruleset_definition(RESPONSE_RULESET).expect("the real file loads");

    let [window] = definition.windows() else {
        panic!("one window is declared, got {:?}", definition.windows());
    };
    assert_eq!(window.id(), &WindowId::new("attack_response"));
    assert_eq!(window.opened_by(), [ActionId::new("attack")]);
    assert_eq!(window.actions(), [ActionId::new("fast_play")]);
    assert_eq!(
        definition
            .window_opened_by(&ActionId::new("attack"))
            .map(|window| window.id()),
        Some(&WindowId::new("attack_response"))
    );
    assert!(definition
        .window_opened_by(&ActionId::new("fast_play"))
        .is_none());
    assert!(definition
        .window_opened_by(&ActionId::new("play_creature"))
        .is_none());

    // `attack` taps its attacker as a cost; `fast_play` taps mana, then moves the card.
    let attack = definition
        .action(&ActionId::new("attack"))
        .expect("attack is declared");
    let [Cost::TapCard { card }] = attack.costs() else {
        panic!("attack has one `tap: card` cost, got {:?}", attack.costs());
    };
    assert_eq!(card.param(), &ParamName::new("attacker"));
    let fast_play = definition
        .action(&ActionId::new("fast_play"))
        .expect("fast_play is declared");
    let [Cost::Tap { .. }, Cost::Move { card, to }] = fast_play.costs() else {
        panic!("fast_play taps, then moves, got {:?}", fast_play.costs());
    };
    assert_eq!(card.param(), &ParamName::new("card"));
    assert_eq!(to.as_str(), "holding");
}

#[test]
fn an_action_listed_only_by_a_window_is_not_unlisted() {
    // The real file: no phase lists `fast_play`, and it loads.
    let definition = load_ruleset_definition(RESPONSE_RULESET).expect("the real file loads");
    let listed_by_a_phase = definition
        .turn()
        .expect("the ruleset has a turn")
        .phases()
        .iter()
        .any(|phase| {
            matches!(phase.kind(), tcg_engine::ruleset_definition::PhaseKind::Actions(ids)
                if ids.contains(&ActionId::new("fast_play")))
        });
    assert!(!listed_by_a_phase);

    // Without the window's `actions`, nothing lists it.
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        "    actions: []\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(error.kind, DefinitionErrorKind::UnlistedAction);
    assert_eq!(error.path, path().key("actions").id("fast_play"));
}

#[test]
fn an_opened_by_id_that_names_no_action_is_a_dangling_reference() {
    let yaml = edit(
        RESPONSE_RULESET,
        "    opened_by: [attack]\n",
        "    opened_by: [attack, nope]\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Action,
            id: "nope".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("windows")
            .id("attack_response")
            .key("opened_by")
            .index(1)
    );
}

#[test]
fn a_window_action_id_that_names_no_action_is_a_dangling_reference() {
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        "    actions: [fast_play, nope]\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Action,
            id: "nope".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("windows")
            .id("attack_response")
            .key("actions")
            .index(1)
    );
}

#[test]
fn an_action_that_opens_two_windows_is_a_duplicate_window_opener() {
    let extra = window("second", "attack", "fast_play");
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        &format!("    actions: [fast_play]\n{extra}"),
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::DuplicateWindowOpener {
            action: "attack".to_string(),
            first: path()
                .key("windows")
                .id("attack_response")
                .key("opened_by")
                .index(0)
        }
    );
    assert_eq!(
        error.path,
        path().key("windows").id("second").key("opened_by").index(0)
    );

    // Twice in one window is the same error.
    let twice = edit(
        RESPONSE_RULESET,
        "    opened_by: [attack]\n",
        "    opened_by: [attack, attack]\n",
    );
    let error = definition_error(&twice);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::DuplicateWindowOpener {
            action: "attack".to_string(),
            first: path()
                .key("windows")
                .id("attack_response")
                .key("opened_by")
                .index(0)
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("windows")
            .id("attack_response")
            .key("opened_by")
            .index(1)
    );
}

#[test]
fn a_repeated_window_id_is_a_duplicate_id() {
    let extra = window("attack_response", "pass_turn", "fast_play");
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        &format!("    actions: [fast_play]\n{extra}"),
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::DuplicateId {
            id: "attack_response".to_string(),
            first: path().key("windows").index(0)
        }
    );
    assert_eq!(error.path, path().key("windows").index(1));
}

#[test]
fn windows_need_exactly_two_players() {
    let yaml = edit(RESPONSE_RULESET, "players: 2", "players: 3");
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::WindowsNeedTwoPlayers { players: 3 }
    );
    assert_eq!(error.path, path().key("windows"));
}

#[test]
fn a_window_action_that_ends_the_turn_is_rejected() {
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        "    actions: [fast_play, pass_turn]\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(error.kind, DefinitionErrorKind::EndTurnInPushedAction);
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("pass_turn")
            .key("effects")
            .index(0)
    );
}

#[test]
fn an_opened_by_action_that_ends_the_turn_is_rejected() {
    let yaml = edit(
        RESPONSE_RULESET,
        "    opened_by: [attack]\n",
        "    opened_by: [attack, pass_turn]\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(error.kind, DefinitionErrorKind::EndTurnInPushedAction);
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("pass_turn")
            .key("effects")
            .index(0)
    );
}

#[test]
fn a_pushed_card_param_that_is_not_public_and_not_moved_to_a_public_zone_is_rejected() {
    // `fast_play` binds its card from the hand, which only its owner sees; its cost moves the card
    // to `holding`, which every Player sees, and that is what makes it public. Without the move:
    let yaml = edit(
        RESPONSE_RULESET,
        "      - move: { card: { ref: card }, to: holding }\n    effects:\n",
        "    effects:\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::PushedCardNotPublic {
            param: "card".to_string(),
            zone: "hand".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("fast_play")
            .key("params")
            .id("card")
    );

    // A move to a zone that is not public does not make it public either: the zone that counts is
    // the one the card is in once the costs are paid, here the hidden `deck`.
    let hidden = edit(
        RESPONSE_RULESET,
        "      - move: { card: { ref: card }, to: holding }\n",
        "      - move: { card: { ref: card }, to: deck }\n",
    );
    let error = definition_error(&hidden);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::PushedCardNotPublic {
            param: "card".to_string(),
            zone: "deck".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("fast_play")
            .key("params")
            .id("card")
    );
}

#[test]
fn a_pushed_card_is_checked_in_the_zone_it_is_in_once_the_costs_are_paid() {
    // `attack` binds its attacker from the public `field`, but a cost then moves it to the hand,
    // which only its owner sees: the card the Stack would show is not public.
    let yaml = edit(
        RESPONSE_RULESET,
        "      - tap: { card: { ref: attacker } }\n",
        "      - tap: { card: { ref: attacker } }\n      - move: { card: { ref: attacker }, to: hand }\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::PushedCardNotPublic {
            param: "attacker".to_string(),
            zone: "hand".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("attack")
            .key("params")
            .id("attacker")
    );

    // Only the last `move` of a param counts: public, then back to the hand.
    let yaml = edit(
        RESPONSE_RULESET,
        "      - move: { card: { ref: card }, to: holding }\n",
        "      - move: { card: { ref: card }, to: holding }\n      - move: { card: { ref: card }, to: hand }\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::PushedCardNotPublic {
            param: "card".to_string(),
            zone: "hand".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("fast_play")
            .key("params")
            .id("card")
    );
}

#[test]
fn a_pushed_alternative_in_a_non_public_zone_is_rejected_at_its_one_of_position() {
    // The opponent's Leader is taken from `hand` instead of `leader_zone`.
    let yaml = edit(
        RESPONSE_RULESET,
        "{ zone: leader_zone, card_type: leader, owner: opponent }",
        "{ zone: hand, card_type: leader, owner: opponent }",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::PushedCardNotPublic {
            param: "target".to_string(),
            zone: "hand".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("attack")
            .key("params")
            .id("target")
            .key("one_of")
            .index(1)
    );
}

#[test]
fn a_choice_in_a_pushed_action_is_rejected() {
    let yaml = edit(
        RESPONSE_RULESET,
        "      - move: { card: { ref: card }, to: holding }\n    effects:\n",
        "      - move: { card: { ref: card }, to: holding }\n    effects:\n      - choose_one: { decider: controller, options: [ [], [] ] }\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::ChoiceOutsideTriggeredEffect
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("fast_play")
            .key("effects")
            .index(0)
            .key("choose_one")
    );
}

#[test]
fn a_tap_card_cost_needs_every_card_type_of_its_param_to_declare_tapped() {
    // The Leader no longer declares `tapped`, and `attack` taps its target, which may be one.
    let yaml = edit(
        RESPONSE_RULESET,
        "      - { name: tapped,       type: bool, init: false }\n    effects: false\n  - id: creature",
        "    effects: false\n  - id: creature",
    );
    let yaml = edit(
        &yaml,
        "tap: { card: { ref: attacker } }",
        "tap: { card: { ref: target } }",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::TapCardWithoutTappedState {
            card_type: "leader".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("attack")
            .key("cost")
            .index(0)
            .key("tap")
            .key("card")
    );
}

#[test]
fn a_move_cost_has_the_checks_of_a_move_effect() {
    let yaml = edit(
        RESPONSE_RULESET,
        "      - move: { card: { ref: card }, to: holding }\n",
        "      - move: { card: { ref: card }, to: nowhere }\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Zone,
            id: "nowhere".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("fast_play")
            .key("cost")
            .index(1)
            .key("move")
            .key("to")
    );

    let yaml = edit(
        RESPONSE_RULESET,
        "      - move: { card: { ref: card }, to: holding }\n",
        "      - move: { card: { ref: nope }, to: holding }\n",
    );
    let error = definition_error(&yaml);
    assert_eq!(
        error.kind,
        DefinitionErrorKind::DanglingReference {
            target: ReferenceTarget::Param,
            id: "nope".to_string()
        }
    );
    assert_eq!(
        error.path,
        path()
            .key("actions")
            .id("fast_play")
            .key("cost")
            .index(1)
            .key("move")
            .key("card")
    );
}

#[test]
fn a_tap_cost_is_a_zone_and_a_count_or_a_card_and_nothing_else() {
    let tap_path = path()
        .key("actions")
        .index(1)
        .key("cost")
        .index(0)
        .key("tap");

    // Both forms at once.
    let yaml = edit(
        RESPONSE_RULESET,
        "tap: { card: { ref: attacker } }",
        "tap: { zone: mana_zone, card: { ref: attacker } }",
    );
    let error = yaml_error(&yaml);
    assert!(matches!(error.kind, YamlErrorKind::Malformed { .. }));
    assert_eq!(error.path, tap_path);

    // A zone without its count, as before.
    let yaml = edit(
        RESPONSE_RULESET,
        "tap: { card: { ref: attacker } }",
        "tap: { zone: mana_zone }",
    );
    let error = yaml_error(&yaml);
    assert!(matches!(error.kind, YamlErrorKind::Malformed { .. }));
    assert_eq!(error.path, tap_path);

    // Neither form.
    let yaml = edit(
        RESPONSE_RULESET,
        "tap: { card: { ref: attacker } }",
        "tap: {}",
    );
    let error = yaml_error(&yaml);
    assert!(matches!(error.kind, YamlErrorKind::Malformed { .. }));
    assert_eq!(error.path, tap_path);

    // An unknown key is still an unknown key.
    let yaml = edit(
        RESPONSE_RULESET,
        "tap: { card: { ref: attacker } }",
        "tap: { card: { ref: attacker }, extra: 1 }",
    );
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "extra".to_string()
        }
    );
}

#[test]
fn a_window_has_an_id_opened_by_and_actions_and_nothing_else() {
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        "    actions: [fast_play]\n    extra: 1\n",
    );
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "extra".to_string()
        }
    );
    assert_eq!(error.path, path().key("windows").index(0).key("extra"));

    let yaml = edit(RESPONSE_RULESET, "    actions: [fast_play]\n", "");
    let error = yaml_error(&yaml);
    assert!(matches!(error.kind, YamlErrorKind::Malformed { .. }));
    assert_eq!(error.path, path().key("windows").index(0));
}
