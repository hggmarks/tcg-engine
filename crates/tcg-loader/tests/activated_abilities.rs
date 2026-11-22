//! Acceptance tests for ticket #86: activated abilities (ADR-0038), on the real Response Ruleset,
//! its real catalog (`skittish_hare`) and its real sample decks, with test-only cards appended to
//! the catalog and test-only actions in the window where the real ones cannot produce a behaviour:
//! a card whose ability costs nothing, a card with several effects of several kinds (one of them
//! taps the card itself), a cost that cannot be paid, a cost that moves its card, a response that
//! removes the ability's card.
//!
//! An activated ability is a Card Effect `activated: { cost, do }`. Its controller uses it as a
//! response: only while its card is in the Active Zone, only in a Response Window whose
//! declaration has `abilities: true`. Using it (`Action::Activate`) pays the cost at once and
//! pushes a Stack Item whose source is the card and the Ability Index (the index of the effect
//! among all the card's effects, whatever their kinds). The item resolves, or Fizzles, like any
//! other.
//!
//! The Event Traces are asserted whole, as literals read off the rules and off the arrangement of
//! each scenario, never computed by the code under test. A scenario is arranged with
//! `GameState::apply` of Events before the Action under test (an arrangement is not a transition,
//! so it wakes no trigger): the hands are cleared, seat 0 has an attacker on the field and seat 1
//! a tapped target, and each test puts the cards it needs in place.
//!
//! The real `skittish_hare` pays 1 mana (`tap: { zone: mana_zone, count: { ref: self.cost } }`)
//! rather than tapping itself: the target of an attack is tapped, so a hare that taps itself as its
//! cost could never dodge the attack that targets it. `CardAlreadyTapped` is covered by the
//! test-only `juggler`, whose second ability taps the card.

use tcg_engine::card_catalog::{CardEffect, CardId, CatalogError, CatalogErrorKind};
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Answer, Argument, BoundCard, CardInstanceId, DeclarativeRuleset, Event, GameCreation,
    GameRecord, GameState, ObservationError, ObservedAction, ObservedBoundCard, ObservedCard,
    ObservedEvent, ObservedGameState, ObservedStackItem, ObservedStackSource, Observer, Rejection,
    StackItem, StackSource, TakenAction, Viewer,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, CardValue, Cost, DefinitionError, DefinitionErrorKind, Effect, FieldName, FieldValue,
    ParamName, RulesetDefinition, StateName, WindowId, ZoneId,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, CatalogLoadError, LoadError,
    YamlErrorKind,
};

const RESPONSE_RULESET: &str =
    include_str!("../../../rulesets/response-ruleset/response-ruleset.yaml");
const RESPONSE_CATALOG: &str =
    include_str!("../../../rulesets/response-ruleset/response-catalog.yaml");
const RESPONSE_DECK_A: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-a.yaml");
const RESPONSE_DECK_B: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-b.yaml");

/// The Seed of every scenario. Scenarios do not depend on what it deals: each arranges the cards
/// it needs.
const SEED: u64 = 4;

/// The Seeds a scripted Game is played for, to find ones whose Games use an ability.
const SCRIPTED_SEEDS: u64 = 100;

/// A scripted Game stops after this many accepted Actions at the latest.
const SCRIPT_CAP: usize = 400;

// ---- test-only rulesets

