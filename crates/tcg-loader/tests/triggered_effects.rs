//! Acceptance tests for ticket #50: resolving triggered Card Effects from Events (ADR-0023), on the
//! real Effects Ruleset with a catalog written in this file.
//!
//! The catalog and the Deck Lists are text built here, so each card exists for the one behaviour a
//! test needs and no shipped deck changes. Every card costs 0, so a creature is played without
//! paying mana. The Event Traces are asserted whole: a scenario's trace is the trace of the same
//! Action when the cards under test are parked in a zone where they do nothing (the plain trace)
//! followed by exactly the Events the triggers add.
//!
//! Scenarios arrange cards with `GameState::apply` of `CardMoved` Events before the Action under
//! test. The tests that replay a Game record start from the Seed alone instead, because a record
//! holds no arrangement.
//!
//! `leader_dead` cannot be reached by a trigger today: a Card Effect only reads and changes its own
//! card (the one name in scope is `self`), and a Leader carries no Card Effect. The win-check test
//! therefore uses `deck_out`, the outcome a trigger can cause.

use tcg_engine::card_catalog::CardCatalog;
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameRecord, GameState, Loss,
    Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::Ruleset;
use tcg_engine::ruleset_definition::{
    ActionId, FieldValue, RulesetDefinition, StateName, WinConditionId, ZoneId,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_ruleset_draft,
    CatalogLoadError, YamlErrorKind,
};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");

// ---- the catalog and the decks

const DRAW: &str = "{ draw: { player: controller, from: deck, to: hand, count: 1 } }";
const TAP_SELF: &str = "{ set_state: { card: { ref: self }, state: tapped, value: true } }";
const SELF_TO_TRASH: &str = "{ move: { card: { ref: self }, to: trash } }";
const SELF_TO_FIELD: &str = "{ move: { card: { ref: self }, to: field } }";
const NEVER: &str = "{ gte: [ { ref: self.power }, 99 ] }";

/// A creature (cost 0, power 1) with these Card Effects, written as YAML list items.
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

fn triggered(on: &str, condition: Option<&str>, body: &[&str]) -> String {
    let condition = condition.map_or(String::new(), |condition| format!(" if: {condition},"));
    format!("{{ on: {on},{condition} do: [ {} ] }}", body.join(", "))
}

/// The catalog of this file: two Leaders, the mana card, 13 plain creatures (`filler_01` to
/// `filler_13`) and one creature for each behaviour under test.
fn catalog_yaml() -> String {
    let mut yaml = String::from("meta: { ruleset: effects, ruleset_version: 1 }\ncards:\n");
    for leader in ["lead_a", "lead_b"] {
        yaml.push_str(&format!(
            "  - {{ id: {leader}, type: leader, fields: {{ name: \"{leader}\", power: 1, life: 5 }} }}\n"
        ));
    }
    yaml.push_str("  - { id: mana, type: mana, fields: {} }\n");
    for n in 1..=13 {
        yaml.push_str(&creature(&format!("filler_{n:02}"), &[]));
    }
    let enter_field = "{ enter: field }";
    let enter_trash = "{ enter: trash }";
    let turn_start = "{ turn_start: {} }";
    // Draws when it enters the field.
    yaml.push_str(&creature(
        "lantern_bearer",
        &[triggered(enter_field, None, &[DRAW])],
    ));
    // Draws, which is a draw from an empty deck when there is none, and then taps.
    yaml.push_str(&creature(
        "starved_scribe",
        &[triggered(enter_field, None, &[DRAW, TAP_SELF])],
    ));
    // Draws at the start of its controller's Turn.
    yaml.push_str(&creature(
        "dawn_herald",
        &[triggered(turn_start, None, &[DRAW])],
    ));
    // Draws when it enters the trash.
    yaml.push_str(&creature(
        "grave_hound",
        &[triggered(enter_trash, None, &[DRAW])],
    ));
    // Two Card Effects at the start of its controller's Turn, in this order.
    yaml.push_str(&creature(
        "signal_pair",
        &[
            triggered(
                turn_start,
                None,
                &["{ set_state: { card: { ref: self }, state: recently_played, value: true } }"],
            ),
            triggered(turn_start, None, &[TAP_SELF]),
        ],
    ));
    // Entering the field wakes two Card Effects; the first moves the card to the trash, which wakes
    // the third while the second is still queued.
    yaml.push_str(&creature(
        "ember_phoenix",
        &[
            triggered(enter_field, None, &[SELF_TO_TRASH]),
            triggered(enter_field, None, &[TAP_SELF]),
            triggered(enter_trash, None, &[DRAW]),
        ],
    ));
    // Each of its two Card Effects wakes the other: a mutually-triggering pair.
    yaml.push_str(&creature(
        "restless_wraith",
        &[
            triggered(enter_field, None, &[SELF_TO_TRASH]),
            triggered(enter_trash, None, &[SELF_TO_FIELD]),
        ],
    ));
    // 25 Card Effects at the start of its Turn whose `if` is always false, and one.
    let idle = triggered(turn_start, Some(NEVER), &[DRAW]);
    yaml.push_str(&creature("idle_25", &vec![idle.clone(); 25]));
    yaml.push_str(&creature("idle_1", &[idle]));
    yaml
}

