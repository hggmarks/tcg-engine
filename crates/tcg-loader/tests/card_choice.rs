//! Acceptance tests for ticket #76: the Card Choice, which asks a Player to pick one card from a
//! set during a Resolution (ADR-0034, on the pause of ADR-0033), on the real Effects Ruleset and
//! its real catalog (`snare_vine`, `mind_leech`), with test-only cards appended to the catalog text
//! for each behaviour a test needs.
//!
//! The Event Traces and the Paused Resolutions are asserted whole, as literals read off the rules,
//! never computed by the code under test. The candidates are listed in ascending Card Instance Id
//! (ADR-0018): the tests sort the ids they arranged, which is the rule and not the code's output.
//! Every test-only card costs 0, so it is played without paying mana. Scenarios arrange cards with
//! `GameState::apply` of `CardMoved` Events before the Action under test (an arrangement is not a
//! transition, so it wakes no trigger).

use tcg_engine::card_catalog::{CatalogError, CatalogErrorKind};
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Answer, Argument, BoundName, Branch, BranchKind, CardInstanceId, DecisionKind,
    DeclarativeRuleset, EffectPath, Event, GameRecord, GameState, PausedResolution, QueuedTrigger,
    Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset};
use tcg_engine::ruleset_definition::{
    ActionId, DefinitionError, DefinitionErrorKind, FieldValue, ParamName, ReferenceTarget,
    StateName, ZoneId,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_ruleset_draft,
    CatalogLoadError, LoadError, YamlError, YamlErrorKind,
};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");

// ---- the test-only cards

const DRAW: &str = "{ draw: { player: controller, from: deck, to: hand, count: 1 } }";
const TAP_SELF: &str = "{ set_state: { card: { ref: self }, state: tapped, value: true } }";
const ENTER_FIELD: &str = "{ enter: field }";
const ENTER_HAND: &str = "{ enter: hand }";
const UNTAPPED: &str = "{ eq: [ { ref: self.tapped }, false ] }";

/// `{ choose_one: { decider: controller, options: [ [..], [..] ] } }`.
fn choose(options: &[&[&str]]) -> String {
    let options: Vec<String> = options
        .iter()
        .map(|option| format!("[ {} ]", option.join(", ")))
        .collect();
    format!(
        "{{ choose_one: {{ decider: controller, options: [ {} ] }} }}",
        options.join(", ")
    )
}

/// `{ choose_card: { name, decider, zone, card_type: creature, owner?, where? } }`.
fn choose_card(
    name: &str,
    decider: &str,
    zone: &str,
    owner: Option<&str>,
    condition: Option<&str>,
) -> String {
    let owner = owner.map_or(String::new(), |owner| format!(", owner: {owner}"));
    let condition = condition.map_or(String::new(), |condition| format!(", where: {condition}"));
    format!(
        "{{ choose_card: {{ name: {name}, decider: {decider}, zone: {zone}, card_type: creature{owner}{condition} }} }}"
    )
}

/// The controller picks a creature in an opponent's field.
fn pick_opponent_creature(name: &str) -> String {
    choose_card(name, "controller", "field", Some("opponent"), None)
}

/// Taps the card a name is bound to.
fn tap(name: &str) -> String {
    format!("{{ set_state: {{ card: {{ ref: {name} }}, state: tapped, value: true }} }}")
}

/// A triggered Card Effect, as a YAML flow mapping.
fn triggered(on: &str, body: &[&str]) -> String {
    format!("{{ on: {on}, do: [ {} ] }}", body.join(", "))
}

/// A creature (cost 0, power 1) with these Card Effects, as a catalog list item.
fn creature(id: &str, effects: &[String]) -> String {
    format!(
        "  - {{ id: {id}, type: creature, fields: {{ name: \"{id}\", cost: 0, power: 1 }}, effects: [ {} ] }}\n",
        effects.join(", ")
    )
}

/// The cards each test builds on, appended to the real catalog.
fn test_cards() -> String {
    let mut yaml = String::new();
    let mut add = |id: &str, effects: &[String]| yaml.push_str(&creature(id, effects));
    let pick_t = pick_opponent_creature("t");
    let pick_x = pick_opponent_creature("x");
    let tap_t = tap("t");
    let tap_x = tap("x");

    // Draws, picks an opponent's creature and taps it, draws again; a second Card Effect taps the
    // card itself. An empty candidate set skips what follows the pick, not the second Card Effect.
    add(
        "grabber",
        &[
            triggered(ENTER_FIELD, &[DRAW, pick_t.as_str(), tap_t.as_str(), DRAW]),
            triggered(ENTER_FIELD, &[TAP_SELF]),
        ],
    );
    // The pick is inside an `if` that is followed by a draw: an empty candidate set skips the draw
    // after the `if` too, the whole `do`. Tapped, the `if` is false and the draw runs.
    let guarded = format!("{{ if: {UNTAPPED}, then: [ {pick_t}, {DRAW} ] }}");
    add(
        "deep_grabber",
        &[triggered(ENTER_FIELD, &[guarded.as_str(), DRAW])],
    );
    // The name is read after a nested Option Choice that comes after the pick.
    let ask = choose(&[&[TAP_SELF], &[]]);
    add(
        "picker",
        &[triggered(
            ENTER_FIELD,
            &[pick_x.as_str(), ask.as_str(), tap_x.as_str()],
        )],
    );
    // The name is read inside an option inside an `if`.
    let ask_to_tap_x = choose(&[&[tap_x.as_str()], &[]]);
    let branching = format!("{{ if: {UNTAPPED}, then: [ {ask_to_tap_x} ] }}");
    add(
        "if_picker",
        &[triggered(
            ENTER_FIELD,
            &[pick_x.as_str(), branching.as_str()],
        )],
    );
    // A name bound inside the `then` of an `if` is out of scope after it, so a later pick may use
    // it again, and reads its own card.
    let scoped = format!("{{ if: {UNTAPPED}, then: [ {pick_t}, {tap_t} ] }}");
    add(
        "leak_picker",
        &[triggered(
            ENTER_FIELD,
            &[scoped.as_str(), pick_t.as_str(), tap_t.as_str()],
        )],
    );
    // The `where` of the second pick reads the first: a creature at least as strong.
    let pick_a = pick_opponent_creature("a");
    let tap_b = tap("b");
    let stronger = choose_card(
        "b",
        "controller",
        "field",
        Some("opponent"),
        Some("{ gte: [ { ref: b.power }, { ref: a.power } ] }"),
    );
    add(
        "twin_picker",
        &[triggered(
            ENTER_FIELD,
            &[pick_a.as_str(), stronger.as_str(), tap_b.as_str()],
        )],
    );
    // Two alternatives: an opponent's creature in the field or an opponent's leader. The name is
    // narrowed by `is_card_type`: a creature taps, a leader (the only one with `current_life`)
    // loses a life.
    let wide = "{ choose_card: { name: t, decider: controller, one_of: [ { zone: field, card_type: creature, owner: opponent }, { zone: leader_zone, card_type: leader, owner: opponent } ] } }";
    let narrowing = format!(
        "{{ if: {{ is_card_type: [ {{ ref: t }}, creature ] }}, then: [ {tap_t} ], else: [ {{ add_state: {{ card: {{ ref: t }}, state: current_life, amount: -1 }} }} ] }}"
    );
    add(
        "wide_picker",
        &[triggered(ENTER_FIELD, &[wide, narrowing.as_str()])],
    );
    // Taps itself when a card arrives in the hand: what the draw of `grabber` wakes.
    add("hand_tapper", &[triggered(ENTER_HAND, &[TAP_SELF])]);
    yaml
}

