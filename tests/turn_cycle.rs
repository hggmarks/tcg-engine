//! Acceptance tests for ticket #22: running the Turn cycle of the declarative Ruleset (Start-phase
//! automation, PassTurn, the Action rejections), built from code the way a code frontend would
//! build it.
//!
//! The Small Ruleset's `turn` read from the real file, over two full rounds, is tested in the
//! loader crate, in `crates/tcg-loader/tests/turn_cycle.rs`. How the definition of a turn is
//! validated is tested in `tests/ruleset_definition.rs`.

mod common;

use common::{creature_id, path, small_catalog, small_definition, small_draft};
use tcg_engine::deck_list::{DeckEntryDraft, DeckInputCardsDraft, DeckList, DeckListDraft};
use tcg_engine::declarative::{
    Action, ApplyError, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState,
    Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionDraft, ActionId, AutomaticStepDraft, ConditionDraft, DefinitionError,
    DefinitionErrorKind, EffectDraft, FieldTypeDraft, FieldValue, InitializerDraft,
    InstanceStateDraft, PhaseDraft, PhaseId, RulesetDefinition, RulesetDraft, SetupStepDraft,
    StateName, TurnDraft, ZoneId,
};
use tcg_engine::runtime::Runtime;
use tcg_engine::turn::TurnNumber;