/// Test-only creatures, none of them fast.
///
/// - `dodger`: its ability costs nothing and moves the card to the hand.
/// - `juggler`: three effects, a triggered one (index 0) and two activated ones (indexes 1 and 2:
///   a free one and one that taps the card), each of them draws.
/// - `phoenix`: its ability costs moving the card to the trash, where a trigger draws, and its
///   effect moves the card back to the field.
/// - `pricey`: of cost 3, its ability taps `cost` mana, more than a Player has in the scenarios.
/// - `asker`: fast, and asks its controller whether to draw when it enters the `holding` zone.
const TEST_CARDS: &str = "
  - { id: dodger,  type: creature, fields: { name: \"Dodger\",  cost: 0, power: 1, fast: false }, effects: [ { activated: { cost: [], do: [ { move: { card: { ref: self }, to: hand } } ] } } ] }
  - { id: juggler, type: creature, fields: { name: \"Juggler\", cost: 0, power: 1, fast: false }, effects: [ { on: { enter: trash }, do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] }, { activated: { do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } }, { activated: { cost: [ { tap: { card: { ref: self } } } ], do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } } ] }
  - { id: phoenix, type: creature, fields: { name: \"Phoenix\", cost: 0, power: 1, fast: false }, effects: [ { on: { enter: trash }, do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] }, { activated: { cost: [ { move: { card: { ref: self }, to: trash } } ], do: [ { move: { card: { ref: self }, to: field } } ] } } ] }
  - { id: pricey,  type: creature, fields: { name: \"Pricey\",  cost: 3, power: 1, fast: false }, effects: [ { activated: { cost: [ { tap: { zone: mana_zone, count: { ref: self.cost } } } ], do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } } ] }
  - { id: asker,   type: creature, fields: { name: \"Asker\",   cost: 0, power: 1, fast: true  }, effects: [ { on: { enter: holding }, do: [ { choose_one: { decider: controller, options: [ [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ], [] ] } } ] } ] }
";

/// A response that moves an opponent's creature to the trash.
const SNIPE: &str = "  - id: snipe
    params:
      - { name: victim, zone: field, card_type: creature, owner: opponent }
    effects:
      - move: { card: { ref: victim }, to: trash }
  - id: pass_turn
";

/// A response that moves an opponent's creature to its owner's hand.
const BOUNCE: &str = "  - id: bounce
    params:
      - { name: victim, zone: field, card_type: creature, owner: opponent }
    effects:
      - move: { card: { ref: victim }, to: hand }
  - id: pass_turn
";

/// An Action-phase action that opens a window whose declaration has no `abilities`: its cost taps
/// a creature it does not check.
const POKE: &str = "  - id: poke
    params:
      - { name: card, zone: field, card_type: creature }
    cost:
      - tap: { card: { ref: card } }
    effects: []
  - id: pass_turn
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

/// The real Response Ruleset and the real catalog with the test-only cards.
fn response_with_test_cards() -> DeclarativeRuleset {
    build(RESPONSE_RULESET, &format!("{RESPONSE_CATALOG}{TEST_CARDS}"))
}

/// The test-card ruleset whose window also lists the response `action` (`snipe` or `bounce`).
fn response_with_window_action(id: &str, text: &str) -> DeclarativeRuleset {
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        &format!("    actions: [fast_play, {id}]\n"),
    );
    let yaml = edit(&yaml, "  - id: pass_turn\n", text);
    build(&yaml, &format!("{RESPONSE_CATALOG}{TEST_CARDS}"))
}

/// The test-card ruleset with a second window, `quiet`, opened by `poke` and without `abilities`.
fn response_with_quiet_window() -> DeclarativeRuleset {
    let yaml = edit(
        RESPONSE_RULESET,
        "windows:\n",
        "windows:\n  - { id: quiet, opened_by: [poke], actions: [fast_play] }\n",
    );
    let yaml = edit(&yaml, "  - id: pass_turn\n", POKE);
    let yaml = edit(
        &yaml,
        "actions: [play_creature, attack, pass_turn]",
        "actions: [play_creature, attack, pass_turn, poke]",
    );
    build(&yaml, RESPONSE_CATALOG)
}

/// A sample deck with three of its four `river_otter`s swapped for `dodger`, `juggler` and
/// `phoenix`, and two of its four `shadow_cat`s for `pricey` and `asker`.
fn with_test_cards(deck: &str) -> String {
    let deck = edit(
        deck,
        "      - { card: river_otter,    count: 4 }\n",
        "      - { card: river_otter,    count: 1 }\n      - { card: dodger,         count: 1 }\n      - { card: juggler,        count: 1 }\n      - { card: phoenix,        count: 1 }\n",
    );
    edit(
        &deck,
        "      - { card: shadow_cat,     count: 4 }\n",
        "      - { card: shadow_cat,     count: 2 }\n      - { card: pricey,         count: 1 }\n      - { card: asker,          count: 1 }\n",
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

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(p(seat))
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

fn activate(player: u32, card: CardInstanceId, ability: u32) -> Action {
    Action::Activate {
        player: p(player),
        card,
        ability,
    }
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

/// Asserts the exact Rejection, that the GameState is what it was before the call, and that the
/// Action is not among the Legal Actions.
fn assert_rejected(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
    expected: Rejection,
) {
    let before = state.clone();
    assert_eq!(ruleset.transition(state, action), Err(expected));
    assert_eq!(state, &before, "a rejected Action changes nothing");
    assert!(
        !ruleset.legal_actions(state).contains(action),
        "a rejected Action is not listed"
    );
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
    Event::CardStateChanged {
        card,
        state: StateName::new("tapped"),
        value: FieldValue::Bool(true),
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

fn bound(param: &str, card: CardInstanceId, zone_id: &str, left: bool) -> BoundCard {
    BoundCard {
        param: ParamName::new(param),
        card,
        zone: zone(zone_id),
        left,
    }
}

/// The Stack Item of `attack(attacker, target)`; `attacker_left` and `target_left` are set once
/// that card left.
fn attack_item(
    attacker: CardInstanceId,
    target: CardInstanceId,
    attacker_left: bool,
    target_left: bool,
) -> StackItem {
    StackItem {
        controller: p(0),
        source: StackSource::Action(ActionId::new("attack")),
        bound: vec![
            bound("attacker", attacker, "field", attacker_left),
            bound("target", target, "field", target_left),
        ],
    }
}

/// The Stack Item of the ability `ability` of `card`, used by `seat`: it binds the card as `self`,
/// in the zone it was in once the cost was paid.
fn ability_item(
    seat: u32,
    card: CardInstanceId,
    ability: u32,
    zone_id: &str,
    left: bool,
) -> StackItem {
    StackItem {
        controller: p(seat),
        source: StackSource::Ability { card, ability },
        bound: vec![bound("self", card, zone_id, left)],
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

/// A scenario: seat 0's `attacker` (a `river_otter`, power 3) is in the field, untapped and not
/// recently played; seat 1's `target` (by default a `moss_sprite`, power 2) is in the field,
/// tapped. Both hands are empty (their cards are back in the decks), seat 0 has its two untapped
/// mana from the start of the Turn and seat 1 has two untapped mana. Seat 0 is the Active Player.
/// The Events of the creation and of the arrangement are kept, for the Observers.
struct Scene {
    creation: GameCreation,
    state: GameState,
    events: Vec<Event>,
    attacker: CardInstanceId,
    target: CardInstanceId,
    /// The cards `place` has put somewhere: it never picks one twice.
    placed: Vec<CardInstanceId>,
}

impl Scene {
    fn apply(&mut self, event: Event) {
        self.state
            .apply(&event)
            .expect("the scenario Event is accepted");
        self.events.push(event);
    }

    /// Moves the first card of `seat` that is of Card Definition `definition` and was not placed
    /// before into `to`, as an arrangement; returns it.
    fn place(&mut self, definition: &str, seat: u32, to: &str) -> CardInstanceId {
        let found = self
            .state
            .cards()
            .iter()
            .find(|card| {
                card.owner() == p(seat)
                    && card.definition().as_str() == definition
                    && !self.placed.contains(&card.id())
            })
            .map(|card| card.id())
            .expect("the deck holds the card");
        self.placed.push(found);
        let from = self.state.zone_of(found).expect("in a zone").clone();
        self.apply(Event::CardMoved {
            card: found,
            player: p(seat),
            from,
            to: zone(to),
        });
        found
    }

    /// A `skittish_hare` of `seat`, in `to`.
    fn hare(&mut self, seat: u32, to: &str) -> CardInstanceId {
        self.place("skittish_hare", seat, to)
    }

    /// The Observer of `seat` over every Event so far.
    fn observer(&self, ruleset: &DeclarativeRuleset, seat: u32) -> Observer {
        ruleset
            .observer(viewer(seat), &self.creation.empty, &self.events)
            .expect("the Observer is built")
    }
}

fn arrange(ruleset: &DeclarativeRuleset, decks: &[DeckList], target_definition: &str) -> Scene {
    let creation = ruleset
        .create_game(decks, Seed(SEED))
        .expect("the decks create a game");
    let mut scene = Scene {
        state: creation.opening.state.clone(),
        events: creation.opening.trace.events().to_vec(),
        creation,
        attacker: CardInstanceId(0),
        target: CardInstanceId(0),
        placed: Vec::new(),
    };
    for seat in [0, 1] {
        for card in in_zone(&scene.state, seat, "hand") {
            scene.apply(moved(card, seat, "hand", "deck"));
        }
    }
    scene.place("mana", 1, "mana_zone");
    scene.place("mana", 1, "mana_zone");
    scene.attacker = scene.place("river_otter", 0, "field");
    scene.target = scene.place(target_definition, 1, "field");
    scene.apply(tapped(scene.target));
    assert_eq!(in_zone(&scene.state, 0, "mana_zone").len(), 2);
    assert_eq!(scene.state.active_player(), p(0));
    assert!(scene.state.stack().is_empty());
    scene
}

/// The default scenario, on the real decks.
fn arrange_real(ruleset: &DeclarativeRuleset) -> Scene {
    arrange(ruleset, &response_decks(ruleset), "moss_sprite")
}

/// The default scenario, on the decks with the test-only cards.
fn arrange_with_test_cards(ruleset: &DeclarativeRuleset) -> Scene {
    arrange(ruleset, &test_card_decks(ruleset), "moss_sprite")
}

// ---- listing and using an ability

#[test]
fn an_activated_ability_is_a_legal_action_of_the_player_with_priority_in_an_attack_window() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange_with_test_cards(&ruleset);
    let sparrow = scene.place("swift_sparrow", 1, "hand");
    let hare = scene.hare(1, "field");
    let juggler = scene.place("juggler", 1, "field");
    // Not listed: a `pricey` (its cost of 3 mana cannot be paid with two), a phoenix in the hand
    // (not in the Active Zone), and the hare of the Player who has no Priority. A hare that is
    // tapped is listed: its cost is mana, not its own tap.
    let tapped_hare = scene.hare(1, "field");
    scene.apply(tapped(tapped_hare));
    let _pricey = scene.place("pricey", 1, "field");
    let _phoenix_in_hand = scene.place("phoenix", 1, "hand");
    let _other_players_hare = scene.hare(0, "field");
    let (a, t) = (scene.attacker, scene.target);
    // The same position with seat 1's mana all tapped: no cost of mana can be paid.
    let mut dry = scene.state.clone();
    for mana in in_zone(&dry, 1, "mana_zone") {
        dry.apply(&tapped(mana))
            .expect("the scenario Event is accepted");
    }

    // Outside a window, in the Action phase, no ability is listed.
    assert!(ruleset
        .legal_actions(&scene.state)
        .iter()
        .all(|action| !matches!(action, Action::Activate { .. })));

    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    // `Pass`, then the window's Actions, then the abilities in ascending Card Instance Id, then
    // ability index. The juggler's effects are [triggered, activated, activated]: its abilities
    // are the indexes 1 and 2, the index counts every kind of effect. The pricey, the phoenix in the
    // hand and the other Player's hare are absent.
    let mut abilities = [(hare, 0), (tapped_hare, 0), (juggler, 1), (juggler, 2)];
    abilities.sort();
    let mut expected = vec![pass(1), fast_play(1, sparrow)];
    expected.extend(
        abilities
            .iter()
            .map(|&(card, ability)| activate(1, card, ability)),
    );
    assert_eq!(ruleset.legal_actions(&state), expected);

    // With no untapped mana the hares are not listed, nor is the `fast_play`; the juggler's
    // abilities, which cost nothing or tap the card, still are.
    let (dry, _) = accept_all(&ruleset, &dry, &[attack(a, t)]);
    let mut kept = [(juggler, 1), (juggler, 2)];
    kept.sort();
    let mut expected = vec![pass(1)];
    expected.extend(
        kept.iter()
            .map(|&(card, ability)| activate(1, card, ability)),
    );
    assert_eq!(ruleset.legal_actions(&dry), expected);

    // The Active Player has no Legal Action while the window is open.
    assert!(ruleset
        .legal_actions(&state)
        .iter()
        .all(|action| action.player() == p(1)));
}

#[test]
fn activating_pays_the_cost_and_pushes_an_ability_item() {
    let ruleset = response();
    let mut scene = arrange_real(&ruleset);
    let hare = scene.hare(1, "field");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);
    // The hare costs 1: the first of seat 1's untapped mana, in the order it entered the zone.
    let mana = in_zone(&state, 1, "mana_zone")[0];

    let transition = accepted(&ruleset, &state, &activate(1, hare, 0));

    // The cost's tap of one mana, then `ItemPushed` last, naming the card and the ability, with
    // `self` bound in the field. The hare itself is not tapped.
    let item = ability_item(1, hare, 0, "field", false);
    assert_eq!(
        transition.trace.events(),
        [tapped(mana), pushed(item.clone())]
    );
    let next = &transition.state;
    assert_eq!(next.stack(), [attack_item(a, t, false, false), item]);
    assert_eq!(
        state_value(next, mana, "tapped"),
        Some(FieldValue::Bool(true))
    );
    assert_eq!(
        state_value(next, hare, "tapped"),
        Some(FieldValue::Bool(false))
    );
    // Priority flips to the attacker, who is the Deciding Player and may pass.
    assert_eq!(next.priority(), Some(p(0)));
    assert_eq!(next.deciding_player(), p(0));
    assert_eq!(ruleset.legal_actions(next).first(), Some(&pass(0)));
}

#[test]
fn activating_with_a_tap_self_cost_taps_the_card_and_pushes_an_ability_item() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange_with_test_cards(&ruleset);
    let juggler = scene.place("juggler", 1, "field");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    // The juggler's second ability (index 2) costs `tap: { card: { ref: self } }`.
    let transition = accepted(&ruleset, &state, &activate(1, juggler, 2));

    let item = ability_item(1, juggler, 2, "field", false);
    assert_eq!(
        transition.trace.events(),
        [tapped(juggler), pushed(item.clone())]
    );
    let next = &transition.state;
    assert_eq!(next.stack(), [attack_item(a, t, false, false), item]);
    assert_eq!(
        state_value(next, juggler, "tapped"),
        Some(FieldValue::Bool(true))
    );
    // Priority flips to the attacker, who is the Deciding Player and may pass.
    assert_eq!(next.priority(), Some(p(0)));
    assert_eq!(next.deciding_player(), p(0));
    assert_eq!(ruleset.legal_actions(next).first(), Some(&pass(0)));
}

#[test]
fn a_cost_that_wakes_a_trigger_resolves_it_after_the_push() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange_with_test_cards(&ruleset);
    let phoenix = scene.place("phoenix", 1, "field");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);
    let top = top_of_deck(&state, 1);

    let transition = accepted(&ruleset, &state, &activate(1, phoenix, 1));

    // The cost moves the phoenix to the trash, which is the zone `self` is bound in; then
    // `ItemPushed`; then the Trigger Resolution, which scanned the cost's Event: the phoenix's
    // `enter: trash` trigger draws.
    assert_eq!(
        transition.trace.events(),
        [
            moved(phoenix, 1, "field", "trash"),
            pushed(ability_item(1, phoenix, 1, "trash", false)),
            moved(top, 1, "deck", "hand")
        ]
    );
    assert!(transition.state.pending_decision().is_none());

    // Priority is the attacker's: passing resolves the ability, which moves the phoenix back.
    let resolving = accepted(&ruleset, &transition.state, &pass(0));
    assert_eq!(
        resolving.trace.events(),
        [
            resolved(ability_item(1, phoenix, 1, "trash", false)),
            moved(phoenix, 1, "trash", "field")
        ]
    );
}

// ---- resolving and Fizzling

#[test]
fn passing_resolves_the_ability_do() {
    // The target of the attack is a hare: tapped, as every target is. Its ability costs mana, so
    // it can still be used, and its `do` moves the targeted card away.
    let ruleset = response();
    let scene = arrange(&ruleset, &response_decks(&ruleset), "skittish_hare");
    let (a, hare) = (scene.attacker, scene.target);
    let (state, _) = accept_all(
        &ruleset,
        &scene.state,
        &[attack(a, hare), activate(1, hare, 0)],
    );
    assert_eq!(
        state.stack(),
        [
            attack_item(a, hare, false, false),
            ability_item(1, hare, 0, "field", false)
        ]
    );

    // Seat 0 passes: `ItemResolved` first, then the `do`, which moves the hare to the hand. The
    // attack item below records that its target left.
    let transition = accepted(&ruleset, &state, &pass(0));
    assert_eq!(
        transition.trace.events(),
        [
            resolved(ability_item(1, hare, 0, "field", false)),
            moved(hare, 1, "field", "hand")
        ]
    );
    let next = transition.state;
    assert_eq!(next.stack(), [attack_item(a, hare, false, true)]);
    assert!(in_zone(&next, 1, "hand").contains(&hare));
    assert!(in_zone(&next, 1, "field").is_empty());
    // Priority is seat 1's again: the attack, controlled by seat 0, is on top.
    assert_eq!(next.priority(), Some(p(1)));

    // Seat 1 passes: the attack Fizzles. Nothing of the combat happens, and the attacker's tap, a
    // cost, stands.
    let fizzling = accepted(&ruleset, &next, &pass(1));
    assert_eq!(
        fizzling.trace.events(),
        [fizzled(attack_item(a, hare, false, true))]
    );
    let end = fizzling.state;
    assert!(end.stack().is_empty());
    assert!(in_zone(&end, 1, "hand").contains(&hare));
    assert!(in_zone(&end, 1, "trash").is_empty());
    assert_eq!(state_value(&end, a, "tapped"), Some(FieldValue::Bool(true)));
    assert_eq!(end.deciding_player(), p(0));
}

#[test]
fn an_ability_without_a_cost_pushes_with_item_pushed_alone() {
    let ruleset = response_with_test_cards();
    let scene = arrange(&ruleset, &test_card_decks(&ruleset), "dodger");
    let (a, dodger) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, dodger)]);

    let transition = accepted(&ruleset, &state, &activate(1, dodger, 0));

    assert_eq!(
        transition.trace.events(),
        [pushed(ability_item(1, dodger, 0, "field", false))]
    );
    assert_eq!(transition.state.priority(), Some(p(0)));
}

#[test]
fn an_ability_fizzles_when_its_card_left_the_active_zone() {
    let ruleset = response_with_window_action("snipe", SNIPE);
    let mut scene = arrange_with_test_cards(&ruleset);
    let hare = scene.hare(1, "field");
    let (a, t) = (scene.attacker, scene.target);
    let mana = in_zone(&scene.state, 1, "mana_zone")[0];
    let (state, _) = accept_all(
        &ruleset,
        &scene.state,
        &[
            attack(a, t),
            activate(1, hare, 0),
            declared(0, "snipe", &[hare]),
        ],
    );
    let snipe_item = StackItem {
        controller: p(0),
        source: StackSource::Action(ActionId::new("snipe")),
        bound: vec![bound("victim", hare, "field", false)],
    };
    assert_eq!(
        state.stack(),
        [
            attack_item(a, t, false, false),
            ability_item(1, hare, 0, "field", false),
            snipe_item.clone()
        ]
    );

    // Seat 1 passes: the snipe resolves and the hare goes to the trash, which the ability item
    // below it records.
    let resolving = accepted(&ruleset, &state, &pass(1));
    assert_eq!(
        resolving.trace.events(),
        [resolved(snipe_item), moved(hare, 1, "field", "trash")]
    );
    let state = resolving.state;
    assert_eq!(
        state.stack(),
        [
            attack_item(a, t, false, false),
            ability_item(1, hare, 0, "field", true)
        ]
    );

    // Seat 0 passes: the ability Fizzles, `ItemFizzled` first and alone: no `do` Events. The mana
    // tap, a cost, stands.
    let fizzling = accepted(&ruleset, &state, &pass(0));
    assert_eq!(
        fizzling.trace.events(),
        [fizzled(ability_item(1, hare, 0, "field", true))]
    );
    let next = fizzling.state;
    assert_eq!(next.stack(), [attack_item(a, t, false, false)]);
    assert_eq!(in_zone(&next, 1, "trash"), [hare]);
    assert_eq!(
        state_value(&next, mana, "tapped"),
        Some(FieldValue::Bool(true))
    );
    assert_eq!(next.priority(), Some(p(1)));
}

// ---- rejections

#[test]
fn activate_is_rejected_in_the_action_phase() {
    let ruleset = response();
    let mut scene = arrange_real(&ruleset);
    let hare = scene.hare(0, "field");
    let other_hare = scene.hare(1, "field");

    // The Active Player holds the Action phase, and no window is open: the Stack is empty.
    assert_rejected(
        &ruleset,
        &scene.state,
        &activate(0, hare, 0),
        Rejection::ActivateWithoutWindow,
    );
    // The other Player is not the Deciding Player: that check comes first.
    assert_rejected(
        &ruleset,
        &scene.state,
        &activate(1, other_hare, 0),
        Rejection::NotDecidingPlayer {
            player: p(1),
            deciding: p(0),
        },
    );
}

#[test]
fn activate_is_rejected_in_a_window_without_abilities() {
    let ruleset = response_with_quiet_window();
    let mut scene = arrange_real(&ruleset);
    let hare = scene.hare(1, "field");
    let a = scene.attacker;
    let (state, _) = accept_all(&ruleset, &scene.state, &[declared(0, "poke", &[a])]);
    // The `poke` opened the `quiet` window and Priority is seat 1's.
    assert_eq!(state.priority(), Some(p(1)));

    assert_rejected(
        &ruleset,
        &state,
        &activate(1, hare, 0),
        Rejection::AbilitiesNotInWindow {
            window: WindowId::new("quiet"),
        },
    );
    assert_eq!(ruleset.legal_actions(&state), [pass(1)]);

    // The window is checked before the card: even a card that does not exist.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, CardInstanceId(9999), 0),
        Rejection::AbilitiesNotInWindow {
            window: WindowId::new("quiet"),
        },
    );
}

#[test]
fn activate_is_rejected_for_a_player_who_does_not_control_the_card() {
    let ruleset = response();
    let mut scene = arrange_real(&ruleset);
    let other_hare = scene.hare(0, "field");
    let other_hare_in_hand = scene.hare(0, "hand");
    scene.hare(1, "field");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    // Seat 1 has Priority and tries seat 0's hare.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, other_hare, 0),
        Rejection::NotCardOwner {
            param: ParamName::new("self"),
            card: other_hare,
            owner: p(0),
        },
    );
    // The owner is checked before the zone: seat 0's hare in the hand is not seat 1's either.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, other_hare_in_hand, 0),
        Rejection::NotCardOwner {
            param: ParamName::new("self"),
            card: other_hare_in_hand,
            owner: p(0),
        },
    );
}

#[test]
fn activate_is_rejected_for_a_card_outside_the_active_zone() {
    let ruleset = response();
    let mut scene = arrange_real(&ruleset);
    let in_hand = scene.hare(1, "hand");
    let in_trash = scene.hare(1, "trash");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    for card in [in_hand, in_trash] {
        assert_rejected(
            &ruleset,
            &state,
            &activate(1, card, 0),
            Rejection::CardNotInZone {
                param: ParamName::new("self"),
                card,
                zone: zone("field"),
            },
        );
    }
}

#[test]
fn activate_is_rejected_when_its_cost_cannot_be_paid() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange_with_test_cards(&ruleset);
    let juggler = scene.place("juggler", 1, "field");
    scene.apply(tapped(juggler));
    let pricey = scene.place("pricey", 1, "field");
    let hare = scene.hare(1, "field");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);

    // The `tap: self` cost on a tapped card: the juggler's second ability (index 2).
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, juggler, 2),
        Rejection::CardAlreadyTapped { card: juggler },
    );
    // A cost of 3 mana, with two untapped.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, pricey, 0),
        Rejection::NotEnoughUntappedCards {
            zone: zone("mana_zone"),
            required: 3,
            available: 2,
        },
    );

    // The hare's cost of 1 mana, with none untapped.
    for mana in in_zone(&scene.state, 1, "mana_zone") {
        scene.apply(tapped(mana));
    }
    let (dry, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);
    assert_rejected(
        &ruleset,
        &dry,
        &activate(1, hare, 0),
        Rejection::NotEnoughUntappedCards {
            zone: zone("mana_zone"),
            required: 1,
            available: 0,
        },
    );
}

