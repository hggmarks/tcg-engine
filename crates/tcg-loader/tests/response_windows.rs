//! Acceptance tests for ticket #85: Response Windows and the Stack (ADR-0037), on the real
//! Response Ruleset, its real catalog and its real sample decks, with test-only rulesets (the real
//! files edited as text, and test-only cards appended to the catalog) where the real one cannot
//! produce a behaviour: a response that removes the attacker, a trigger woken by a push, a Choice
//! during a push, a win condition a push meets.
//!
//! The Event Traces are asserted whole, as literals read off the rules and off the arrangement of
//! each scenario, never computed by the code under test. A scenario is arranged with
//! `GameState::apply` of Events before the Action under test (an arrangement is not a transition,
//! so it wakes no trigger): the hands are cleared, seat 0 has an attacker on the field and seat 1 a
//! tapped target, and the fast creatures a test needs are put in the hands.
//!
//! A window opens when an Action named in `opened_by` is taken outside one: its cost is paid at
//! once, its effects become a Stack Item (`ItemPushed`), and the Player with Priority, the opponent
//! of the top item's controller, answers with a window Action (pushed too) or a `Pass`, which
//! resolves the top item.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Answer, Argument, BoundCard, CardInstanceId, DecisionKind, DeclarativeRuleset,
    EffectPath, Event, GameRecord, GameState, Loss, PausedResolution, QueuedTrigger, Rejection,
    StackItem, StackSource,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, FieldValue, ParamName, StateName, WinConditionId, ZoneId,
};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const RESPONSE_RULESET: &str =
    include_str!("../../../rulesets/response-ruleset/response-ruleset.yaml");
const RESPONSE_CATALOG: &str =
    include_str!("../../../rulesets/response-ruleset/response-catalog.yaml");
const RESPONSE_DECK_A: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-a.yaml");
const RESPONSE_DECK_B: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-b.yaml");

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");
const EFFECTS_DECK_A: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-a.yaml");
const EFFECTS_DECK_B: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-b.yaml");

/// The Seed of every scenario. Scenarios do not depend on what it deals: each arranges the cards
/// it needs.
const SEED: u64 = 4;

/// The Seeds a scripted Game is played for, to find ones whose Games hold a response.
const SCRIPTED_SEEDS: u64 = 100;

/// A scripted Game stops after this many accepted Actions at the latest.
const SCRIPT_CAP: usize = 400;

// ---- test-only rulesets

