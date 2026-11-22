//! Acceptance tests for ticket #75: the Option Choice, which pauses a Resolution until the Deciding
//! Player answers (ADR-0033), on the real Effects Ruleset and its real catalog (`curious_owl`),
//! with test-only cards appended to the catalog text for each behaviour a test needs.
//!
//! The Event Traces and the Paused Resolutions are asserted whole, as literals read off the rules,
//! never computed by the code under test. Every test-only card costs 0, so it is played without
//! paying mana. Scenarios arrange cards with `GameState::apply` of `CardMoved` Events before the
//! Action under test (an arrangement is not a transition, so it wakes no trigger).
//!
//! A Declared Action and an Answer are the two kinds of `Action`. While a decision is pending the
//! only Legal Actions are the Answers, in index order, and only the Deciding Player may take them.

use tcg_engine::card_catalog::{CatalogError, CatalogErrorKind};
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Answer, ApplyError, Argument, Branch, BranchKind, CardInstanceId, DecisionKind,
    DeclarativeRuleset, EffectPath, Event, GameRecord, GameState, Loss, PausedResolution,
    QueuedTrigger, Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset};
use tcg_engine::ruleset_definition::{
    ActionId, DefinitionError, DefinitionErrorKind, FieldValue, PhaseId, StateName, WinConditionId,
    ZoneId,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_card_catalog_draft,
    parse_ruleset_draft, CatalogLoadError, LoadError, YamlError, YamlErrorKind,
};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");

// ---- the test-only cards

const DRAW: &str = "{ draw: { player: controller, from: deck, to: hand, count: 1 } }";
const TAP_SELF: &str = "{ set_state: { card: { ref: self }, state: tapped, value: true } }";
const UNTAP_SELF: &str = "{ set_state: { card: { ref: self }, state: tapped, value: false } }";
const SELF_TO_TRASH: &str = "{ move: { card: { ref: self }, to: trash } }";
const SELF_TO_FIELD: &str = "{ move: { card: { ref: self }, to: field } }";
const ENTER_FIELD: &str = "{ enter: field }";
const ENTER_TRASH: &str = "{ enter: trash }";
const ENTER_HAND: &str = "{ enter: hand }";
const UNTAPPED: &str = "{ eq: [ { ref: self.tapped }, false ] }";

/// `{ choose_one: { decider, options: [ [..], [..] ] } }`.
fn choose(decider: &str, options: &[&[&str]]) -> String {
    let options: Vec<String> = options
        .iter()
        .map(|option| format!("[ {} ]", option.join(", ")))
        .collect();
    format!(
        "{{ choose_one: {{ decider: {decider}, options: [ {} ] }} }}",
        options.join(", ")
    )
}

/// A triggered Card Effect, as a YAML flow mapping.
fn triggered(on: &str, body: &[&str]) -> String {
    format!("{{ on: {on}, do: [ {} ] }}", body.join(", "))
}

/// A creature (cost 0, power 1) with these Card Effects, as a catalog list item.
fn creature(id: &str, effects: &[String]) -> String {
    let effects = if effects.is_empty() {
        String::new()
    } else {
        format!(", effects: [ {} ]", effects.join(", "))
    };
    format!(
        "  - {{ id: {id}, type: creature, fields: {{ name: \"{id}\", cost: 0, power: 1 }}{effects} }}\n"
    )
}

/// The cards each test builds on, appended to the real catalog. With `inline_the_choice` the
/// `weaver` has the chosen option written into its `do` instead of the Choice: the same cards and
/// ids, so the Games of the two catalogs can be compared Event for Event.
fn test_cards(inline_the_choice: bool) -> String {
    let mut yaml = String::new();
    let mut add = |id: &str, effects: &[String]| yaml.push_str(&creature(id, effects));
    let ask_to_draw = choose("controller", &[&[DRAW], &[]]);
    let ask_to_tap = choose("controller", &[&[TAP_SELF], &[]]);

    // Two Card Effects on entering the field: the first asks, the second taps.
    add(
        "pair_owl",
        &[
            triggered(ENTER_FIELD, &[ask_to_draw.as_str()]),
            triggered(ENTER_FIELD, &[TAP_SELF]),
        ],
    );
    // The controller's opponent decides whether the controller draws.
    let ask_opponent = choose("opponent", &[&[DRAW], &[]]);
    add(
        "meddler",
        &[triggered(ENTER_FIELD, &[ask_opponent.as_str()])],
    );
    // A Choice inside an option: the first option asks again.
    let inner = choose("controller", &[&[DRAW], &[TAP_SELF]]);
    let outer = choose("controller", &[&[inner.as_str()], &[]]);
    add("nest_owl", &[triggered(ENTER_FIELD, &[outer.as_str()])]);
    // A second Choice later in the same `do`.
    add(
        "twice_owl",
        &[triggered(
            ENTER_FIELD,
            &[ask_to_draw.as_str(), ask_to_tap.as_str()],
        )],
    );
    // A Choice inside an `if`: in `then` when untapped, in `else` (after a draw) when tapped.
    let ask_to_untap = choose("controller", &[&[UNTAP_SELF], &[]]);
    let branching =
        format!("{{ if: {UNTAPPED}, then: [ {ask_to_draw} ], else: [ {DRAW}, {ask_to_untap} ] }}");
    add(
        "branch_owl",
        &[triggered(ENTER_FIELD, &[branching.as_str()])],
    );
    // Its `if` reads the state the `do` itself changes before it asks: a resume that read the
    // condition again would take another branch.
    let latching = format!("{{ if: {UNTAPPED}, then: [ {TAP_SELF}, {ask_to_draw} ] }}");
    add("latch_owl", &[triggered(ENTER_FIELD, &[latching.as_str()])]);
    // Taps itself when a card arrives in the hand: what the drawn cards of `weaver` wake.
    add("hand_tapper", &[triggered(ENTER_HAND, &[TAP_SELF])]);
    // Three Card Effects on entering the field: one that fires before the pausing one, the pausing
    // one (which draws, asks, then draws), and one that is still queued when it pauses.
    let middle = if inline_the_choice {
        triggered(ENTER_FIELD, &[DRAW, DRAW, DRAW])
    } else {
        triggered(ENTER_FIELD, &[DRAW, ask_to_draw.as_str(), DRAW])
    };
    add(
        "weaver",
        &[
            triggered(ENTER_FIELD, &[TAP_SELF]),
            middle,
            triggered(ENTER_FIELD, &[UNTAP_SELF]),
        ],
    );
    // Each of its two Card Effects asks whether to move it to the other zone, which wakes the other
    // one: a loop through an Option Choice.
    let ask_to_trash = choose("controller", &[&[SELF_TO_TRASH], &[]]);
    let ask_to_field = choose("controller", &[&[SELF_TO_FIELD], &[]]);
    add(
        "restless_pair",
        &[
            triggered(ENTER_FIELD, &[ask_to_trash.as_str()]),
            triggered(ENTER_TRASH, &[ask_to_field.as_str()]),
        ],
    );
    // Draws (from an empty deck, that is a loss) and then asks, with two empty options.
    let ask_nothing = choose("controller", &[&[], &[]]);
    add(
        "starved_owl",
        &[triggered(ENTER_FIELD, &[DRAW, ask_nothing.as_str()])],
    );
    yaml
}