// ---- builders

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn phase_id(id: &str) -> PhaseId {
    PhaseId::new(id)
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

fn untap_step(player: &str, zones: &[&str]) -> AutomaticStepDraft {
    AutomaticStepDraft::Untap {
        player: player.to_string(),
        zones: zones.iter().map(|zone| zone.to_string()).collect(),
    }
}

fn set_state_step(player: &str, zone: &str, state: &str, value: FieldValue) -> AutomaticStepDraft {
    AutomaticStepDraft::SetState {
        player: player.to_string(),
        zone: zone.to_string(),
        state: state.to_string(),
        value,
    }
}

fn draw_step(player: &str, from: &str, to: &str, count: u32) -> AutomaticStepDraft {
    AutomaticStepDraft::Draw {
        player: player.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        count,
    }
}

fn move_step(player: &str, from: &str, to: &str, count: u32) -> AutomaticStepDraft {
    AutomaticStepDraft::Move {
        player: player.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        count,
        position: "top".to_string(),
    }
}

/// The Start phase of the Small Ruleset.
fn small_start_steps() -> Vec<AutomaticStepDraft> {
    vec![
        untap_step("active", &["mana_zone", "field"]),
        set_state_step(
            "active",
            "field",
            "recently_played",
            FieldValue::Bool(false),
        ),
        AutomaticStepDraft::If {
            condition: ConditionDraft::FirstTurn,
            then: vec![],
            otherwise: vec![draw_step("active", "deck", "hand", 1)],
        },
        move_step("active", "mana_deck", "mana_zone", 2),
    ]
}

fn automatic_phase(id: &str, steps: Vec<AutomaticStepDraft>) -> PhaseDraft {
    PhaseDraft {
        id: id.to_string(),
        automatic: Some(steps),
        actions: None,
    }
}

fn action_phase(id: &str, actions: &[&str]) -> PhaseDraft {
    PhaseDraft {
        id: id.to_string(),
        automatic: None,
        actions: Some(actions.iter().map(|action| action.to_string()).collect()),
    }
}

fn recently_played() -> InstanceStateDraft {
    InstanceStateDraft {
        name: "recently_played".to_string(),
        state_type: FieldTypeDraft::Bool,
        init: InitializerDraft::Literal(FieldValue::Bool(false)),
    }
}

/// The code-built Small Ruleset with the given Start and End phases, Main waiting for `pass_turn`,
/// and the declared but unlisted action `idle`. Creatures can be `recently_played`; Leaders and
/// mana cards cannot.
fn draft_with_turn(start: Vec<AutomaticStepDraft>, end: Vec<AutomaticStepDraft>) -> RulesetDraft {
    let mut draft = small_draft();
    draft.card_types[1].instance_state.push(recently_played());
    draft.actions = vec![
        ActionDraft {
            id: "pass_turn".to_string(),
            effects: vec![EffectDraft::EndTurn],
        },
        ActionDraft {
            id: "idle".to_string(),
            effects: vec![],
        },
    ];
    draft.turn = Some(TurnDraft {
        phases: vec![
            automatic_phase("start", start),
            action_phase("main", &["pass_turn"]),
            automatic_phase("end", end),
        ],
    });
    draft
}

fn small_turn_draft() -> RulesetDraft {
    draft_with_turn(small_start_steps(), vec![])
}

fn ruleset_of(draft: RulesetDraft) -> DeclarativeRuleset {
    let definition = RulesetDefinition::validate(draft).expect("a valid draft");
    DeclarativeRuleset::new(definition, small_catalog())
}

fn ruleset() -> DeclarativeRuleset {
    ruleset_of(small_turn_draft())
}

fn create_with(ruleset: &DeclarativeRuleset, decks: &[DeckList], seed: u64) -> GameCreation {
    ruleset
        .create_game(decks, Seed(seed))
        .expect("the decks create a game")
}

/// The Game of `ruleset`, as of the end of creation: the first Turn has begun.
fn opening(ruleset: &DeclarativeRuleset) -> GameState {
    create_with(ruleset, &decks(), 1).opening.state
}

fn pass(player: u32) -> Action {
    Action {
        player: p(player),
        action: ActionId::new("pass_turn"),
    }
}

fn pass_turn(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    player: u32,
) -> Transition<GameState, Event> {
    ruleset
        .transition(state, &pass(player))
        .expect("PassTurn is accepted")
}

// ---- Events and reading the state

fn turn_began(player: u32, turn: u32) -> Event {
    Event::TurnBegan {
        player: p(player),
        turn: TurnNumber::new(turn).expect("a Turn Number is at least 1"),
    }
}

fn phase_began(id: &str) -> Event {
    Event::PhaseBegan {
        phase: phase_id(id),
    }
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

fn draw_from_empty(player: u32, zone_id: &str) -> Event {
    Event::DrawFromEmptyZone {
        player: p(player),
        zone: zone(zone_id),
    }
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

/// The `n`-th card from the Top of a zone, 0 being the Top.
fn from_top(state: &GameState, player: u32, zone_id: &str, n: usize) -> CardInstanceId {
    let cards = cards_in(state, player, zone_id);
    *cards
        .iter()
        .rev()
        .nth(n)
        .expect("the zone has that many cards")
}

fn count(state: &GameState, player: u32, zone_id: &str) -> usize {
    cards_in(state, player, zone_id).len()
}

fn state_value(state: &GameState, card: CardInstanceId, name: &str) -> Option<FieldValue> {
    state
        .card(card)
        .and_then(|instance| instance.state_value(&StateName::new(name)))
        .cloned()
}

/// Applies Events that set a scenario up; each must be accepted.
fn apply_all(state: &mut GameState, events: &[Event]) {
    for event in events {
        state.apply(event).expect("the scenario Event is accepted");
    }
}

// ---- the first Turn

#[test]
fn game_creation_begins_the_first_turn_and_waits_in_the_action_phase() {
    let creation = create_with(&ruleset(), &decks(), 1);
    let state = &creation.opening.state;
    assert_eq!(state.turn_number(), Some(TurnNumber::FIRST));
    assert_eq!(state.phase(), Some(&phase_id("main")));
    assert_eq!(state.active_player(), p(0));

    // The Empty GameState has no Turn in progress.
    assert_eq!(creation.empty.turn_number(), None);
    assert_eq!(creation.empty.phase(), None);

    // Card instance ids are dense in creation order: the Leader is 0, the main deck 1 to 50 and the
    // mana deck 51 to 60. The mana deck is not shuffled, so its Top is 60.
    let events = creation.opening.trace.events();
    let turn_events = [
        turn_began(0, 1),
        phase_began("start"),
        // The starting Player skips the draw on the first Turn: only the mana moves.
        moved(CardInstanceId(60), 0, "mana_deck", "mana_zone"),
        moved(CardInstanceId(59), 0, "mana_deck", "mana_zone"),
        phase_began("main"),
    ];
    // 122 cards created, 2 shuffles and 10 draws of setup, then the first Turn.
    assert_eq!(events.len(), 122 + 2 + 10 + turn_events.len());
    assert!(
        events.ends_with(&turn_events),
        "the trace ends with the first Turn: {:?}",
        events.iter().rev().take(6).collect::<Vec<_>>()
    );

    // The Event Trace applied to the Empty GameState gives the Opening Position.
    assert_eq!(
        creation.empty.apply_trace(events),
        Ok(creation.opening.state.clone())
    );
    assert_eq!(
        cards_in(state, 0, "mana_zone"),
        [CardInstanceId(60), CardInstanceId(59)]
    );
}

#[test]
fn a_ruleset_without_a_turn_creates_a_game_without_turns() {
    // `turn` absent: creation stops after setup, and no Action is legal.
    let mut draft = small_turn_draft();
    draft.turn = None;
    let ruleset = ruleset_of(draft);
    let creation = create_with(&ruleset, &decks(), 1);
    let state = &creation.opening.state;
    assert_eq!(state.turn_number(), None);
    assert_eq!(state.phase(), None);
    assert_eq!(state.active_player(), p(0));
    assert!(creation.opening.trace.events().iter().all(|event| matches!(
        event,
        Event::CardCreated { .. } | Event::ZoneShuffled { .. } | Event::CardMoved { .. }
    )));
    assert_eq!(
        ruleset.transition(state, &pass(0)),
        Err(Rejection::ActionNotInCurrentPhase {
            action: ActionId::new("pass_turn")
        })
    );
}

// ---- passing the Turn

#[test]
fn pass_turn_resolves_end_and_begins_the_next_players_turn() {
    let ruleset = ruleset();
    let state = opening(&ruleset);
    let before = state.clone();

    // What Player 1's Start phase will take: the Top of the deck, then the Top 2 of the mana deck.
    let drawn = from_top(&state, 1, "deck", 0);
    let first_mana = from_top(&state, 1, "mana_deck", 0);
    let second_mana = from_top(&state, 1, "mana_deck", 1);

    let transition = pass_turn(&ruleset, &state, 0);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            turn_began(1, 2),
            phase_began("start"),
            moved(drawn, 1, "deck", "hand"),
            moved(first_mana, 1, "mana_deck", "mana_zone"),
            moved(second_mana, 1, "mana_deck", "mana_zone"),
            phase_began("main"),
        ]
    );
    assert_eq!(transition.state.active_player(), p(1));
    assert_eq!(transition.state.turn_number(), TurnNumber::new(2));
    assert_eq!(transition.state.phase(), Some(&phase_id("main")));
    assert_eq!(state, before, "the prior GameState is untouched");
}

#[test]
fn pass_turn_follows_the_player_order_and_wraps_around() {
    let mut draft = small_turn_draft();
    draft.parameters.players = 3;
    let ruleset = ruleset_of(draft);
    let three = vec![deck("hero"), deck("second_hero"), deck("hero")];
    let mut state = create_with(&ruleset, &three, 1).opening.state;
    assert_eq!(state.active_player(), p(0));

    let mut seen = Vec::new();
    for active in [0, 1, 2] {
        let transition = pass_turn(&ruleset, &state, active);
        seen.push((
            transition.state.active_player(),
            transition.state.turn_number(),
        ));
        let events = transition.trace.events();
        assert_eq!(events.first(), Some(&phase_began("end")));
        assert_eq!(
            events.get(1),
            Some(&turn_began((active + 1) % 3, active + 2))
        );
        state = transition.state;
    }
    assert_eq!(
        seen,
        [
            (p(1), TurnNumber::new(2)),
            (p(2), TurnNumber::new(3)),
            (p(0), TurnNumber::new(4)),
        ]
    );
    // Player 0 draws on Turn 4: the first-Turn skip is for the Game's first Turn only.
    assert_eq!(count(&state, 0, "hand"), 6);
}

#[test]
fn the_starting_player_skips_the_start_phase_draw_on_the_first_turn_only() {
    let ruleset = ruleset();
    let state = opening(&ruleset);
    // After creation both hands are the 5 cards of setup: Player 0 skipped the draw on Turn 1.
    assert_eq!(count(&state, 0, "hand"), 5);
    assert_eq!(count(&state, 1, "hand"), 5);
    assert_eq!(count(&state, 0, "deck"), 45);

    // Player 1 draws on Turn 2, and the card is the Top of their deck.
    let top = from_top(&state, 1, "deck", 0);
    let turn_two = pass_turn(&ruleset, &state, 0).state;
    assert_eq!(count(&turn_two, 1, "hand"), 6);
    assert_eq!(count(&turn_two, 1, "deck"), 44);
    assert_eq!(from_top(&turn_two, 1, "hand", 0), top);
    assert_eq!(
        count(&turn_two, 0, "hand"),
        5,
        "Player 0 does not draw on Turn 2"
    );

    // Player 0 draws on Turn 3.
    let top = from_top(&turn_two, 0, "deck", 0);
    let turn_three = pass_turn(&ruleset, &turn_two, 1).state;
    assert_eq!(count(&turn_three, 0, "hand"), 6);
    assert_eq!(count(&turn_three, 0, "deck"), 44);
    assert_eq!(from_top(&turn_three, 0, "hand", 0), top);
}

#[test]
fn a_ruleset_can_start_at_another_seat() {
    // The skip belongs to the Game's first Turn, not to seat 0, and `active` means the starting
    // Player during setup.
    let mut draft = small_turn_draft();
    draft.parameters.starting_player = 1;
    draft.setup = vec![
        SetupStepDraft::Shuffle {
            player: "each".to_string(),
            zone: "deck".to_string(),
        },
        SetupStepDraft::Draw {
            player: "active".to_string(),
            from: "deck".to_string(),
            to: "hand".to_string(),
            count: 5,
        },
    ];
    let ruleset = ruleset_of(draft);
    let state = opening(&ruleset);
    assert_eq!(state.active_player(), p(1));
    assert_eq!(state.turn_number(), Some(TurnNumber::FIRST));
    assert_eq!(count(&state, 1, "hand"), 5, "only the starting Player drew");
    assert_eq!(count(&state, 0, "hand"), 0);

    // Seat 1 is last in the Player Order, so the next Turn wraps to seat 0, which draws.
    let next = pass_turn(&ruleset, &state, 1).state;
    assert_eq!(next.active_player(), p(0));
    assert_eq!(next.turn_number(), TurnNumber::new(2));
    assert_eq!(count(&next, 0, "hand"), 1);
    assert_eq!(count(&next, 1, "hand"), 5);
}

#[test]
fn each_pass_turn_increments_the_turn_number() {
    let ruleset = ruleset();
    let mut state = opening(&ruleset);
    let mut numbers = vec![state.turn_number()];
    for _ in 0..2 {
        let active = state.active_player().0;
        state = pass_turn(&ruleset, &state, active).state;
        numbers.push(state.turn_number());
    }
    assert_eq!(
        numbers,
        [TurnNumber::new(1), TurnNumber::new(2), TurnNumber::new(3)]
    );
}

#[test]
fn pass_turn_at_the_maximum_turn_number_is_rejected() {
    let ruleset = ruleset();
    let maximum = TurnNumber::new(u32::MAX).expect("the maximum is a Turn Number");

    // `TurnBegan` is not sequence-checked, so a trace can put a Game at the last Turn Number.
    let mut state = opening(&ruleset);
    apply_all(
        &mut state,
        &[
            Event::TurnBegan {
                player: p(0),
                turn: maximum,
            },
            phase_began("main"),
        ],
    );
    let before = state.clone();
    assert_eq!(
        ruleset.transition(&state, &pass(0)),
        Err(Rejection::TurnNumberOverflow)
    );
    assert_eq!(state, before);

    // One Turn earlier, the last pass is accepted.
    let mut state = opening(&ruleset);
    apply_all(
        &mut state,
        &[turn_began(0, u32::MAX - 1), phase_began("main")],
    );
    let last = pass_turn(&ruleset, &state, 0);
    assert_eq!(last.state.turn_number(), Some(maximum));
}

// ---- Start-phase primitives

/// The Small Ruleset whose mana cards have no instance state, so the mana Zone can hold a card that
/// does not declare `tapped`.
fn ruleset_with_stateless_mana() -> DeclarativeRuleset {
    let mut draft = small_turn_draft();
    draft.card_types[2].instance_state.clear();
    ruleset_of(draft)
}

#[test]
fn untap_and_set_state_change_only_cards_whose_type_declares_the_state() {
    let ruleset = ruleset_with_stateless_mana();
    let mut state = opening(&ruleset);

    // Player 1 will be the Active Player next. Their cards, with card types as in the Small
    // Ruleset: creatures have `tapped` and `recently_played`, the Leader has `tapped` only, and
    // here mana has no state at all.
    let hand = cards_in(&state, 1, "hand");
    let (tapped_creature, ready_creature, other_creature) = (hand[0], hand[1], hand[2]);
    let leader = from_top(&state, 1, "leader_zone", 0);
    let mana = from_top(&state, 1, "mana_deck", 0);
    // Player 0's creature, on the field of the Player who is not Active next.
    let opponent_creature = from_top(&state, 0, "hand", 0);

    apply_all(
        &mut state,
        &[
            // The field of Player 1, in this order.
            moved(tapped_creature, 1, "hand", "field"),
            changed(tapped_creature, "tapped", true),
            changed(tapped_creature, "recently_played", true),
            moved(ready_creature, 1, "hand", "field"),
            changed(ready_creature, "recently_played", true),
            moved(leader, 1, "leader_zone", "field"),
            changed(leader, "tapped", true),
            // A card without state in the Mana Zone.
            moved(mana, 1, "mana_deck", "mana_zone"),
            // A tapped card in a zone the steps do not list.
            changed(other_creature, "tapped", true),
            // The opponent's field.
            moved(opponent_creature, 0, "hand", "field"),
            changed(opponent_creature, "tapped", true),
            changed(opponent_creature, "recently_played", true),
        ],
    );

    let drawn = from_top(&state, 1, "deck", 0);
    let first_mana = from_top(&state, 1, "mana_deck", 0);
    let second_mana = from_top(&state, 1, "mana_deck", 1);
    let transition = pass_turn(&ruleset, &state, 0);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            turn_began(1, 2),
            phase_began("start"),
            // untap: the Mana Zone (the stateless card gets no Event), then the field bottom
            // first. The ready creature was already untapped, so it gets no Event.
            changed(tapped_creature, "tapped", false),
            changed(leader, "tapped", false),
            // set_state recently_played on the field: the Leader does not declare it.
            changed(tapped_creature, "recently_played", false),
            changed(ready_creature, "recently_played", false),
            moved(drawn, 1, "deck", "hand"),
            moved(first_mana, 1, "mana_deck", "mana_zone"),
            moved(second_mana, 1, "mana_deck", "mana_zone"),
            phase_began("main"),
        ]
    );

    let next = &transition.state;
    assert_eq!(
        state_value(next, tapped_creature, "tapped"),
        Some(FieldValue::Bool(false))
    );
    assert_eq!(
        state_value(next, leader, "tapped"),
        Some(FieldValue::Bool(false))
    );
    assert_eq!(state_value(next, leader, "recently_played"), None);
    assert_eq!(state_value(next, mana, "tapped"), None);
    // The unlisted zone and the opponent are untouched.
    assert_eq!(
        state_value(next, other_creature, "tapped"),
        Some(FieldValue::Bool(true))
    );
    assert_eq!(
        state_value(next, opponent_creature, "tapped"),
        Some(FieldValue::Bool(true))
    );
    assert_eq!(
        state_value(next, opponent_creature, "recently_played"),
        Some(FieldValue::Bool(true))
    );
}