/// The real creatures that fill a deck to 50 cards.
const FILLERS: [&str; 2] = ["goblin_scout", "moss_sprite"];

/// The cards of the decks, four copies each.
const SPECIALS: [&str; 12] = [
    "snare_vine",
    "mind_leech",
    "grabber",
    "deep_grabber",
    "picker",
    "if_picker",
    "leak_picker",
    "twin_picker",
    "wide_picker",
    "hand_tapper",
    "curious_owl",
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
        yaml.push_str(&format!("      - {{ card: {card}, count: {count} }}\n"));
        total += count;
    }
    assert_eq!(total, 50);
    yaml.push_str("  - input: mana_deck\n    cards:\n      - { card: mana, count: 10 }\n");
    yaml
}

fn ruleset() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(EFFECTS_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, &format!("{EFFECTS_CATALOG}{}", test_cards()))
        .expect("the test catalog loads");
    DeclarativeRuleset::new(definition, catalog)
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

fn answer_option(player: u32, option: u32) -> Action {
    Action::Answer {
        player: p(player),
        answer: Answer::Option(option),
    }
}

fn answer_card(player: u32, card: CardInstanceId) -> Action {
    Action::Answer {
        player: p(player),
        answer: Answer::Card(card),
    }
}

fn answered_option(player: u32, option: u32) -> Event {
    Event::DecisionAnswered {
        player: p(player),
        answer: Answer::Option(option),
    }
}

fn answered_card(player: u32, card: CardInstanceId) -> Event {
    Event::DecisionAnswered {
        player: p(player),
        answer: Answer::Card(card),
    }
}

/// The ids in ascending order: the order the candidates are listed in.
fn ascending(mut cards: Vec<CardInstanceId>) -> Vec<CardInstanceId> {
    cards.sort();
    cards
}