#[test]
fn activate_is_rejected_for_an_index_that_is_not_an_activated_ability() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange_with_test_cards(&ruleset);
    let hare = scene.hare(1, "field");
    let juggler = scene.place("juggler", 1, "field");
    let keeper = scene.place("lamp_keeper", 1, "field");
    let other_hare = scene.hare(0, "field");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t)]);
    let not_an_ability = |card, ability| Rejection::NotAnActivatedAbility { card, ability };

    // Out of range.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, hare, 1),
        not_an_ability(hare, 1),
    );
    // In range, but a triggered effect: the juggler's index 0.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, juggler, 0),
        not_an_ability(juggler, 0),
    );
    // A card with a triggered effect only, and a card that does not exist.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, keeper, 0),
        not_an_ability(keeper, 0),
    );
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, CardInstanceId(9999), 0),
        Rejection::UnknownCard {
            param: ParamName::new("self"),
            card: CardInstanceId(9999),
        },
    );
    // The index is checked before the owner: another Player's card with a bad index.
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, other_hare, 7),
        not_an_ability(other_hare, 7),
    );
}

#[test]
fn activate_is_rejected_while_a_decision_is_pending() {
    let ruleset = response_with_test_cards();
    let mut scene = arrange_with_test_cards(&ruleset);
    let asker = scene.place("asker", 1, "hand");
    let hare = scene.hare(1, "field");
    let (a, t) = (scene.attacker, scene.target);
    let (state, _) = accept_all(&ruleset, &scene.state, &[attack(a, t), fast_play(1, asker)]);

    // The asker's trigger paused at a Choice that seat 1, the Deciding Player, must answer.
    assert!(state.pending_decision().is_some());
    assert_eq!(state.deciding_player(), p(1));
    assert_eq!(ruleset.legal_actions(&state), [answer(1, 0), answer(1, 1)]);
    assert_rejected(
        &ruleset,
        &state,
        &activate(1, hare, 0),
        Rejection::ActivateWhileDecisionPending,
    );
}