#[test]
fn drawing_from_an_empty_zone_draws_nothing_and_emits_draw_from_empty_zone() {
    // Player 0's trash is empty when the first Turn begins.
    let ruleset = ruleset_of(draft_with_turn(
        vec![draw_step("active", "trash", "hand", 2)],
        vec![],
    ));
    let creation = create_with(&ruleset, &decks(), 1);
    let events = creation.opening.trace.events();
    assert!(events.ends_with(&[
        turn_began(0, 1),
        phase_began("start"),
        draw_from_empty(0, "trash"),
        phase_began("main"),
    ]));
    assert_eq!(creation.opening.state.phase(), Some(&phase_id("main")));

    // One card and a count of 2: the card moves, then ONE DrawFromEmptyZone, and the Game goes on.
    let mut state = creation.opening.state;
    let card = from_top(&state, 1, "deck", 0);
    apply_all(&mut state, &[moved(card, 1, "deck", "trash")]);
    let transition = pass_turn(&ruleset, &state, 0);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            turn_began(1, 2),
            phase_began("start"),
            moved(card, 1, "trash", "hand"),
            draw_from_empty(1, "trash"),
            phase_began("main"),
        ]
    );
    assert_eq!(transition.state.phase(), Some(&phase_id("main")));
    assert_eq!(transition.state.active_player(), p(1));
    assert_eq!(count(&transition.state, 1, "trash"), 0);

    // Two cards and a count of 2: both move, in order from the Top, and there is no
    // DrawFromEmptyZone.
    let mut state = opening(&ruleset);
    let (first, second) = (
        from_top(&state, 1, "deck", 0),
        from_top(&state, 1, "deck", 1),
    );
    apply_all(
        &mut state,
        &[
            moved(first, 1, "deck", "trash"),
            moved(second, 1, "deck", "trash"),
        ],
    );
    // The trash holds `first` then `second`: the Top is `second`.
    let transition = pass_turn(&ruleset, &state, 0);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            turn_began(1, 2),
            phase_began("start"),
            moved(second, 1, "trash", "hand"),
            moved(first, 1, "trash", "hand"),
            phase_began("main"),
        ]
    );
}