/// Fast creatures of cost 0 with a trigger each. `lurker` draws when it enters the `holding` zone,
/// which the cost of its `fast_play` does; `settler` draws when it enters the field, which the
/// resolution of its `fast_play` does; `asker` asks its controller whether to draw when it enters
/// the `holding` zone.
const TEST_CARDS: &str = "
  - { id: lurker,  type: creature, fields: { name: \"Lurker\",  cost: 0, power: 1, fast: true }, effects: [ { on: { enter: holding }, do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } ] }
  - { id: settler, type: creature, fields: { name: \"Settler\", cost: 0, power: 1, fast: true }, effects: [ { on: { enter: field }, do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } ] }
  - { id: asker,   type: creature, fields: { name: \"Asker\",   cost: 0, power: 1, fast: true }, effects: [ { on: { enter: holding }, do: [ { choose_one: { decider: controller, options: [ [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ], [] ] } } ] } ] }
";

/// The test-only actions: `snipe` and `snipe_back` are responses (they move an opponent's creature
/// to the trash, and the second one moves it back to the field at once); `poke` is an Action-phase
/// action whose cost taps a creature it does not check.
const TEST_ACTIONS: &str = "  - id: snipe
    params:
      - { name: victim, zone: field, card_type: creature, owner: opponent }
    effects:
      - move: { card: { ref: victim }, to: trash }
  - id: snipe_back
    params:
      - { name: victim, zone: field, card_type: creature, owner: opponent }
    effects:
      - move: { card: { ref: victim }, to: trash }
      - move: { card: { ref: victim }, to: field }
  - id: poke
    params:
      - { name: card, zone: field, card_type: creature }
    cost:
      - tap: { card: { ref: card } }
    effects: []
  - id: pass_turn
";

/// A win condition a push can meet: a Player loses while one of their mana cards is tapped, which
/// the cost of a `fast_play` makes true.
const MANA_TAPPED_LOSS: &str = "win_conditions:
  - id: mana_tapped
    when: { eq: [ { ref: mana.tapped }, true ] }
    loser: owner
  - id: leader_dead
";

fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(
        yaml.matches(from).count(),
        1,
        "`{from}` occurs once in the fixture"
    );
    yaml.replace(from, to)
}

fn build(ruleset_yaml: &str, catalog_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog_yaml).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The real Response Ruleset and its real catalog.
fn response() -> DeclarativeRuleset {
    build(RESPONSE_RULESET, RESPONSE_CATALOG)
}

/// The real Response Ruleset with `snipe`, `snipe_back` and `poke` (the window lists the first
/// two), and the real catalog.
fn response_with_test_actions() -> DeclarativeRuleset {
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        "    actions: [fast_play, snipe, snipe_back]\n",
    );
    let yaml = edit(&yaml, "  - id: pass_turn\n", TEST_ACTIONS);
    let yaml = edit(
        &yaml,
        "actions: [play_creature, attack, pass_turn]",
        "actions: [play_creature, attack, pass_turn, poke]",
    );
    build(&yaml, RESPONSE_CATALOG)
}

/// The real Response Ruleset with the test-only cards in the catalog.
fn response_with_test_cards() -> DeclarativeRuleset {
    build(RESPONSE_RULESET, &format!("{RESPONSE_CATALOG}{TEST_CARDS}"))
}

/// The real Response Ruleset with the `mana_tapped` win condition.
fn response_with_mana_tapped_loss() -> DeclarativeRuleset {
    let yaml = edit(
        RESPONSE_RULESET,
        "win_conditions:\n  - id: leader_dead\n",
        MANA_TAPPED_LOSS,
    );
    build(&yaml, RESPONSE_CATALOG)
}

/// A sample deck with three of its four `river_otter`s swapped for the three test-only cards.
fn with_test_cards(deck: &str) -> String {
    edit(
        deck,
        "      - { card: river_otter,    count: 4 }\n",
        "      - { card: river_otter,    count: 1 }\n      - { card: lurker,         count: 1 }\n      - { card: settler,        count: 1 }\n      - { card: asker,          count: 1 }\n",
    )
}

fn deck_lists(ruleset: &DeclarativeRuleset, yamls: [&str; 2]) -> Vec<DeckList> {
    yamls
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn response_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    deck_lists(ruleset, [RESPONSE_DECK_A, RESPONSE_DECK_B])
}

fn test_card_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    deck_lists(
        ruleset,
        [
            &with_test_cards(RESPONSE_DECK_A),
            &with_test_cards(RESPONSE_DECK_B),
        ],
    )
}

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn declared(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn attack(attacker: CardInstanceId, target: CardInstanceId) -> Action {
    declared(0, "attack", &[attacker, target])
}

fn fast_play(player: u32, card: CardInstanceId) -> Action {
    declared(player, "fast_play", &[card])
}

fn pass(player: u32) -> Action {
    Action::Pass { player: p(player) }
}

fn answer(player: u32, option: u32) -> Action {
    Action::Answer {
        player: p(player),
        answer: Answer::Option(option),
    }
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

/// Accepts every Action in turn; returns the last state and the trace of the last Action.
fn accept_all(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    actions: &[Action],
) -> (GameState, Vec<Event>) {
    let mut state = state.clone();
    let mut trace = Vec::new();
    for action in actions {
        let transition = accepted(ruleset, &state, action);
        trace = transition.trace.into_events();
        state = transition.state;
    }
    (state, trace)
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

fn in_zone(state: &GameState, seat: u32, id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(seat), &zone(id))
        .expect("the zone exists")
        .to_vec()
}

fn top_of_deck(state: &GameState, seat: u32) -> CardInstanceId {
    *in_zone(state, seat, "deck")
        .last()
        .expect("the deck is not empty")
}

fn state_value(state: &GameState, card: CardInstanceId, name: &str) -> Option<FieldValue> {
    state
        .card(card)
        .and_then(|instance| instance.state_value(&StateName::new(name)))
        .cloned()
}

fn tapped(card: CardInstanceId) -> Event {
    state_changed(card, "tapped", FieldValue::Bool(true))
}

fn state_changed(card: CardInstanceId, name: &str, value: FieldValue) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new(name),
        value,
    }
}

fn moved(card: CardInstanceId, seat: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(seat),
        from: zone(from),
        to: zone(to),
    }
}

fn apply_all(state: &mut GameState, events: &[Event]) {
    for event in events {
        state.apply(event).expect("the scenario Event is accepted");
    }
}

/// Moves the first card of `seat` that is of Card Definition `definition` and not yet in `to`
/// (wherever it is) into `to`, as an arrangement; returns it.
fn place(state: &mut GameState, definition: &str, seat: u32, to: &str) -> CardInstanceId {
    let found = state
        .cards()
        .iter()
        .find(|card| {
            card.owner() == p(seat)
                && card.definition().as_str() == definition
                && state.zone_of(card.id()) != Some(&zone(to))
        })
        .map(|card| card.id())
        .expect("the deck holds the card");
    let from = state.zone_of(found).expect("the card is in a zone").clone();
    apply_all(
        state,
        &[Event::CardMoved {
            card: found,
            player: p(seat),
            from,
            to: zone(to),
        }],
    );
    found
}

/// The cards of a scenario: seat 0's `attacker` (a `river_otter`, power 3) is in the field, untapped
/// and not recently played; seat 1's `target` (a `moss_sprite`, power 2) is in the field, tapped.
/// Both hands are empty (their cards are back in the decks), seat 0 has its two untapped mana from
/// the start of the Turn and seat 1 has two untapped mana. Seat 0 is the Active Player.
struct Scene {
    state: GameState,
    attacker: CardInstanceId,
    target: CardInstanceId,
}

fn arrange(ruleset: &DeclarativeRuleset, decks: &[DeckList]) -> Scene {
    let mut state = ruleset
        .create_game(decks, Seed(SEED))
        .expect("the decks create a game")
        .opening
        .state;
    for seat in [0, 1] {
        for card in in_zone(&state, seat, "hand") {
            apply_all(&mut state, &[moved(card, seat, "hand", "deck")]);
        }
    }
    place(&mut state, "mana", 1, "mana_zone");
    place(&mut state, "mana", 1, "mana_zone");
    let attacker = place(&mut state, "river_otter", 0, "field");
    let target = place(&mut state, "moss_sprite", 1, "field");
    apply_all(&mut state, &[tapped(target)]);
    assert_eq!(in_zone(&state, 0, "mana_zone").len(), 2);
    assert_eq!(state.active_player(), p(0));
    assert!(state.stack().is_empty());
    Scene {
        state,
        attacker,
        target,
    }
}

fn bound(param: &str, card: CardInstanceId, zone_id: &str, left: bool) -> BoundCard {
    BoundCard {
        param: ParamName::new(param),
        card,
        zone: zone(zone_id),
        left,
    }
}

/// The Stack Item of `attack(attacker, target)`; `attacker_left` is set once the attacker left.
fn attack_item(attacker: CardInstanceId, target: CardInstanceId, attacker_left: bool) -> StackItem {
    StackItem {
        controller: p(0),
        source: StackSource::Action(ActionId::new("attack")),
        bound: vec![
            bound("attacker", attacker, "field", attacker_left),
            bound("target", target, "field", false),
        ],
    }
}

/// The Stack Item of a `fast_play` of `card` by `seat`: the cost moved the card to `holding`.
fn fast_item(seat: u32, card: CardInstanceId) -> StackItem {
    StackItem {
        controller: p(seat),
        source: StackSource::Action(ActionId::new("fast_play")),
        bound: vec![bound("card", card, "holding", false)],
    }
}

fn pushed(item: StackItem) -> Event {
    Event::ItemPushed { item }
}

fn resolved(item: StackItem) -> Event {
    Event::ItemResolved { item }
}

fn fizzled(item: StackItem) -> Event {
    Event::ItemFizzled { item }
}

fn effects_decks_for(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    deck_lists(ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B])
}

// ---- the window and the Stack

#[test]
fn an_attack_pays_its_tap_and_pushes_one_stack_item() {
    let ruleset = response();
    let mut scene = arrange(&ruleset, &response_decks(&ruleset));
    let sparrow = place(&mut scene.state, "swift_sparrow", 1, "hand");
    let fox = place(&mut scene.state, "flash_fox", 1, "hand");
    let (a, t) = (scene.attacker, scene.target);

    let transition = accepted(&ruleset, &scene.state, &attack(a, t));

    // The cost's tap, then `ItemPushed` last: the combat has not happened.
    let item = attack_item(a, t, false);
    assert_eq!(transition.trace.events(), [tapped(a), pushed(item.clone())]);
    let next = &transition.state;
    assert_eq!(next.stack(), [item]);
    assert_eq!(state_value(next, a, "tapped"), Some(FieldValue::Bool(true)));
    assert_eq!(in_zone(next, 1, "field"), [t], "the target is still there");
    // Priority belongs to the attacker's opponent, who is the Deciding Player.
    assert_eq!(next.priority(), Some(p(1)));
    assert_eq!(next.deciding_player(), p(1));
    // `Pass`, then the legal `fast_play`s in Listing Order (ascending card, for a lone window Action).
    let mut fast = [sparrow, fox];
    fast.sort();
    assert_eq!(
        ruleset.legal_actions(next),
        [pass(1), fast_play(1, fast[0]), fast_play(1, fast[1])]
    );
    // The Active Player has no Legal Action while the window is open.
    assert!(ruleset
        .legal_actions(next)
        .iter()
        .all(|action| action.player() == p(1)));
}

#[test]
fn a_pass_resolves_the_attack() {
    let ruleset = response();
    let scene = arrange(&ruleset, &response_decks(&ruleset));
    let (a, t) = (scene.attacker, scene.target);
    let (pushed_state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    let transition = accepted(&ruleset, &pushed_state, &pass(1));

    // `ItemResolved` first, then the combat as the attack of the Effects Ruleset produces it
    // (power 3 against power 2: the target goes to the trash), minus the tap, which was the cost.
    let events = transition.trace.events().to_vec();
    assert_eq!(
        events,
        [
            resolved(attack_item(a, t, false)),
            moved(t, 1, "field", "trash")
        ]
    );
    let next = &transition.state;
    assert!(next.stack().is_empty());
    assert_eq!(next.priority(), None);
    assert_eq!(next.deciding_player(), p(0), "the Active Player acts again");
    assert_eq!(in_zone(next, 1, "trash"), [t]);

    // The same position under the Effects Ruleset: its attack is that trace with the tap first.
    let effects = build(EFFECTS_RULESET, EFFECTS_CATALOG);
    let effects_scene = arrange(&effects, &effects_decks_for(&effects));
    let direct = accepted(
        &effects,
        &effects_scene.state,
        &attack(effects_scene.attacker, effects_scene.target),
    );
    assert_eq!(
        direct.trace.events(),
        [
            tapped(effects_scene.attacker),
            moved(effects_scene.target, 1, "field", "trash")
        ]
    );
    assert_eq!(
        events.get(1..),
        direct.trace.events().get(1..),
        "the combat Events are the Effects Ruleset's"
    );
}

#[test]
fn three_passes_resolve_a_response_to_a_response_last_in_first_out() {
    let ruleset = response();
    let mut scene = arrange(&ruleset, &response_decks(&ruleset));
    let sparrow = place(&mut scene.state, "swift_sparrow", 1, "hand");
    let fox = place(&mut scene.state, "flash_fox", 0, "hand");
    let (a, t) = (scene.attacker, scene.target);
    let (mana_0, mana_1) = {
        let mana = in_zone(&scene.state, 0, "mana_zone");
        (mana[0], mana[1])
    };
    let mana_of_seat_1 = in_zone(&scene.state, 1, "mana_zone")[0];

    // The attack (seat 0), a response by seat 1, a response to it by seat 0.
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);
    assert_eq!(state.priority(), Some(p(1)));

    let second = accepted(&ruleset, &state, &fast_play(1, sparrow));
    let sparrow_item = fast_item(1, sparrow);
    assert_eq!(
        second.trace.events(),
        [
            state_changed(mana_of_seat_1, "tapped", FieldValue::Bool(true)),
            moved(sparrow, 1, "hand", "holding"),
            pushed(sparrow_item.clone())
        ]
    );
    assert_eq!(second.state.priority(), Some(p(0)), "Priority flips");
    assert_eq!(second.state.deciding_player(), p(0));

    let third = accepted(&ruleset, &second.state, &fast_play(0, fox));
    let fox_item = fast_item(0, fox);
    assert_eq!(
        third.trace.events(),
        [
            tapped(mana_0),
            tapped(mana_1),
            moved(fox, 0, "hand", "holding"),
            pushed(fox_item.clone())
        ]
    );
    let state = third.state;
    assert_eq!(
        state.stack(),
        [
            attack_item(a, t, false),
            sparrow_item.clone(),
            fox_item.clone()
        ]
    );
    let controllers: Vec<PlayerId> = state.stack().iter().map(|item| item.controller).collect();
    assert_eq!(controllers, [p(0), p(1), p(0)]);
    assert_eq!(state.priority(), Some(p(1)), "Priority flips again");

    // Pass 1, by seat 1: the fox, last in, resolves first.
    let first_pass = accepted(&ruleset, &state, &pass(1));
    assert_eq!(
        first_pass.trace.events(),
        [
            resolved(fox_item),
            moved(fox, 0, "holding", "field"),
            state_changed(fox, "recently_played", FieldValue::Bool(true))
        ]
    );
    assert_eq!(first_pass.state.stack().len(), 2);
    assert_eq!(first_pass.state.priority(), Some(p(0)));

    // Pass 2, by seat 0: the sparrow.
    let second_pass = accepted(&ruleset, &first_pass.state, &pass(0));
    assert_eq!(
        second_pass.trace.events(),
        [
            resolved(sparrow_item),
            moved(sparrow, 1, "holding", "field"),
            state_changed(sparrow, "recently_played", FieldValue::Bool(true))
        ]
    );
    assert_eq!(second_pass.state.stack(), [attack_item(a, t, false)]);
    assert_eq!(second_pass.state.priority(), Some(p(1)));

    // Pass 3, by seat 1: the attack, first in, resolves last; the window closes.
    let third_pass = accepted(&ruleset, &second_pass.state, &pass(1));
    assert_eq!(
        third_pass.trace.events(),
        [
            resolved(attack_item(a, t, false)),
            moved(t, 1, "field", "trash")
        ]
    );
    assert!(third_pass.state.stack().is_empty());
    assert_eq!(third_pass.state.priority(), None);
    assert_eq!(third_pass.state.deciding_player(), p(0));
}

// ---- the Fizzle

#[test]
fn an_item_whose_bound_card_left_fizzles() {
    let ruleset = response_with_test_actions();
    let scene = arrange(&ruleset, &response_decks(&ruleset));
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(
        &ruleset,
        &scene.state,
        &[attack(a, t), declared(1, "snipe", &[a])],
    );
    let victim_item = StackItem {
        controller: p(1),
        source: StackSource::Action(ActionId::new("snipe")),
        bound: vec![bound("victim", a, "field", false)],
    };
    assert_eq!(
        state.stack(),
        [attack_item(a, t, false), victim_item.clone()]
    );

    // Seat 0 passes: the snipe resolves and the attacker goes to the trash, which the attack item
    // below it records.
    let resolving = accepted(&ruleset, &state, &pass(0));
    assert_eq!(
        resolving.trace.events(),
        [resolved(victim_item), moved(a, 0, "field", "trash")]
    );
    let state = resolving.state;
    assert_eq!(state.stack(), [attack_item(a, t, true)]);

    // Seat 1 passes: the attack Fizzles. Nothing of the combat happens, and the tap, a cost, stands.
    let fizzling = accepted(&ruleset, &state, &pass(1));
    assert_eq!(fizzling.trace.events(), [fizzled(attack_item(a, t, true))]);
    let next = fizzling.state;
    assert!(next.stack().is_empty());
    assert_eq!(in_zone(&next, 1, "field"), [t], "the target did not die");
    assert!(in_zone(&next, 1, "trash").is_empty());
    assert_eq!(
        state_value(&next, a, "tapped"),
        Some(FieldValue::Bool(true))
    );
    assert_eq!(next.deciding_player(), p(0));
}

#[test]
fn a_card_that_left_and_came_back_still_fizzles() {
    let ruleset = response_with_test_actions();
    let scene = arrange(&ruleset, &response_decks(&ruleset));
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(
        &ruleset,
        &scene.state,
        &[attack(a, t), declared(1, "snipe_back", &[a])],
    );

    // The response sends the attacker to the trash and back to the field in one resolution.
    let resolving = accepted(&ruleset, &state, &pass(0));
    let back_item = StackItem {
        controller: p(1),
        source: StackSource::Action(ActionId::new("snipe_back")),
        bound: vec![bound("victim", a, "field", false)],
    };
    assert_eq!(
        resolving.trace.events(),
        [
            resolved(back_item),
            moved(a, 0, "field", "trash"),
            moved(a, 0, "trash", "field")
        ]
    );
    let state = resolving.state;
    assert_eq!(in_zone(&state, 0, "field"), [a], "the attacker is back");
    // Back in the zone it was bound from, the card still counts as gone: the flag is state.
    assert_eq!(state.stack(), [attack_item(a, t, true)]);

    let fizzling = accepted(&ruleset, &state, &pass(1));
    assert_eq!(fizzling.trace.events(), [fizzled(attack_item(a, t, true))]);
    assert_eq!(in_zone(&fizzling.state, 1, "field"), [t]);
}

// ---- Rejections

#[test]
fn a_player_without_priority_cannot_act_or_pass() {
    let ruleset = response();
    let mut scene = arrange(&ruleset, &response_decks(&ruleset));
    let fox = place(&mut scene.state, "flash_fox", 0, "hand");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    // Seat 0 is the Active Player and controls the top item, but Priority is seat 1's.
    let not_deciding = Rejection::NotDecidingPlayer {
        player: p(0),
        deciding: p(1),
    };
    assert_rejected(&ruleset, &state, &pass(0), not_deciding.clone());
    assert_rejected(&ruleset, &state, &fast_play(0, fox), not_deciding.clone());
    assert_rejected(
        &ruleset,
        &state,
        &declared(0, "pass_turn", &[]),
        not_deciding,
    );
}

#[test]
fn a_pass_with_no_window_open_is_rejected() {
    let ruleset = response();
    let scene = arrange(&ruleset, &response_decks(&ruleset));

    // The Active Player passes with an empty Stack.
    assert_rejected(
        &ruleset,
        &scene.state,
        &pass(0),
        Rejection::PassWithoutWindow,
    );
    // Another Player is not the Deciding Player: that check comes first.
    assert_rejected(
        &ruleset,
        &scene.state,
        &pass(1),
        Rejection::NotDecidingPlayer {
            player: p(1),
            deciding: p(0),
        },
    );
    // No Pass is listed without a window.
    assert!(ruleset
        .legal_actions(&scene.state)
        .iter()
        .all(|action| !matches!(action, Action::Pass { .. })));
}

#[test]
fn an_action_phase_action_is_rejected_while_a_window_is_open() {
    let ruleset = response();
    let mut scene = arrange(&ruleset, &response_decks(&ruleset));
    let sparrow = place(&mut scene.state, "swift_sparrow", 1, "hand");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    // Seat 1 holds Priority, but the window lists only `fast_play`: `play_creature`, which the
    // Main phase lists, and `pass_turn` are not in the window.
    assert_rejected(
        &ruleset,
        &state,
        &declared(1, "play_creature", &[sparrow]),
        Rejection::ActionNotInWindow {
            action: ActionId::new("play_creature"),
        },
    );
    assert_rejected(
        &ruleset,
        &state,
        &declared(1, "pass_turn", &[]),
        Rejection::ActionNotInWindow {
            action: ActionId::new("pass_turn"),
        },
    );
    // `attack` opens the window but is not listed by it: it cannot be taken inside one.
    assert_rejected(
        &ruleset,
        &state,
        &declared(1, "attack", &[t, a]),
        Rejection::ActionNotInWindow {
            action: ActionId::new("attack"),
        },
    );
    // An action the ruleset does not declare is still unknown, window or not.
    assert_rejected(
        &ruleset,
        &state,
        &declared(1, "no_such_action", &[]),
        Rejection::UnknownAction {
            action: ActionId::new("no_such_action"),
        },
    );
}

#[test]
fn a_window_action_outside_a_window_is_not_in_the_current_phase() {
    let ruleset = response();
    let mut scene = arrange(&ruleset, &response_decks(&ruleset));
    let fox = place(&mut scene.state, "flash_fox", 0, "hand");

    // No phase lists `fast_play` and `attack` has not opened the window.
    assert_rejected(
        &ruleset,
        &scene.state,
        &fast_play(0, fox),
        Rejection::ActionNotInCurrentPhase {
            action: ActionId::new("fast_play"),
        },
    );
}

#[test]
fn a_tap_cost_on_a_card_that_is_already_tapped_is_rejected() {
    let ruleset = response_with_test_actions();
    let scene = arrange(&ruleset, &response_decks(&ruleset));
    let a = scene.attacker;
    // `poke` taps the creature it is given and checks nothing else.
    let fresh = accepted(&ruleset, &scene.state, &declared(0, "poke", &[a]));
    assert_eq!(fresh.trace.events(), [tapped(a)]);
    assert!(fresh.state.stack().is_empty(), "poke opens no window");
    // Taken again, the card is tapped already: nothing changes.
    assert_rejected(
        &ruleset,
        &fresh.state,
        &declared(0, "poke", &[a]),
        Rejection::CardAlreadyTapped { card: a },
    );
}

// ---- triggers

#[test]
fn a_trigger_woken_by_a_push_resolves_in_that_transition() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange(&ruleset, &test_card_decks(&ruleset));
    let lurker = place(&mut scene.state, "lurker", 1, "hand");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);
    let top = top_of_deck(&state, 1);

    let transition = accepted(&ruleset, &state, &fast_play(1, lurker));

    // The cost moved the lurker to `holding` (cost 0: nothing is tapped), then `ItemPushed`, then the
    // Trigger Resolution: its `enter: holding` trigger draws. No decision is pending.
    assert_eq!(
        transition.trace.events(),
        [
            moved(lurker, 1, "hand", "holding"),
            pushed(fast_item(1, lurker)),
            moved(top, 1, "deck", "hand")
        ]
    );
    assert!(transition.state.pending_decision().is_none());
    assert_eq!(transition.state.stack().len(), 2);
    assert_eq!(transition.state.priority(), Some(p(0)));
}