// ---- load errors

fn response_definition() -> RulesetDefinition {
    load_ruleset_definition(RESPONSE_RULESET).expect("the Response Ruleset loads")
}

fn path() -> DefinitionPath {
    DefinitionPath::root()
}

/// A catalog text with one creature, `sage`, whose Card Effects are `effects` (lines of a YAML
/// list, each starting with `      - `).
fn sage_catalog(effects: &str) -> String {
    format!(
        "meta: {{ ruleset: response, ruleset_version: 1 }}\ncards:\n  - id: sage\n    type: creature\n    fields: {{ name: Sage, cost: 1, power: 1, fast: false }}\n    effects:\n{effects}"
    )
}

fn catalog_load_error(effects: &str) -> CatalogLoadError {
    match load_card_catalog(&response_definition(), &sage_catalog(effects)) {
        Err(error) => error,
        Ok(_) => panic!("the catalog loads, an error was expected"),
    }
}

fn catalog_error(effects: &str) -> CatalogError {
    match catalog_load_error(effects) {
        CatalogLoadError::Catalog(error) => error,
        other => panic!("expected a catalog error, got {other:?}"),
    }
}

/// `cards[sage].effects[0].activated.do[..]`.
fn sage_activated_do() -> DefinitionPath {
    path()
        .key("cards")
        .id("sage")
        .key("effects")
        .index(0)
        .key("activated")
        .key("do")
}