#[test]
fn move_takes_only_the_cards_there_are() {
    let ruleset = ruleset_of(draft_with_turn(
        vec![move_step("active", "trash", "field", 2)],
        vec![],
    ));
    // Zero cards: no Event, besides the phases.
    let creation = create_with(&ruleset, &decks(), 1);
    assert!(creation.opening.trace.events().ends_with(&[
        turn_began(0, 1),
        phase_began("start"),
        phase_began("main")
    ]));

    // One card and a count of 2: one CardMoved.
    let mut state = creation.opening.state;
    let card = from_top(&state, 1, "deck", 0);
    apply_all(&mut state, &[moved(card, 1, "deck", "trash")]);
    let transition = pass_turn(&ruleset, &state, 0);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            turn_began(1, 2),
            phase_began("start"),
            moved(card, 1, "trash", "field"),
            phase_began("main"),
        ]
    );
    assert_eq!(cards_in(&transition.state, 1, "field"), [card]);

    // Three cards and a count of 2: the Top 2 move one at a time, each onto the Top of the field.
    let mut state = opening(&ruleset);
    let (a, b, c) = (
        from_top(&state, 1, "deck", 0),
        from_top(&state, 1, "deck", 1),
        from_top(&state, 1, "deck", 2),
    );
    apply_all(
        &mut state,
        &[
            moved(a, 1, "deck", "trash"),
            moved(b, 1, "deck", "trash"),
            moved(c, 1, "deck", "trash"),
        ],
    );
    // The trash holds a, b, c: the Top is `c`.
    let transition = pass_turn(&ruleset, &state, 0);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            turn_began(1, 2),
            phase_began("start"),
            moved(c, 1, "trash", "field"),
            moved(b, 1, "trash", "field"),
            phase_began("main"),
        ]
    );
    assert_eq!(cards_in(&transition.state, 1, "field"), [c, b]);
    assert_eq!(cards_in(&transition.state, 1, "trash"), [a]);
}