/// The real creatures that fill a deck to 50 cards.
const FILLERS: [&str; 6] = [
    "goblin_scout",
    "moss_sprite",
    "ember_hound",
    "river_otter",
    "stone_golem",
    "shadow_cat",
];

/// Every card of `test_cards` and the real owl, four copies each.
const SPECIALS: [&str; 12] = [
    "curious_owl",
    "pair_owl",
    "meddler",
    "nest_owl",
    "twice_owl",
    "branch_owl",
    "latch_owl",
    "hand_tapper",
    "weaver",
    "restless_pair",
    "starved_owl",
    "goblin_scout",
];

fn deck_yaml(leader: &str) -> String {
    let mut yaml = format!(
        "inputs:\n  - input: leader\n    cards:\n      - {{ card: {leader}, count: 1 }}\n  - input: main_deck\n    cards:\n"
    );
    let mut total = 0;
    for card in SPECIALS {
        yaml.push_str(&format!("      - {{ card: {card}, count: 4 }}\n"));
        total += 4;
    }
    for card in FILLERS.iter().filter(|card| !SPECIALS.contains(card)) {
        let count = (50 - total).min(4);
        if count == 0 {
            break;
        }
        yaml.push_str(&format!("      - {{ card: {card}, count: {count} }}\n"));
        total += count;
    }
    assert_eq!(total, 50);
    yaml.push_str("  - input: mana_deck\n    cards:\n      - { card: mana, count: 10 }\n");
    yaml
}

fn ruleset_with(inline_the_choice: bool) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(EFFECTS_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(
        &definition,
        &format!("{EFFECTS_CATALOG}{}", test_cards(inline_the_choice)),
    )
    .expect("the test catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn ruleset() -> DeclarativeRuleset {
    ruleset_with(false)
}

fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    ["captain_ardel", "warden_ilse"]
        .iter()
        .map(|leader| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), &deck_yaml(leader))
                .expect("the test deck is legal")
        })
        .collect()
}

// ---- cards, actions and arrangement

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn play(player: u32, card: CardInstanceId) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new("play_creature"),
        arguments: vec![Argument::Card(card)],
    }
}

fn pass(player: u32) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new("pass_turn"),
        arguments: vec![],
    }
}

fn answer(player: u32, option: u32) -> Action {
    Action::Answer {
        player: p(player),
        answer: Answer::Option(option),
    }
}

fn answered(player: u32, option: u32) -> Event {
    Event::DecisionAnswered {
        player: p(player),
        answer: Answer::Option(option),
    }
}

/// The `n`th card (0-based, in `state.cards()` order) of `player` made from the definition `id`.
fn nth(state: &GameState, player: u32, id: &str, n: usize) -> CardInstanceId {
    state
        .cards()
        .iter()
        .filter(|card| card.owner() == p(player) && card.definition().as_str() == id)
        .nth(n)
        .unwrap_or_else(|| panic!("player {player} has no card {n} of `{id}`"))
        .id()
}

/// Moves `card` to the top of `to` of its owner with an Event: an arrangement, not a transition.
fn park(state: &mut GameState, card: CardInstanceId, to: &str) {
    let owner = state.card(card).expect("the card exists").owner();
    let from = state.zone_of(card).expect("the card is in a zone").clone();
    if from == zone(to) {
        return;
    }
    state
        .apply(&Event::CardMoved {
            card,
            player: owner,
            from,
            to: zone(to),
        })
        .expect("the arrangement is a legal move");
}

fn set_tapped(state: &mut GameState, card: CardInstanceId) {
    state
        .apply(&state_changed(card, "tapped", true))
        .expect("the card has the state `tapped`");
}

fn in_zone(state: &GameState, player: u32, id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(id))
        .expect("the zone exists")
        .to_vec()
}

fn top_of_deck(state: &GameState, player: u32) -> CardInstanceId {
    *in_zone(state, player, "deck")
        .last()
        .expect("the deck is not empty")
}

/// The Event that draws `card` for `player`.
fn drew(player: u32, card: CardInstanceId) -> Event {
    moved(player, card, "deck", "hand")
}

fn moved(player: u32, card: CardInstanceId, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(player),
        from: zone(from),
        to: zone(to),
    }
}

fn state_changed(card: CardInstanceId, state: &str, value: bool) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new(state),
        value: FieldValue::Bool(value),
    }
}

/// Seed 1's opening: Player 0 is in Main of Turn 1, nothing arranged.
fn start(ruleset: &DeclarativeRuleset) -> GameState {
    ruleset
        .create_game(&decks(ruleset), Seed(1))
        .expect("the test decks create a game")
        .opening
        .state
}

/// `state` with the first card of `player` made from `id` in their hand.
fn with_in_hand(state: &GameState, player: u32, id: &str) -> (GameState, CardInstanceId) {
    let mut state = state.clone();
    let card = nth(&state, player, id, 0);
    park(&mut state, card, "hand");
    (state, card)
}

fn accepted(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
) -> (GameState, Vec<Event>) {
    let transition = ruleset
        .transition(state, action)
        .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"));
    (transition.state, transition.trace.into_events())
}

fn pending_paused(state: &GameState) -> PausedResolution {
    state
        .pending_decision()
        .expect("a decision is pending")
        .paused()
        .clone()
}