#[test]
fn a_trigger_woken_by_a_resolving_item_resolves_in_that_transition() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange(&ruleset, &test_card_decks(&ruleset));
    let settler = place(&mut scene.state, "settler", 1, "hand");
    let (a, t) = (scene.attacker, scene.target);
    let (state, push_trace) = accept_all(
        &ruleset,
        &scene.state,
        &[attack(a, t), fast_play(1, settler)],
    );
    // Entering `holding` wakes nothing: the settler's trigger is `enter: field`.
    assert_eq!(
        push_trace,
        [
            moved(settler, 1, "hand", "holding"),
            pushed(fast_item(1, settler))
        ]
    );
    let top = top_of_deck(&state, 1);

    let transition = accepted(&ruleset, &state, &pass(0));

    // `ItemResolved`, the item's effects, then the Trigger Resolution they wake.
    assert_eq!(
        transition.trace.events(),
        [
            resolved(fast_item(1, settler)),
            moved(settler, 1, "holding", "field"),
            state_changed(settler, "recently_played", FieldValue::Bool(true)),
            moved(top, 1, "deck", "hand")
        ]
    );
    // The attack is still on the Stack, under the item that resolved.
    assert_eq!(transition.state.stack(), [attack_item(a, t, false)]);
    assert_eq!(transition.state.priority(), Some(p(1)));
}