const SPECIALS: [(&str, u32); 10] = [
    ("lantern_bearer", 4),
    ("starved_scribe", 4),
    ("dawn_herald", 4),
    ("grave_hound", 4),
    ("signal_pair", 2),
    ("ember_phoenix", 4),
    ("restless_wraith", 4),
    ("idle_25", 4),
    ("idle_1", 1),
    ("filler_01", 4),
];

/// A deck of 50 creatures: the specials and then fillers up to 50.
fn deck_yaml(leader: &str) -> String {
    let mut yaml = format!(
        "inputs:\n  - input: leader\n    cards:\n      - {{ card: {leader}, count: 1 }}\n  - input: main_deck\n    cards:\n"
    );
    let mut total = 0;
    for (card, count) in SPECIALS {
        yaml.push_str(&format!("      - {{ card: {card}, count: {count} }}\n"));
        total += count;
    }
    let mut filler = 2;
    while total < 50 {
        let count = (50 - total).min(4);
        yaml.push_str(&format!(
            "      - {{ card: filler_{filler:02}, count: {count} }}\n"
        ));
        total += count;
        filler += 1;
    }
    yaml.push_str("  - input: mana_deck\n    cards:\n      - { card: mana, count: 10 }\n");
    yaml
}

fn definition() -> RulesetDefinition {
    load_ruleset_definition(EFFECTS_RULESET).expect("the Effects Ruleset loads")
}

