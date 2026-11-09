//! Acceptance tests for ticket #22: the loader reads `turn` and `actions` (syntax only) into the
//! draft, and the engine checks their meaning.
//!
//! The other sections are tested in `load_ruleset_definition.rs`; the loaded Small Ruleset running
//! its turns is tested in `turn_cycle.rs`.

use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    ActionDraft, AutomaticStepDraft, ConditionDraft, DefinitionError, DefinitionErrorKind,
    EffectDraft, FieldType, FieldValue, PhaseDraft, TurnDraft,
};
use tcg_loader::{
    load_ruleset_definition, parse_ruleset_draft, LoadError, YamlError, YamlErrorKind,
};

/// The structure of the Small Ruleset: meta, parameters, zones, card types and inputs.
const FIXTURE: &str = include_str!("fixtures/small-ruleset-structure.yaml");

/// The turn and the actions of the Small Ruleset's Start, Main and End, as the file spells them,
/// with Main waiting for `pass_turn` only.
const TURN_YAML: &str = "\
turn:
  phases:
    - id: start
      automatic:
        - untap: { player: active, zones: [mana_zone, field] }
        - set_state: { player: active, zone: field, state: recently_played, value: false }
        - if: { first_turn: {} }
          then: []
          else:
            - draw: { player: active, from: deck, to: hand, count: 1 }
        - move: { player: active, from: mana_deck, to: mana_zone, count: 2, position: top }
    - id: main
      actions: [pass_turn]
    - id: end
      automatic: []

actions:
  - id: pass_turn
    effects: [ { end_turn: {} } ]
";

/// The `turn` of a document whose single automatic phase `start` has the given steps (each line of
/// `steps` is indented by the caller).
fn start_phase(steps: &str) -> String {
    format!("turn:\n  phases:\n    - id: start\n      automatic:\n{steps}")
}

fn document(sections: &str) -> String {
    format!("{FIXTURE}{sections}")
}

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

/// The path of step `step` of the first phase's automatic list, as the loader reports it.
fn positional_step(step: usize) -> DefinitionPath {
    path()
        .key("turn")
        .key("phases")
        .index(0)
        .key("automatic")
        .index(step)
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

/// The first step of the phase `start` of a document.
fn first_step(steps: &str) -> AutomaticStepDraft {
    let draft = parse_ruleset_draft(&document(&start_phase(steps))).expect("the document parses");
    let turn = draft.turn.expect("the document has a turn");
    match turn.phases.into_iter().next() {
        Some(PhaseDraft {
            automatic: Some(steps),
            ..
        }) => steps.into_iter().next().expect("the phase has a step"),
        other => panic!("expected an automatic phase, got {other:?}"),
    }
}

#[test]
fn turn_and_actions_are_loaded_from_yaml() {
    let draft = parse_ruleset_draft(&document(TURN_YAML)).expect("the document parses");
    assert_eq!(
        draft.turn,
        Some(TurnDraft {
            phases: vec![
                PhaseDraft {
                    id: "start".to_string(),
                    automatic: Some(vec![
                        AutomaticStepDraft::Untap {
                            player: "active".to_string(),
                            zones: vec!["mana_zone".to_string(), "field".to_string()],
                        },
                        AutomaticStepDraft::SetState {
                            player: "active".to_string(),
                            zone: "field".to_string(),
                            state: "recently_played".to_string(),
                            value: FieldValue::Bool(false),
                        },
                        AutomaticStepDraft::If {
                            condition: ConditionDraft::FirstTurn,
                            then: vec![],
                            otherwise: vec![AutomaticStepDraft::Draw {
                                player: "active".to_string(),
                                from: "deck".to_string(),
                                to: "hand".to_string(),
                                count: 1,
                            }],
                        },
                        AutomaticStepDraft::Move {
                            player: "active".to_string(),
                            from: "mana_deck".to_string(),
                            to: "mana_zone".to_string(),
                            count: 2,
                            position: "top".to_string(),
                        },
                    ]),
                    actions: None,
                },
                PhaseDraft {
                    id: "main".to_string(),
                    automatic: None,
                    actions: Some(vec!["pass_turn".to_string()]),
                },
                PhaseDraft {
                    id: "end".to_string(),
                    automatic: Some(vec![]),
                    actions: None,
                },
            ],
        })
    );
    assert_eq!(
        draft.actions,
        vec![ActionDraft {
            id: "pass_turn".to_string(),
            params: vec![],
            cost: vec![],
            effects: vec![EffectDraft::EndTurn],
        }]
    );

    // The whole document loads and validates.
    let definition = load_ruleset_definition(&document(TURN_YAML)).expect("a valid ruleset");
    assert!(definition.turn().is_some());
    assert_eq!(definition.actions().len(), 1);
}

#[test]
fn turn_and_actions_are_optional() {
    let draft = parse_ruleset_draft(FIXTURE).expect("the fixture parses");
    assert_eq!(draft.turn, None);
    assert_eq!(draft.actions, vec![]);
}

#[test]
fn an_if_step_may_leave_out_else() {
    let step = first_step(
        "        - if: { first_turn: {} }\n          then:\n            - draw: { player: each, from: deck, to: hand, count: 2 }\n",
    );
    assert_eq!(
        step,
        AutomaticStepDraft::If {
            condition: ConditionDraft::FirstTurn,
            then: vec![AutomaticStepDraft::Draw {
                player: "each".to_string(),
                from: "deck".to_string(),
                to: "hand".to_string(),
                count: 2,
            }],
            otherwise: vec![],
        }
    );
}

#[test]
fn if_steps_nest() {
    let step = first_step(
        "        - if: { first_turn: {} }\n          then:\n            - if: { first_turn: {} }\n              then: []\n              else: []\n          else: []\n",
    );
    assert_eq!(
        step,
        AutomaticStepDraft::If {
            condition: ConditionDraft::FirstTurn,
            then: vec![AutomaticStepDraft::If {
                condition: ConditionDraft::FirstTurn,
                then: vec![],
                otherwise: vec![],
            }],
            otherwise: vec![],
        }
    );
}

#[test]
fn a_set_state_value_keeps_its_yaml_kind() {
    let steps =
        "        - set_state: { player: active, zone: field, state: tapped, value: \"false\" }\n";
    assert_eq!(
        first_step(steps),
        AutomaticStepDraft::SetState {
            player: "active".to_string(),
            zone: "field".to_string(),
            state: "tapped".to_string(),
            value: FieldValue::Text("false".to_string()),
        }
    );

    // The engine, not the loader, reports the type.
    assert_eq!(
        definition_error(&document(&start_phase(steps))),
        DefinitionError {
            path: path()
                .key("turn")
                .key("phases")
                .id("start")
                .key("automatic")
                .index(0)
                .key("set_state")
                .key("value"),
            kind: DefinitionErrorKind::StateValueTypeMismatch {
                state: "tapped".to_string(),
                declared: FieldType::Bool,
                found: FieldType::Text,
            },
        }
    );
}

#[test]
fn an_unknown_step_primitive_is_an_unknown_key_of_its_list_element() {
    let error = yaml_error(&document(&start_phase(
        "        - untap: { player: active, zones: [field] }\n        - jump: {}\n",
    )));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "jump".to_string()
        }
    );
    assert_eq!(error.path, positional_step(1));
}