/// The Card Answers of `player` for these cards, in ascending Card Instance Id.
fn card_answers(player: u32, cards: Vec<CardInstanceId>) -> Vec<Action> {
    ascending(cards)
        .into_iter()
        .map(|card| answer_card(player, card))
        .collect()
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

fn is_tapped(state: &GameState, card: CardInstanceId) -> bool {
    state
        .card(card)
        .and_then(|instance| instance.state_value(&StateName::new("tapped")))
        == Some(&FieldValue::Bool(true))
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

fn moved(player: u32, card: CardInstanceId, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(player),
        from: zone(from),
        to: zone(to),
    }
}

fn drew(player: u32, card: CardInstanceId) -> Event {
    moved(player, card, "deck", "hand")
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

/// `state` with every card of the hand of `player` in their trash.
fn with_empty_hand(state: &GameState, player: u32) -> GameState {
    let mut state = state.clone();
    for card in in_zone(&state, player, "hand") {
        park(&mut state, card, "trash");
    }
    state
}

/// `state` with the `n`th card of Player 1 made from each `(id, n)` in their field.
fn with_opponent_creatures(
    state: &GameState,
    wanted: &[(&str, usize)],
) -> (GameState, Vec<CardInstanceId>) {
    let mut state = state.clone();
    let mut cards = Vec::new();
    for &(id, n) in wanted {
        let card = nth(&state, 1, id, n);
        park(&mut state, card, "field");
        cards.push(card);
    }
    (state, cards)
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

fn paused(
    trigger: (CardInstanceId, u32),
    at: EffectPath,
    queue: Vec<QueuedTrigger>,
    unscanned: Vec<Event>,
    resolutions: u32,
    bound: Vec<BoundName>,
) -> PausedResolution {
    PausedResolution {
        trigger: QueuedTrigger {
            card: trigger.0,
            effect: trigger.1,
        },
        at,
        queue,
        unscanned,
        resolutions,
        bound,
    }
}

fn pending_event(decider: u32, kind: DecisionKind, paused: PausedResolution) -> Event {
    Event::DecisionPending {
        decider: p(decider),
        kind,
        paused,
    }
}

fn path(branches: Vec<Branch>, choice: u32) -> EffectPath {
    EffectPath { branches, choice }
}

fn bound(name: &str, card: CardInstanceId) -> BoundName {
    BoundName {
        name: ParamName::new(name),
        card,
    }
}

// ---- the criteria: snare_vine and mind_leech

#[test]
fn snare_vine_pauses_and_taps_the_chosen_creature() {
    let ruleset = ruleset();
    let (state, snare) = with_in_hand(&start(&ruleset), 0, "snare_vine");
    // Two creatures in the opponent's field, and a creature of the controller's own field: only the
    // opponent's are candidates.
    let (mut state, theirs) =
        with_opponent_creatures(&state, &[("moss_sprite", 0), ("goblin_scout", 0)]);
    let own = nth(&state, 0, "goblin_scout", 0);
    park(&mut state, own, "field");
    let mana = in_zone(&state, 0, "mana_zone");
    let (first, second) = (theirs[0], theirs[1]);

    let (paused_state, trace) = accepted(&ruleset, &state, &play(0, snare));

    // The cost taps both mana cards; then the pause, the last Event: the controller is asked, the
    // Choice is the first effect of the `do`, nothing is bound yet.
    assert_eq!(
        trace,
        [
            state_changed(mana[0], "tapped", true),
            state_changed(mana[1], "tapped", true),
            moved(0, snare, "hand", "field"),
            state_changed(snare, "recently_played", true),
            pending_event(
                0,
                DecisionKind::Card,
                paused((snare, 0), path(vec![], 0), vec![], vec![], 1, vec![])
            ),
        ]
    );
    let pending = paused_state
        .pending_decision()
        .expect("a decision is pending");
    assert_eq!(pending.decider(), p(0));
    assert_eq!(pending.kind(), DecisionKind::Card);
    assert_eq!(paused_state.deciding_player(), p(0));
    // The Legal Actions are the two Card Answers, in ascending Card Instance Id.
    let expected = card_answers(0, theirs.clone());
    assert_eq!(expected.len(), 2);
    assert_eq!(ruleset.legal_actions(&paused_state), expected);
    assert_eq!(
        ruleset.listing_with_rejections(&paused_state),
        expected
            .iter()
            .cloned()
            .map(|action| (action, None::<Rejection>))
            .collect::<Vec<_>>()
    );

    // Answering taps exactly the chosen creature.
    let (after, trace) = accepted(&ruleset, &paused_state, &answer_card(0, first));
    assert_eq!(
        trace,
        [
            answered_card(0, first),
            state_changed(first, "tapped", true)
        ]
    );
    assert!(after.pending_decision().is_none());
    assert!(is_tapped(&after, first));
    assert!(!is_tapped(&after, second));
    assert!(!is_tapped(&after, own));
    assert_eq!(after.deciding_player(), p(0));
    // The other candidate gives the mirror image.
    let (after, trace) = accepted(&ruleset, &paused_state, &answer_card(0, second));
    assert_eq!(
        trace,
        [
            answered_card(0, second),
            state_changed(second, "tapped", true)
        ]
    );
    assert!(is_tapped(&after, second) && !is_tapped(&after, first));
}

#[test]
fn mind_leech_pauses_for_the_opponent_on_the_active_players_turn() {
    let ruleset = ruleset();
    let (state, leech) = with_in_hand(&start(&ruleset), 0, "mind_leech");
    // The opponent's hand: two creatures and a mana card, which is not a creature. The controller's
    // own hand holds creatures too, and none of them is a candidate.
    let mut state = with_empty_hand(&state, 1);
    let creatures = [
        nth(&state, 1, "goblin_scout", 0),
        nth(&state, 1, "moss_sprite", 0),
    ];
    let mana_card = nth(&state, 1, "mana", 0);
    for card in [creatures[0], creatures[1], mana_card] {
        park(&mut state, card, "hand");
    }
    let mana = in_zone(&state, 0, "mana_zone");
    let candidates = ascending(creatures.to_vec());

    let (paused_state, trace) = accepted(&ruleset, &state, &play(0, leech));

    assert_eq!(
        trace,
        [
            state_changed(mana[0], "tapped", true),
            state_changed(mana[1], "tapped", true),
            moved(0, leech, "hand", "field"),
            state_changed(leech, "recently_played", true),
            pending_event(
                1,
                DecisionKind::Card,
                paused((leech, 0), path(vec![], 0), vec![], vec![], 1, vec![])
            ),
        ]
    );
    // The opponent is the Deciding Player on the other Player's Turn, and the only one with Legal
    // Actions: its creatures in hand, in order; the mana card and the controller's hand are not.
    assert_eq!(paused_state.deciding_player(), p(1));
    assert_eq!(paused_state.active_player(), p(0));
    assert_eq!(
        paused_state
            .pending_decision()
            .map(|pending| pending.kind()),
        Some(DecisionKind::Card)
    );
    assert_eq!(
        ruleset.legal_actions(&paused_state),
        card_answers(1, creatures.to_vec())
    );

    // The Active Player cannot answer.
    assert_eq!(
        ruleset
            .transition(&paused_state, &answer_card(0, candidates[0]))
            .map(|_| ()),
        Err(Rejection::NotDecidingPlayer {
            player: p(0),
            deciding: p(1)
        })
    );

    // The answer moves exactly that card to its own trash.
    let chosen = candidates[1];
    let controllers_trash = in_zone(&paused_state, 0, "trash");
    let (after, trace) = accepted(&ruleset, &paused_state, &answer_card(1, chosen));
    assert_eq!(
        trace,
        [answered_card(1, chosen), moved(1, chosen, "hand", "trash")]
    );
    assert_eq!(in_zone(&after, 1, "trash").last(), Some(&chosen));
    assert!(!in_zone(&after, 1, "hand").contains(&chosen));
    assert!(in_zone(&after, 1, "hand").contains(&candidates[0]));
    assert_eq!(in_zone(&after, 0, "trash"), controllers_trash);
    assert!(after.pending_decision().is_none());
    assert_eq!(after.deciding_player(), p(0));
}

// ---- no candidate, one candidate

#[test]
fn an_empty_card_choice_skips_the_rest_of_the_do() {
    let ruleset = ruleset();
    let (mut state, grabber) = with_in_hand(&start(&ruleset), 0, "grabber");
    // A `hand_tapper` on top of the deck: the draw before the Choice wakes it, so the Events emitted
    // before the skip are scanned. (Through the trash, so that it goes to the top of the deck.)
    let tapper = nth(&state, 0, "hand_tapper", 0);
    park(&mut state, tapper, "trash");
    park(&mut state, tapper, "deck");
    assert!(in_zone(&state, 1, "field").is_empty());

    let (after, trace) = accepted(&ruleset, &state, &play(0, grabber));

    // The first Card Effect draws, finds no creature in an opponent's field, and its remaining
    // effects (the tap and the second draw) are skipped, with no Event for the Choice. The second
    // Card Effect, queued before, still fires, and so does what the draw woke.
    assert_eq!(
        trace,
        [
            moved(0, grabber, "hand", "field"),
            state_changed(grabber, "recently_played", true),
            drew(0, tapper),
            state_changed(grabber, "tapped", true),
            state_changed(tapper, "tapped", true),
        ]
    );
    assert!(after.pending_decision().is_none());
    assert_eq!(after.deciding_player(), p(0));
    assert!(ruleset.legal_actions(&after).contains(&pass(0)));
}

#[test]
fn an_empty_card_choice_in_an_if_skips_the_rest_of_the_whole_do() {
    let ruleset = ruleset();
    let (state, deep) = with_in_hand(&start(&ruleset), 0, "deep_grabber");

    // Untapped: the `if` is true, its Choice is empty, and neither the draw after the Choice in
    // `then` nor the draw after the `if` runs.
    let (after, trace) = accepted(&ruleset, &state, &play(0, deep));
    assert_eq!(
        trace,
        [
            moved(0, deep, "hand", "field"),
            state_changed(deep, "recently_played", true),
        ]
    );
    assert!(after.pending_decision().is_none());
    assert_eq!(
        in_zone(&after, 0, "hand").len(),
        in_zone(&state, 0, "hand").len() - 1
    );

    // Tapped: the `if` is false, nothing is skipped, and the draw after the `if` runs.
    let mut tapped = state.clone();
    set_tapped(&mut tapped, deep);
    let top = top_of_deck(&tapped, 0);
    let (_, trace) = accepted(&ruleset, &tapped, &play(0, deep));
    assert_eq!(
        trace[..3],
        [
            moved(0, deep, "hand", "field"),
            state_changed(deep, "recently_played", true),
            drew(0, top),
        ]
    );
}

#[test]
fn mind_leech_with_an_empty_hand_asks_nobody() {
    let ruleset = ruleset();
    let (state, leech) = with_in_hand(&start(&ruleset), 0, "mind_leech");
    let mut state = with_empty_hand(&state, 1);
    let mana = in_zone(&state, 0, "mana_zone");

    let (after, trace) = accepted(&ruleset, &state, &play(0, leech));

    // The play, and nothing for the Choice: the opponent has no creature in hand.
    assert_eq!(
        trace,
        [
            state_changed(mana[0], "tapped", true),
            state_changed(mana[1], "tapped", true),
            moved(0, leech, "hand", "field"),
            state_changed(leech, "recently_played", true),
        ]
    );
    assert!(after.pending_decision().is_none());
    assert_eq!(after.deciding_player(), p(0));
    assert!(ruleset.legal_actions(&after).contains(&pass(0)));

    // A hand with a mana card and no creature is as empty.
    let mana_card = nth(&state, 1, "mana", 0);
    park(&mut state, mana_card, "hand");
    let (after, _) = accepted(&ruleset, &state, &play(0, leech));
    assert!(after.pending_decision().is_none());
}

#[test]
fn a_single_candidate_is_still_asked() {
    let ruleset = ruleset();
    let (state, snare) = with_in_hand(&start(&ruleset), 0, "snare_vine");
    let (state, theirs) = with_opponent_creatures(&state, &[("moss_sprite", 0)]);

    let (paused_state, trace) = accepted(&ruleset, &state, &play(0, snare));

    assert!(matches!(
        trace.last(),
        Some(Event::DecisionPending {
            kind: DecisionKind::Card,
            ..
        })
    ));
    assert_eq!(
        ruleset.legal_actions(&paused_state),
        [answer_card(0, theirs[0])]
    );
}

// ---- rejections

#[test]
fn a_card_that_is_not_a_candidate_is_rejected() {
    let ruleset = ruleset();
    let (state, snare) = with_in_hand(&start(&ruleset), 0, "snare_vine");
    let (mut state, theirs) =
        with_opponent_creatures(&state, &[("moss_sprite", 0), ("goblin_scout", 0)]);
    let own = nth(&state, 0, "goblin_scout", 0);
    park(&mut state, own, "field");
    let (paused_state, _) = accepted(&ruleset, &state, &play(0, snare));
    let before = paused_state.clone();

    // Not candidates: the controller's own creature in the field, an opponent's creature that is
    // in its hand, the carrying card itself, a leader, and an id no card has.
    let in_their_hand = in_zone(&paused_state, 1, "hand")[0];
    let their_leader = nth(&paused_state, 1, "warden_ilse", 0);
    for card in [
        own,
        in_their_hand,
        snare,
        their_leader,
        CardInstanceId(99_999),
    ] {
        assert!(!theirs.contains(&card));
        assert_eq!(
            ruleset
                .transition(&paused_state, &answer_card(0, card))
                .map(|_| ()),
            Err(Rejection::CardNotCandidate { card }),
            "{card:?}"
        );
        // The input GameState is unchanged: a Rejection carries no Events.
        assert_eq!(paused_state, before);
    }
}

#[test]
fn an_option_answer_to_a_card_choice_is_rejected() {
    let ruleset = ruleset();
    let (state, snare) = with_in_hand(&start(&ruleset), 0, "snare_vine");
    let (state, _) = with_opponent_creatures(&state, &[("moss_sprite", 0)]);
    let (paused_state, _) = accepted(&ruleset, &state, &play(0, snare));
    let before = paused_state.clone();

    assert_eq!(
        ruleset
            .transition(&paused_state, &answer_option(0, 0))
            .map(|_| ()),
        Err(Rejection::WrongAnswerKind {
            kind: DecisionKind::Card
        })
    );
    assert_eq!(paused_state, before);
}

#[test]
fn a_card_answer_to_an_option_choice_is_rejected() {
    let ruleset = ruleset();
    let (state, owl) = with_in_hand(&start(&ruleset), 0, "curious_owl");
    let (paused_state, _) = accepted(&ruleset, &state, &play(0, owl));
    let before = paused_state.clone();
    assert_eq!(
        paused_state
            .pending_decision()
            .map(|pending| pending.kind()),
        Some(DecisionKind::Option)
    );

    // The kind is checked before the card: even a card that is nothing at all is the wrong kind.
    for card in [owl, CardInstanceId(99_999)] {
        assert_eq!(
            ruleset
                .transition(&paused_state, &answer_card(0, card))
                .map(|_| ()),
            Err(Rejection::WrongAnswerKind {
                kind: DecisionKind::Option
            })
        );
    }
    assert_eq!(paused_state, before);
}

#[test]
fn a_card_answer_with_no_decision_pending_is_rejected() {
    let ruleset = ruleset();
    let (state, theirs) = with_opponent_creatures(&start(&ruleset), &[("moss_sprite", 0)]);

    assert_eq!(
        ruleset
            .transition(&state, &answer_card(0, theirs[0]))
            .map(|_| ()),
        Err(Rejection::NoDecisionPending)
    );
}

// ---- bound names

#[test]
fn a_bound_name_reads_the_chosen_card_after_a_nested_option_choice() {
    let ruleset = ruleset();
    let (state, picker) = with_in_hand(&start(&ruleset), 0, "picker");
    let (state, theirs) =
        with_opponent_creatures(&state, &[("moss_sprite", 0), ("goblin_scout", 0)]);
    let (other, chosen) = (theirs[0], theirs[1]);

    let (first, _) = accepted(&ruleset, &state, &play(0, picker));
    assert_eq!(
        pending_paused(&first),
        paused((picker, 0), path(vec![], 0), vec![], vec![], 1, vec![])
    );

    // The Card Answer binds `x` and the Resolution pauses again at the Option Choice, which keeps
    // the name: the second pause holds the card chosen.
    let (second, trace) = accepted(&ruleset, &first, &answer_card(0, chosen));
    assert_eq!(
        trace,
        [
            answered_card(0, chosen),
            pending_event(
                0,
                DecisionKind::Option,
                paused(
                    (picker, 0),
                    path(vec![], 1),
                    vec![],
                    vec![],
                    1,
                    vec![bound("x", chosen)]
                )
            ),
        ]
    );
    assert_eq!(
        second.pending_decision().map(|pending| pending.kind()),
        Some(DecisionKind::Option)
    );

    // Either option goes on to the effect after the Choice, which reads `x`: it taps the chosen
    // card and no other.
    let (done, trace) = accepted(&ruleset, &second, &answer_option(0, 0));
    assert_eq!(
        trace,
        [
            answered_option(0, 0),
            state_changed(picker, "tapped", true),
            state_changed(chosen, "tapped", true),
        ]
    );
    assert!(!is_tapped(&done, other));
    let (done, trace) = accepted(&ruleset, &second, &answer_option(0, 1));
    assert_eq!(
        trace,
        [answered_option(0, 1), state_changed(chosen, "tapped", true)]
    );
    assert!(!is_tapped(&done, other) && !is_tapped(&done, picker));
}

#[test]
fn a_bound_name_reads_the_chosen_card_inside_an_if() {
    let ruleset = ruleset();
    let (state, picker) = with_in_hand(&start(&ruleset), 0, "if_picker");
    let (state, theirs) =
        with_opponent_creatures(&state, &[("moss_sprite", 0), ("goblin_scout", 0)]);
    let (chosen, other) = (theirs[0], theirs[1]);

    let (first, _) = accepted(&ruleset, &state, &play(0, picker));
    let (second, trace) = accepted(&ruleset, &first, &answer_card(0, chosen));

    // The Option Choice is the first effect of `then`, which is the second effect of the `do`; the
    // pause keeps `x` from before the `if`.
    let in_then = path(
        vec![Branch {
            effect: 1,
            into: BranchKind::Then,
        }],
        0,
    );
    assert_eq!(
        trace,
        [
            answered_card(0, chosen),
            pending_event(
                0,
                DecisionKind::Option,
                paused(
                    (picker, 0),
                    in_then,
                    vec![],
                    vec![],
                    1,
                    vec![bound("x", chosen)]
                )
            ),
        ]
    );

    // The name is read inside the chosen option, inside the `if`.
    let (done, trace) = accepted(&ruleset, &second, &answer_option(0, 0));
    assert_eq!(
        trace,
        [answered_option(0, 0), state_changed(chosen, "tapped", true)]
    );
    assert!(!is_tapped(&done, other));
    let (_, trace) = accepted(&ruleset, &second, &answer_option(0, 1));
    assert_eq!(trace, [answered_option(0, 1)]);
}

#[test]
fn a_name_out_of_scope_does_not_leak_into_a_later_choice_that_reuses_it() {
    let ruleset = ruleset();
    let (state, picker) = with_in_hand(&start(&ruleset), 0, "leak_picker");
    let (state, theirs) =
        with_opponent_creatures(&state, &[("moss_sprite", 0), ("goblin_scout", 0)]);
    let (a, b) = (theirs[0], theirs[1]);

    // The first pick is inside `then`: the Choice is its first effect.
    let (first, _) = accepted(&ruleset, &state, &play(0, picker));
    let in_then = path(
        vec![Branch {
            effect: 0,
            into: BranchKind::Then,
        }],
        0,
    );
    assert_eq!(
        pending_paused(&first),
        paused((picker, 0), in_then, vec![], vec![], 1, vec![])
    );

    // Choosing `a` taps it, and the `do` goes on to its own second pick, outside the `if`: `t` is
    // out of scope there, so nothing is bound at that pause.
    let (second, trace) = accepted(&ruleset, &first, &answer_card(0, a));
    assert_eq!(
        trace,
        [
            answered_card(0, a),
            state_changed(a, "tapped", true),
            pending_event(
                0,
                DecisionKind::Card,
                paused(
                    (picker, 0),
                    path(vec![], 1),
                    vec![],
                    vec![state_changed(a, "tapped", true)],
                    1,
                    vec![]
                )
            ),
        ]
    );

    // Answering `b` taps `b`: the second `t` is `b`, not the `a` of the first.
    let (done, trace) = accepted(&ruleset, &second, &answer_card(0, b));
    assert_eq!(
        trace,
        [answered_card(0, b), state_changed(b, "tapped", true)]
    );
    assert!(is_tapped(&done, a) && is_tapped(&done, b));
}

#[test]
fn the_where_of_a_choice_reads_an_earlier_name() {
    let ruleset = ruleset();
    let (state, picker) = with_in_hand(&start(&ruleset), 0, "twin_picker");
    // Powers: goblin_scout 1, moss_sprite 2.
    let (state, theirs) = with_opponent_creatures(
        &state,
        &[("goblin_scout", 0), ("moss_sprite", 0), ("moss_sprite", 1)],
    );
    let (goblin, moss_a, moss_b) = (theirs[0], theirs[1], theirs[2]);

    let (first, _) = accepted(&ruleset, &state, &play(0, picker));
    assert_eq!(ruleset.legal_actions(&first), card_answers(0, theirs));

    // Picking a power 2 creature leaves the two of power 2 as candidates of the second pick, which
    // keeps the first.
    let (second, _) = accepted(&ruleset, &first, &answer_card(0, moss_a));
    assert_eq!(pending_paused(&second).bound, vec![bound("a", moss_a)]);
    assert_eq!(
        ruleset.legal_actions(&second),
        card_answers(0, vec![moss_a, moss_b])
    );
    // The goblin, weaker, is not one, and its answer is a typed Rejection.
    assert_eq!(
        ruleset
            .transition(&second, &answer_card(0, goblin))
            .map(|_| ()),
        Err(Rejection::CardNotCandidate { card: goblin })
    );
    let (done, trace) = accepted(&ruleset, &second, &answer_card(0, moss_b));
    assert_eq!(
        trace,
        [
            answered_card(0, moss_b),
            state_changed(moss_b, "tapped", true)
        ]
    );
    assert!(!is_tapped(&done, goblin) && !is_tapped(&done, moss_a));

    // Picking the goblin leaves all three.
    let (second, _) = accepted(&ruleset, &first, &answer_card(0, goblin));
    assert_eq!(
        ruleset.legal_actions(&second),
        card_answers(0, vec![goblin, moss_a, moss_b])
    );
}

#[test]
fn a_choice_with_alternatives_lists_each_one_and_narrows_its_name() {
    let ruleset = ruleset();
    let (state, picker) = with_in_hand(&start(&ruleset), 0, "wide_picker");
    let (state, theirs) = with_opponent_creatures(&state, &[("goblin_scout", 0)]);
    let creature = theirs[0];
    let leader = nth(&state, 1, "warden_ilse", 0);

    // The candidates of both alternatives, in one ascending order.
    let (paused_state, _) = accepted(&ruleset, &state, &play(0, picker));
    assert_eq!(
        ruleset.legal_actions(&paused_state),
        card_answers(0, vec![creature, leader])
    );

    // A creature takes the `then`, a leader the `else`: Warden Ilse has 6 life.
    let (_, trace) = accepted(&ruleset, &paused_state, &answer_card(0, creature));
    assert_eq!(
        trace,
        [
            answered_card(0, creature),
            state_changed(creature, "tapped", true)
        ]
    );
    let (_, trace) = accepted(&ruleset, &paused_state, &answer_card(0, leader));
    assert_eq!(
        trace,
        [
            answered_card(0, leader),
            Event::CardStateChanged {
                card: leader,
                state: StateName::new("current_life"),
                value: FieldValue::Int(5),
            }
        ]
    );
}

// ---- Event Application

#[test]
fn applying_card_decision_events_stores_and_clears_the_pending_decision() {
    let ruleset = ruleset();
    let (state, snare) = with_in_hand(&start(&ruleset), 0, "snare_vine");
    let (state, theirs) = with_opponent_creatures(&state, &[("moss_sprite", 0)]);
    let (paused_state, trace) = accepted(&ruleset, &state, &play(0, snare));

    // Event-sourced: the same Events applied to the GameState before the Action give the same
    // GameState, with the Pending Decision in it.
    assert_eq!(state.apply_trace(&trace), Ok(paused_state.clone()));
    let mut after = paused_state.clone();
    after
        .apply(&answered_card(0, theirs[0]))
        .expect("the Deciding Player answers");
    assert!(after.pending_decision().is_none());
    assert_eq!(after.deciding_player(), p(0));
}

#[test]
fn a_pending_kind_that_is_not_the_kind_of_its_choice_is_a_data_mismatch() {
    let ruleset = ruleset();
    let (state, snare) = with_in_hand(&start(&ruleset), 0, "snare_vine");
    let (state, theirs) = with_opponent_creatures(&state, &[("moss_sprite", 0)]);
    let (paused_state, _) = accepted(&ruleset, &state, &play(0, snare));
    let at_a_card_choice = pending_paused(&paused_state);

    // A crafted `DecisionPending` that says `Option` for a Card Choice: the Answers of either kind
    // are an inconsistency, not a `WrongAnswerKind`.
    let mut crafted = state.clone();
    crafted
        .apply(&pending_event(0, DecisionKind::Option, at_a_card_choice))
        .expect("the crafted decision is applied");
    for action in [answer_option(0, 0), answer_card(0, theirs[0])] {
        assert_eq!(
            ruleset.transition(&crafted, &action).map(|_| ()),
            Err(Rejection::CardDataMismatch { card: snare }),
            "{action:?}"
        );
    }
}

// ---- replay and determinism

/// The Actions of a Game from `seed` in which Player 1 plays a creature with no Card Effect, and
/// Player 0 then plays a `snare_vine`, answers with the first candidate listed and passes; `None`
/// when the Seed deals no such Game. Chosen from the Legal Actions, so the script is a Game of the
/// rules.
fn script_from_seed(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    seed: u64,
) -> Option<Vec<Action>> {
    let mut state = ruleset.create_game(decks, Seed(seed)).ok()?.opening.state;
    let mut actions = Vec::new();
    let plays_one_of = |state: &GameState, action: &Action, ids: &[&str]| match action {
        Action::Declared {
            action: id,
            arguments,
            ..
        } if id.as_str() == "play_creature" => {
            let Some(Argument::Card(card)) = arguments.first() else {
                return false;
            };
            state
                .card(*card)
                .is_some_and(|instance| ids.contains(&instance.definition().as_str()))
        }
        _ => false,
    };
    let step = |state: &mut GameState, action: Action, actions: &mut Vec<Action>| {
        let transition = ruleset.transition(state, &action).ok()?;
        *state = transition.state;
        actions.push(action);
        Some(())
    };
    step(&mut state, pass(0), &mut actions)?;
    let plain = ruleset
        .legal_actions(&state)
        .into_iter()
        .find(|action| plays_one_of(&state, action, &["goblin_scout", "moss_sprite"]))?;
    step(&mut state, plain, &mut actions)?;
    step(&mut state, pass(1), &mut actions)?;
    let snare = ruleset
        .legal_actions(&state)
        .into_iter()
        .find(|action| plays_one_of(&state, action, &["snare_vine"]))?;
    step(&mut state, snare, &mut actions)?;
    let first = ruleset.legal_actions(&state).into_iter().next()?;
    step(&mut state, first, &mut actions)?;
    step(&mut state, pass(0), &mut actions)?;
    Some(actions)
}

#[test]
fn a_record_with_card_answers_replays_exactly() {
    let ruleset = ruleset();
    let decks = decks(&ruleset);
    // A record holds no arrangement: find a Seed whose Game reaches the pick on its own.
    let (seed, actions) = (0..2000)
        .find_map(|seed| script_from_seed(&ruleset, &decks, seed).map(|actions| (seed, actions)))
        .expect("some Seed deals a Game with a snare_vine pick");
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Answer {
            answer: Answer::Card(_),
            ..
        }
    )));

    let mut record = GameRecord::new(&ruleset, &decks, Seed(seed));
    let mut state = ruleset
        .create_game(&decks, Seed(seed))
        .expect("the decks create a game")
        .opening
        .state;
    let mut traces = Vec::new();
    for action in actions {
        let transition = ruleset
            .transition(&state, &action)
            .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"));
        record.push_accepted(action);
        state = transition.state;
        traces.push(transition.trace);
    }
    assert!(traces
        .iter()
        .any(|trace| trace.events().iter().any(|event| {
            matches!(
                event,
                Event::DecisionPending {
                    kind: DecisionKind::Card,
                    ..
                }
            )
        })));

    // Replaying the record gives the whole GameState and the whole Event Trace of every step.
    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(replay.traces, traces);
    assert_eq!(replay.state, state);
}