#[test]
fn a_choice_in_an_activated_do_is_a_load_error() {
    let at = |list: DefinitionPath, key: &str| CatalogError {
        path: list.key(key),
        kind: CatalogErrorKind::InvalidCardEffect {
            error: Box::new(DefinitionErrorKind::ChoiceOutsideTriggeredEffect),
        },
    };

    // A Choice directly in the `do`, of either kind.
    assert_eq!(
        catalog_error(
            "      - { activated: { do: [ { choose_one: { decider: controller, options: [ [], [] ] } } ] } }\n"
        ),
        at(sage_activated_do().index(0), "choose_one")
    );
    assert_eq!(
        catalog_error(
            "      - { activated: { do: [ { choose_card: { name: t, decider: controller, zone: field, card_type: creature } } ] } }\n"
        ),
        at(sage_activated_do().index(0), "choose_card")
    );
    // Nested in an `if`, at its own path, after an effect that is fine.
    assert_eq!(
        catalog_error(
            "      - { activated: { do: [ { move: { card: { ref: self }, to: hand } }, { if: true, then: [ { choose_one: { decider: controller, options: [ [], [] ] } } ] } ] } }\n"
        ),
        at(sage_activated_do().index(1).key("then").index(0), "choose_one")
    );
}

#[test]
fn an_unknown_cost_step_in_an_activated_cost_is_a_load_error() {
    let error = match catalog_load_error(
        "      - { activated: { cost: [ { levitate: {} } ], do: [ { move: { card: { ref: self }, to: hand } } ] } }\n",
    ) {
        CatalogLoadError::Yaml(error) => error,
        other => panic!("expected a YAML error, got {other:?}"),
    };
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "levitate".to_string()
        }
    );
    // The catalog is addressed by position until the draft is validated.
    assert_eq!(
        error.path,
        path()
            .key("cards")
            .index(0)
            .key("effects")
            .index(0)
            .key("activated")
            .key("cost")
            .index(0)
    );
}