fn pause(
    decider: u32,
    trigger: (CardInstanceId, u32),
    at: EffectPath,
    queue: Vec<QueuedTrigger>,
    unscanned: Vec<Event>,
    resolutions: u32,
) -> Event {
    Event::DecisionPending {
        decider: p(decider),
        kind: DecisionKind::Option,
        paused: PausedResolution {
            trigger: QueuedTrigger {
                card: trigger.0,
                effect: trigger.1,
            },
            at,
            queue,
            unscanned,
            resolutions,
            bound: Vec::new(),
        },
    }
}

fn path(branches: Vec<Branch>, choice: u32) -> EffectPath {
    EffectPath { branches, choice }
}

fn queued(card: CardInstanceId, effect: u32) -> QueuedTrigger {
    QueuedTrigger { card, effect }
}

// ---- the pause and the Answers

#[test]
fn playing_curious_owl_pauses_for_its_controller() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let mana = in_zone(&state, 0, "mana_zone")[0];

    let (paused, trace) = accepted(&ruleset, &state, &play(0, owl));

    // The play's Events, then the pause as the last Event: the owl's `do` is `choose_one` alone, so
    // nothing of it is unscanned, and it is the first trigger of the Resolution.
    assert_eq!(
        trace,
        [
            state_changed(mana, "tapped", true),
            moved(0, owl, "hand", "field"),
            state_changed(owl, "recently_played", true),
            pause(0, (owl, 0), path(vec![], 0), vec![], vec![], 1),
        ]
    );
    // The Pending Decision is in the GameState, and it is the controller's.
    let pending = paused.pending_decision().expect("a decision is pending");
    assert_eq!(pending.decider(), p(0));
    assert_eq!(pending.kind(), DecisionKind::Option);
    assert_eq!(paused.deciding_player(), p(0));
    // The Answers are the only Legal Actions: Option 0 and Option 1, in that order.
    assert_eq!(ruleset.legal_actions(&paused), [answer(0, 0), answer(0, 1)]);
    // Without a pending decision the Deciding Player is the Active Player.
    assert_eq!(state.deciding_player(), p(0));
    assert!(state.pending_decision().is_none());
}

#[test]
fn answering_the_first_option_draws_a_card() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let (paused, _) = accepted(&ruleset, &state, &play(0, owl));
    let top = top_of_deck(&paused, 0);
    let hand = in_zone(&paused, 0, "hand");

    let (after, trace) = accepted(&ruleset, &paused, &answer(0, 0));

    // The trace starts with `DecisionAnswered`; the option's draw follows.
    assert_eq!(trace, [answered(0, 0), drew(0, top)]);
    assert!(after.pending_decision().is_none());
    assert_eq!(in_zone(&after, 0, "hand").len(), hand.len() + 1);
    assert!(in_zone(&after, 0, "hand").contains(&top));
    // The Game goes on: the Active Player's Declared Actions are Legal again.
    assert!(ruleset.legal_actions(&after).contains(&pass(0)));
}

#[test]
fn answering_the_empty_option_draws_none() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let (paused, _) = accepted(&ruleset, &state, &play(0, owl));
    let deck = in_zone(&paused, 0, "deck");
    let hand = in_zone(&paused, 0, "hand");

    let (after, trace) = accepted(&ruleset, &paused, &answer(0, 1));

    // "May" is an Option Choice with an empty option: the trace is the Answer alone.
    assert_eq!(trace, [answered(0, 1)]);
    assert!(after.pending_decision().is_none());
    assert_eq!(in_zone(&after, 0, "deck"), deck);
    assert_eq!(in_zone(&after, 0, "hand"), hand);
}

#[test]
fn the_resolution_continues_with_the_next_queued_trigger() {
    let ruleset = ruleset();
    let (state, pair) = with_in_hand(&start(&ruleset), 0, "pair_owl");

    // Entering the field wakes both Card Effects; the first asks, the second waits in the queue.
    let (paused, trace) = accepted(&ruleset, &state, &play(0, pair));
    assert_eq!(
        trace,
        [
            moved(0, pair, "hand", "field"),
            state_changed(pair, "recently_played", true),
            pause(
                0,
                (pair, 0),
                path(vec![], 0),
                vec![queued(pair, 1)],
                vec![],
                1
            ),
        ]
    );

    // The Answer runs the option and then the queued trigger, in its own trace.
    let top = top_of_deck(&paused, 0);
    let (after, trace) = accepted(&ruleset, &paused, &answer(0, 0));
    assert_eq!(
        trace,
        [
            answered(0, 0),
            drew(0, top),
            state_changed(pair, "tapped", true)
        ]
    );
    assert!(after.pending_decision().is_none());

    // The empty option leaves the second trigger to fire all the same.
    let (_, trace) = accepted(&ruleset, &paused, &answer(0, 1));
    assert_eq!(trace, [answered(0, 1), state_changed(pair, "tapped", true)]);
}

#[test]
fn an_opponent_decider_is_the_deciding_player() {
    let ruleset = ruleset();
    let (state, meddler) = with_in_hand(&start(&ruleset), 0, "meddler");

    let (paused, trace) = accepted(&ruleset, &state, &play(0, meddler));
    assert_eq!(
        trace.last(),
        Some(&pause(1, (meddler, 0), path(vec![], 0), vec![], vec![], 1))
    );
    // The non-active Player is the Deciding Player and the only one with Legal Actions.
    assert_eq!(paused.deciding_player(), p(1));
    assert_eq!(paused.active_player(), p(0));
    assert_eq!(ruleset.legal_actions(&paused), [answer(1, 0), answer(1, 1)]);

    // The Active Player's Actions are rejected with the typed Rejection, and nothing changes.
    let before = paused.clone();
    for refused in [pass(0), answer(0, 0)] {
        assert_eq!(
            ruleset.transition(&paused, &refused).map(|_| ()),
            Err(Rejection::NotDecidingPlayer {
                player: p(0),
                deciding: p(1)
            }),
            "{refused:?}"
        );
        assert_eq!(paused, before);
    }

    // The opponent's Answer decides whether the controller draws.
    let top = top_of_deck(&paused, 0);
    let (_, trace) = accepted(&ruleset, &paused, &answer(1, 0));
    assert_eq!(trace, [answered(1, 0), drew(0, top)]);
    let (_, trace) = accepted(&ruleset, &paused, &answer(1, 1));
    assert_eq!(trace, [answered(1, 1)]);
}