#[test]
fn two_runs_with_card_choices_are_equal() {
    let ruleset = ruleset();
    let (state, snare) = with_in_hand(&start(&ruleset), 0, "snare_vine");
    let (arranged, theirs) =
        with_opponent_creatures(&state, &[("moss_sprite", 0), ("goblin_scout", 0)]);
    let chosen = ascending(theirs)[0];
    let actions = [play(0, snare), answer_card(0, chosen), pass(0)];
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

/// Whether the catalog text of `sage` with this `do` loads.
fn loads(effects: &str) -> bool {
    load_card_catalog(&effects_definition(), &sage_yaml(effects)).is_ok()
}

fn invalid_card_effect(at: DefinitionPath, kind: DefinitionErrorKind) -> CatalogError {
    CatalogError {
        path: at,
        kind: CatalogErrorKind::InvalidCardEffect {
            error: Box::new(kind),
        },
    }
}

/// `cards[sage].effects[0].do[index]`, where the engine reports an error of the effect there.
fn sage_effect(index: usize) -> DefinitionPath {
    path_of()
        .key("cards")
        .id("sage")
        .key("effects")
        .index(0)
        .key("do")
        .index(index)
}

/// A catalog text for the card `sage` whose one Card Effect has this `do`.
fn sage_do(effects: &[&str]) -> String {
    format!(
        "      - {{ on: {{ enter: field }}, do: [ {} ] }}\n",
        effects.join(", ")
    )
}

/// `cards[sage].effects[0].do[0].choose_card`, where the engine reports an error of a Card Choice.
fn sage_choice() -> DefinitionPath {
    sage_effect(0).key("choose_card")
}

fn cannot_see(zone: &str) -> DefinitionErrorKind {
    DefinitionErrorKind::DeciderCannotSeeZone {
        zone: zone.to_string(),
    }
}

fn dangling(id: &str) -> DefinitionErrorKind {
    DefinitionErrorKind::DanglingReference {
        target: ReferenceTarget::Param,
        id: id.to_string(),
    }
}

fn duplicate(name: &str) -> DefinitionErrorKind {
    DefinitionErrorKind::DuplicateChoiceName {
        name: name.to_string(),
    }
}

#[test]
fn a_decider_that_cannot_see_the_zone_does_not_load() {
    // `opponent` choosing from the controller's hand: the hand is `owner`, and it is the
    // controller's.
    let effects = sage_do(&[&choose_card("t", "opponent", "hand", None, None)]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(sage_choice().key("zone"), cannot_see("hand"))
    );
    // `controller` choosing from an opponent's hand.
    let effects = sage_do(&[&choose_card(
        "t",
        "controller",
        "hand",
        Some("opponent"),
        None,
    )]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(sage_choice().key("zone"), cannot_see("hand"))
    );
    // Anyone choosing from a `hidden` zone, whoever owns it.
    for (decider, owner) in [
        ("controller", None),
        ("controller", Some("opponent")),
        ("opponent", None),
        ("opponent", Some("opponent")),
    ] {
        let effects = sage_do(&[&choose_card("t", decider, "deck", owner, None)]);
        assert_eq!(
            catalog_error(&effects),
            invalid_card_effect(sage_choice().key("zone"), cannot_see("deck")),
            "{decider} {owner:?}"
        );
    }
    // An alternative of a `one_of` that the decider cannot see is reported at its own path, the
    // first one in order.
    let effects = sage_do(&[
        "{ choose_card: { name: t, decider: controller, one_of: [ { zone: field, card_type: creature, owner: opponent }, { zone: deck, card_type: creature }, { zone: hand, card_type: creature, owner: opponent } ] } }",
    ]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(
            sage_choice().key("one_of").index(1).key("zone"),
            cannot_see("deck")
        )
    );
}

#[test]
fn a_decider_that_sees_the_zone_loads() {
    // A public zone for either decider, and the decider's own hand.
    for (decider, zone, owner) in [
        ("controller", "field", None),
        ("controller", "field", Some("opponent")),
        ("opponent", "trash", Some("opponent")),
        ("controller", "hand", None),
        ("opponent", "hand", Some("opponent")),
    ] {
        let effects = sage_do(&[&choose_card("t", decider, zone, owner, None)]);
        assert!(loads(&effects), "{decider} {zone} {owner:?}");
    }
}

#[test]
fn the_checks_of_a_card_choice_run_in_a_fixed_order() {
    let error_of = |effect: &str| catalog_error(&sage_do(&[effect]));
    // A name already in scope (`self` included) comes before the decider.
    assert_eq!(
        error_of(&choose_card("self", "referee", "field", None, None)),
        invalid_card_effect(sage_choice().key("name"), duplicate("self"))
    );
    // A name that is not an id comes before the duplicate check.
    assert_eq!(
        error_of(&choose_card("Not_An_Id", "controller", "field", None, None)),
        invalid_card_effect(
            sage_choice().key("name"),
            DefinitionErrorKind::InvalidId {
                id: "Not_An_Id".to_string()
            }
        )
    );
    // The decider comes before the zone.
    assert_eq!(
        error_of(&choose_card("t", "referee", "nowhere", None, None)),
        invalid_card_effect(
            sage_choice().key("decider"),
            DefinitionErrorKind::UnknownDecider {
                decider: "referee".to_string()
            }
        )
    );
    // The params of every alternative are checked before any alternative is seen: the dangling
    // zone of the second beats the hidden zone of the first.
    assert_eq!(
        error_of("{ choose_card: { name: t, decider: controller, one_of: [ { zone: deck, card_type: creature }, { zone: nowhere, card_type: creature } ] } }"),
        invalid_card_effect(
            sage_choice().key("one_of").index(1).key("zone"),
            DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "nowhere".to_string()
            }
        )
    );
}