#[test]
fn abilities_without_an_active_zone_is_a_load_error() {
    let yaml = edit(RESPONSE_RULESET, "effects:\n  active_zone: field\n", "");
    let error = match load_ruleset_definition(&yaml) {
        Err(LoadError::Definition(error)) => error,
        other => panic!("expected a definition error, got {other:?}"),
    };
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("windows").id("attack_response").key("abilities"),
            kind: DefinitionErrorKind::AbilitiesWithoutActiveZone,
        }
    );
}

#[test]
fn a_cost_that_moves_self_to_a_zone_that_is_not_public_is_a_load_error() {
    // `hand` is `owner`: the opponent would not see the card the item binds.
    let error = catalog_error(
        "      - { activated: { cost: [ { move: { card: { ref: self }, to: hand } } ], do: [ { move: { card: { ref: self }, to: field } } ] } }\n",
    );
    assert_eq!(
        error,
        CatalogError {
            path: path()
                .key("cards")
                .id("sage")
                .key("effects")
                .index(0)
                .key("activated")
                .key("cost")
                .index(0)
                .key("move")
                .key("to"),
            kind: CatalogErrorKind::InvalidCardEffect {
                error: Box::new(DefinitionErrorKind::PushedCardNotPublic {
                    param: "self".to_string(),
                    zone: "hand".to_string(),
                }),
            },
        }
    );

    // Only the last move of `self` decides, reported at its own position (here the second step);
    // a move to `hand` that a later move to the public `trash` follows is fine.
    let error = catalog_error(
        "      - { activated: { cost: [ { move: { card: { ref: self }, to: trash } }, { move: { card: { ref: self }, to: deck } } ], do: [ { move: { card: { ref: self }, to: field } } ] } }\n",
    );
    assert!(matches!(
        error.kind,
        CatalogErrorKind::InvalidCardEffect { .. }
    ));
    assert_eq!(
        error.path,
        path()
            .key("cards")
            .id("sage")
            .key("effects")
            .index(0)
            .key("activated")
            .key("cost")
            .index(1)
            .key("move")
            .key("to")
    );
    let public = "      - { activated: { cost: [ { move: { card: { ref: self }, to: hand } }, { move: { card: { ref: self }, to: trash } } ], do: [ { move: { card: { ref: self }, to: field } } ] } }\n";
    assert!(load_card_catalog(&response_definition(), &sage_catalog(public)).is_ok());
}