#[test]
fn a_declared_action_while_a_decision_is_pending_is_rejected() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let (paused, _) = accepted(&ruleset, &state, &play(0, owl));
    let before = paused.clone();

    // The Deciding Player cannot take a Declared Action either.
    assert_eq!(
        ruleset.transition(&paused, &pass(0)).map(|_| ()),
        Err(Rejection::DeclaredWhileDecisionPending)
    );
    assert_eq!(paused, before);
    // Whoever takes a Declared Action: the other Player is not the Deciding Player.
    assert_eq!(
        ruleset.transition(&paused, &pass(1)).map(|_| ()),
        Err(Rejection::NotDecidingPlayer {
            player: p(1),
            deciding: p(0)
        })
    );
    assert_eq!(paused, before);
}

#[test]
fn an_answer_with_no_decision_pending_is_rejected() {
    let ruleset = ruleset();
    let state = start(&ruleset);
    let before = state.clone();

    assert_eq!(
        ruleset.transition(&state, &answer(0, 0)).map(|_| ()),
        Err(Rejection::NoDecisionPending)
    );
    assert_eq!(state, before);
    // The other Player is not the Deciding Player, which is checked first.
    assert_eq!(
        ruleset.transition(&state, &answer(1, 0)).map(|_| ()),
        Err(Rejection::NotDecidingPlayer {
            player: p(1),
            deciding: p(0)
        })
    );
    assert_eq!(
        ruleset.transition(&state, &answer(7, 0)).map(|_| ()),
        Err(Rejection::UnknownPlayer { player: p(7) })
    );
    assert_eq!(state, before);
}

#[test]
fn an_option_index_out_of_range_is_rejected() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let (paused, _) = accepted(&ruleset, &state, &play(0, owl));
    let before = paused.clone();

    for index in [2, 3, u32::MAX] {
        assert_eq!(
            ruleset.transition(&paused, &answer(0, index)).map(|_| ()),
            Err(Rejection::OptionOutOfRange { index, options: 2 })
        );
        assert_eq!(paused, before);
    }
}

// ---- pausing again

#[test]
fn a_choice_nested_in_an_option_pauses_again() {
    let ruleset = ruleset();
    let (state, nest) = with_in_hand(&start(&ruleset), 0, "nest_owl");
    let (first, trace) = accepted(&ruleset, &state, &play(0, nest));
    assert_eq!(
        trace.last(),
        Some(&pause(0, (nest, 0), path(vec![], 0), vec![], vec![], 1))
    );

    // Option 0 enters the inner Choice, which pauses again at its path: option 0 of the outer one,
    // then the Choice at index 0 of it. The Trigger Cap count is not reset by the Answer.
    let (second, trace) = accepted(&ruleset, &first, &answer(0, 0));
    let at = path(
        vec![Branch {
            effect: 0,
            into: BranchKind::Option(0),
        }],
        0,
    );
    assert_eq!(
        trace,
        [
            answered(0, 0),
            pause(0, (nest, 0), at.clone(), vec![], vec![], 1)
        ]
    );
    assert_eq!(pending_paused(&second).at, at);
    assert_eq!(ruleset.legal_actions(&second), [answer(0, 0), answer(0, 1)]);

    // The inner Choice's option 1 taps the card, and nothing is pending any more.
    let (done, trace) = accepted(&ruleset, &second, &answer(0, 1));
    assert_eq!(trace, [answered(0, 1), state_changed(nest, "tapped", true)]);
    assert!(done.pending_decision().is_none());

    // The outer Choice's empty option ends it at once.
    let (done, trace) = accepted(&ruleset, &first, &answer(0, 1));
    assert_eq!(trace, [answered(0, 1)]);
    assert!(done.pending_decision().is_none());
}

#[test]
fn a_second_choice_later_in_the_resolution_pauses_again() {
    let ruleset = ruleset();
    let (state, twice) = with_in_hand(&start(&ruleset), 0, "twice_owl");
    let (first, trace) = accepted(&ruleset, &state, &play(0, twice));
    assert_eq!(
        trace.last(),
        Some(&pause(0, (twice, 0), path(vec![], 0), vec![], vec![], 1))
    );

    // The draw of the first option is an Event no trigger has scanned yet: it is carried by the new
    // Paused Resolution, and the second Choice is at index 1 of the same `do`.
    let top = top_of_deck(&first, 0);
    let (second, trace) = accepted(&ruleset, &first, &answer(0, 0));
    assert_eq!(
        trace,
        [
            answered(0, 0),
            drew(0, top),
            pause(
                0,
                (twice, 0),
                path(vec![], 1),
                vec![],
                vec![drew(0, top)],
                1
            )
        ]
    );
    assert_eq!(ruleset.legal_actions(&second), [answer(0, 0), answer(0, 1)]);

    let (done, trace) = accepted(&ruleset, &second, &answer(0, 0));
    assert_eq!(
        trace,
        [answered(0, 0), state_changed(twice, "tapped", true)]
    );
    assert!(done.pending_decision().is_none());

    // The empty option of the first Choice goes straight to the second.
    let (second, trace) = accepted(&ruleset, &first, &answer(0, 1));
    assert_eq!(
        trace,
        [
            answered(0, 1),
            pause(0, (twice, 0), path(vec![], 1), vec![], vec![], 1)
        ]
    );
    assert_eq!(pending_paused(&second).at, path(vec![], 1));
}