fn ruleset() -> DeclarativeRuleset {
    let definition = definition();
    let catalog = load_card_catalog(&definition, &catalog_yaml()).expect("the test catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    ["lead_a", "lead_b"]
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

fn attack(player: u32, attacker: CardInstanceId, target: CardInstanceId) -> Action {
    action(player, "attack", &[attacker, target])
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

/// Moves `card` to `to` of its owner with an Event, as an arrangement: it is not a transition, so
/// it wakes no trigger.
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
        .apply(&Event::CardStateChanged {
            card,
            state: StateName::new("tapped"),
            value: FieldValue::Bool(true),
        })
        .expect("the card has the state `tapped`");
}

fn in_zone(state: &GameState, player: u32, id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(id))
        .expect("the zone exists")
        .to_vec()
}

/// The Event that draws `card` for `player`.
fn drew(player: u32, card: CardInstanceId) -> Event {
    Event::CardMoved {
        card,
        player: p(player),
        from: zone("deck"),
        to: zone("hand"),
    }
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

/// The Game as created from `seed`, before any Action.
fn opening(ruleset: &DeclarativeRuleset, seed: u64) -> GameState {
    ruleset
        .create_game(&decks(ruleset), Seed(seed))
        .expect("the test decks create a game")
        .opening
        .state
}

/// Seed 1's opening: Player 0 is in Main of Turn 1, nothing arranged.
fn start(ruleset: &DeclarativeRuleset) -> GameState {
    opening(ruleset, 1)
}

fn events_of(ruleset: &DeclarativeRuleset, state: &GameState, action: &Action) -> Vec<Event> {
    ruleset
        .transition(state, action)
        .expect("the Action is accepted")
        .trace
        .into_events()
}

/// The first seed from 0 whose opening hand of Player 0 holds a card of the definition `id`.
fn seed_with_in_hand(ruleset: &DeclarativeRuleset, id: &str) -> (u64, CardInstanceId) {
    (0..2000)
        .find_map(|seed| {
            let state = opening(ruleset, seed);
            in_zone(&state, 0, "hand")
                .into_iter()
                .find(|&card| {
                    state
                        .card(card)
                        .is_some_and(|instance| instance.definition().as_str() == id)
                })
                .map(|card| (seed, card))
        })
        .expect("some seed deals the card to Player 0's hand")
}

// ---- criteria

#[test]
fn enter_field_trigger_draws_its_controller_a_card() {
    let ruleset = ruleset();
    let (seed, lantern) = seed_with_in_hand(&ruleset, "lantern_bearer");
    let state = opening(&ruleset, seed);
    let hand = in_zone(&state, 0, "hand");
    let deck = in_zone(&state, 0, "deck");
    let top = *deck.last().expect("the deck is not empty");

    let played = play(0, lantern);
    let transition = ruleset
        .transition(&state, &played)
        .expect("the play is accepted");

    // The trigger's Events come after the play's, in the one Event Trace.
    assert_eq!(
        transition.trace.events(),
        [
            moved(0, lantern, "hand", "field"),
            state_changed(lantern, "recently_played", true),
            drew(0, top),
        ]
    );
    // One card left the hand for the field and one came from the deck: the hand is as large as
    // before, the deck one smaller.
    let after = &transition.state;
    assert_eq!(in_zone(after, 0, "hand").len(), hand.len());
    assert_eq!(in_zone(after, 0, "deck").len(), deck.len() - 1);
    assert!(in_zone(after, 0, "field").contains(&lantern));
    assert!(in_zone(after, 0, "hand").contains(&top));

    // Replaying the record reproduces the Event Trace and the GameState exactly.
    let decks = decks(&ruleset);
    let mut record = GameRecord::new(&ruleset, &decks, Seed(seed));
    record.push_accepted(played);
    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(replay.traces.len(), 1);
    assert_eq!(replay.traces[0].events(), transition.trace.events());
    assert_eq!(replay.state, transition.state);
}

#[test]
fn turn_start_trigger_fires_only_on_its_controllers_turn() {
    let ruleset = ruleset();
    let mut state = start(&ruleset);
    let herald_0 = nth(&state, 0, "dawn_herald", 0);
    let herald_1 = nth(&state, 1, "dawn_herald", 0);
    let mut plain = state.clone();
    for herald in [herald_0, herald_1] {
        park(&mut plain, herald, "trash");
        park(&mut state, herald, "field");
    }

    // Player 0 passes: Player 1's Turn begins, and only Player 1's herald fires: it draws the card
    // under the one the Start phase drew.
    let deck_1 = in_zone(&state, 1, "deck");
    let second = deck_1[deck_1.len() - 2];
    let mut expected = events_of(&ruleset, &plain, &pass(0));
    expected.push(drew(1, second));
    let first = ruleset
        .transition(&state, &pass(0))
        .expect("the pass is accepted");
    assert_eq!(first.trace.events(), expected);

    // Player 1 passes: Player 0's Turn begins, and only Player 0's herald fires.
    let mut plain = first.state.clone();
    park(&mut plain, herald_0, "trash");
    let deck_0 = in_zone(&first.state, 0, "deck");
    let second = deck_0[deck_0.len() - 2];
    let mut expected = events_of(&ruleset, &plain, &pass(1));
    expected.push(drew(0, second));
    let next = ruleset
        .transition(&first.state, &pass(1))
        .expect("the pass is accepted");
    assert_eq!(next.trace.events(), expected);
}

#[test]
fn card_effects_are_silent_outside_the_active_zone() {
    let ruleset = ruleset();
    let base = start(&ruleset);
    let herald = nth(&base, 1, "dawn_herald", 0);

    // A turn_start card fires only in the field: parked anywhere else, the Turn of its controller
    // begins with no extra Event.
    let trace_parked_in = |zone: &str| {
        let mut state = base.clone();
        park(&mut state, herald, zone);
        events_of(&ruleset, &state, &pass(0))
    };
    let in_trash = trace_parked_in("trash");
    for zone in ["hand", "mana_zone", "leader_zone"] {
        assert_eq!(trace_parked_in(zone), in_trash, "parked in {zone}");
    }
    let in_field = trace_parked_in("field");
    assert_eq!(in_field.len(), in_trash.len() + 1);
    assert_eq!(in_field[..in_trash.len()], in_trash[..]);

    // An `enter: field` card is silent when it leaves the field: it dies in combat and nothing
    // follows the move to the trash.
    let mut state = base.clone();
    let lantern = nth(&state, 1, "lantern_bearer", 0);
    let attacker = nth(&state, 0, "filler_01", 0);
    park(&mut state, lantern, "field");
    set_tapped(&mut state, lantern);
    park(&mut state, attacker, "field");
    assert_eq!(
        events_of(&ruleset, &state, &attack(0, attacker, lantern)),
        [
            state_changed(attacker, "tapped", true),
            moved(1, lantern, "field", "trash"),
        ]
    );
}

#[test]
fn enter_trash_trigger_fires_when_the_card_dies_in_combat() {
    let ruleset = ruleset();
    let mut state = start(&ruleset);
    let hound = nth(&state, 1, "grave_hound", 0);
    let attacker = nth(&state, 0, "filler_01", 0);
    park(&mut state, hound, "field");
    set_tapped(&mut state, hound);
    park(&mut state, attacker, "field");
    let deck = in_zone(&state, 1, "deck");
    let top = *deck.last().expect("the deck is not empty");

    // The hound arrives in the trash, the zone its matcher names although it is not in the field
    // any more, and draws for its controller.
    let transition = ruleset
        .transition(&state, &attack(0, attacker, hound))
        .expect("the attack is accepted");
    assert_eq!(
        transition.trace.events(),
        [
            state_changed(attacker, "tapped", true),
            moved(1, hound, "field", "trash"),
            drew(1, top),
        ]
    );
}

#[test]
fn two_triggers_woken_by_one_event_fire_in_canonical_order() {
    let ruleset = ruleset();
    let mut state = start(&ruleset);
    let low = nth(&state, 1, "signal_pair", 0);
    let high = nth(&state, 1, "signal_pair", 1);
    assert!(low.0 < high.0);
    let mut plain = state.clone();
    for card in [high, low] {
        park(&mut plain, card, "trash");
        // The higher id enters the field first: the order of firing is the order of
        // `state.cards()`, not of the zone.
        park(&mut state, card, "field");
    }

    // Both signal_pairs are woken by the one TurnBegan Event of Player 1: by card, then by the order
    // of the card's effects.
    let mut expected = events_of(&ruleset, &plain, &pass(0));
    expected.extend([
        state_changed(low, "recently_played", true),
        state_changed(low, "tapped", true),
        state_changed(high, "recently_played", true),
        state_changed(high, "tapped", true),
    ]);
    assert_eq!(events_of(&ruleset, &state, &pass(0)), expected);
}

#[test]
fn a_trigger_moving_a_card_cascades_its_enter_trigger_fifo() {
    let ruleset = ruleset();
    let mut state = start(&ruleset);
    let phoenix = nth(&state, 0, "ember_phoenix", 0);
    park(&mut state, phoenix, "hand");
    let top = *in_zone(&state, 0, "deck")
        .last()
        .expect("the deck is not empty");

    // Entering the field wakes effects 1 (move to the trash) and 2 (tap). Effect 1 fires first and
    // its move wakes effect 3 (draw on entering the trash), which is queued after effect 2: so the
    // tap comes before the draw.
    assert_eq!(
        events_of(&ruleset, &state, &play(0, phoenix)),
        [
            moved(0, phoenix, "hand", "field"),
            state_changed(phoenix, "recently_played", true),
            moved(0, phoenix, "field", "trash"),
            state_changed(phoenix, "tapped", true),
            drew(0, top),
        ]
    );
}

#[test]
fn exceeding_the_trigger_cap_rejects_the_transition() {
    let ruleset = ruleset();
    let mut state = start(&ruleset);
    let wraith = nth(&state, 0, "restless_wraith", 0);
    park(&mut state, wraith, "hand");
    let before = state.clone();

    // The wraith's two effects wake each other for ever: the 101st resolution is refused, and the
    // whole transition with it.
    let rejection = ruleset
        .transition(&state, &play(0, wraith))
        .expect_err("the cap is exceeded");
    assert_eq!(rejection, Rejection::TriggerCapExceeded { cap: 100 });
    assert_eq!(state, before);
    assert!(in_zone(&state, 0, "hand").contains(&wraith));
}

#[test]
fn a_false_if_fires_no_events_and_counts_toward_the_cap() {
    let ruleset = ruleset();
    let base = start(&ruleset);
    let mut plain = base.clone();
    let mut state = base.clone();
    // Four idle_25 are 100 Card Effects, all woken by the Turn of Player 1, none of which holds.
    for n in 0..4 {
        let card = nth(&base, 1, "idle_25", n);
        park(&mut plain, card, "trash");
        park(&mut state, card, "field");
    }
    // The 100 resolutions are exactly the cap: they are accepted and emit no Events.
    assert_eq!(
        events_of(&ruleset, &state, &pass(0)),
        events_of(&ruleset, &plain, &pass(0))
    );

    // One more false `if` is the 101st resolution: a false `if` counts.
    park(&mut state, nth(&base, 1, "idle_1", 0), "field");
    let before = state.clone();
    assert_eq!(
        ruleset
            .transition(&state, &pass(0))
            .expect_err("over the cap"),
        Rejection::TriggerCapExceeded { cap: 100 }
    );
    assert_eq!(state, before);
}

#[test]
fn a_trigger_drawing_from_an_empty_deck_ends_the_game() {
    let ruleset = ruleset();
    let mut state = start(&ruleset);
    let scribe = nth(&state, 0, "starved_scribe", 0);
    park(&mut state, scribe, "hand");
    for card in in_zone(&state, 0, "deck") {
        park(&mut state, card, "trash");
    }
    assert!(in_zone(&state, 0, "deck").is_empty());

    // The draw finds the deck empty; the second effect of the same trigger still runs; the win
    // check runs once, after all of it, and its GameEnded is the last Event.
    let transition = ruleset
        .transition(&state, &play(0, scribe))
        .expect("the play is accepted");
    let events = transition.trace.events();
    assert_eq!(
        events,
        [
            moved(0, scribe, "hand", "field"),
            state_changed(scribe, "recently_played", true),
            Event::DrawFromEmptyZone {
                player: p(0),
                zone: zone("deck"),
            },
            state_changed(scribe, "tapped", true),
            Event::GameEnded {
                losses: vec![Loss {
                    player: p(0),
                    reason: WinConditionId::new("deck_out"),
                }],
            },
        ]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::GameEnded { .. }))
            .count(),
        1
    );
    let outcome = transition.state.outcome().expect("the Game ended");
    assert_eq!(outcome.winners(), [p(1)]);
}