#[test]
fn a_ref_to_an_unbound_name_does_not_load() {
    // A name nothing binds.
    let effects = sage_do(&[&tap("ghost")]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(
            sage_effect(0).key("set_state").key("card"),
            dangling("ghost")
        )
    );
    // A name read before the Choice that binds it.
    let effects = sage_do(&[&tap("t"), &pick_opponent_creature("t")]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(sage_effect(0).key("set_state").key("card"), dangling("t"))
    );
    // The `where` of a Choice reads the names before it and its own candidate, and no other.
    let effects = sage_do(&[&choose_card(
        "t",
        "controller",
        "field",
        Some("opponent"),
        Some("{ eq: [ { ref: later.power }, 1 ] }"),
    )]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(
            sage_choice().key("where").key("eq").index(0),
            dangling("later")
        )
    );
}

#[test]
fn a_ref_out_of_scope_does_not_load() {
    let pick = pick_opponent_creature("t");
    // After the `if` that holds the Choice.
    let effects = sage_do(&[&format!("{{ if: true, then: [ {pick} ] }}"), &tap("t")]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(sage_effect(1).key("set_state").key("card"), dangling("t"))
    );
    // In the `else` of the `if` whose `then` holds it.
    let effects = sage_do(&[&format!(
        "{{ if: true, then: [ {pick} ], else: [ {} ] }}",
        tap("t")
    )]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(
            sage_effect(0)
                .key("else")
                .index(0)
                .key("set_state")
                .key("card"),
            dangling("t")
        )
    );
    // After an Option Choice, one of whose options holds it.
    let effects = sage_do(&[&choose(&[&[pick.as_str()], &[]]), &tap("t")]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(sage_effect(1).key("set_state").key("card"), dangling("t"))
    );
}