#[test]
fn a_choice_inside_an_if_records_the_branch_it_is_in() {
    let ruleset = ruleset();
    let (state, branch) = with_in_hand(&start(&ruleset), 0, "branch_owl");

    // Untapped: the Choice is the first effect of `then`, which is the first effect of the `do`.
    let (paused, trace) = accepted(&ruleset, &state, &play(0, branch));
    let in_then = path(
        vec![Branch {
            effect: 0,
            into: BranchKind::Then,
        }],
        0,
    );
    assert_eq!(
        trace.last(),
        Some(&pause(0, (branch, 0), in_then, vec![], vec![], 1))
    );
    let (_, trace) = accepted(&ruleset, &paused, &answer(0, 1));
    assert_eq!(trace, [answered(0, 1)]);

    // Tapped: the draw of `else` runs first, then the Choice is the second effect of `else`.
    let mut tapped = state.clone();
    set_tapped(&mut tapped, branch);
    let top = top_of_deck(&tapped, 0);
    let (paused, trace) = accepted(&ruleset, &tapped, &play(0, branch));
    let in_else = path(
        vec![Branch {
            effect: 0,
            into: BranchKind::Else,
        }],
        1,
    );
    assert_eq!(
        trace[trace.len() - 2..],
        [
            drew(0, top),
            pause(0, (branch, 0), in_else, vec![], vec![drew(0, top)], 1)
        ]
    );
    // The first option of that Choice untaps the card.
    let (_, trace) = accepted(&ruleset, &paused, &answer(0, 0));
    assert_eq!(
        trace,
        [answered(0, 0), state_changed(branch, "tapped", false)]
    );
}

#[test]
fn a_resume_does_not_read_an_if_on_its_path_again() {
    let ruleset = ruleset();
    let (state, latch) = with_in_hand(&start(&ruleset), 0, "latch_owl");

    // The `do` taps the card inside `then`, and then asks.
    let (paused, trace) = accepted(&ruleset, &state, &play(0, latch));
    let in_then = path(
        vec![Branch {
            effect: 0,
            into: BranchKind::Then,
        }],
        1,
    );
    assert_eq!(
        trace[trace.len() - 2..],
        [
            state_changed(latch, "tapped", true),
            pause(
                0,
                (latch, 0),
                in_then,
                vec![],
                vec![state_changed(latch, "tapped", true)],
                1
            )
        ]
    );

    // The condition (`tapped == false`) is false now, and the Answer still runs the option it
    // chose: the draw happens, in the branch the pause was in.
    let top = top_of_deck(&paused, 0);
    let (_, trace) = accepted(&ruleset, &paused, &answer(0, 0));
    assert_eq!(trace, [answered(0, 0), drew(0, top)]);
}

// ---- a pause changes no order

#[test]
fn a_pause_changes_no_order() {
    let paused_ruleset = ruleset_with(false);
    let inlined_ruleset = ruleset_with(true);

    // The weaver is played with three `hand_tapper`s on top of the deck (the first drawn is the
    // top): the first is woken by the draw before the pause, the second by the option's draw, the
    // third by the draw after it.
    let arrange = |ruleset: &DeclarativeRuleset| {
        let (mut state, weaver) = with_in_hand(&start(ruleset), 0, "weaver");
        let tappers: Vec<CardInstanceId> =
            (0..3).map(|n| nth(&state, 0, "hand_tapper", n)).collect();
        for &tapper in tappers.iter().rev() {
            // Through the trash, so that a tapper already in the deck goes to the top.
            park(&mut state, tapper, "trash");
            park(&mut state, tapper, "deck");
        }
        (state, weaver, tappers)
    };
    let (state, weaver, tappers) = arrange(&paused_ruleset);
    let (inlined_state, inlined_weaver, inlined_tappers) = arrange(&inlined_ruleset);
    assert_eq!((weaver, &tappers), (inlined_weaver, &inlined_tappers));
    assert_eq!(state, inlined_state);
    let (t1, t2, t3) = (tappers[0], tappers[1], tappers[2]);
    assert_eq!(top_of_deck(&state, 0), t1);

    // The Events of the Game with no pause, in order: the first Card Effect taps the weaver (it
    // fires before the pausing one), the pausing one draws three cards, the third Card Effect
    // (which was queued) untaps it, and the three `hand_tapper`s tap, in the order they were
    // drawn.
    let (inlined_end, inlined_trace) = accepted(&inlined_ruleset, &inlined_state, &play(0, weaver));
    assert_eq!(
        inlined_trace,
        [
            moved(0, weaver, "hand", "field"),
            state_changed(weaver, "recently_played", true),
            state_changed(weaver, "tapped", true),
            drew(0, t1),
            drew(0, t2),
            drew(0, t3),
            state_changed(weaver, "tapped", false),
            state_changed(t1, "tapped", true),
            state_changed(t2, "tapped", true),
            state_changed(t3, "tapped", true),
        ]
    );

    // With the pause: the draw before it is carried unscanned, the third Card Effect waits in the
    // queue, and the Answer scans the carried Event first and then its own.
    let (paused, first) = accepted(&paused_ruleset, &state, &play(0, weaver));
    assert_eq!(
        first,
        [
            moved(0, weaver, "hand", "field"),
            state_changed(weaver, "recently_played", true),
            state_changed(weaver, "tapped", true),
            drew(0, t1),
            pause(
                0,
                (weaver, 1),
                path(vec![], 1),
                vec![queued(weaver, 2)],
                vec![drew(0, t1)],
                2
            ),
        ]
    );
    let (end, second) = accepted(&paused_ruleset, &paused, &answer(0, 0));
    assert_eq!(second[0], answered(0, 0));

    // Without the Decision Events, the two traces together are the trace with no pause, and the
    // Game ends in the same GameState.
    let joined: Vec<Event> = first
        .iter()
        .chain(&second)
        .filter(|event| {
            !matches!(
                event,
                Event::DecisionPending { .. } | Event::DecisionAnswered { .. }
            )
        })
        .cloned()
        .collect();
    assert_eq!(joined, inlined_trace);
    assert_eq!(end, inlined_end);
}

// ---- the Trigger Cap

