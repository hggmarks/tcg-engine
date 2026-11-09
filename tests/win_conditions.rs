//! Acceptance tests for ticket #23: win conditions, the Terminal Outcome and Game over, built from
//! code the way a code frontend would build them.
//!
//! The Small Ruleset's `deck_out` read from the real file, and a Game that ends by deck-out over
//! many Turns, are tested in the loader crate, in `crates/tcg-loader/tests/game_end.rs`.

mod common;

use common::{creature_id, path, small_catalog, small_definition, small_draft};
use tcg_engine::deck_list::{DeckEntryDraft, DeckInputCardsDraft, DeckList, DeckListDraft};
use tcg_engine::declarative::{
    Action, ApplyError, DeclarativeRuleset, Event, GameCreation, GameState, Loss, Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::Ruleset;
use tcg_engine::ruleset_definition::{
    ActionDraft, ActionId, AutomaticStepDraft, DefinitionError, DefinitionErrorKind, EffectDraft,
    Loser, PhaseDraft, PhaseId, ReferenceTarget, RulesetDefinition, RulesetDraft, SetupStepDraft,
    TurnDraft, WinConditionDraft, WinConditionId, WinConditionWhen, WinConditionWhenDraft, ZoneId,
};

// ---- builders

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn reason(id: &str) -> WinConditionId {
    WinConditionId::new(id)
}

fn loss(seat: u32, id: &str) -> Loss {
    Loss {
        player: p(seat),
        reason: reason(id),
    }
}

fn entry(card: &str, count: u32) -> DeckEntryDraft {
    DeckEntryDraft {
        card: card.to_string(),
        count,
    }
}

fn provided(input: &str, cards: Vec<DeckEntryDraft>) -> DeckInputCardsDraft {
    DeckInputCardsDraft {
        input: input.to_string(),
        cards,
    }
}

/// A legal deck: the Leader `leader`, 50 creatures and the 10 mana cards.
fn deck(leader: &str) -> DeckList {
    let mut creatures: Vec<DeckEntryDraft> = (1..=12).map(|n| entry(&creature_id(n), 4)).collect();
    creatures.push(entry(&creature_id(13), 2));
    DeckList::validate(
        &small_definition(),
        &small_catalog(),
        DeckListDraft {
            inputs: vec![
                provided("leader", vec![entry(leader, 1)]),
                provided("main_deck", creatures),
                provided("mana_deck", vec![entry("mana", 10)]),
            ],
        },
    )
    .expect("a legal deck")
}

fn decks() -> Vec<DeckList> {
    vec![deck("hero"), deck("second_hero")]
}

/// The only place this file builds an Action.
fn action(player: u32, id: &str) -> Action {
    Action {
        player: p(player),
        action: ActionId::new(id),
    }
}

fn pass(player: u32) -> Action {
    action(player, "pass_turn")
}

fn draw_step(player: &str) -> AutomaticStepDraft {
    AutomaticStepDraft::Draw {
        player: player.to_string(),
        from: "deck".to_string(),
        to: "hand".to_string(),
        count: 1,
    }
}

fn deck_out(id: &str) -> WinConditionDraft {
    WinConditionDraft {
        id: id.to_string(),
        when: WinConditionWhenDraft::DrawFromEmpty {
            zone: "deck".to_string(),
        },
        loser: "owner".to_string(),
    }
}

/// The code-built Small Ruleset where setup draws `setup_draw` cards from the 50 of each main
/// deck, the Start phase runs `start`, and Main waits for `pass_turn`.
fn draft_with(
    setup_draw: u32,
    start: Vec<AutomaticStepDraft>,
    win_conditions: Vec<WinConditionDraft>,
) -> RulesetDraft {
    let mut draft = small_draft();
    draft.setup = vec![
        SetupStepDraft::Shuffle {
            player: "each".to_string(),
            zone: "deck".to_string(),
        },
        SetupStepDraft::Draw {
            player: "each".to_string(),
            from: "deck".to_string(),
            to: "hand".to_string(),
            count: setup_draw,
        },
    ];
    draft.actions = vec![ActionDraft {
        id: "pass_turn".to_string(),
        effects: vec![EffectDraft::EndTurn],
    }];
    draft.turn = Some(TurnDraft {
        phases: vec![
            PhaseDraft {
                id: "start".to_string(),
                automatic: Some(start),
                actions: None,
            },
            PhaseDraft {
                id: "main".to_string(),
                automatic: None,
                actions: Some(vec!["pass_turn".to_string()]),
            },
        ],
    });
    RulesetDraft {
        win_conditions,
        ..draft
    }
}

fn ruleset_of(draft: RulesetDraft) -> DeclarativeRuleset {
    let definition = RulesetDefinition::validate(draft).expect("a valid draft");
    DeclarativeRuleset::new(definition, small_catalog())
}

fn create(ruleset: &DeclarativeRuleset) -> GameCreation {
    ruleset
        .create_game(&decks(), Seed(1))
        .expect("the decks create a game")
}

/// A Game that is in progress: 5 cards drawn in setup, the Start phase draws for the Active
/// Player, and `deck_out` is the only win condition.
fn game_in_progress() -> (DeclarativeRuleset, GameState) {
    let ruleset = ruleset_of(draft_with(
        5,
        vec![draw_step("active")],
        vec![deck_out("deck_out")],
    ));
    let state = create(&ruleset).opening.state;
    (ruleset, state)
}

fn game_ended(losses: Vec<Loss>) -> Event {
    Event::GameEnded { losses }
}

fn draw_from_empty(seat: u32) -> Event {
    Event::DrawFromEmptyZone {
        player: p(seat),
        zone: zone("deck"),
    }
}

fn definition_error(draft: RulesetDraft) -> DefinitionError {
    RulesetDefinition::validate(draft).expect_err("the draft is invalid")
}

// ---- validation

#[test]
fn win_condition_on_an_unknown_zone_is_a_dangling_reference() {
    let mut condition = deck_out("deck_out");
    condition.when = WinConditionWhenDraft::DrawFromEmpty {
        zone: "graveyard".to_string(),
    };
    let error = definition_error(draft_with(5, vec![], vec![condition]));
    assert_eq!(
        error,
        DefinitionError {
            path: path()
                .key("win_conditions")
                .id("deck_out")
                .key("when")
                .key("draw_from_empty"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "graveyard".to_string()
            },
        }
    );
}

#[test]
fn win_condition_with_an_unknown_loser_is_rejected() {
    let mut condition = deck_out("deck_out");
    condition.loser = "opponent".to_string();
    let error = definition_error(draft_with(5, vec![], vec![condition]));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("win_conditions").id("deck_out").key("loser"),
            kind: DefinitionErrorKind::UnknownLoser {
                loser: "opponent".to_string()
            },
        }
    );
}