#[test]
fn a_duplicated_choice_name_does_not_load() {
    let pick = pick_opponent_creature("t");
    // The same name twice in one list.
    let effects = sage_do(&[&pick, &pick]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(
            sage_effect(1).key("choose_card").key("name"),
            duplicate("t")
        )
    );
    // `self` is a name in scope.
    let effects = sage_do(&[&pick_opponent_creature("self")]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(sage_choice().key("name"), duplicate("self"))
    );
    // A name of the enclosing list reused inside an `if` that follows its Choice.
    let effects = sage_do(&[&pick, &format!("{{ if: true, then: [ {pick} ] }}")]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(
            sage_effect(1)
                .key("then")
                .index(0)
                .key("choose_card")
                .key("name"),
            duplicate("t")
        )
    );
    // The same inside an option of a Choice that follows it.
    let effects = sage_do(&[&pick, &choose(&[&[pick.as_str()], &[]])]);
    assert_eq!(
        catalog_error(&effects),
        invalid_card_effect(
            sage_effect(1)
                .key("choose_one")
                .key("options")
                .index(0)
                .index(0)
                .key("choose_card")
                .key("name"),
            duplicate("t")
        )
    );
}

#[test]
fn siblings_and_later_lists_may_reuse_a_name() {
    let pick = pick_opponent_creature("t");
    // `then` and `else` are siblings: neither sees the other's names. After the `if` the name is
    // out of scope again.
    let effects = sage_do(&[
        &format!("{{ if: true, then: [ {pick} ], else: [ {pick} ] }}"),
        &pick,
        &tap("t"),
    ]);
    assert!(loads(&effects));
    // So are the options of a Choice.
    let effects = sage_do(&[&choose(&[&[pick.as_str()], &[pick.as_str()]])]);
    assert!(loads(&effects));
}