#[test]
fn the_trigger_cap_counts_across_pauses() {
    let ruleset = ruleset();
    let (state, pair) = with_in_hand(&start(&ruleset), 0, "restless_pair");

    // Playing the card fires its first trigger; each Answer 0 moves it to the other zone, which
    // wakes the other trigger: one trigger per transition, all of one Resolution.
    let (mut state, _) = accepted(&ruleset, &state, &play(0, pair));
    let mut taken: u32 = 1;
    let rejection = loop {
        let paused = pending_paused(&state);
        assert_eq!(
            paused.resolutions, taken,
            "the count carries across Answers"
        );
        match ruleset.transition(&state, &answer(0, 0)) {
            Ok(next) => {
                state = next.state;
                taken += 1;
            }
            Err(rejection) => break rejection,
        }
        assert!(taken <= 100, "the cap is not enforced");
    };

    // The Answer that would take the 101st trigger of the Resolution is rejected, not reset.
    assert_eq!(taken, 100);
    assert_eq!(rejection, Rejection::TriggerCapExceeded { cap: 100 });
    let before = state.clone();
    assert_eq!(
        ruleset.transition(&state, &answer(0, 0)).map(|_| ()),
        Err(Rejection::TriggerCapExceeded { cap: 100 })
    );
    assert_eq!(state, before);
    // It is absent from the Legal Actions; the empty option, which fires nothing, is not.
    assert_eq!(ruleset.legal_actions(&state), [answer(0, 1)]);
    assert_eq!(
        ruleset.listing_with_rejections(&state),
        [
            (
                answer(0, 0),
                Some(Rejection::TriggerCapExceeded { cap: 100 })
            ),
            (answer(0, 1), None)
        ]
    );
    // The empty option ends the Resolution.
    let (done, trace) = accepted(&ruleset, &state, &answer(0, 1));
    assert_eq!(trace, [answered(0, 1)]);
    assert!(done.pending_decision().is_none());
}

// ---- win timing

#[test]
fn a_pausing_transition_that_meets_a_win_condition_ends_the_game() {
    let ruleset = ruleset();
    let mut state = start(&ruleset);
    let starved = nth(&state, 0, "starved_owl", 0);
    // Player 0's deck is empty, so the owl's draw finds nothing.
    for card in in_zone(&state, 0, "deck") {
        park(&mut state, card, "trash");
    }
    park(&mut state, starved, "hand");

    let (ended, trace) = accepted(&ruleset, &state, &play(0, starved));

    // `GameEnded` follows `DecisionPending`: the loss is Player 0's `deck_out`, and the Draw from
    // Empty Zone before the pause is carried unscanned.
    assert_eq!(
        trace,
        [
            moved(0, starved, "hand", "field"),
            state_changed(starved, "recently_played", true),
            Event::DrawFromEmptyZone {
                player: p(0),
                zone: zone("deck")
            },
            pause(
                0,
                (starved, 0),
                path(vec![], 1),
                vec![],
                vec![Event::DrawFromEmptyZone {
                    player: p(0),
                    zone: zone("deck")
                }],
                1
            ),
            Event::GameEnded {
                losses: vec![Loss {
                    player: p(0),
                    reason: WinConditionId::new("deck_out")
                }]
            },
        ]
    );
    // The outcome holds with no decision pending, and the Game is over for everyone.
    assert!(ended.pending_decision().is_none());
    let outcome = ended.outcome().expect("the Game has ended");
    assert_eq!(outcome.winners(), [p(1)]);
    assert_eq!(ended.deciding_player(), ended.active_player());
    assert!(ruleset.legal_actions(&ended).is_empty());
    for action in [answer(0, 0), pass(0), answer(1, 0)] {
        assert_eq!(
            ruleset.transition(&ended, &action).map(|_| ()),
            Err(Rejection::GameOver)
        );
    }
}

// ---- Event Application

#[test]
fn applying_decision_events_stores_and_clears_the_pending_decision() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let (paused, trace) = accepted(&ruleset, &state, &play(0, owl));

    // Event-sourced: the same Events applied to the GameState before the Action give the same
    // GameState, with the Pending Decision in it.
    assert_eq!(state.apply_trace(&trace), Ok(paused.clone()));
    let Some(Event::DecisionPending { .. }) = trace.last() else {
        panic!("the trace ends with DecisionPending");
    };
    let mut after = paused.clone();
    after
        .apply(&answered(0, 1))
        .expect("the Deciding Player answers");
    assert!(after.pending_decision().is_none());
    assert_eq!(after.deciding_player(), p(0));
}

#[test]
fn the_engine_fold_rejects_a_misplaced_decision_event() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let (paused, trace) = accepted(&ruleset, &state, &play(0, owl));
    let pending_event = trace.last().cloned().expect("the trace is not empty");
    let before = paused.clone();

    let fold = |state: &mut GameState, event: &Event| state.apply(event);
    // A second decision while one is pending; any other Event while one is pending.
    assert_eq!(
        fold(&mut paused.clone(), &pending_event),
        Err(ApplyError::DecisionAlreadyPending)
    );
    for event in [
        Event::PhaseBegan {
            phase: PhaseId::new("main"),
        },
        drew(0, top_of_deck(&paused, 0)),
        state_changed(owl, "tapped", true),
    ] {
        let mut working = paused.clone();
        assert_eq!(fold(&mut working, &event), Err(ApplyError::DecisionPending));
        assert_eq!(working, before);
    }
    // An Answer from another Player than the decider, or with nothing pending.
    let mut working = paused.clone();
    assert_eq!(
        fold(&mut working, &answered(1, 0)),
        Err(ApplyError::AnswerFromAnotherPlayer {
            player: p(1),
            decider: p(0)
        })
    );
    assert_eq!(working, before);
    let mut idle = state.clone();
    assert_eq!(
        fold(&mut idle, &answered(0, 0)),
        Err(ApplyError::NoDecisionPending)
    );
    assert_eq!(idle, state);
    // A decider that is not in the Game.
    let unknown = Event::DecisionPending {
        decider: p(9),
        kind: DecisionKind::Option,
        paused: pending_paused(&paused),
    };
    let mut idle = state.clone();
    assert_eq!(
        fold(&mut idle, &unknown),
        Err(ApplyError::UnknownPlayer { player: p(9) })
    );
    assert_eq!(idle, state);

    // `GameEnded` applies while a decision is pending, and clears it.
    let mut ended = paused.clone();
    ended
        .apply(&Event::GameEnded {
            losses: vec![Loss {
                player: p(1),
                reason: WinConditionId::new("deck_out"),
            }],
        })
        .expect("the Game can end while a decision is pending");
    assert!(ended.pending_decision().is_none());
    assert!(ended.outcome().is_some());
}

// ---- replay and determinism

/// The Actions of a scripted Game with Answers: Player 0 plays the owl and draws, then passes.
fn script(ruleset: &DeclarativeRuleset) -> (Vec<Action>, GameState) {
    let (state, owl) = with_in_hand(&start(ruleset), 0, "curious_owl");
    (vec![play(0, owl), answer(0, 0), pass(0)], state)
}