#[test]
fn end_phase_steps_run_for_the_outgoing_player_before_the_next_turn_begins() {
    // `active` is the Player whose Turn is ending; `each` goes in seat order.
    let ruleset = ruleset_of(draft_with_turn(
        vec![],
        vec![
            draw_step("active", "deck", "hand", 1),
            draw_step("each", "deck", "hand", 1),
        ],
    ));
    let state = opening(&ruleset);
    let (outgoing_first, outgoing_second) = (
        from_top(&state, 0, "deck", 0),
        from_top(&state, 0, "deck", 1),
    );
    let other = from_top(&state, 1, "deck", 0);
    let transition = pass_turn(&ruleset, &state, 0);
    assert_eq!(
        transition.trace.events(),
        [
            phase_began("end"),
            moved(outgoing_first, 0, "deck", "hand"),
            moved(outgoing_second, 0, "deck", "hand"),
            moved(other, 1, "deck", "hand"),
            turn_began(1, 2),
            phase_began("start"),
            phase_began("main"),
        ]
    );
}

// ---- rejections

fn assert_action_rejected(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
    expected: Rejection,
) {
    let before = state.clone();
    assert_eq!(ruleset.transition(state, action), Err(expected));
    assert_eq!(state, &before, "a rejected Action leaves the state intact");
}