#[test]
fn a_choice_during_a_push_makes_the_decider_decide_then_priority_returns() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange(&ruleset, &test_card_decks(&ruleset));
    let asker = place(&mut scene.state, "asker", 1, "hand");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    let transition = accepted(&ruleset, &state, &fast_play(1, asker));

    // The push, then the pause as the last Event: the asker's controller (seat 1) is asked.
    assert_eq!(
        transition.trace.events(),
        [
            moved(asker, 1, "hand", "holding"),
            pushed(fast_item(1, asker)),
            Event::DecisionPending {
                decider: p(1),
                kind: DecisionKind::Option,
                paused: PausedResolution {
                    trigger: QueuedTrigger {
                        card: asker,
                        effect: 0
                    },
                    at: EffectPath {
                        branches: vec![],
                        choice: 0
                    },
                    queue: vec![],
                    unscanned: vec![],
                    resolutions: 1,
                    bound: vec![],
                }
            }
        ]
    );
    let paused = transition.state;
    // Priority belongs to seat 0 (seat 1 controls the top item), but the decider decides first.
    assert_eq!(paused.priority(), Some(p(0)));
    assert_eq!(paused.deciding_player(), p(1));
    assert_eq!(ruleset.legal_actions(&paused), [answer(1, 0), answer(1, 1)]);
    // Neither a Pass by the Player with Priority nor one by the decider is accepted yet.
    assert_rejected(
        &ruleset,
        &paused,
        &pass(0),
        Rejection::NotDecidingPlayer {
            player: p(0),
            deciding: p(1),
        },
    );
    assert_rejected(
        &ruleset,
        &paused,
        &pass(1),
        Rejection::PassWhileDecisionPending,
    );

    // The Answer draws; then the Player with Priority, seat 0, is the Deciding Player.
    let top = top_of_deck(&paused, 1);
    let answered = accepted(&ruleset, &paused, &answer(1, 0));
    assert_eq!(
        answered.trace.events(),
        [
            Event::DecisionAnswered {
                player: p(1),
                answer: Answer::Option(0)
            },
            moved(top, 1, "deck", "hand")
        ]
    );
    assert!(answered.state.pending_decision().is_none());
    assert_eq!(answered.state.stack().len(), 2);
    assert_eq!(answered.state.deciding_player(), p(0));
    assert_eq!(
        ruleset.legal_actions(&answered.state).first(),
        Some(&pass(0))
    );
}