#[test]
fn a_malformed_if_step_is_a_syntax_error_at_its_list_element() {
    for (what, step) in [
        (
            "an if with a primitive beside it",
            "        - if: { first_turn: {} }\n          then: []\n          draw: { player: each, from: deck, to: hand, count: 1 }\n",
        ),
        (
            "an if without then",
            "        - if: { first_turn: {} }\n          else: []\n",
        ),
        ("a then without if", "        - then: []\n"),
    ] {
        let error = yaml_error(&document(&start_phase(step)));
        assert_eq!(error.path, positional_step(0), "{what}");
        assert!(
            matches!(error.kind, YamlErrorKind::Malformed { .. }),
            "{what} gave {:?}",
            error.kind
        );
    }
}

#[test]
fn an_unknown_condition_or_effect_is_an_unknown_key() {
    // An unknown condition primitive is an unknown key of the condition's map, like an unknown step
    // primitive is of its list element.
    let error = yaml_error(&document(&start_phase(
        "        - if: { not: {} }\n          then: []\n",
    )));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "not".to_string()
        }
    );
    assert_eq!(error.path, positional_step(0).key("if"));

    let error = yaml_error(&document(
        "actions:\n  - id: boom\n    effects: [ { explode: {} } ]\n",
    ));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "explode".to_string()
        }
    );
    assert_eq!(
        error.path,
        path().key("actions").index(0).key("effects").index(0)
    );
}

#[test]
fn step_counts_are_unquoted_integers() {
    let error = yaml_error(&document(&start_phase(
        "        - move: { player: active, from: mana_deck, to: mana_zone, count: \"2\", position: top }\n",
    )));
    assert_eq!(error.path, positional_step(0).key("move").key("count"));
    assert!(
        matches!(error.kind, YamlErrorKind::Malformed { .. }),
        "got {:?}",
        error.kind
    );
}

#[test]
fn an_unknown_key_inside_a_phase_or_an_action_is_reported_inside_it() {
    let error = yaml_error(&document(
        "turn:\n  phases:\n    - id: start\n      automatic: []\n      repeat: 2\n",
    ));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "repeat".to_string()
        }
    );
    assert_eq!(
        error.path,
        path().key("turn").key("phases").index(0).key("repeat")
    );

    let error = yaml_error(&document(
        "actions:\n  - id: pass_turn\n    effects: []\n    once_per_turn: true\n",
    ));
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "once_per_turn".to_string()
        }
    );
    assert_eq!(
        error.path,
        path().key("actions").index(0).key("once_per_turn")
    );
}

#[test]
fn turn_meaning_is_checked_by_the_engine_with_phases_addressed_by_id() {
    // A phase with neither `automatic` nor `actions` parses; the engine rejects it.
    assert_eq!(
        definition_error(&document("turn:\n  phases:\n    - id: main\n")),
        DefinitionError {
            path: path().key("turn").key("phases").id("main"),
            kind: DefinitionErrorKind::InvalidPhaseKind,
        }
    );

    // A step that draws from an unordered zone, nested in `else`.
    let error = definition_error(&document(&start_phase(
        "        - if: { first_turn: {} }\n          then: []\n          else:\n            - draw: { player: active, from: hand, to: deck, count: 1 }\n",
    )));
    assert_eq!(
        error,
        DefinitionError {
            path: path()
                .key("turn")
                .key("phases")
                .id("start")
                .key("automatic")
                .index(0)
                .key("else")
                .index(0)
                .key("draw")
                .key("from"),
            kind: DefinitionErrorKind::ZoneNotOrdered {
                zone: "hand".to_string()
            },
        }
    );
}
