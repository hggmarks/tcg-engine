//! Acceptance tests for ticket #51: Continuous Card Effects as derived reads, on the carrying card
//! (ADR-0024), on the whole real Effects Ruleset, its real catalog and a test deck.
//!
//! A Field Modifier is never stored and never emitted: a read of a field through a `ref` is the base
//! value plus the modifiers that are active at that moment. The tests observe a read through what it
//! decides (a combat, a legal Action, an `if`, a cost), never through a getter.
//!
//! The cards under test: `battle_veteran` (power 2, +1 power while tapped) and `weary_ox` (power 3,
//! -1 power while tapped) are in the real catalog; the others are appended to its text here:
//! `fading_giant` (power 3, -1 power while its own power is at least 3), `discounter` (cost 2, -1
//! cost always), and two cards whose derived read overflows. A creature taps when it attacks, before
//! the combat compares powers, so a "while tapped" modifier is the one gameplay flips.
//!
//! The Game of a scenario is the five-turn duel: seat 0 plays the attacker on Turn 1, seat 1 plays
//! the victim on Turn 2, seat 1's victim attacks the Leader on Turn 4 (so it is tapped, a legal
//! target), and seat 0's attacker attacks the victim on Turn 5. Seeds were chosen so the hands hold
//! the cards each scenario needs; each test asserts the cards it relies on by finding them in the
//! hands. A few tests edit the text of the ruleset (one more action, a `where` on power, the active
//! zone) and say so.

use tcg_engine::card_catalog::CardCatalog;
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameRecord,
    GameState, Rejection,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{EventTrace, LegalActions, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, DefinitionError, DefinitionErrorKind, FieldName, FieldValue, ParamName,
    ReferenceTarget, StateName, ZoneId,
};
use tcg_loader::{
    load_card_catalog, load_deck_list, load_ruleset_definition, parse_ruleset_draft, LoadError,
    YamlErrorKind,
};

const RULESET: &str = include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const CATALOG: &str = include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");
const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");