// ---- winning

#[test]
fn a_push_that_meets_a_win_condition_ends_the_game() {
    let ruleset = response_with_mana_tapped_loss();
    let mut scene = arrange(&ruleset, &response_decks(&ruleset));
    let sparrow = place(&mut scene.state, "swift_sparrow", 1, "hand");
    let (a, t) = (scene.attacker, scene.target);
    let mana = in_zone(&scene.state, 1, "mana_zone")[0];
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);
    assert!(state.outcome().is_none(), "the attack's push meets nothing");

    // The cost taps one of seat 1's mana: seat 1 loses at the end of the push.
    let transition = accepted(&ruleset, &state, &fast_play(1, sparrow));
    assert_eq!(
        transition.trace.events(),
        [
            tapped(mana),
            moved(sparrow, 1, "hand", "holding"),
            pushed(fast_item(1, sparrow)),
            Event::GameEnded {
                losses: vec![Loss {
                    player: p(1),
                    reason: WinConditionId::new("mana_tapped")
                }]
            }
        ]
    );
    let ended = transition.state;
    let outcome = ended.outcome().expect("the Game has ended");
    assert_eq!(outcome.winners(), [p(0)]);
    assert!(
        ended.stack().is_empty(),
        "a Terminal Outcome clears the Stack"
    );
    assert_eq!(ended.priority(), None);
    assert!(ruleset.legal_actions(&ended).is_empty());
}