#[test]
fn an_action_by_an_unknown_player_is_rejected() {
    let ruleset = ruleset();
    assert_action_rejected(
        &ruleset,
        &opening(&ruleset),
        &pass(9),
        Rejection::UnknownPlayer { player: p(9) },
    );
}

#[test]
fn an_action_by_a_player_who_is_not_active_is_rejected() {
    let ruleset = ruleset();
    assert_action_rejected(
        &ruleset,
        &opening(&ruleset),
        &pass(1),
        Rejection::NotActivePlayer {
            player: p(1),
            active: p(0),
        },
    );
}

#[test]
fn an_unknown_action_is_rejected() {
    let ruleset = ruleset();
    let action = Action {
        player: p(0),
        action: ActionId::new("cast_spell"),
    };
    assert_action_rejected(
        &ruleset,
        &opening(&ruleset),
        &action,
        Rejection::UnknownAction {
            action: ActionId::new("cast_spell"),
        },
    );
}

#[test]
fn an_action_not_in_the_current_phase_is_rejected() {
    let ruleset = ruleset();
    let not_in_phase = |name: &str| Rejection::ActionNotInCurrentPhase {
        action: ActionId::new(name),
    };

    // `idle` is declared but Main does not list it.
    let idle = Action {
        player: p(0),
        action: ActionId::new("idle"),
    };
    assert_action_rejected(&ruleset, &opening(&ruleset), &idle, not_in_phase("idle"));

    // The Empty GameState has no phase.
    let empty = create_with(&ruleset, &decks(), 1).empty;
    assert_action_rejected(&ruleset, &empty, &pass(0), not_in_phase("pass_turn"));

    // A phase the ruleset declares but that is not an action phase.
    let mut state = opening(&ruleset);
    apply_all(&mut state, &[turn_began(0, 5), phase_began("start")]);
    assert_action_rejected(&ruleset, &state, &pass(0), not_in_phase("pass_turn"));

    // A phase the ruleset does not know.
    apply_all(&mut state, &[phase_began("ghost")]);
    assert_action_rejected(&ruleset, &state, &pass(0), not_in_phase("pass_turn"));
}