/// The cards only these tests need, appended to the text of the real catalog.
const TEST_CARDS: &str = "
  - { id: fading_giant, type: creature, fields: { name: \"Fading Giant\", cost: 2, power: 3 }, effects: [ { while: { gte: [ { ref: self.power }, 3 ] }, modify: { field: power, amount: -1 } } ] }
  - { id: discounter, type: creature, fields: { name: \"Discounter\", cost: 2, power: 1 }, effects: [ { while: true, modify: { field: cost, amount: -1 } } ] }
  - { id: overflow_edge, type: creature, fields: { name: \"Overflow Edge\", cost: 1, power: 9223372036854775807 }, effects: [ { while: true, modify: { field: power, amount: 1 } } ] }
  - { id: overflow_sum, type: creature, fields: { name: \"Overflow Sum\", cost: 1, power: 0 }, effects: [ { while: true, modify: { field: power, amount: 9223372036854775807 } }, { while: true, modify: { field: power, amount: 9223372036854775807 } } ] }
";

/// The deck of both seats: 1 Leader, 50 creatures (at most 4 of each), 10 mana.
const DECK: &str = "
inputs:
  - input: leader
    cards:
      - { card: captain_ardel, count: 1 }
  - input: main_deck
    cards:
      - { card: battle_veteran, count: 4 }
      - { card: weary_ox, count: 4 }
      - { card: fading_giant, count: 4 }
      - { card: discounter, count: 4 }
      - { card: overflow_edge, count: 4 }
      - { card: overflow_sum, count: 4 }
      - { card: river_otter, count: 4 }
      - { card: moss_sprite, count: 4 }
      - { card: goblin_scout, count: 4 }
      - { card: ember_hound, count: 4 }
      - { card: stone_golem, count: 4 }
      - { card: shadow_cat, count: 4 }
      - { card: iron_guard, count: 2 }
  - input: mana_deck
    cards:
      - { card: mana, count: 10 }
";

/// With this Seed seat 0 holds `battle_veteran`, `weary_ox` and `fading_giant` and seat 1 holds
/// `river_otter` (power 3) and `moss_sprite` (power 2).
const DUEL_SEED: u64 = 11;
/// With this Seed seat 0 holds `overflow_edge` and `overflow_sum`.
const OVERFLOW_SEED: u64 = 10;
/// With this Seed seat 0 holds `discounter`.
const DISCOUNT_SEED: u64 = 4;

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

/// `yaml` with the one occurrence of `from` replaced by `to`.
fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(
        yaml.matches(from).count(),
        1,
        "`{from}` occurs once in the fixture"
    );
    yaml.replace(from, to)
}

fn catalog_yaml() -> String {
    format!("{CATALOG}\n{TEST_CARDS}")
}

/// The same catalog with every Card Effect removed: the baseline a modifier is measured against.
fn catalog_yaml_without_card_effects() -> String {
    catalog_yaml()
        .lines()
        .map(|line| match line.find(", effects: [") {
            Some(at) => format!("{} }}", &line[..at]),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn ruleset_from(ruleset_yaml: &str, catalog_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog_yaml).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The real Effects Ruleset with the real catalog and the test cards.
fn effects() -> DeclarativeRuleset {
    ruleset_from(RULESET, &catalog_yaml())
}

/// The real Effects Ruleset, but no card carries a Card Effect.
fn effects_without_card_effects() -> DeclarativeRuleset {
    ruleset_from(RULESET, &catalog_yaml_without_card_effects())
}

/// The Effects Ruleset text with the action `flex`: for a creature of the acting Player's field,
/// when its power is at least 3, it becomes `recently_played`. It is an effect `if` that reads a
/// field.
fn flex_ruleset_yaml() -> String {
    let yaml = edit(
        RULESET,
        "actions: [play_creature, attack, pass_turn]",
        "actions: [play_creature, attack, pass_turn, flex]",
    );
    edit(
        &yaml,
        "  - id: pass_turn\n",
        "  - id: flex\n    params:\n      - { name: card, zone: field, card_type: creature }\n    effects:\n      - if: { gte: [{ ref: card.power }, 3] }\n        then: [ { set_state: { card: { ref: card }, state: recently_played, value: true } } ]\n\n  - id: pass_turn\n",
    )
}

/// `flex_ruleset_yaml` and a target `where` that also reads power: a tapped creature is a legal
/// target only with a power of at least 3.
fn probe_ruleset_yaml() -> String {
    edit(
        &flex_ruleset_yaml(),
        "where: { eq: [{ ref: target.tapped }, true] }",
        "where: { and: [ { eq: [{ ref: target.tapped }, true] }, { gte: [{ ref: target.power }, 3] } ] }",
    )
}

fn deck(ruleset: &DeclarativeRuleset) -> DeckList {
    load_deck_list(ruleset.definition(), ruleset.catalog(), DECK).expect("the test deck is legal")
}

fn create(ruleset: &DeclarativeRuleset, seed: u64) -> GameCreation {
    let decks = [deck(ruleset), deck(ruleset)];
    ruleset
        .create_game(&decks, Seed(seed))
        .expect("the test decks create a game")
}

/// The first card of `player`'s `zone` whose Card Definition is `definition`.
fn find(state: &GameState, player: u32, zone_id: &str, definition: &str) -> CardInstanceId {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .iter()
        .copied()
        .find(|&card| {
            state
                .card(card)
                .is_some_and(|instance| instance.definition().as_str() == definition)
        })
        .unwrap_or_else(|| panic!("seat {player} has no `{definition}` in `{zone_id}`"))
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

fn flex(player: u32, card: CardInstanceId) -> Action {
    action(player, "flex", &[card])
}

fn tapped() -> StateName {
    StateName::new("tapped")
}

fn recently_played() -> StateName {
    StateName::new("recently_played")
}

fn tap(card: CardInstanceId) -> Event {
    Event::CardStateChanged {
        card,
        state: tapped(),
        value: FieldValue::Bool(true),
    }
}

/// A Game as played: its creation and each accepted Action with its Transition.
struct Played {
    creation: GameCreation,
    steps: Vec<(Action, Transition<GameState, Event>)>,
}

impl Played {
    fn final_state(&self) -> &GameState {
        self.steps
            .last()
            .map_or(&self.creation.opening.state, |(_, step)| &step.state)
    }

    fn last_events(&self) -> &[Event] {
        self.steps
            .last()
            .map_or(&[], |(_, step)| step.trace.events())
    }

    fn all_events(&self) -> Vec<&Event> {
        let creation = self.creation.opening.trace.events().iter();
        let steps = self.steps.iter().flat_map(|(_, step)| step.trace.events());
        creation.chain(steps).collect()
    }
}

/// Plays `actions` from the Opening Position. Every Action must be accepted.
fn play_all(ruleset: &DeclarativeRuleset, creation: GameCreation, actions: &[Action]) -> Played {
    let mut state = creation.opening.state.clone();
    let mut steps = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        let transition = ruleset
            .transition(&state, action)
            .unwrap_or_else(|rejection| {
                panic!(
                    "action {} ({action:?}) is rejected: {rejection:?}",
                    index + 1
                )
            });
        state = transition.state.clone();
        steps.push((action.clone(), transition));
    }
    Played { creation, steps }
}

/// The five-turn duel of the module doc: seat 0's `attacker_definition` attacks seat 1's
/// `victim_definition` once the victim is tapped.
struct Duel {
    played: Played,
    attacker: CardInstanceId,
    victim: CardInstanceId,
}

impl Duel {
    fn victim_died(&self) -> bool {
        self.played.final_state().zone_of(self.victim) == Some(&zone("trash"))
    }
}

fn play_duel(
    ruleset: &DeclarativeRuleset,
    seed: u64,
    attacker_definition: &str,
    victim_definition: &str,
) -> Duel {
    let creation = create(ruleset, seed);
    let opening = &creation.opening.state;
    let attacker = find(opening, 0, "hand", attacker_definition);
    let victim = find(opening, 1, "hand", victim_definition);
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let script = [
        play(0, attacker),
        pass(0),
        play(1, victim),
        pass(1),
        pass(0),
        attack(1, victim, leader_0),
        pass(1),
        attack(0, attacker, victim),
    ];
    Duel {
        played: play_all(ruleset, creation, &script),
        attacker,
        victim,
    }
}

// ---- the criteria

#[test]
fn a_field_modifier_turns_a_losing_combat() {
    let ruleset = effects();
    let duel = play_duel(&ruleset, DUEL_SEED, "battle_veteran", "river_otter");

    // The veteran (power 2) taps to attack and is then power 3: it kills the otter (power 3).
    assert!(duel.victim_died());
    assert_eq!(
        duel.played.last_events(),
        [
            tap(duel.attacker),
            Event::CardMoved {
                card: duel.victim,
                player: p(1),
                from: zone("field"),
                to: zone("trash"),
            }
        ]
    );

    // Nothing in the trace of the whole Game, nor in the stored state, is the bonus: the only
    // instance states there are are the ones the card types declare.
    for event in duel.played.all_events() {
        if let Event::CardStateChanged { state, .. } = event {
            assert!(
                *state == tapped()
                    || *state == recently_played()
                    || state.as_str() == "current_life",
                "unexpected state change {event:?}"
            );
        }
    }
    let stored: Vec<&str> = duel
        .played
        .final_state()
        .card(duel.attacker)
        .expect("the attacker exists")
        .state()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(stored, ["tapped", "recently_played"]);

    // The same Game with no Card Effect: the otter survives, and every earlier trace is identical,
    // so the modifier left no Event behind.
    let plain = effects_without_card_effects();
    let baseline = play_duel(&plain, DUEL_SEED, "battle_veteran", "river_otter");
    assert!(!baseline.victim_died());
    assert_eq!(baseline.played.last_events(), [tap(baseline.attacker)]);
    assert_eq!(
        baseline.played.creation.opening.trace,
        duel.played.creation.opening.trace
    );
    let last = duel.played.steps.len() - 1;
    for (index, (one, other)) in duel.played.steps[..last]
        .iter()
        .zip(&baseline.played.steps[..last])
        .enumerate()
    {
        assert_eq!(one, other, "action {}", index + 1);
    }
}

#[test]
fn reads_give_the_base_value_when_while_is_false_or_outside_the_active_zone() {
    // The condition is false: the untapped veteran is power 2, so `flex` (power at least 3) does
    // nothing.
    let ruleset = ruleset_from(&flex_ruleset_yaml(), &catalog_yaml());
    let creation = create(&ruleset, DUEL_SEED);
    let veteran = find(&creation.opening.state, 0, "hand", "battle_veteran");
    let played = play_all(
        &ruleset,
        creation,
        &[play(0, veteran), pass(0), pass(1), flex(0, veteran)],
    );
    assert_eq!(played.last_events(), []);

    // The card is outside the active zone: the discounter in the hand costs its base 2, so the
    // play taps two mana cards, never one.
    let ruleset = effects();
    let creation = create(&ruleset, DISCOUNT_SEED);
    let discounter = find(&creation.opening.state, 0, "hand", "discounter");
    let played = play_all(&ruleset, creation, &[play(0, discounter)]);
    let taps = played
        .last_events()
        .iter()
        .filter(
            |event| matches!(event, Event::CardStateChanged { state, .. } if *state == tapped()),
        )
        .count();
    assert_eq!(taps, 2);

    // No active zone declared: no modifier is ever active, so the veteran of the first test loses.
    let no_zone = edit(RULESET, "effects:\n  active_zone: field\n", "");
    let ruleset = ruleset_from(&no_zone, &catalog_yaml());
    let duel = play_duel(&ruleset, DUEL_SEED, "battle_veteran", "river_otter");
    assert!(!duel.victim_died());
}

#[test]
fn the_modifier_is_visible_through_every_read_path() {
    // The combat `gte`: the veteran kills the otter only through its derived power.
    let duel = play_duel(&effects(), DUEL_SEED, "battle_veteran", "river_otter");
    assert!(duel.victim_died());

    // A param `where` and an effect `if`, on the probe ruleset (a target must be tapped and have a
    // power of at least 3; `flex` reads power in an `if`).
    let yaml = probe_ruleset_yaml();
    let probe = ruleset_from(&yaml, &catalog_yaml());
    let plain = ruleset_from(&yaml, &catalog_yaml_without_card_effects());
    let creation = create(&probe, DUEL_SEED);
    let opening = creation.opening.state.clone();
    let veteran = find(&opening, 0, "hand", "battle_veteran");
    let otter = find(&opening, 1, "hand", "river_otter");
    let leader_1 = find(&opening, 1, "leader_zone", "captain_ardel");

    // The effect `if`: before the veteran taps its power is 2, after attacking it is 3.
    let script = [
        play(0, veteran),
        pass(0),
        play(1, otter),
        pass(1),
        flex(0, veteran),
        attack(0, veteran, leader_1),
        flex(0, veteran),
        pass(0),
    ];
    let played = play_all(&probe, creation, &script);
    assert_eq!(played.steps[4].1.trace.events(), []);
    assert_eq!(
        played.steps[6].1.trace.events(),
        [Event::CardStateChanged {
            card: veteran,
            state: recently_played(),
            value: FieldValue::Bool(true),
        }]
    );

    // The `where`: the tapped veteran is a legal target of the otter because its derived power is
    // 3, and is listed as one. Without the Card Effect the same position refuses it.
    let state = played.final_state();
    let target = attack(1, otter, veteran);
    assert!(probe.transition(state, &target).is_ok());
    assert!(probe.legal_actions(state).contains(&target));

    let creation = create(&plain, DUEL_SEED);
    let baseline = play_all(&plain, creation, &script[..4]);
    let mut state = baseline.final_state().clone();
    for step in [attack(0, veteran, leader_1), pass(0)] {
        state = plain.transition(&state, &step).expect("accepted").state;
    }
    assert_eq!(
        plain.transition(&state, &target),
        Err(Rejection::NoAlternativeMatched {
            param: ParamName::new("target"),
            card: veteran,
        })
    );
    assert!(!plain.legal_actions(&state).contains(&target));

    // The cost `tap.count`: with the active zone moved to the hand the discounter costs 1, so the
    // play taps one mana card.
    let in_hand = ruleset_from(
        &edit(RULESET, "  active_zone: field", "  active_zone: hand"),
        &catalog_yaml(),
    );
    let creation = create(&in_hand, DISCOUNT_SEED);
    let discounter = find(&creation.opening.state, 0, "hand", "discounter");
    let played = play_all(&in_hand, creation, &[play(0, discounter)]);
    let taps = played
        .last_events()
        .iter()
        .filter(
            |event| matches!(event, Event::CardStateChanged { state, .. } if *state == tapped()),
        )
        .count();
    assert_eq!(taps, 1);
}

#[test]
fn a_negative_modifier_lowers_the_derived_value() {
    // The ox is power 3 and -1 while tapped: power 2 when it attacks. It kills the moss sprite
    // (power 2) and not the otter (power 3), which its base power would kill.
    let ruleset = effects();
    let against_moss = play_duel(&ruleset, DUEL_SEED, "weary_ox", "moss_sprite");
    assert!(against_moss.victim_died());
    let against_otter = play_duel(&ruleset, DUEL_SEED, "weary_ox", "river_otter");
    assert!(!against_otter.victim_died());
    assert_eq!(
        against_otter.played.last_events(),
        [tap(against_otter.attacker)]
    );

    let plain = effects_without_card_effects();
    let base = play_duel(&plain, DUEL_SEED, "weary_ox", "river_otter");
    assert!(base.victim_died());
}

#[test]
fn a_derived_read_overflow_is_a_typed_error() {
    let ruleset = effects();
    let creation = create(&ruleset, OVERFLOW_SEED);
    let opening = creation.opening.state.clone();
    let edge = find(&opening, 0, "hand", "overflow_edge");
    let sum = find(&opening, 0, "hand", "overflow_sum");
    let leader_1 = find(&opening, 1, "leader_zone", "captain_ardel");
    let played = play_all(
        &ruleset,
        creation,
        &[play(0, edge), play(0, sum), pass(0), pass(1)],
    );
    let state = played.final_state().clone();
    let before = state.clone();

    // The base power plus the modifier overflows an int, and so does the sum of two modifiers.
    for card in [edge, sum] {
        assert_eq!(
            ruleset.transition(&state, &attack(0, card, leader_1)),
            Err(Rejection::DerivedValueOverflow {
                card,
                field: FieldName::new("power"),
            })
        );
        // The refused Action changed nothing and emitted nothing.
        assert_eq!(state, before);
    }

    // Listing never panics and offers no attack the engine would refuse; the rest still works.
    let listed = ruleset.legal_actions(&state);
    assert!(listed.contains(&pass(0)));
    for card in [edge, sum] {
        assert!(!listed.contains(&attack(0, card, leader_1)));
    }
    assert!(ruleset.transition(&state, &pass(0)).is_ok());
}

#[test]
fn applying_the_trace_reproduces_the_state_with_modifiers_in_play() {
    let ruleset = effects();
    let duel = play_duel(&ruleset, DUEL_SEED, "battle_veteran", "river_otter");
    let played = &duel.played;
    assert!(duel.victim_died());

    // Applying each trace to the prior state gives the next state, the creation included.
    let creation = &played.creation;
    let mut state = creation
        .empty
        .apply_trace(creation.opening.trace.events())
        .expect("the creation trace applies");
    assert_eq!(state, creation.opening.state);
    for (index, (_, step)) in played.steps.iter().enumerate() {
        state = state
            .apply_trace(step.trace.events())
            .expect("the trace applies");
        assert_eq!(state, step.state, "action {}", index + 1);
    }

    // The modifier is not in the stored state: the veteran holds only its two declared states.
    let stored = played
        .final_state()
        .card(duel.attacker)
        .expect("the attacker exists");
    assert_eq!(
        stored.state(),
        [
            (tapped(), FieldValue::Bool(true)),
            (recently_played(), FieldValue::Bool(false)),
        ]
    );

    // A full GameRecord replays to the same creation, traces and final state.
    let mut record = GameRecord::new(&ruleset, &[deck(&ruleset), deck(&ruleset)], Seed(DUEL_SEED));
    for (action, _) in &played.steps {
        record.push_accepted(action.clone());
    }
    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(&replay.creation, creation);
    let traces: Vec<EventTrace<Event>> = played
        .steps
        .iter()
        .map(|(_, step)| step.trace.clone())
        .collect();
    assert_eq!(replay.traces, traces);
    assert_eq!(&replay.state, played.final_state());
}

#[test]
fn a_while_over_its_own_modified_field_reads_the_base_value() {
    // The giant is power 3 with `while: power >= 3`, `modify: power -1`. The `while` reads the base
    // 3, so it holds and the derived power is 2, always: it kills the moss sprite (power 2) and not
    // the otter (power 3). Read as derived, the condition would depend on its own result.
    let ruleset = effects();
    let against_moss = play_duel(&ruleset, DUEL_SEED, "fading_giant", "moss_sprite");
    assert!(against_moss.victim_died());
    let against_otter = play_duel(&ruleset, DUEL_SEED, "fading_giant", "river_otter");
    assert!(!against_otter.victim_died());

    // Without the Card Effect the giant is power 3 and kills the otter.
    let plain = effects_without_card_effects();
    assert!(play_duel(&plain, DUEL_SEED, "fading_giant", "river_otter").victim_died());
}

#[test]
fn identical_runs_produce_identical_states_and_traces() {
    let ruleset = effects();
    let first = play_duel(&ruleset, DUEL_SEED, "battle_veteran", "river_otter");
    let second = play_duel(&effects(), DUEL_SEED, "battle_veteran", "river_otter");
    assert_eq!(first.played.creation, second.played.creation);
    assert_eq!(first.played.steps, second.played.steps);
    assert_eq!(first.played.final_state(), second.played.final_state());
}

// ---- the active zone in the ruleset file

#[test]
fn the_effects_ruleset_declares_its_active_zone() {
    let definition = load_ruleset_definition(RULESET).expect("the Effects Ruleset loads");
    assert_eq!(definition.effects_active_zone(), Some(&zone("field")));

    // A ruleset with no `effects` block has no active zone.
    let small = load_ruleset_definition(SMALL_RULESET).expect("the Small Ruleset loads");
    assert_eq!(small.effects_active_zone(), None);
}

#[test]
fn the_active_zone_must_be_a_declared_zone() {
    let yaml = edit(RULESET, "  active_zone: field", "  active_zone: graveyard");
    assert_eq!(
        load_ruleset_definition(&yaml),
        Err(LoadError::Definition(DefinitionError {
            path: path().key("effects").key("active_zone"),
            kind: DefinitionErrorKind::DanglingReference {
                target: ReferenceTarget::Zone,
                id: "graveyard".to_string(),
            },
        }))
    );
}

#[test]
fn the_effects_block_needs_its_active_zone_and_knows_no_other_key() {
    let missing = edit(RULESET, "  active_zone: field", "  {}");
    let Err(error) = parse_ruleset_draft(&missing) else {
        panic!("an effects block without an active zone is refused");
    };
    assert!(matches!(error.kind, YamlErrorKind::Malformed { .. }));

    let unknown = edit(
        RULESET,
        "  active_zone: field",
        "  active_zone: field\n  layers: 2",
    );
    let Err(error) = parse_ruleset_draft(&unknown) else {
        panic!("an unknown key of the effects block is refused");
    };
    assert_eq!(
        error.kind,
        YamlErrorKind::UnknownKey {
            key: "layers".to_string()
        }
    );
    assert_eq!(error.path, path().key("effects").key("layers"));
}

// ---- the catalog

#[test]
fn the_real_catalog_carries_the_two_continuous_cards() {
    let ruleset = effects();
    let catalog: &CardCatalog = ruleset.catalog();
    for (id, amount) in [("battle_veteran", 1), ("weary_ox", -1)] {
        let card = catalog
            .cards()
            .iter()
            .find(|card| card.id().as_str() == id)
            .unwrap_or_else(|| panic!("the catalog has {id}"));
        assert_eq!(card.effects().len(), 1, "{id}");
        assert!(
            matches!(
                &card.effects()[0],
                tcg_engine::card_catalog::CardEffect::Continuous(effect)
                    if effect.modify().field().as_str() == "power" && effect.modify().amount() == amount
            ),
            "{id}"
        );
    }
}