#[test]
fn a_resolution_that_meets_a_win_condition_ends_the_game() {
    let ruleset = response();
    let mut scene = arrange(&ruleset, &response_decks(&ruleset));
    let leader = in_zone(&scene.state, 1, "leader_zone")[0];
    apply_all(
        &mut scene.state,
        &[state_changed(leader, "current_life", FieldValue::Int(1))],
    );
    let a = scene.attacker;
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, leader)]);
    assert!(state.outcome().is_none());

    // The resolving attack takes the Leader's last life: `GameEnded` is the last Event.
    let transition = accepted(&ruleset, &state, &pass(1));
    let item = StackItem {
        controller: p(0),
        source: StackSource::Action(ActionId::new("attack")),
        bound: vec![
            bound("attacker", a, "field", false),
            bound("target", leader, "leader_zone", false),
        ],
    };
    assert_eq!(
        transition.trace.events(),
        [
            resolved(item),
            state_changed(leader, "current_life", FieldValue::Int(0)),
            Event::GameEnded {
                losses: vec![Loss {
                    player: p(1),
                    reason: WinConditionId::new("leader_dead")
                }]
            }
        ]
    );
    let ended = transition.state;
    assert_eq!(ended.outcome().expect("ended").winners(), [p(0)]);
    assert!(ended.stack().is_empty());
}