#[test]
fn a_card_choice_in_an_action_does_not_load() {
    let pick = pick_opponent_creature("t");
    // Directly among the effects of an action.
    let yaml = edit(
        EFFECTS_RULESET,
        "    effects: [ { end_turn: {} } ]",
        &format!("    effects: [ {pick}, {{ end_turn: {{}} }} ]"),
    );
    assert_eq!(
        definition_error(&yaml),
        DefinitionError {
            path: path_of()
                .key("actions")
                .id("pass_turn")
                .key("effects")
                .index(0)
                .key("choose_card"),
            kind: DefinitionErrorKind::ChoiceOutsideTriggeredEffect,
        }
    );
    // Nested in an `if` of an action.
    let yaml = edit(
        EFFECTS_RULESET,
        "    effects: [ { end_turn: {} } ]",
        &format!("    effects: [ {{ if: true, then: [ {pick} ] }}, {{ end_turn: {{}} }} ]"),
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
                .key("choose_card"),
            kind: DefinitionErrorKind::ChoiceOutsideTriggeredEffect,
        }
    );
}

#[test]
fn a_card_choice_in_a_step_is_a_load_error() {
    // A step has no effect grammar: `choose_card` is an unknown key of the step, at its position.
    let yaml = edit(
        EFFECTS_RULESET,
        "        - untap: { player: active, zones: [mana_zone, field] }\n",
        "        - choose_card: { name: t, decider: controller, zone: field, card_type: creature }\n        - untap: { player: active, zones: [mana_zone, field] }\n",
    );
    let error = yaml_error(&yaml);
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "choose_card".to_string()
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
}