#[test]
fn duplicate_win_condition_id_is_rejected() {
    let error = definition_error(draft_with(
        5,
        vec![],
        vec![
            deck_out("deck_out"),
            deck_out("other"),
            deck_out("deck_out"),
        ],
    ));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("win_conditions").index(2),
            kind: DefinitionErrorKind::DuplicateId {
                id: "deck_out".to_string(),
                first: path().key("win_conditions").index(0),
            },
        }
    );
}

#[test]
fn win_condition_with_an_invalid_id_is_rejected() {
    let error = definition_error(draft_with(5, vec![], vec![deck_out("Deck-Out")]));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("win_conditions").index(0).key("id"),
            kind: DefinitionErrorKind::InvalidId {
                id: "Deck-Out".to_string()
            },
        }
    );
}

#[test]
fn a_win_condition_is_checked_by_id_then_when_then_loser() {
    // The id of the second condition is wrong, so it wins over its bad `when` and `loser`.
    let mut broken = deck_out("Broken");
    broken.when = WinConditionWhenDraft::DrawFromEmpty {
        zone: "graveyard".to_string(),
    };
    broken.loser = "opponent".to_string();
    let error = definition_error(draft_with(
        5,
        vec![],
        vec![deck_out("fine"), broken.clone()],
    ));
    assert_eq!(error.path, path().key("win_conditions").index(1).key("id"));

    // With a valid id, the `when` wins over the `loser`.
    broken.id = "broken".to_string();
    let error = definition_error(draft_with(5, vec![], vec![broken.clone()]));
    assert_eq!(
        error.path,
        path()
            .key("win_conditions")
            .id("broken")
            .key("when")
            .key("draw_from_empty")
    );

    // An expression is unsupported before its `loser` is looked at.
    broken.when = WinConditionWhenDraft::Expression {
        operator: "lte".to_string(),
    };
    let error = definition_error(draft_with(5, vec![], vec![broken]));
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("win_conditions").id("broken").key("when"),
            kind: DefinitionErrorKind::UnsupportedWinCondition {
                operator: "lte".to_string()
            },
        }
    );
}