// ---- replay and determinism

/// Plays a scripted Game of `seed` and returns its record, the Event Trace of each Action and the
/// final GameState. The script is a deterministic rule over the Legal Actions: with the Stack
/// non-empty take a `fast_play` if one is Legal, else `Pass`; otherwise an `attack`, else a
/// `play_creature` (once per Turn), else `pass_turn`. It stops when the Game ends or after `SCRIPT_CAP` Actions.
fn scripted_game(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    seed: u64,
) -> (GameRecord, Vec<Vec<Event>>, GameState) {
    let mut record = GameRecord::new(ruleset, decks, Seed(seed));
    let mut state = ruleset
        .create_game(decks, Seed(seed))
        .expect("the decks create a game")
        .opening
        .state;
    let mut traces = Vec::new();
    let mut last_play = None;
    for _ in 0..SCRIPT_CAP {
        let legal = ruleset.legal_actions(&state);
        let named = |id: &str| {
            legal.iter().find(
                |action| matches!(action, Action::Declared { action, .. } if action.as_str() == id),
            )
        };
        let any_answer = legal.iter().find(|a| matches!(a, Action::Answer { .. }));
        let chosen = if any_answer.is_some() {
            any_answer
        } else if state.stack().is_empty() {
            // At most one creature is played per Turn, so mana is left for a response.
            let turn = state.turn_number();
            let play = if last_play == Some(turn) {
                None
            } else {
                named("play_creature")
            };
            if play.is_some() {
                last_play = Some(turn);
            }
            named("attack").or(play).or_else(|| named("pass_turn"))
        } else {
            named("fast_play").or_else(|| legal.iter().find(|a| matches!(a, Action::Pass { .. })))
        };
        let Some(action) = chosen.cloned() else {
            break;
        };
        let transition = accepted(ruleset, &state, &action);
        record.push_accepted(action);
        traces.push(transition.trace.into_events());
        state = transition.state;
        if state.outcome().is_some() {
            break;
        }
    }
    (record, traces, state)
}