#[test]
fn abilities_with_an_active_zone_that_is_not_public_is_a_load_error() {
    // `hand` is `owner`, not `public`.
    let yaml = edit(
        RESPONSE_RULESET,
        "effects:\n  active_zone: field\n",
        "effects:\n  active_zone: hand\n",
    );
    let error = match load_ruleset_definition(&yaml) {
        Err(LoadError::Definition(error)) => error,
        other => panic!("expected a definition error, got {other:?}"),
    };
    assert_eq!(
        error,
        DefinitionError {
            path: path().key("windows").id("attack_response").key("abilities"),
            kind: DefinitionErrorKind::AbilitiesActiveZoneNotPublic {
                zone: "hand".to_string()
            },
        }
    );
}

#[test]
fn activated_beside_on_is_mixed_card_effect_shapes() {
    let error = catalog_error(
        "      - { on: { enter: field }, activated: { do: [ { move: { card: { ref: self }, to: hand } } ] } }\n",
    );
    assert_eq!(
        error,
        CatalogError {
            path: path().key("cards").id("sage").key("effects").index(0),
            kind: CatalogErrorKind::MixedCardEffectShapes,
        }
    );
}

#[test]
fn the_real_response_files_declare_the_hare_and_the_ability_window() {
    let definition = response_definition();
    let [window] = definition.windows() else {
        panic!("one window is declared, got {:?}", definition.windows());
    };
    assert!(window.abilities());

    let ruleset = response();
    let hare = ruleset
        .catalog()
        .card(&CardId::new("skittish_hare"))
        .expect("the real catalog has the hare");
    // One effect, activated: it taps `cost` mana (its own cost, 1) and moves the hare to the hand.
    let [CardEffect::Activated(ability)] = hare.effects() else {
        panic!(
            "the hare has one activated effect, got {:?}",
            hare.effects()
        );
    };
    let [Cost::Tap {
        zone: tapped_zone,
        count,
    }] = ability.costs()
    else {
        panic!("its cost taps mana, got {:?}", ability.costs());
    };
    assert_eq!(tapped_zone.as_str(), "mana_zone");
    assert_eq!(count.param(), &ParamName::new("self"));
    assert_eq!(count.value(), &CardValue::Field(FieldName::new("cost")));
    let [Effect::Move { card, to }] = ability.body() else {
        panic!("its do moves a card, got {:?}", ability.body());
    };
    assert_eq!(card.param(), &ParamName::new("self"));
    assert_eq!(to.as_str(), "hand");

    // Each Player's sample deck holds two of them.
    let creation = ruleset
        .create_game(&response_decks(&ruleset), Seed(SEED))
        .expect("the decks create a game");
    for seat in [0, 1] {
        let hares = creation
            .opening
            .state
            .cards()
            .iter()
            .filter(|card| card.owner() == p(seat) && card.definition().as_str() == "skittish_hare")
            .count();
        assert_eq!(hares, 2, "seat {seat}");
    }
}

// ---- replay

/// Plays a scripted Game of `seed` and returns its record, the Event Trace of each Action and the
/// final GameState. The script is a deterministic rule over the Legal Actions: with the Stack
/// non-empty take a `fast_play` if one is Legal, else an `Activate`, else `Pass`; otherwise an
/// `attack`, else a `play_creature` (once per Turn), else `pass_turn`. It stops when the Game ends
/// or after `SCRIPT_CAP` Actions.
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
            named("fast_play")
                .or_else(|| legal.iter().find(|a| matches!(a, Action::Activate { .. })))
                .or_else(|| legal.iter().find(|a| matches!(a, Action::Pass { .. })))
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
fn replay_reproduces_a_game_that_uses_an_ability() {
    let ruleset = response();
    let decks = response_decks(&ruleset);
    let mut activations = 0;
    let mut resolved_abilities = 0;
    for seed in 0..SCRIPTED_SEEDS {
        let (record, traces, state) = scripted_game(&ruleset, &decks, seed);
        activations += record
            .actions()
            .iter()
            .filter(|action| matches!(action, Action::Activate { .. }))
            .count();
        resolved_abilities += traces
            .iter()
            .flatten()
            .filter(|event| {
                matches!(
                    event,
                    Event::ItemResolved { item }
                        if matches!(item.source, StackSource::Ability { .. })
                )
            })
            .count();

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
    assert!(activations >= 1, "no scripted Game activates an ability");
    assert!(
        resolved_abilities >= 1,
        "no scripted Game resolves an ability"
    );
}

// ---- observing

fn observed_bound(param: &str, zone_id: &str, card: Option<ObservedCard>) -> ObservedBoundCard {
    ObservedBoundCard {
        param: ParamName::new(param),
        zone: zone(zone_id),
        card,
    }
}

/// The Observed Stack Item of an ability of seat 1's `card`, as the Viewer of `observer` sees it;
/// `left` is set once the card left.
fn observed_ability(
    observer: &Observer,
    card: CardInstanceId,
    ability: u32,
    left: bool,
) -> ObservedStackItem {
    let handle = if left {
        None
    } else {
        Some(observer.observed_card(card).expect("a seen card"))
    };
    ObservedStackItem {
        controller: p(1),
        source: ObservedStackSource::Ability {
            card: handle,
            ability,
        },
        bound: vec![observed_bound("self", "field", handle)],
    }
}

/// One Viewer's Observer and Observed GameState, advanced one accepted Action at a time.
struct Follow<'a> {
    ruleset: &'a DeclarativeRuleset,
    seat: u32,
    state: GameState,
    observer: Observer,
    observed: ObservedGameState,
}

impl<'a> Follow<'a> {
    fn new(ruleset: &'a DeclarativeRuleset, scene: &Scene, seat: u32) -> Self {
        let observer = scene.observer(ruleset, seat);
        let observed = ruleset
            .observe(&observer, &scene.state)
            .expect("the GameState is observed");
        Follow {
            ruleset,
            seat,
            state: scene.state.clone(),
            observer,
            observed,
        }
    }