#[test]
fn win_conditions_are_kept_in_declared_order_and_absent_means_none() {
    let definition = RulesetDefinition::validate(draft_with(
        5,
        vec![],
        vec![deck_out("second"), deck_out("first")],
    ))
    .expect("a valid draft");
    let ids: Vec<&str> = definition
        .win_conditions()
        .iter()
        .map(|condition| condition.id().as_str())
        .collect();
    assert_eq!(ids, ["second", "first"]);
    assert_eq!(
        definition.win_conditions()[0].when(),
        &WinConditionWhen::DrawFromEmpty(zone("deck"))
    );
    assert_eq!(definition.win_conditions()[0].loser(), Loser::Owner);

    let none = RulesetDefinition::validate(draft_with(5, vec![], vec![])).expect("a valid draft");
    assert!(none.win_conditions().is_empty());
}

// ---- evaluated at Game creation

#[test]
fn win_conditions_are_evaluated_at_game_creation() {
    // Setup draws all 50 cards of each main deck, and the Start phase of the first Turn draws one
    // more for the Active Player (there is no `first_turn` skip): seat 0 must draw from an empty
    // deck, so the Game is over in the Opening Position.
    let ruleset = ruleset_of(draft_with(
        50,
        vec![draw_step("active")],
        vec![deck_out("deck_out")],
    ));
    let creation = create(&ruleset);

    let outcome = creation
        .opening
        .state
        .outcome()
        .expect("the Opening Position holds a Terminal Outcome");
    assert_eq!(outcome.losses(), [loss(0, "deck_out")]);
    assert_eq!(outcome.winners(), [p(1)]);
    assert!(!outcome.is_drawn_game());

    // The trace ends with the facts and then `GameEnded`, and replaying it reproduces the state.
    let events = creation.opening.trace.events();
    assert_eq!(events.last(), Some(&game_ended(vec![loss(0, "deck_out")])));
    assert!(events.contains(&draw_from_empty(0)));
    assert_eq!(creation.empty.outcome(), None);
    assert_eq!(
        creation.empty.apply_trace(events),
        Ok(creation.opening.state.clone())
    );

    // Any Action is rejected.
    for player in [0, 1] {
        assert_eq!(
            ruleset.transition(&creation.opening.state, &pass(player)),
            Err(Rejection::GameOver)
        );
    }
}

#[test]
fn every_player_losing_in_the_same_transition_is_a_drawn_game() {
    // As above, but the Start phase draws for each Player: both decks are empty.
    let ruleset = ruleset_of(draft_with(
        50,
        vec![draw_step("each")],
        vec![deck_out("deck_out")],
    ));
    let creation = create(&ruleset);

    let outcome = creation.opening.state.outcome().expect("the Game ended");
    assert_eq!(outcome.losses(), [loss(0, "deck_out"), loss(1, "deck_out")]);
    assert!(outcome.winners().is_empty());
    assert!(outcome.is_drawn_game());
    assert_eq!(
        creation.opening.trace.events().last(),
        Some(&game_ended(vec![loss(0, "deck_out"), loss(1, "deck_out")]))
    );
}