#[test]
fn identical_runs_produce_identical_states_and_traces() {
    let ruleset = ruleset();
    let (seed, lantern) = seed_with_in_hand(&ruleset, "lantern_bearer");
    let run = || {
        let decks = decks(&ruleset);
        let created = ruleset
            .create_game(&decks, Seed(seed))
            .expect("the test decks create a game");
        let mut state = created.opening.state.clone();
        let mut traces = vec![created.opening.trace.events().to_vec()];
        let script = [play(0, lantern), pass(0), pass(1), pass(0), pass(1)];
        for action in &script {
            let transition = ruleset
                .transition(&state, action)
                .expect("the script is accepted");
            traces.push(transition.trace.events().to_vec());
            state = transition.state;
        }
        (traces, state)
    };
    let (traces, state) = run();
    let (again_traces, again_state) = run();
    assert_eq!(traces, again_traces);
    assert_eq!(state, again_state);
    // The lantern's trigger is part of what was compared.
    assert!(traces[1].contains(&moved(0, lantern, "hand", "field")));
    assert_eq!(traces[1].len(), 3);
}

// ---- the active zone block of the ruleset file

#[test]
fn the_effects_ruleset_declares_the_field_as_its_active_zone() {
    assert_eq!(definition().effects_active_zone(), Some(&zone("field")));
}