    /// Takes `action`, which must be accepted, and returns its Taken Action and Observed Events.
    /// They must fold into the Observed GameState the Observer gives for the next GameState.
    fn step(&mut self, action: &Action) -> (TakenAction, Vec<ObservedEvent>) {
        let transition = self
            .ruleset
            .transition(&self.state, action)
            .expect("the Action is accepted");
        let followed = self
            .ruleset
            .follow_transition(&mut self.observer, &self.state, action, &transition)
            .expect("the transition is followed");
        assert_eq!(
            self.observed.apply_events(&followed.events),
            Ok(followed.state.clone()),
            "seat {}",
            self.seat
        );
        self.observed = followed.state;
        self.state = transition.state;
        (followed.action, followed.events)
    }

    fn handle(&self, card: CardInstanceId) -> ObservedCard {
        self.observer.observed_card(card).expect("a seen card")
    }
}

#[test]
fn an_observed_activate_round_trips() {
    let ruleset = response();
    let mut scene = arrange_real(&ruleset);
    scene.place("swift_sparrow", 1, "hand");
    let hare = scene.hare(1, "field");
    let (a, t) = (scene.attacker, scene.target);
    // The hare costs 1: the first of seat 1's mana, which is public.
    let mana = in_zone(&scene.state, 1, "mana_zone")[0];

    let mut follows = [0, 1].map(|seat| Follow::new(&ruleset, &scene, seat));
    for follow in &mut follows {
        follow.step(&attack(a, t));
    }
    let state = follows[0].state.clone();

    // The Player with Priority is offered `Pass`, the `fast_play`, then the ability, each as an
    // Observed Action; the other Player is offered nothing.
    let hare_handle = follows[1].handle(hare);
    let offered = ruleset
        .observed_legal_actions(&follows[1].observer, &state)
        .expect("the Legal Actions are observed");
    assert_eq!(offered.len(), 3);
    assert_eq!(offered[0], ObservedAction::Pass { player: p(1) });
    assert!(matches!(
        &offered[1],
        ObservedAction::Declared { action, .. } if action.as_str() == "fast_play"
    ));
    assert_eq!(
        offered[2],
        ObservedAction::Activate {
            player: p(1),
            card: hare_handle,
            ability: 0
        }
    );
    assert_eq!(
        ruleset.observed_legal_actions(&follows[0].observer, &state),
        Ok(vec![])
    );

    // Translated back, the Observed Legal Actions are the Legal Actions, in order.
    let translated: Vec<Action> = offered
        .iter()
        .map(|action| {
            ruleset
                .translate_action(&follows[1].observer, action)
                .expect("the Observed Action translates")
        })
        .collect();
    assert_eq!(translated, ruleset.legal_actions(&state));
    assert_eq!(translated.get(2), Some(&activate(1, hare, 0)));

    // An Observed Activate of another Player than the Viewer is not translated.
    assert_eq!(
        ruleset.translate_action(
            &follows[0].observer,
            &ObservedAction::Activate {
                player: p(1),
                card: follows[0].handle(hare),
                ability: 0
            }
        ),
        Err(ObservationError::ActionOfAnotherPlayer { player: p(1) })
    );

    // Taking it: the Taken Action shows the card as the Observed Card, to both Viewers (it is in
    // the public Active Zone), the cost shows the tap of the mana, and the item's source shows the
    // same card and the ability index.
    for follow in &mut follows {
        let seat = follow.seat;
        let (taken, events) = follow.step(&activate(1, hare, 0));
        let handle = follow.handle(hare);
        let mana_handle = follow.handle(mana);
        assert_eq!(
            taken,
            TakenAction::Activate {
                player: p(1),
                card: handle,
                ability: 0
            },
            "seat {seat}"
        );
        let item = observed_ability(&follow.observer, hare, 0, false);
        assert_eq!(
            events,
            [
                ObservedEvent::CardStateChanged {
                    card: mana_handle,
                    state: StateName::new("tapped"),
                    value: FieldValue::Bool(true)
                },
                ObservedEvent::ItemPushed { item: item.clone() }
            ],
            "seat {seat}"
        );
        assert_eq!(follow.observed.stack().last(), Some(&item), "seat {seat}");
        assert_eq!(follow.observed.priority(), Some(p(0)), "seat {seat}");
        // The Observer's Observed GameState is the one the folded Events give.
        assert_eq!(
            ruleset.observe(&follow.observer, &follow.state).as_ref(),
            Ok(&follow.observed),
            "seat {seat}"
        );
    }
}

#[test]
fn an_observed_ability_source_hides_a_card_that_left() {
    // `bounce` moves the hare to its owner's hand, which seat 0 cannot see.
    let ruleset = response_with_window_action("bounce", BOUNCE);
    let mut scene = arrange_with_test_cards(&ruleset);
    let hare = scene.hare(1, "field");
    let (a, t) = (scene.attacker, scene.target);

    for seat in [0, 1] {
        let mut follow = Follow::new(&ruleset, &scene, seat);
        follow.step(&attack(a, t));
        follow.step(&activate(1, hare, 0));
        follow.step(&declared(0, "bounce", &[hare]));

        // While the hare is in the field both Viewers see it in the source and in the bound card.
        let shown = observed_ability(&follow.observer, hare, 0, false);
        assert_eq!(follow.observed.stack().get(1), Some(&shown), "seat {seat}");

        // Seat 1 passes: the bounce resolves, the hare goes to seat 1's hand, and the item below
        // shows no card, neither in its source nor in its bound card: a handle names a card only
        // while the Viewer sees it, and seat 0 does not see the hand.
        follow.step(&pass(1));
        let hidden = observed_ability(&follow.observer, hare, 0, true);
        assert_eq!(
            hidden.source,
            ObservedStackSource::Ability {
                card: None,
                ability: 0
            }
        );
        assert_eq!(follow.observed.stack().get(1), Some(&hidden), "seat {seat}");

        // Seat 0 passes: the ability Fizzles, `ItemFizzled` alone, with the item as it was.
        let (_, events) = follow.step(&pass(0));
        assert_eq!(
            events,
            [ObservedEvent::ItemFizzled { item: hidden }],
            "seat {seat}"
        );
        assert_eq!(follow.observed.stack().len(), 1, "seat {seat}");
    }
}