#[test]
fn the_reason_is_the_first_declared_win_condition_that_holds() {
    // Two conditions hold for seat 0, and the Start phase draws twice from the empty deck, so there
    // are two `DrawFromEmptyZone` facts. The Player still loses once, for the first declared id.
    for (first, second) in [("a_first", "b_second"), ("b_second", "a_first")] {
        let ruleset = ruleset_of(draft_with(
            50,
            vec![draw_step("active"), draw_step("active")],
            vec![deck_out(first), deck_out(second)],
        ));
        let creation = create(&ruleset);

        let empty_draws = creation
            .opening
            .trace
            .events()
            .iter()
            .filter(|event| **event == draw_from_empty(0))
            .count();
        assert_eq!(empty_draws, 2);
        let outcome = creation.opening.state.outcome().expect("the Game ended");
        assert_eq!(outcome.losses(), [loss(0, first)]);
        assert_eq!(
            creation
                .opening
                .trace
                .events()
                .iter()
                .filter(|event| matches!(event, Event::GameEnded { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn without_win_conditions_an_empty_draw_does_not_end_the_game() {
    let ruleset = ruleset_of(draft_with(50, vec![draw_step("active")], vec![]));
    let creation = create(&ruleset);
    assert_eq!(creation.opening.state.outcome(), None);
    assert!(creation
        .opening
        .trace
        .events()
        .contains(&draw_from_empty(0)));
    assert!(!creation
        .opening
        .trace
        .events()
        .iter()
        .any(|event| matches!(event, Event::GameEnded { .. })));
    // The Game goes on: an Action is accepted, and its transition does not end the Game either.
    let transition = ruleset
        .transition(&creation.opening.state, &pass(0))
        .expect("PassTurn is accepted: the Game is not over");
    assert_eq!(transition.state.outcome(), None);
    assert!(!transition
        .trace
        .events()
        .iter()
        .any(|event| matches!(event, Event::GameEnded { .. })));
}

// ---- Event Application

#[test]
fn game_ended_sets_the_terminal_outcome() {
    let (_, mut state) = game_in_progress();
    assert_eq!(state.outcome(), None);

    state
        .apply(&game_ended(vec![loss(1, "deck_out")]))
        .expect("GameEnded applies");

    let outcome = state.outcome().expect("the Game ended");
    assert_eq!(outcome.losses(), [loss(1, "deck_out")]);
    assert_eq!(outcome.winners(), [p(0)]);
    assert!(!outcome.is_drawn_game());

    // Every Player lost: no winner, a drawn game.
    let (_, mut state) = game_in_progress();
    state
        .apply(&game_ended(vec![loss(0, "deck_out"), loss(1, "deck_out")]))
        .expect("GameEnded applies");
    let outcome = state.outcome().expect("the Game ended");
    assert!(outcome.winners().is_empty());
    assert!(outcome.is_drawn_game());
}

#[test]
fn no_event_applies_after_the_game_ended() {
    let (_, mut state) = game_in_progress();
    state
        .apply(&game_ended(vec![loss(1, "deck_out")]))
        .expect("GameEnded applies");
    let ended = state.clone();

    for event in [
        draw_from_empty(0),
        Event::PhaseBegan {
            phase: PhaseId::new("main"),
        },
        game_ended(vec![loss(0, "deck_out")]),
    ] {
        assert_eq!(state.apply(&event), Err(ApplyError::GameAlreadyEnded));
        assert_eq!(state, ended);
    }

    // A trace stops at the first Event after `GameEnded`.
    let (_, in_progress) = game_in_progress();
    assert_eq!(
        in_progress.apply_trace(&[game_ended(vec![loss(1, "deck_out")]), draw_from_empty(0)]),
        Err(ApplyError::GameAlreadyEnded)
    );
}

#[test]
fn game_ended_without_a_loser_is_rejected() {
    let (_, mut state) = game_in_progress();
    let before = state.clone();
    assert_eq!(state.apply(&game_ended(vec![])), Err(ApplyError::NoLoser));
    assert_eq!(state, before);
    assert_eq!(state.outcome(), None);
}

#[test]
fn game_ended_with_losers_out_of_seat_order_is_rejected() {
    let (_, mut state) = game_in_progress();
    let before = state.clone();

    // Reversed: seat 0 is not after seat 1.
    assert_eq!(
        state.apply(&game_ended(vec![loss(1, "deck_out"), loss(0, "deck_out")])),
        Err(ApplyError::LosersOutOfSeatOrder { player: p(0) })
    );
    assert_eq!(state, before);

    // Duplicate: a Player loses once.
    assert_eq!(
        state.apply(&game_ended(vec![loss(0, "deck_out"), loss(0, "deck_out")])),
        Err(ApplyError::LosersOutOfSeatOrder { player: p(0) })
    );
    assert_eq!(state, before);
    assert_eq!(state.outcome(), None);
}

#[test]
fn game_ended_with_an_unknown_loser_is_rejected() {
    let (_, mut state) = game_in_progress();
    let before = state.clone();
    assert_eq!(
        state.apply(&game_ended(vec![loss(9, "deck_out")])),
        Err(ApplyError::UnknownPlayer { player: p(9) })
    );
    assert_eq!(state, before);
}