#[test]
fn a_record_with_answers_replays_exactly() {
    let ruleset = ruleset();
    // A record holds no arrangement: find the Seed whose opening hand of Player 0 has the owl.
    let decks = decks(&ruleset);
    let (seed, owl) = (0..2000)
        .find_map(|seed| {
            let opening = ruleset
                .create_game(&decks, Seed(seed))
                .expect("the decks create a game")
                .opening
                .state;
            in_zone(&opening, 0, "hand")
                .into_iter()
                .find(|&card| {
                    opening
                        .card(card)
                        .is_some_and(|c| c.definition().as_str() == "curious_owl")
                })
                .map(|card| (seed, card))
        })
        .expect("some Seed deals the owl to Player 0");

    let mut record = GameRecord::new(&ruleset, &decks, Seed(seed));
    let mut state = ruleset
        .create_game(&decks, Seed(seed))
        .expect("the decks create a game")
        .opening
        .state;
    let mut traces = Vec::new();
    // The owl costs 1 and the opening position has mana: play it, answer 0, pass.
    for action in [play(0, owl), answer(0, 0), pass(0)] {
        let transition = ruleset
            .transition(&state, &action)
            .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"));
        record.push_accepted(action);
        state = transition.state;
        traces.push(transition.trace);
    }
    assert!(traces[0]
        .events()
        .iter()
        .any(|event| matches!(event, Event::DecisionPending { .. })));
    assert_eq!(traces[1].events()[0], answered(0, 0));

    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(replay.traces, traces);
    assert_eq!(replay.state, state);
}

#[test]
fn two_runs_from_the_same_seed_with_answers_are_equal() {
    let ruleset = ruleset();
    let (actions, arranged) = script(&ruleset);
    let run = || {
        let mut state = arranged.clone();
        let mut traces = Vec::new();
        for action in &actions {
            let transition = ruleset
                .transition(&state, action)
                .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"));
            state = transition.state;
            traces.push(transition.trace);
        }
        (state, traces)
    };
    let (first_state, first_traces) = run();
    let (second_state, second_traces) = run();
    assert_eq!(first_state, second_state);
    assert_eq!(first_traces, second_traces);
    // The whole opening from the same Seed is equal too, before any Action.
    assert_eq!(start(&ruleset), start(&ruleset));
}

// ---- load errors

fn path_of() -> DefinitionPath {
    DefinitionPath::root()
}

fn edit(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "`{from}` occurs once");
    text.replace(from, to)
}

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

const CHOOSE_NOTHING: &str = "{ choose_one: { decider: controller, options: [ [], [] ] } }";

#[test]
fn a_choice_in_a_ruleset_action_is_a_load_error() {
    // Directly among the effects of an action.
    let yaml = edit(
        EFFECTS_RULESET,
        "    effects: [ { end_turn: {} } ]",
        &format!("    effects: [ {CHOOSE_NOTHING}, {{ end_turn: {{}} }} ]"),
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: path_of()
                .key("actions")
                .id("pass_turn")
                .key("effects")
                .index(0)
                .key("choose_one"),
            kind: DefinitionErrorKind::ChoiceOutsideTriggeredEffect,
        }
    );

    // Nested in an `if` of an action: at its own path.
    let yaml = edit(
        EFFECTS_RULESET,
        "    effects: [ { end_turn: {} } ]",
        &format!(
            "    effects: [ {{ if: true, then: [ {CHOOSE_NOTHING} ] }}, {{ end_turn: {{}} }} ]"
        ),
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: path_of()
                .key("actions")
                .id("pass_turn")
                .key("effects")
                .index(0)
                .key("then")
                .index(0)
                .key("choose_one"),
            kind: DefinitionErrorKind::ChoiceOutsideTriggeredEffect,
        }
    );
}

#[test]
fn a_choice_in_a_step_is_a_load_error() {
    // A step has no effect grammar: `choose_one` is an unknown key of the step, at its position.
    let yaml = edit(
        EFFECTS_RULESET,
        "        - untap: { player: active, zones: [mana_zone, field] }\n",
        "        - choose_one: { decider: controller, options: [ [], [] ] }\n        - untap: { player: active, zones: [mana_zone, field] }\n",
    );
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "choose_one".to_string()
        }
    );
    assert_eq!(
        error.path,
        path_of()
            .key("turn")
            .key("phases")
            .index(0)
            .key("automatic")
            .index(0)
    );
    // The syntax stage reports it before any engine check.
    assert!(parse_ruleset_draft(&yaml).is_err());

    // The same for a setup step.
    let yaml = edit(
        EFFECTS_RULESET,
        "  - shuffle: { player: each, zone: deck }\n",
        "  - choose_one: { decider: controller, options: [ [], [] ] }\n  - shuffle: { player: each, zone: deck }\n",
    );
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "choose_one".to_string()
        }
    );
    assert_eq!(error.path, path_of().key("setup").index(0));
}

fn sage_yaml(effects: &str) -> String {
    format!(
        "meta: {{ ruleset: effects, ruleset_version: 1 }}\ncards:\n  - id: sage\n    type: creature\n    fields: {{ name: Sage, cost: 1, power: 1 }}\n    effects:\n{effects}"
    )
}

fn effects_definition() -> tcg_engine::ruleset_definition::RulesetDefinition {
    load_ruleset_definition(EFFECTS_RULESET).expect("the Effects Ruleset loads")
}

fn catalog_error(effects: &str) -> CatalogError {
    match load_card_catalog(&effects_definition(), &sage_yaml(effects)) {
        Err(CatalogLoadError::Catalog(error)) => error,
        other => panic!("expected a catalog error, got {other:?}"),
    }
}

fn invalid_card_effect(at: DefinitionPath, kind: DefinitionErrorKind) -> CatalogError {
    CatalogError {
        path: at,
        kind: CatalogErrorKind::InvalidCardEffect {
            error: Box::new(kind),
        },
    }
}

/// `cards[sage].effects[0].do[0].choose_one`, where the engine reports an error of a Choice.
fn sage_choice_path() -> DefinitionPath {
    path_of()
        .key("cards")
        .id("sage")
        .key("effects")
        .index(0)
        .key("do")
        .index(0)
        .key("choose_one")
}