#[test]
fn rejections_are_checked_in_order() {
    let ruleset = ruleset();
    let state = opening(&ruleset);
    let unknown_action = |player: u32| Action {
        player: p(player),
        action: ActionId::new("cast_spell"),
    };
    // An unknown Player beats an unknown Action; a non-active Player beats an unknown Action.
    assert_eq!(
        ruleset.transition(&state, &unknown_action(9)),
        Err(Rejection::UnknownPlayer { player: p(9) })
    );
    assert_eq!(
        ruleset.transition(&state, &unknown_action(1)),
        Err(Rejection::NotActivePlayer {
            player: p(1),
            active: p(0)
        })
    );
    // An unknown Action beats a phase that does not list it, even with no phase at all.
    let empty = create_with(&ruleset, &decks(), 1).empty;
    assert_eq!(
        ruleset.transition(&empty, &unknown_action(0)),
        Err(Rejection::UnknownAction {
            action: ActionId::new("cast_spell")
        })
    );
}

#[test]
fn a_rejection_names_what_went_wrong() {
    // Display texts are for people; they name the Player and the Action.
    let rejection = Rejection::NotActivePlayer {
        player: p(1),
        active: p(0),
    };
    let text = rejection.to_string();
    assert!(
        text.contains('1') && text.contains('0'),
        "the text names both Players: {text}"
    );
    let text = Rejection::UnknownAction {
        action: ActionId::new("cast_spell"),
    }
    .to_string();
    assert!(
        text.contains("cast_spell"),
        "the text names the Action: {text}"
    );
    let text = Rejection::EventRejected(ApplyError::NoTurnInProgress).to_string();
    assert!(!text.is_empty());
}