#[test]
fn a_record_with_pushes_and_passes_replays() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    let (mut passes, mut pushes, mut responses) = (0, 0, 0);
    for seed in 0..SCRIPTED_SEEDS {
        let (record, traces, state) = scripted_game(&ruleset, &decks, seed);
        passes += record
            .actions()
            .iter()
            .filter(|action| matches!(action, Action::Pass { .. }))
            .count();
        for event in traces.iter().flatten() {
            if let Event::ItemPushed { item } = event {
                pushes += 1;
                if item.source == StackSource::Action(ActionId::new("fast_play")) {
                    responses += 1;
                }
            }
        }

        // The record replays to the same Event Traces and the same final GameState.
        let replay = ruleset.replay(&record).expect("the record replays");
        let replayed: Vec<Vec<Event>> = replay
            .traces
            .iter()
            .map(|trace| trace.events().to_vec())
            .collect();
        assert!(replayed == traces, "seed {seed}: the Event Traces differ");
        assert!(
            replay.state == state,
            "seed {seed}: the final GameState differs"
        );
    }
    assert!(passes >= 1, "no scripted Game holds a Pass");
    assert!(pushes >= 1, "no scripted Game pushes an item");
    assert!(responses >= 1, "no scripted Game holds a response");
}

#[test]
fn two_runs_from_the_same_seed_are_equal() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    for seed in 0..5 {
        let first = scripted_game(&ruleset, &decks, seed);
        let second = scripted_game(&ruleset, &decks, seed);
        assert!(first.0 == second.0, "seed {seed}: records");
        assert!(first.1 == second.1, "seed {seed}: Event Traces");
        assert!(first.2 == second.2, "seed {seed}: final GameState");
    }
}