#[test]
fn effects_block_loads_and_its_unknown_keys_are_syntax_errors() {
    let draft = parse_ruleset_draft(EFFECTS_RULESET).expect("parses");
    assert_eq!(
        draft.effects.map(|effects| effects.active_zone),
        Some("field".to_string())
    );

    let yaml = EFFECTS_RULESET.replace("  active_zone: field", "  active_zone: field\n  bogus: 1");
    let error = parse_ruleset_draft(&yaml).expect_err("an unknown key");
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "bogus".to_string()
        }
    );
    assert_eq!(
        error.path,
        DefinitionPath::root().key("effects").key("bogus")
    );

    // An effects block without its one key is malformed.
    let yaml = EFFECTS_RULESET.replace("  active_zone: field", "  {}");
    assert!(parse_ruleset_draft(&yaml).is_err());
}

// ---- the draw effect in the catalog file

#[test]
fn draw_effect_count_is_an_unquoted_positive_integer() {
    let with_count = |count: &str| {
        let yaml = format!(
            "meta: {{ ruleset: effects, ruleset_version: 1 }}\ncards:\n  - {{ id: sage, type: creature, fields: {{ name: Sage, cost: 1, power: 1 }}, effects: [ {{ on: {{ enter: field }}, do: [ {{ draw: {{ player: controller, from: deck, to: hand, count: {count} }} }} ] }} ] }}\n"
        );
        load_card_catalog(&definition(), &yaml)
    };
    assert!(with_count("2").is_ok());
    for bad in ["\"1\"", "1.5", "true"] {
        assert!(
            matches!(with_count(bad), Err(CatalogLoadError::Yaml(_))),
            "count {bad}"
        );
    }
    assert!(matches!(with_count("0"), Err(CatalogLoadError::Catalog(_))));
}

/// The test catalog is a valid catalog of the Effects Ruleset, and the triggered Card Effects are
/// held by their definitions.
#[test]
fn triggered_card_effects_are_visible_on_their_card_definitions() {
    let ruleset = ruleset();
    let catalog: &CardCatalog = ruleset.catalog();
    let lantern = catalog
        .cards()
        .iter()
        .find(|card| card.id().as_str() == "lantern_bearer")
        .expect("the catalog has the card");
    assert_eq!(lantern.effects().len(), 1);
}