// ---- replay and determinism

/// A Game of 10 passes from creation: the states and the traces, creation included.
fn play(seed: u64) -> (Vec<GameState>, Vec<Vec<Event>>) {
    let ruleset = ruleset();
    let creation = create_with(&ruleset, &decks(), seed);
    let mut state = creation.opening.state;
    let mut states = vec![state.clone()];
    let mut traces = vec![creation.opening.trace.into_events()];
    for _ in 0..10 {
        let active = state.active_player().0;
        let transition = pass_turn(&ruleset, &state, active);
        state = transition.state;
        states.push(state.clone());
        traces.push(transition.trace.into_events());
    }
    (states, traces)
}

#[test]
fn replaying_a_pass_turn_trace_reproduces_the_next_state() {
    let ruleset = ruleset();
    let mut state = opening(&ruleset);
    for _ in 0..10 {
        let active = state.active_player().0;
        let transition = pass_turn(&ruleset, &state, active);
        assert_eq!(
            state.apply_trace(transition.trace.events()),
            Ok(transition.state.clone())
        );
        state = transition.state;
    }
}

#[test]
fn two_runs_with_the_same_seed_produce_the_same_states_and_traces() {
    assert_eq!(play(7), play(7));
    // The seed matters: the shuffled decks, hence the draws, differ.
    assert_ne!(play(7).1, play(8).1);
}

#[test]
fn the_seam_runs_pass_turn_through_the_runtime() {
    let ruleset = ruleset();
    let state = opening(&ruleset);
    let expected = pass_turn(&ruleset, &state, 0);
    let runtime = Runtime::new(ruleset);
    assert_eq!(runtime.apply(&state, &pass(0)), Ok(expected));
}

// ---- the definition and the cycle

#[test]
fn a_turn_that_draws_from_an_unordered_zone_never_reaches_the_cycle() {
    // `DeclarativeRuleset::new` takes a validated definition: an unordered source was rejected.
    let draft = draft_with_turn(vec![draw_step("active", "hand", "deck", 1)], vec![]);
    assert_eq!(
        RulesetDefinition::validate(draft),
        Err(DefinitionError {
            path: path()
                .key("turn")
                .key("phases")
                .id("start")
                .key("automatic")
                .index(0)
                .key("draw")
                .key("from"),
            kind: DefinitionErrorKind::ZoneNotOrdered {
                zone: "hand".to_string()
            },
        })
    );
}