#[test]
fn a_card_choice_follows_the_param_grammar_of_the_loader() {
    // An unknown key is refused, as elsewhere.
    let effects = sage_do(&[
        "{ choose_card: { name: t, decider: controller, zone: field, card_type: creature, bogus: 1 } }",
    ]);
    assert!(matches!(
        load_card_catalog(&effects_definition(), &sage_yaml(&effects)),
        Err(CatalogLoadError::Yaml(YamlError {
            kind: YamlErrorKind::UnknownKey { ref key },
            ..
        })) if key == "bogus"
    ));
    // The two forms cannot be mixed, one of them is needed, and so is the `decider`.
    for body in [
        "{ name: t, decider: controller, zone: field, card_type: creature, one_of: [ { zone: field, card_type: creature } ] }",
        "{ name: t, decider: controller }",
        "{ name: t, zone: field, card_type: creature }",
    ] {
        let effects = sage_do(&[&format!("{{ choose_card: {body} }}")]);
        assert!(
            matches!(
                load_card_catalog(&effects_definition(), &sage_yaml(&effects)),
                Err(CatalogLoadError::Yaml(YamlError {
                    kind: YamlErrorKind::Malformed { .. },
                    ..
                }))
            ),
            "{body}"
        );
    }
    // The `one_of` form loads, with the `where` of an alternative reading the name.
    let effects = sage_do(&[
        "{ choose_card: { name: t, decider: controller, one_of: [ { zone: field, card_type: creature, owner: opponent, where: { eq: [ { ref: t.tapped }, true ] } }, { zone: leader_zone, card_type: leader, owner: opponent } ] } }",
    ]);
    assert!(loads(&effects));
}

#[test]
fn an_opponent_decider_of_a_card_choice_needs_exactly_two_players() {
    // The real `mind_leech` asks the opponent: in a ruleset of 3 Players "the controller's
    // opponent" is not defined, and the error is at the `decider` of its `choose_card`.
    let definition =
        load_ruleset_definition(&edit(EFFECTS_RULESET, "  players: 2", "  players: 3"))
            .expect("the 3-Player ruleset loads");
    let error = match load_card_catalog(&definition, EFFECTS_CATALOG) {
        Err(CatalogLoadError::Catalog(error)) => error,
        other => panic!("expected a catalog error, got {other:?}"),
    };
    assert_eq!(
        error,
        invalid_card_effect(
            path_of()
                .key("cards")
                .id("mind_leech")
                .key("effects")
                .index(0)
                .key("do")
                .index(0)
                .key("choose_card")
                .key("decider"),
            DefinitionErrorKind::OpponentDeciderNeedsTwoPlayers { players: 3 }
        )
    );
}