#[test]
fn a_choice_in_a_continuous_card_effect_is_a_load_error() {
    // A Continuous Card Effect has no effect grammar: `choose_one` is an unknown key of the Card
    // Effect, at its position.
    let yaml = sage_yaml(
        "      - { while: true, modify: { field: power, amount: 1 }, choose_one: { decider: controller, options: [ [], [] ] } }\n",
    );
    let Err(error) = parse_card_catalog_draft(&yaml) else {
        panic!("an unknown key of a Card Effect is refused");
    };
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "choose_one".to_string()
        }
    );
    assert_eq!(
        error.path,
        path_of()
            .key("cards")
            .index(0)
            .key("effects")
            .index(0)
            .key("choose_one")
    );
}

#[test]
fn a_choose_one_with_fewer_than_two_options_is_a_load_error() {
    for (options, found) in [("[ [] ]", 1), ("[ [ { end_turn: {} } ] ]", 1), ("[]", 0)] {
        let effects = format!(
            "      - {{ on: {{ enter: field }}, do: [ {{ choose_one: {{ decider: controller, options: {options} }} }} ] }}\n"
        );
        assert_eq!(
            catalog_error(&effects),
            invalid_card_effect(
                sage_choice_path().key("options"),
                DefinitionErrorKind::TooFewOptions { found }
            ),
            "{options}"
        );
    }
}

#[test]
fn a_choose_one_with_an_unknown_decider_is_a_load_error() {
    let effects = "      - { on: { enter: field }, do: [ { choose_one: { decider: referee, options: [ [], [] ] } } ] }\n";
    assert_eq!(
        catalog_error(effects),
        invalid_card_effect(
            sage_choice_path().key("decider"),
            DefinitionErrorKind::UnknownDecider {
                decider: "referee".to_string()
            }
        )
    );
}

#[test]
fn the_effects_inside_an_option_are_checked_where_they_stand() {
    // An effect of an option is checked like one of the `do`: this draw names an unknown zone.
    let effects = "      - { on: { enter: field }, do: [ { choose_one: { decider: controller, options: [ [], [ { draw: { player: controller, from: nowhere, to: hand, count: 1 } } ] ] } } ] }\n";
    assert_eq!(
        catalog_error(effects),
        invalid_card_effect(
            sage_choice_path()
                .key("options")
                .index(1)
                .index(0)
                .key("draw")
                .key("from"),
            DefinitionErrorKind::DanglingReference {
                target: tcg_engine::ruleset_definition::ReferenceTarget::Zone,
                id: "nowhere".to_string()
            }
        )
    );
}

#[test]
fn an_end_turn_inside_an_option_is_rejected_like_one_in_the_do() {
    let effects = "      - { on: { enter: field }, do: [ { choose_one: { decider: controller, options: [ [], [ { end_turn: {} } ] ] } } ] }\n";
    assert_eq!(
        catalog_error(effects),
        CatalogError {
            path: sage_choice_path().key("options").index(1).index(0),
            kind: CatalogErrorKind::EndTurnInCardEffect,
        }
    );
}

/// The Effects Ruleset with `players` Players.
fn effects_definition_of(players: u32) -> tcg_engine::ruleset_definition::RulesetDefinition {
    load_ruleset_definition(&edit(
        EFFECTS_RULESET,
        "  players: 2",
        &format!("  players: {players}"),
    ))
    .expect("the test-only ruleset loads")
}

#[test]
fn an_opponent_decider_needs_exactly_two_players() {
    use tcg_engine::ruleset_definition::RulesetDefinition;

    let direct = format!(
        "      - {{ on: {{ enter: field }}, do: [ {} ] }}\n",
        choose("opponent", &[&[DRAW], &[]])
    );
    // Nested in an `if` and an option, the path is still the `decider` of that Choice.
    let inner = choose("opponent", &[&[], &[]]);
    let outer = choose("controller", &[&[inner.as_str()], &[]]);
    let nested = format!(
        "      - {{ on: {{ enter: field }}, do: [ {{ if: true, then: [ {outer} ] }} ] }}\n"
    );
    let error_of = |definition: &RulesetDefinition, effects: &str| match load_card_catalog(
        definition,
        &sage_yaml(effects),
    ) {
        Err(CatalogLoadError::Catalog(error)) => error,
        other => panic!("expected a catalog error, got {other:?}"),
    };
    for players in [1, 3] {
        let definition = effects_definition_of(players);
        assert_eq!(
            error_of(&definition, &direct),
            invalid_card_effect(
                sage_choice_path().key("decider"),
                DefinitionErrorKind::OpponentDeciderNeedsTwoPlayers {
                    players: u64::from(players)
                }
            ),
            "{players} Players"
        );
        let at = path_of()
            .key("cards")
            .id("sage")
            .key("effects")
            .index(0)
            .key("do")
            .index(0)
            .key("then")
            .index(0)
            .key("choose_one")
            .key("options")
            .index(0)
            .index(0)
            .key("choose_one")
            .key("decider");
        assert_eq!(
            error_of(&definition, &nested),
            invalid_card_effect(
                at,
                DefinitionErrorKind::OpponentDeciderNeedsTwoPlayers {
                    players: u64::from(players)
                }
            ),
            "{players} Players, nested"
        );
        // A controller decider needs no second Player.
        let controller = format!(
            "      - {{ on: {{ enter: field }}, do: [ {} ] }}\n",
            choose("controller", &[&[DRAW], &[]])
        );
        load_card_catalog(&definition, &sage_yaml(&controller))
            .expect("a controller decider loads whatever the player count");
    }
    // With exactly 2 Players it loads.
    load_card_catalog(&effects_definition_of(2), &sage_yaml(&direct))
        .expect("an opponent decider loads with 2 Players");
}

#[test]
fn a_well_formed_choice_loads_in_a_card_effect() {
    // Two options, either one empty, nesting and an `if` around a Choice are all valid.
    let asking = choose("opponent", &[&[DRAW], &[]]);
    let inner = choose("controller", &[&[], &[]]);
    let nested = choose("controller", &[&[inner.as_str()], &[], &[]]);
    let effects = format!(
        "      - {{ on: {{ enter: field }}, do: [ {asking}, {{ if: true, then: [ {nested} ] }} ] }}\n"
    );
    load_card_catalog(&effects_definition(), &sage_yaml(&effects))
        .expect("a well-formed Choice loads");
}
