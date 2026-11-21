//! Acceptance tests for ticket #52: Continuous Card Effects over matched sets (ADR-0025), on the whole
//! real Effects Ruleset, its real catalog and a test deck.
//!
//! An aura is a Continuous Card Effect with a Modifier Scope: while its carrier is in `field` and its
//! `while` holds, it adds its amount to the derived read of a field of every card the scope matches.
//! Like a Field Modifier of the carrier alone (ADR-0024) it is never stored and never emitted: the
//! tests observe a read through what it decides (a combat, a trigger's `if`), never through a getter.
//!
//! The auras are in the real catalog: `banner_bearer` (+1 power to the controller's other creatures),
//! `dread_herald` (-1 power to every opponent creature) and `rally_captain` (+1 power to the
//! controller's creatures, itself included). The cards only these tests need are appended to the
//! catalog text: `pit_brute` (power 4), `proud_recruit` (power 2, draws when it enters the field if
//! its power is at least 3), `titan_aura` (an aura of `i64::MAX`), `drill_sergeant` (+1 power to the
//! controller's other creatures while it is tapped), `tithe_collector` (-1 cost to the creatures in
//! its controller's hand), `proud_aura` (power 2; an aura over itself and the other creatures, and a
//! trigger that draws when it enters the field if its power is at least 3), `exit_aura` (the same
//! with a trigger on entering the trash, used in a deck that replaces `titan_aura`) and `frail_leader`
//! (life 1).
//!
//! A combat is decided by `attacker.power >= target.power`, and a Leader has power 2 and loses 1 life
//! when it is hit. So a `goblin_scout` (power 1) hits a Leader only with a bonus of at least 1, a
//! `moss_sprite` (power 2) only with no malus, and a power-1 creature that attacks a creature of
//! power 2 is the read that tells the bonus. A creature taps when it attacks and a target creature
//! must be tapped, so the scenarios follow the same pattern: an opponent's creature attacks the
//! Leader on its Turn and is a target on the next Turn of the other Player.
//!
//! Every scenario is a literal list of Actions from a Seed whose opening hands hold the cards it
//! needs; each test finds those cards in the hands and asserts that it did.

use tcg_engine::card_catalog::{CardEffect, ScopeOwner};
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameRecord,
    GameState, Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{EventTrace, LegalActions, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, FieldName, FieldValue, ParamName, StateName, ZoneId,
};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const RULESET: &str = include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const CATALOG: &str = include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");

const GOLDEN_SNAPSHOT: &str = include_str!("golden/effects-aura-combat.trace");
const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/effects-aura-combat.trace"
);
const REGENERATE_COMMAND: &str =
    "cargo test -p tcg-loader --test set_modifiers -- --ignored regenerate_golden_snapshot";

/// The cards only these tests need, appended to the text of the real catalog.
const TEST_CARDS: &str = "
  - { id: pit_brute, type: creature, fields: { name: \"Pit Brute\", cost: 2, power: 4 } }
  - { id: proud_recruit, type: creature, fields: { name: \"Proud Recruit\", cost: 1, power: 2 }, effects: [ { on: { enter: field }, if: { gte: [ { ref: self.power }, 3 ] }, do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } ] }
  - { id: titan_aura, type: creature, fields: { name: \"Titan Aura\", cost: 1, power: 0 }, effects: [ { while: true, scope: { zone: field, card_type: creature }, modify: { field: power, amount: 9223372036854775807 } } ] }
  - { id: drill_sergeant, type: creature, fields: { name: \"Drill Sergeant\", cost: 1, power: 1 }, effects: [ { while: { eq: [ { ref: self.tapped }, true ] }, scope: { zone: field, card_type: creature }, modify: { field: power, amount: 1 } } ] }
  - { id: tithe_collector, type: creature, fields: { name: \"Tithe Collector\", cost: 1, power: 1 }, effects: [ { while: true, scope: { zone: hand, card_type: creature }, modify: { field: cost, amount: -1 } } ] }
  - { id: proud_aura, type: creature, fields: { name: \"Proud Aura\", cost: 1, power: 2 }, effects: [ { while: true, scope: { zone: field, card_type: creature, include_self: true }, modify: { field: power, amount: 1 } }, { on: { enter: field }, if: { gte: [ { ref: self.power }, 3 ] }, do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } ] }
  - { id: exit_aura, type: creature, fields: { name: \"Exit Aura\", cost: 1, power: 2 }, effects: [ { while: true, scope: { zone: field, card_type: creature, include_self: true }, modify: { field: power, amount: 1 } }, { on: { enter: trash }, if: { gte: [ { ref: self.power }, 3 ] }, do: [ { draw: { player: controller, from: deck, to: hand, count: 1 } } ] } ] }
  - { id: frail_leader, type: leader, fields: { name: \"Frail Leader\", power: 2, life: 1 } }
";

/// The deck of a seat: 1 Leader, 50 creatures (at most 4 of each), 10 mana.
fn deck_yaml(leader: &str) -> String {
    format!(
        "
inputs:
  - input: leader
    cards:
      - {{ card: {leader}, count: 1 }}
  - input: main_deck
    cards:
      - {{ card: banner_bearer, count: 4 }}
      - {{ card: dread_herald, count: 4 }}
      - {{ card: rally_captain, count: 4 }}
      - {{ card: battle_veteran, count: 4 }}
      - {{ card: goblin_scout, count: 4 }}
      - {{ card: moss_sprite, count: 4 }}
      - {{ card: river_otter, count: 4 }}
      - {{ card: ember_hound, count: 1 }}
      - {{ card: pit_brute, count: 3 }}
      - {{ card: proud_recruit, count: 4 }}
      - {{ card: titan_aura, count: 3 }}
      - {{ card: drill_sergeant, count: 4 }}
      - {{ card: tithe_collector, count: 4 }}
      - {{ card: proud_aura, count: 3 }}
  - input: mana_deck
    cards:
      - {{ card: mana, count: 10 }}
"
    )
}

// Seeds whose opening hands hold the cards of each scenario (the seat that needs a card is named).
/// Seat 0: `banner_bearer`, `goblin_scout`. Seat 1: `goblin_scout`.
const BANNER_SEED: u64 = 45;
/// Seat 0: `moss_sprite`, `dread_herald`. Seat 1: `moss_sprite`.
const DREAD_SEED: u64 = 19;
/// Seat 0: two `goblin_scout`, `banner_bearer`. Seat 1: `moss_sprite`. Also the golden Game.
const ENTER_SEED: u64 = 244;
/// Seat 0: `moss_sprite`, `dread_herald`. Seat 1: `moss_sprite`, `river_otter`.
const LEAVE_SEED: u64 = 19;
/// Seat 0: two `banner_bearer`, `goblin_scout`. Seat 1: `river_otter`.
const STACK_SEED: u64 = 771;
/// Seat 0: `battle_veteran`, `banner_bearer`. Seat 1: `pit_brute`.
const COMPOSE_SEED: u64 = 50;
/// Seat 0: `ember_hound`. Seat 1: `moss_sprite`, `dread_herald`.
const LOSING_SEED: u64 = 105;
/// Seat 0: `rally_captain`. Seat 1: `river_otter`.
const RALLY_SEED: u64 = 3;
/// Seat 0: `banner_bearer`, `proud_recruit`.
const TRIGGER_SEED: u64 = 9;
/// Seat 0: two `titan_aura`, `goblin_scout`.
const OVERFLOW_SEED: u64 = 218;
/// Seat 0: two `goblin_scout`, `drill_sergeant`.
const CONDITION_SEED: u64 = 99;
/// Seat 0: `tithe_collector`, `goblin_scout`.
const DISCOUNT_SEED: u64 = 8;
/// Seat 0: `proud_aura`.
const PROUD_SEED: u64 = 2;
/// Seat 0: `river_otter`. Seat 1: `exit_aura` (in a deck that replaces `titan_aura` with it).
const EXIT_SEED: u64 = 20;
/// Three Players. Seat 0: `dread_herald`. Seats 1 and 2: `moss_sprite`.
const THREE_SEED: u64 = 6;

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

/// The catalog text with the Card Effects of the card `id` removed (one card is one line).
fn strip_effects_of(yaml: &str, id: &str) -> String {
    let marker = format!("{{ id: {id},");
    let mut found = 0;
    let stripped = yaml
        .lines()
        .map(|line| {
            if !line.contains(&marker) {
                return line.to_string();
            }
            found += 1;
            let at = line
                .find(", effects: [")
                .unwrap_or_else(|| panic!("`{id}` carries Card Effects on its line"));
            format!("{} }}", &line[..at])
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(found, 1, "`{id}` is one line of the catalog");
    stripped
}

fn catalog_yaml() -> String {
    format!("{CATALOG}\n{TEST_CARDS}")
}

fn ruleset_from(catalog_yaml: &str) -> DeclarativeRuleset {
    ruleset_with(RULESET, catalog_yaml)
}

fn ruleset_with(ruleset_yaml: &str, catalog_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog_yaml).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The real Effects Ruleset with the real catalog and the test cards.
fn effects() -> DeclarativeRuleset {
    ruleset_from(&catalog_yaml())
}

/// The same, but the Card Effects of the card `id` are removed: the baseline an aura is measured
/// against.
fn effects_without(id: &str) -> DeclarativeRuleset {
    ruleset_from(&strip_effects_of(&catalog_yaml(), id))
}

fn decks(ruleset: &DeclarativeRuleset, leader_1: &str) -> [DeckList; 2] {
    let load = |leader: &str| {
        load_deck_list(ruleset.definition(), ruleset.catalog(), &deck_yaml(leader))
            .expect("the test deck is legal")
    };
    [load("captain_ardel"), load(leader_1)]
}

fn create(ruleset: &DeclarativeRuleset, seed: u64, leader_1: &str) -> GameCreation {
    ruleset
        .create_game(&decks(ruleset, leader_1), Seed(seed))
        .expect("the test decks create a game")
}

/// The cards of `player`'s `zone` whose Card Definition is `definition`, in stored order.
fn find_all(
    state: &GameState,
    player: u32,
    zone_id: &str,
    definition: &str,
) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .iter()
        .copied()
        .filter(|&card| {
            state
                .card(card)
                .is_some_and(|instance| instance.definition().as_str() == definition)
        })
        .collect()
}

/// The first card of `player`'s `zone` whose Card Definition is `definition`.
fn find(state: &GameState, player: u32, zone_id: &str, definition: &str) -> CardInstanceId {
    *find_all(state, player, zone_id, definition)
        .first()
        .unwrap_or_else(|| panic!("seat {player} has no `{definition}` in `{zone_id}`"))
}

/// The first and the second card of `player`'s `zone` whose Card Definition is `definition`.
fn find_two(
    state: &GameState,
    player: u32,
    zone_id: &str,
    definition: &str,
) -> (CardInstanceId, CardInstanceId) {
    match find_all(state, player, zone_id, definition)[..] {
        [first, second, ..] => (first, second),
        _ => panic!("seat {player} has fewer than two `{definition}` in `{zone_id}`"),
    }
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
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

fn tap(card: CardInstanceId) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new("tapped"),
        value: FieldValue::Bool(true),
    }
}

/// A hit on a Leader: its `current_life` becomes `life`.
fn hit(leader: CardInstanceId, life: i64) -> Event {
    Event::CardStateChanged {
        card: leader,
        state: StateName::new("current_life"),
        value: FieldValue::Int(life),
    }
}

fn moved(card: CardInstanceId, owner: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card,
        player: p(owner),
        from: zone(from),
        to: zone(to),
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

    /// The Events of the Action at `index`.
    fn events(&self, index: usize) -> &[Event] {
        self.steps[index].1.trace.events()
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

/// Nothing in the Events of a Game, and nothing in a stored card, is a modifier: the only instance
/// states that change are the ones the card types declare.
fn assert_no_event_for_a_bonus(played: &Played) {
    for event in played.all_events() {
        if let Event::CardStateChanged { state, .. } = event {
            assert!(
                ["tapped", "recently_played", "current_life"].contains(&state.as_str()),
                "unexpected state change {event:?}"
            );
        }
    }
}

// ---- the scenarios

/// Seat 0's `banner_bearer` and `goblin_scout`, seat 1's `goblin_scout`: the aura reaches only the
/// controller's other creature.
struct Banner {
    played: Played,
    banner: CardInstanceId,
    goblin: CardInstanceId,
    opposing_goblin: CardInstanceId,
    leader_0: CardInstanceId,
    leader_1: CardInstanceId,
}

fn play_banner(ruleset: &DeclarativeRuleset) -> Banner {
    let creation = create(ruleset, BANNER_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let banner = find(opening, 0, "hand", "banner_bearer");
    let goblin = find(opening, 0, "hand", "goblin_scout");
    let opposing_goblin = find(opening, 1, "hand", "goblin_scout");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let leader_1 = find(opening, 1, "leader_zone", "captain_ardel");
    let script = [
        play(0, banner),
        pass(0),
        play(1, opposing_goblin),
        pass(1),
        play(0, goblin),
        attack(0, banner, leader_1),
        pass(0),
        attack(1, opposing_goblin, leader_0),
        pass(1),
        attack(0, goblin, leader_1),
    ];
    Banner {
        played: play_all(ruleset, creation, &script),
        banner,
        goblin,
        opposing_goblin,
        leader_0,
        leader_1,
    }
}

/// Seat 0's `moss_sprite` and `dread_herald`, seat 1's `moss_sprite`: the opponent-scoped aura
/// lowers the opponent's creature only.
struct Dread {
    played: Played,
    moss: CardInstanceId,
    herald: CardInstanceId,
    opposing_moss: CardInstanceId,
    leader_0: CardInstanceId,
    leader_1: CardInstanceId,
}

fn play_dread(ruleset: &DeclarativeRuleset) -> Dread {
    let creation = create(ruleset, DREAD_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let moss = find(opening, 0, "hand", "moss_sprite");
    let herald = find(opening, 0, "hand", "dread_herald");
    let opposing_moss = find(opening, 1, "hand", "moss_sprite");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let leader_1 = find(opening, 1, "leader_zone", "captain_ardel");
    let script = [
        play(0, moss),
        pass(0),
        play(1, opposing_moss),
        pass(1),
        play(0, herald),
        attack(0, moss, leader_1),
        pass(0),
        attack(1, opposing_moss, leader_0),
        pass(1),
        attack(0, herald, leader_1),
    ];
    Dread {
        played: play_all(ruleset, creation, &script),
        moss,
        herald,
        opposing_moss,
        leader_0,
        leader_1,
    }
}

/// Seat 0's two `goblin_scout` and `banner_bearer`: the first goblin attacks before the aura enters
/// the field and the second after, in the same Turn.
struct Enter {
    played: Played,
    banner: CardInstanceId,
    first: CardInstanceId,
    second: CardInstanceId,
    leader_1: CardInstanceId,
}

fn play_enter(ruleset: &DeclarativeRuleset) -> Enter {
    let creation = create(ruleset, ENTER_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let (first, second) = find_two(opening, 0, "hand", "goblin_scout");
    let banner = find(opening, 0, "hand", "banner_bearer");
    let leader_1 = find(opening, 1, "leader_zone", "captain_ardel");
    let script = [
        play(0, first),
        play(0, second),
        pass(0),
        pass(1),
        attack(0, first, leader_1),
        play(0, banner),
        attack(0, second, leader_1),
    ];
    Enter {
        played: play_all(ruleset, creation, &script),
        banner,
        first,
        second,
        leader_1,
    }
}

/// Seat 0's `moss_sprite` and `dread_herald`, seat 1's `moss_sprite` and `river_otter`. The herald
/// lowers seat 1's creatures by 1. On Turn 6 seat 1's otter (power 3, so 2 under the herald) attacks
/// the tapped herald (power 2), and then seat 1's moss sprite (power 2, so 1 under the herald)
/// attacks seat 0's tapped moss sprite (power 2). With `kill_the_herald: false` the otter attacks
/// the Leader instead.
struct Leave {
    played: Played,
    herald: CardInstanceId,
    moss: CardInstanceId,
    opposing_moss: CardInstanceId,
    leader_0: CardInstanceId,
}

fn play_leave(ruleset: &DeclarativeRuleset, kill_the_herald: bool) -> Leave {
    let creation = create(ruleset, LEAVE_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let moss = find(opening, 0, "hand", "moss_sprite");
    let herald = find(opening, 0, "hand", "dread_herald");
    let opposing_moss = find(opening, 1, "hand", "moss_sprite");
    let otter = find(opening, 1, "hand", "river_otter");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let leader_1 = find(opening, 1, "leader_zone", "captain_ardel");
    let otter_target = if kill_the_herald { herald } else { leader_0 };
    let script = [
        play(0, moss),
        pass(0),
        play(1, opposing_moss),
        pass(1),
        play(0, herald),
        attack(0, moss, leader_1),
        pass(0),
        play(1, otter),
        pass(1),
        attack(0, moss, leader_1),
        attack(0, herald, leader_1),
        pass(0),
        attack(1, otter, otter_target),
        attack(1, opposing_moss, moss),
    ];
    Leave {
        played: play_all(ruleset, creation, &script),
        herald,
        moss,
        opposing_moss,
        leader_0,
    }
}

/// Seat 0's two `banner_bearer` and `goblin_scout`, seat 1's `river_otter` (power 3): the goblin is
/// power 1 and the bonuses are +2, the banner bearers +1 from each other.
struct Stack {
    played: Played,
    first_banner: CardInstanceId,
    second_banner: CardInstanceId,
    goblin: CardInstanceId,
    otter: CardInstanceId,
    leader_1: CardInstanceId,
}

fn play_stack(ruleset: &DeclarativeRuleset) -> Stack {
    let creation = create(ruleset, STACK_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let (first_banner, second_banner) = find_two(opening, 0, "hand", "banner_bearer");
    let goblin = find(opening, 0, "hand", "goblin_scout");
    let otter = find(opening, 1, "hand", "river_otter");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let leader_1 = find(opening, 1, "leader_zone", "captain_ardel");
    let script = [
        play(0, first_banner),
        pass(0),
        play(1, otter),
        pass(1),
        play(0, second_banner),
        play(0, goblin),
        pass(0),
        attack(1, otter, leader_0),
        pass(1),
        attack(0, first_banner, otter),
        attack(0, second_banner, leader_1),
        attack(0, goblin, otter),
    ];
    Stack {
        played: play_all(ruleset, creation, &script),
        first_banner,
        second_banner,
        goblin,
        otter,
        leader_1,
    }
}

/// Seat 0's `battle_veteran` (power 2, +1 while tapped) under one `banner_bearer`, against seat 1's
/// tapped `pit_brute` (power 4).
struct Compose {
    played: Played,
    veteran: CardInstanceId,
    brute: CardInstanceId,
}

fn play_compose(ruleset: &DeclarativeRuleset) -> Compose {
    let creation = create(ruleset, COMPOSE_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let veteran = find(opening, 0, "hand", "battle_veteran");
    let banner = find(opening, 0, "hand", "banner_bearer");
    let brute = find(opening, 1, "hand", "pit_brute");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let script = [
        play(0, banner),
        pass(0),
        play(1, brute),
        pass(1),
        play(0, veteran),
        pass(0),
        attack(1, brute, leader_0),
        pass(1),
        attack(0, veteran, brute),
    ];
    Compose {
        played: play_all(ruleset, creation, &script),
        veteran,
        brute,
    }
}

/// Seat 0's `ember_hound` (power 2) attacks seat 1's tapped `moss_sprite` (power 2) while seat 1's
/// `dread_herald` is in the field.
struct Losing {
    played: Played,
    hound: CardInstanceId,
    moss: CardInstanceId,
}

fn play_losing(ruleset: &DeclarativeRuleset) -> Losing {
    let creation = create(ruleset, LOSING_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let hound = find(opening, 0, "hand", "ember_hound");
    let moss = find(opening, 1, "hand", "moss_sprite");
    let herald = find(opening, 1, "hand", "dread_herald");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let script = [
        play(0, hound),
        pass(0),
        play(1, moss),
        pass(1),
        pass(0),
        attack(1, moss, leader_0),
        play(1, herald),
        pass(1),
        attack(0, hound, moss),
    ];
    Losing {
        played: play_all(ruleset, creation, &script),
        hound,
        moss,
    }
}

/// Seat 0's `rally_captain` (power 2, +1 to itself too) attacks seat 1's tapped `river_otter`
/// (power 3).
struct Rally {
    played: Played,
    captain: CardInstanceId,
    otter: CardInstanceId,
}

fn play_rally(ruleset: &DeclarativeRuleset) -> Rally {
    let creation = create(ruleset, RALLY_SEED, "captain_ardel");
    let opening = &creation.opening.state;
    let captain = find(opening, 0, "hand", "rally_captain");
    let otter = find(opening, 1, "hand", "river_otter");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let script = [
        pass(0),
        play(1, otter),
        pass(1),
        play(0, captain),
        pass(0),
        attack(1, otter, leader_0),
        pass(1),
        attack(0, captain, otter),
    ];
    Rally {
        played: play_all(ruleset, creation, &script),
        captain,
        otter,
    }
}

/// The golden Game: seat 1 has a `frail_leader` (life 1). Seat 0 plays a `banner_bearer` and two
/// `goblin_scout`; seat 1 plays a `moss_sprite`, which attacks the Leader and is tapped. Then the
/// first goblin (power 2 under the banner) attacks the moss sprite (power 2) and the second one
/// attacks the Leader (power 2): the Game ends. Without the aura neither attack does anything.
struct Aura {
    played: Played,
    moss: CardInstanceId,
    first: CardInstanceId,
    second: CardInstanceId,
    leader_1: CardInstanceId,
}

fn aura_script(
    opening: &GameState,
) -> (
    Vec<Action>,
    CardInstanceId,
    CardInstanceId,
    CardInstanceId,
    CardInstanceId,
) {
    let banner = find(opening, 0, "hand", "banner_bearer");
    let (first, second) = find_two(opening, 0, "hand", "goblin_scout");
    let moss = find(opening, 1, "hand", "moss_sprite");
    let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
    let leader_1 = find(opening, 1, "leader_zone", "frail_leader");
    let script = vec![
        play(0, banner),
        pass(0),
        play(1, moss),
        pass(1),
        play(0, first),
        play(0, second),
        pass(0),
        attack(1, moss, leader_0),
        pass(1),
        attack(0, first, moss),
        attack(0, second, leader_1),
    ];
    (script, moss, first, second, leader_1)
}

fn play_aura(ruleset: &DeclarativeRuleset) -> Aura {
    let creation = create(ruleset, ENTER_SEED, "frail_leader");
    let (script, moss, first, second, leader_1) = aura_script(&creation.opening.state);
    Aura {
        played: play_all(ruleset, creation, &script),
        moss,
        first,
        second,
        leader_1,
    }
}

/// The text of a Golden Snapshot (ADR-0017), in the format of the Effects Ruleset's snapshots.
fn render(played: &Played) -> String {
    let mut text = format!(
        "# effects-aura-combat | seed {ENTER_SEED} | seat 0 deck of the aura tests, seat 1 the same with a frail leader\n== creation\n"
    );
    for event in played.creation.opening.trace.events() {
        text.push_str(&format!("{event:?}\n"));
    }
    for (index, (action, step)) in played.steps.iter().enumerate() {
        let Action::Declared {
            player,
            action: id,
            arguments,
        } = action
        else {
            panic!("a Golden Game has no Answer");
        };
        let cards: Vec<String> = arguments
            .iter()
            .map(|argument| match argument {
                Argument::Card(card) => card.0.to_string(),
            })
            .collect();
        text.push_str(&format!(
            "== action {}: player {} {} [{}]\n",
            index + 1,
            player.0,
            id.as_str(),
            cards.join(", ")
        ));
        for event in step.trace.events() {
            text.push_str(&format!("{event:?}\n"));
        }
    }
    let outcome = played
        .final_state()
        .outcome()
        .expect("a Golden Game ends the Game");
    text.push_str(&format!("== outcome\n{outcome:?}\n"));
    text
}

// ---- the criteria

#[test]
fn an_aura_raises_the_controllers_other_creatures_only() {
    let ruleset = effects();
    let banner = play_banner(&ruleset);
    let played = &banner.played;

    // The carrier reads exactly its base power 1: attacking a Leader (power 2) it does nothing.
    assert_eq!(played.events(5), [tap(banner.banner)]);
    // The opponent's creature of the same type reads exactly its base power 1.
    assert_eq!(played.events(7), [tap(banner.opposing_goblin)]);
    // The controller's other creature reads base + 1 = 2 and hits the Leader.
    assert_eq!(
        played.events(9),
        [tap(banner.goblin), hit(banner.leader_1, 4)]
    );
    assert_ne!(banner.leader_0, banner.leader_1);

    // The bonus is no Event and no stored state: the Game up to the last Action is the Game
    // without the aura, step by step, and the last Action is what differs.
    assert_no_event_for_a_bonus(played);
    let plain = play_banner_without_aura();
    assert_eq!(played.creation.opening.trace, plain.creation.opening.trace);
    for (index, (one, other)) in played.steps[..9].iter().zip(&plain.steps[..9]).enumerate() {
        assert_eq!(one, other, "action {}", index + 1);
    }
    assert_eq!(plain.events(9), [tap(banner.goblin)]);
    let stored: Vec<&str> = played
        .final_state()
        .card(banner.banner)
        .expect("the carrier exists")
        .state()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(stored, ["tapped", "recently_played"]);
}

/// The scenario of `play_banner` with the Card Effects of `banner_bearer` removed.
fn play_banner_without_aura() -> Played {
    play_banner(&effects_without("banner_bearer")).played
}

#[test]
fn an_opponent_scoped_aura_lowers_the_opponents_creatures() {
    let ruleset = effects();
    let dread = play_dread(&ruleset);
    let played = &dread.played;

    // The controller's own creature reads its base power 2 and hits the Leader, and so does the
    // carrier, which no scope reaches.
    assert_eq!(played.events(5), [tap(dread.moss), hit(dread.leader_1, 4)]);
    assert_eq!(
        played.events(9),
        [tap(dread.herald), hit(dread.leader_1, 3)]
    );
    // The opponent's creature reads 2 - 1 = 1 and does not hit the Leader (power 2).
    assert_eq!(played.events(7), [tap(dread.opposing_moss)]);
    assert_ne!(dread.leader_0, dread.leader_1);

    // Without the aura the opponent's creature hits.
    let plain = play_dread(&effects_without("dread_herald")).played;
    assert_eq!(
        plain.events(7),
        [tap(dread.opposing_moss), hit(dread.leader_0, 4)]
    );
}

#[test]
fn a_set_field_modifier_follows_its_carrier_out_of_the_active_zone() {
    let ruleset = effects();

    // Entering: the first goblin (power 1) attacks before the aura is in the field and does nothing.
    // The Event of the aura entering the field is in the Transition of its play, and the second
    // goblin, which attacks next, reads the bonus and hits.
    let enter = play_enter(&ruleset);
    let played = &enter.played;
    assert_eq!(played.events(4), [tap(enter.first)]);
    assert!(played
        .events(5)
        .contains(&moved(enter.banner, 0, "hand", "field")));
    assert_eq!(
        played.events(6),
        [tap(enter.second), hit(enter.leader_1, 4)]
    );

    // Leaving: the herald dies in combat. In the Transition whose Events hold its `CardMoved` out of
    // `field`, the opposing moss sprite stops being lowered: the next attack of that moss sprite,
    // which reads power 2 again, kills the tapped moss sprite of seat 0. With the herald alive it
    // reads 1 and does not.
    let leave = play_leave(&ruleset, true);
    let played = &leave.played;
    assert_eq!(
        played.events(12),
        [
            tap(played_otter(played)),
            moved(leave.herald, 0, "field", "trash")
        ]
    );
    assert_eq!(
        played.events(13),
        [
            tap(leave.opposing_moss),
            moved(leave.moss, 0, "field", "trash")
        ]
    );
    let alive = play_leave(&ruleset, false);
    assert_eq!(
        alive.played.events(12),
        [tap(played_otter(&alive.played)), hit(alive.leader_0, 4)]
    );
    assert_eq!(alive.played.events(13), [tap(alive.opposing_moss)]);
    assert_eq!(
        alive.played.final_state().zone_of(alive.herald),
        Some(&zone("field"))
    );
    assert_eq!(
        alive.played.final_state().zone_of(alive.moss),
        Some(&zone("field"))
    );
}

/// The card seat 1 attacks with at Action 13 of `play_leave`: the attacker of that Action.
fn played_otter(played: &Played) -> CardInstanceId {
    let Action::Declared { arguments, .. } = &played.steps[12].0 else {
        panic!("Action 13 is a Declared Action");
    };
    match arguments[0] {
        Argument::Card(card) => card,
    }
}

#[test]
fn two_copies_of_an_aura_stack_additively() {
    let ruleset = effects();
    let stack = play_stack(&ruleset);
    let played = &stack.played;

    // Each banner bearer is reached by the other copy only: power 1 + 1 = 2. It hits a Leader
    // (power 2) and does not kill the otter (power 3).
    assert_eq!(played.events(9), [tap(stack.first_banner)]);
    assert_eq!(
        played.events(10),
        [tap(stack.second_banner), hit(stack.leader_1, 4)]
    );
    // The goblin is reached by both: power 1 + 2 = 3 kills the otter.
    assert_eq!(
        played.events(11),
        [tap(stack.goblin), moved(stack.otter, 1, "field", "trash")]
    );
}

#[test]
fn self_and_set_field_modifiers_compose_on_one_card() {
    let ruleset = effects();
    let compose = play_compose(&ruleset);

    // The tapped veteran reads 2 (base) + 1 (its own modifier) + 1 (the aura) = 4 and kills the
    // power-4 brute.
    assert_eq!(
        compose.played.events(8),
        [
            tap(compose.veteran),
            moved(compose.brute, 1, "field", "trash")
        ]
    );

    // Without the aura it reads 3 and does not.
    let plain = play_compose(&effects_without("banner_bearer"));
    assert_eq!(plain.played.events(8), [tap(compose.veteran)]);
}

#[test]
fn a_set_field_modifier_turns_a_losing_combat() {
    let ruleset = effects();
    let losing = play_losing(&ruleset);

    // The herald lowers the hound to power 1: it does not kill the moss sprite (power 2).
    assert_eq!(losing.played.events(8), [tap(losing.hound)]);
    assert_eq!(
        losing.played.final_state().zone_of(losing.moss),
        Some(&zone("field"))
    );

    // The same Actions with no aura: the hound (power 2) kills it.
    let plain = play_losing(&effects_without("dread_herald"));
    assert_eq!(
        plain.played.events(8),
        [tap(losing.hound), moved(losing.moss, 1, "field", "trash")]
    );
    assert_eq!(
        plain.played.final_state().zone_of(losing.moss),
        Some(&zone("trash"))
    );
    assert_ne!(losing.played.events(8), plain.played.events(8));
}

#[test]
fn golden_effects_aura_combat() {
    let played = play_aura(&effects());
    let actual = render(&played.played);
    if actual != GOLDEN_SNAPSHOT {
        let mut expected_lines = GOLDEN_SNAPSHOT.lines();
        let mut actual_lines = actual.lines();
        for line_number in 1.. {
            let (expected, found) = (expected_lines.next(), actual_lines.next());
            assert!(
                expected == found,
                "the Golden Game effects-aura-combat differs from its snapshot at line \
                 {line_number}\n  expected: {}\n  actual:   {}\nIf the change is deliberate, \
                 rewrite the snapshot with:\n  {REGENERATE_COMMAND}",
                expected.unwrap_or("<end of the snapshot>"),
                found.unwrap_or("<end of the trace>"),
            );
            if expected.is_none() {
                break;
            }
        }
        panic!(
            "the snapshot differs in its line endings; rewrite it with:\n  {REGENERATE_COMMAND}"
        );
    }
    assert!(GOLDEN_SNAPSHOT.ends_with('\n') && !GOLDEN_SNAPSHOT.ends_with("\n\n"));
}

/// Rewrites the Golden Snapshot. Run it on purpose, and read the diff before committing it.
#[test]
#[ignore = "deliberate update: rewrites the golden snapshot"]
fn regenerate_golden_snapshot() {
    let played = play_aura(&effects());
    std::fs::write(GOLDEN_PATH, render(&played.played)).expect("the snapshot is written");
}

#[test]
fn the_aura_changes_the_golden_games_combat_outcome() {
    let aura = play_aura(&effects());
    let played = &aura.played;
    // The first goblin kills the tapped moss sprite and the second one hits the frail Leader: seat 0
    // wins.
    assert_eq!(
        played.events(9),
        [tap(aura.first), moved(aura.moss, 1, "field", "trash")]
    );
    assert_eq!(
        played.events(10)[..2],
        [tap(aura.second), hit(aura.leader_1, 0)]
    );
    let outcome = played.final_state().outcome().expect("the Game ended");
    assert_eq!(outcome.winners(), [p(0)]);

    // The same Seed, decks and Actions with the aura's `effects` stripped: the moss sprite survives
    // and the Leader is not hit, so the Game goes on.
    let plain = play_aura(&effects_without("banner_bearer"));
    assert_eq!(plain.played.events(9), [tap(aura.first)]);
    assert_eq!(plain.played.events(10), [tap(aura.second)]);
    assert_eq!(plain.played.final_state().outcome(), None);
    assert_eq!(
        plain.played.final_state().zone_of(aura.moss),
        Some(&zone("field"))
    );
}

#[test]
fn applying_the_trace_reproduces_the_state_with_auras_in_play() {
    let played = play_aura(&effects()).played;

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
    assert_no_event_for_a_bonus(&played);
}

#[test]
fn replaying_the_record_of_a_game_with_auras_is_exact() {
    let ruleset = effects();
    let aura = play_aura(&ruleset);
    let played = &aura.played;

    let mut record = GameRecord::new(&ruleset, &decks(&ruleset, "frail_leader"), Seed(ENTER_SEED));
    for (action, _) in &played.steps {
        record.push_accepted(action.clone());
    }
    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(&replay.creation, &played.creation);
    let traces: Vec<EventTrace<Event>> = played
        .steps
        .iter()
        .map(|(_, step)| step.trace.clone())
        .collect();
    assert_eq!(replay.traces, traces);
    assert_eq!(&replay.state, played.final_state());
}

#[test]
fn a_trigger_condition_reads_a_set_modified_field_at_fire_time() {
    let ruleset = effects();
    let creation = create(&ruleset, TRIGGER_SEED, "captain_ardel");
    let opening = creation.opening.state.clone();
    let banner = find(&opening, 0, "hand", "banner_bearer");
    let recruit = find(&opening, 0, "hand", "proud_recruit");

    // The recruit is power 2 and its `if` wants 3: with the banner bearer in the field it enters the
    // field reading 3, so its trigger fires and draws a card, in the Transition of the play.
    let with_aura = play_all(
        &ruleset,
        creation,
        &[play(0, banner), pass(0), pass(1), play(0, recruit)],
    );
    let draws = |events: &[Event]| {
        events
            .iter()
            .filter(|event| {
                matches!(event, Event::CardMoved { player, from, to, .. }
                    if *player == p(0) && *from == zone("deck") && *to == zone("hand"))
            })
            .count()
    };
    assert_eq!(draws(with_aura.events(3)), 1);
    assert!(with_aura
        .events(3)
        .contains(&moved(recruit, 0, "hand", "field")));

    // The same Actions with no banner bearer in the field: the condition is false, nothing is drawn.
    let creation = create(&ruleset, TRIGGER_SEED, "captain_ardel");
    let without_aura = play_all(&ruleset, creation, &[pass(0), pass(1), play(0, recruit)]);
    assert_eq!(draws(without_aura.events(2)), 0);
    assert!(without_aura
        .events(2)
        .contains(&moved(recruit, 0, "hand", "field")));
}

#[test]
fn every_scope_form_the_catalog_accepts_acts() {
    let ruleset = effects();

    // The three forms are in the real catalog, and the scope reads as written.
    let scope_of = |id: &str| {
        let card = ruleset
            .catalog()
            .cards()
            .iter()
            .find(|card| card.id().as_str() == id)
            .unwrap_or_else(|| panic!("the catalog has {id}"));
        match card.effects() {
            [CardEffect::Continuous(effect)] => effect
                .scope()
                .unwrap_or_else(|| panic!("{id} has a scope"))
                .clone(),
            other => panic!("{id} has one continuous Card Effect, not {other:?}"),
        }
    };
    let banner = scope_of("banner_bearer");
    assert_eq!(banner.zone(), &zone("field"));
    assert_eq!(banner.card_type().as_str(), "creature");
    assert_eq!(banner.owner(), ScopeOwner::Controller);
    assert!(!banner.include_self());
    let dread = scope_of("dread_herald");
    assert_eq!(dread.owner(), ScopeOwner::Opponent);
    assert!(!dread.include_self());
    let rally = scope_of("rally_captain");
    assert_eq!(rally.owner(), ScopeOwner::Controller);
    assert!(rally.include_self());

    // Each form changes a read. The controller form and the opponent form are decided above; the
    // `include_self` form is decided here: the captain reads 2 + 1 = 3 and kills the otter (power 3).
    let banner = play_banner(&ruleset);
    assert_eq!(banner.played.events(9).len(), 2);
    let dread = play_dread(&ruleset);
    assert_eq!(dread.played.events(7), [tap(dread.opposing_moss)]);
    let rally = play_rally(&ruleset);
    assert_eq!(
        rally.played.events(7),
        [tap(rally.captain), moved(rally.otter, 1, "field", "trash")]
    );
    let plain = play_rally(&effects_without("rally_captain"));
    assert_eq!(plain.played.events(7), [tap(rally.captain)]);

    // An explicit `include_self: false` is the absent key.
    let explicit = CATALOG.replace(
        "scope: { zone: field, card_type: creature }, modify: { field: power, amount: 1 } }",
        "scope: { zone: field, card_type: creature, include_self: false }, modify: { field: power, amount: 1 } }",
    );
    assert_ne!(explicit, CATALOG);
    let explicit = ruleset_from(&format!("{explicit}\n{TEST_CARDS}"));
    assert_eq!(explicit.catalog(), ruleset.catalog());
}

#[test]
fn a_set_field_modifier_overflow_is_a_typed_error() {
    let ruleset = effects();
    let creation = create(&ruleset, OVERFLOW_SEED, "captain_ardel");
    let opening = creation.opening.state.clone();
    let (first, second) = find_two(&opening, 0, "hand", "titan_aura");
    let goblin = find(&opening, 0, "hand", "goblin_scout");
    let leader_1 = find(&opening, 1, "leader_zone", "captain_ardel");
    let played = play_all(
        &ruleset,
        creation,
        &[play(0, first), play(0, goblin), pass(0), pass(1)],
    );
    let overflow = Err(Rejection::DerivedValueOverflow {
        card: goblin,
        field: FieldName::new("power"),
    });

    // One aura: the goblin's base 1 plus i64::MAX overflows.
    let state = played.final_state().clone();
    let before = state.clone();
    assert_eq!(
        ruleset.transition(&state, &attack(0, goblin, leader_1)),
        overflow
    );
    assert_eq!(state, before);

    // Two auras: the sum of the two modifiers overflows. The error names the card being read, not a
    // carrier.
    let state = ruleset
        .transition(&state, &play(0, second))
        .expect("the second aura is played")
        .state;
    let before = state.clone();
    assert_eq!(
        ruleset.transition(&state, &attack(0, goblin, leader_1)),
        overflow
    );
    assert_eq!(state, before);

    // Listing never panics and offers no attack the engine would refuse; the rest still works.
    let listed = ruleset.legal_actions(&state);
    assert!(!listed.contains(&attack(0, goblin, leader_1)));
    assert!(listed.contains(&pass(0)));
    assert!(ruleset.transition(&state, &pass(0)).is_ok());
}

#[test]
fn identical_runs_produce_identical_states_and_traces() {
    let first = play_aura(&effects());
    let second = play_aura(&effects());
    assert_eq!(first.played.creation, second.played.creation);
    assert_eq!(first.played.steps, second.played.steps);
    assert_eq!(first.played.final_state(), second.played.final_state());

    let first = play_stack(&effects());
    let second = play_stack(&effects());
    assert_eq!(first.played.steps, second.played.steps);
    assert_eq!(first.played.final_state(), second.played.final_state());
}

#[test]
fn a_set_field_modifier_is_active_only_while_its_condition_holds() {
    let ruleset = effects();
    let creation = create(&ruleset, CONDITION_SEED, "captain_ardel");
    let opening = creation.opening.state.clone();
    let (first, second) = find_two(&opening, 0, "hand", "goblin_scout");
    let sergeant = find(&opening, 0, "hand", "drill_sergeant");
    let leader_1 = find(&opening, 1, "leader_zone", "captain_ardel");

    // The sergeant's `while` is that it is tapped. While it is untapped the first goblin reads its
    // base power 1 and does not hit the Leader; once the sergeant has attacked and is tapped the
    // second goblin reads 2 and does.
    let played = play_all(
        &ruleset,
        creation,
        &[
            play(0, first),
            play(0, second),
            pass(0),
            pass(1),
            play(0, sergeant),
            attack(0, first, leader_1),
            pass(0),
            pass(1),
            attack(0, sergeant, leader_1),
            attack(0, second, leader_1),
        ],
    );
    assert_eq!(played.events(5), [tap(first)]);
    // The carrier is not reached by its own scope, so it is still power 1 and does not hit either.
    assert_eq!(played.events(8), [tap(sergeant)]);
    assert_eq!(played.events(9), [tap(second), hit(leader_1, 4)]);
}

#[test]
fn a_scope_reaches_the_zone_it_names_and_not_only_the_active_zone() {
    // The carrier must be in `field` (the Active Zone); the targets are where the scope says. The
    // tithe collector lowers by 1 the cost of the creatures in its controller's hand: with it in the
    // field the goblin (cost 1) is played for no mana at all.
    let play_goblin = |ruleset: &DeclarativeRuleset| {
        let creation = create(ruleset, DISCOUNT_SEED, "captain_ardel");
        let opening = &creation.opening.state;
        let collector = find(opening, 0, "hand", "tithe_collector");
        let goblin = find(opening, 0, "hand", "goblin_scout");
        let played = play_all(ruleset, creation, &[play(0, collector), play(0, goblin)]);
        (played, goblin)
    };
    let mana_taps = |events: &[Event]| {
        events
            .iter()
            .filter(|event| {
                matches!(event, Event::CardStateChanged { state, .. } if state.as_str() == "tapped")
            })
            .count()
    };
    let (played, goblin) = play_goblin(&effects());
    assert_eq!(mana_taps(played.events(0)), 1);
    assert_eq!(mana_taps(played.events(1)), 0);
    assert!(played
        .events(1)
        .contains(&moved(goblin, 0, "hand", "field")));

    let (plain, _) = play_goblin(&effects_without("tithe_collector"));
    assert_eq!(mana_taps(plain.events(1)), 1);

    // The scope's zone is a filter on the target: with `zone: field` the collector reaches the
    // creatures in the field and not the goblin in the hand, which costs its base 1.
    let in_field = catalog_yaml().replace(
        "scope: { zone: hand, card_type: creature }",
        "scope: { zone: field, card_type: creature }",
    );
    assert_ne!(in_field, catalog_yaml());
    let (field_scoped, _) = play_goblin(&ruleset_from(&in_field));
    assert_eq!(mana_taps(field_scoped.events(1)), 1);
}

#[test]
fn a_carrier_entering_the_active_zone_changes_a_read_in_the_same_transition() {
    // `proud_aura` is power 2 and carries two Card Effects: an aura over itself and the other
    // creatures (+1 power) and a trigger that draws when it enters the field if its power is at
    // least 3. Playing it moves it into `field` and, in the same Transition, fires the trigger: the
    // `if` reads 3 only because the card's own aura became active on that move.
    let play_it = |ruleset: &DeclarativeRuleset| {
        let creation = create(ruleset, PROUD_SEED, "captain_ardel");
        let proud = find(&creation.opening.state, 0, "hand", "proud_aura");
        (play_all(ruleset, creation, &[play(0, proud)]), proud)
    };
    let draws = |events: &[Event]| {
        events
            .iter()
            .filter(|event| {
                matches!(event, Event::CardMoved { player, from, to, .. }
                    if *player == p(0) && *from == zone("deck") && *to == zone("hand"))
            })
            .count()
    };

    let (played, proud) = play_it(&effects());
    let events = played.events(0);
    assert!(events.contains(&moved(proud, 0, "hand", "field")));
    assert_eq!(draws(events), 1);

    // Without the continuous element the card is power 2, the condition is false, and the same play
    // draws nothing.
    let stripped = catalog_yaml().replace(
        "effects: [ { while: true, scope: { zone: field, card_type: creature, include_self: true }, modify: { field: power, amount: 1 } }, { on: { enter: field }",
        "effects: [ { on: { enter: field }",
    );
    assert_ne!(stripped, catalog_yaml());
    let (plain, proud) = play_it(&ruleset_from(&stripped));
    assert!(plain.events(0).contains(&moved(proud, 0, "hand", "field")));
    assert_eq!(draws(plain.events(0)), 0);
}

#[test]
fn a_set_field_modifier_is_inactive_without_an_active_zone() {
    // A ruleset with no `effects:` block has no Active Zone, so no carrier is ever active: the reads
    // are the base values, and the Game is the Game without the aura.
    let no_zone = RULESET.replace("effects:\n  active_zone: field\n", "");
    assert_ne!(no_zone, RULESET);
    let ruleset = ruleset_with(&no_zone, &catalog_yaml());
    let banner = play_banner(&ruleset);
    let plain = play_banner(&effects_without("banner_bearer"));
    assert_eq!(banner.played.events(9), [tap(banner.goblin)]);
    assert_eq!(banner.played.steps, plain.played.steps);
    assert_eq!(banner.played.final_state(), plain.played.final_state());
}

#[test]
fn a_carrier_leaving_the_active_zone_changes_a_read_in_the_same_transition() {
    // `exit_aura` is power 2 and carries two Card Effects: an aura over itself and the other
    // creatures (+1 power, so power 3 in the field) and a trigger that draws when it enters the trash
    // if its power is at least 3. It dies in combat: the Transition that holds its `CardMoved` out of
    // `field` also fires the trigger, and the `if` reads after the move, when the aura is off, so it
    // reads 2 and nothing is drawn. (A read before the move would see 3.)
    let deck = deck_yaml("captain_ardel")
        .replace("card: titan_aura, count: 3", "card: exit_aura, count: 3");
    let run = |ruleset: &DeclarativeRuleset| {
        let decks = [
            load_deck_list(ruleset.definition(), ruleset.catalog(), &deck).expect("legal"),
            load_deck_list(ruleset.definition(), ruleset.catalog(), &deck).expect("legal"),
        ];
        let creation = ruleset
            .create_game(&decks, Seed(EXIT_SEED))
            .expect("the decks create a game");
        let opening = &creation.opening.state;
        let otter = find(opening, 0, "hand", "river_otter");
        let carrier = find(opening, 1, "hand", "exit_aura");
        let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
        let script = [
            play(0, otter),
            pass(0),
            play(1, carrier),
            pass(1),
            pass(0),
            attack(1, carrier, leader_0),
            pass(1),
            attack(0, otter, carrier),
        ];
        (play_all(ruleset, creation, &script), otter, carrier)
    };
    let draws = |events: &[Event]| {
        events
            .iter()
            .filter(|event| {
                matches!(event, Event::CardMoved { player, from, to, .. }
                    if *player == p(1) && *from == zone("deck") && *to == zone("hand"))
            })
            .count()
    };

    // The aura is on while the card is in the field: the otter (power 3) is needed to kill it. After
    // its death the card reads power 2, so the trigger does not fire.
    let (played, otter, carrier) = run(&ruleset_from(&catalog_yaml()));
    assert_eq!(
        played.events(7),
        [tap(otter), moved(carrier, 1, "field", "trash")]
    );
    assert_eq!(draws(played.events(7)), 0);

    // A card of base power 3 with no aura reads 3 in the trash, and the same trigger fires in the
    // same Transition as its move.
    let base_three = catalog_yaml().replace(
        "{ id: exit_aura, type: creature, fields: { name: \"Exit Aura\", cost: 1, power: 2 }, effects: [ { while: true, scope: { zone: field, card_type: creature, include_self: true }, modify: { field: power, amount: 1 } }, ",
        "{ id: exit_aura, type: creature, fields: { name: \"Exit Aura\", cost: 1, power: 3 }, effects: [ ",
    );
    assert_ne!(base_three, catalog_yaml());
    let (played, _, carrier) = run(&ruleset_from(&base_three));
    assert!(played
        .events(7)
        .contains(&moved(carrier, 1, "field", "trash")));
    assert_eq!(draws(played.events(7)), 1);
}

/// The catalog text without the card `mind_leech`, whose `decider: opponent` is defined for exactly
/// 2 Players (ADR-0033, ADR-0034) and so does not load in the 3-Player ruleset of the test below.
fn without_mind_leech(yaml: &str) -> String {
    let start = yaml
        .find("  - id: mind_leech\n")
        .expect("the real catalog defines `mind_leech`");
    let end = yaml[start..]
        .find("\n\n")
        .map_or(yaml.len(), |length| start + length + 1);
    format!("{}{}", &yaml[..start], &yaml[end..])
}

#[test]
fn owner_opponent_reaches_every_player_other_than_the_carriers_owner() {
    // Three Players: seat 0's `dread_herald` lowers the creatures of seat 1 and of seat 2, not only
    // "the other" Player. Each opposing moss sprite (power 2) reads 1 and misses the Leader (power 2);
    // without the aura both hit.
    let three_players = RULESET.replace("players: 2", "players: 3");
    assert_ne!(three_players, RULESET);
    let run = |catalog_yaml: &str| {
        let ruleset = ruleset_with(&three_players, &without_mind_leech(catalog_yaml));
        let deck = load_deck_list(
            ruleset.definition(),
            ruleset.catalog(),
            &deck_yaml("captain_ardel"),
        )
        .expect("the test deck is legal");
        let creation = ruleset
            .create_game(&[deck.clone(), deck.clone(), deck], Seed(THREE_SEED))
            .expect("the decks create a game");
        let opening = &creation.opening.state;
        let herald = find(opening, 0, "hand", "dread_herald");
        let moss_1 = find(opening, 1, "hand", "moss_sprite");
        let moss_2 = find(opening, 2, "hand", "moss_sprite");
        let leader_0 = find(opening, 0, "leader_zone", "captain_ardel");
        let script = [
            pass(0),
            play(1, moss_1),
            pass(1),
            play(2, moss_2),
            pass(2),
            play(0, herald),
            pass(0),
            attack(1, moss_1, leader_0),
            pass(1),
            attack(2, moss_2, leader_0),
        ];
        (
            play_all(&ruleset, creation, &script),
            moss_1,
            moss_2,
            leader_0,
        )
    };

    let (played, moss_1, moss_2, _) = run(&catalog_yaml());
    assert_eq!(played.events(7), [tap(moss_1)]);
    assert_eq!(played.events(9), [tap(moss_2)]);

    let (plain, moss_1, moss_2, leader_0) = run(&strip_effects_of(&catalog_yaml(), "dread_herald"));
    assert_eq!(plain.events(7), [tap(moss_1), hit(leader_0, 4)]);
    assert_eq!(plain.events(9), [tap(moss_2), hit(leader_0, 3)]);
}

#[test]
fn an_opponent_scoped_aura_with_include_self_does_not_reach_its_carrier() {
    // `owner: opponent` with `include_self: true` is accepted and inert: the carrier is never in the
    // opponent relation to itself. Every read is what the same scenario gives without `include_self`,
    // the carrier's own read included.
    let with_include_self = catalog_yaml().replace(
        "scope: { zone: field, card_type: creature, owner: opponent }",
        "scope: { zone: field, card_type: creature, owner: opponent, include_self: true }",
    );
    assert_ne!(with_include_self, catalog_yaml());
    let ruleset = ruleset_from(&with_include_self);
    let catalog_text_scope = ruleset
        .catalog()
        .cards()
        .iter()
        .find(|card| card.id().as_str() == "dread_herald")
        .and_then(|card| match card.effects() {
            [CardEffect::Continuous(effect)] => effect.scope().cloned(),
            _ => None,
        })
        .expect("dread_herald has a scope");
    assert_eq!(catalog_text_scope.owner(), ScopeOwner::Opponent);
    assert!(catalog_text_scope.include_self());

    let inert = play_dread(&ruleset);
    let plain = play_dread(&effects());
    // The carrier (power 2) still hits the Leader, the controller's moss sprite does, and the
    // opponent's is still lowered: the whole Game is the same as without `include_self`.
    assert_eq!(
        inert.played.events(9),
        [tap(inert.herald), hit(inert.leader_1, 3)]
    );
    assert_eq!(inert.played.events(7), [tap(inert.opposing_moss)]);
    assert_eq!(inert.played.steps, plain.played.steps);
    assert_eq!(inert.played.final_state(), plain.played.final_state());
}

// ---- an overflowing `where` in a `one_of` alternative (ticket #74, ADR-0032)
//
// The `where` of a param alternative reads a derived value, so it can overflow like any other read.
// With several alternatives, an alternative whose `where` overflows does not fit and binding tries
// the next; with one, the overflow is the Rejection. The scenario is the overflow Game of
// `a_set_field_modifier_overflow_is_a_typed_error`: a `titan_aura` and a goblin on the field, so
// the goblin's `power` read overflows. `attack` is replaced by a test-only action whose effects read
// no derived value, so that only the `where` can overflow.

/// An `attack` of the real params shape: `attacker` is an untapped creature that was not recently
/// played (and passes `attacker_extra`, a line of the `and`), `target` is a Leader of an opponent
/// that fits one of the two alternatives `first` and `second`. Its effects are the tap and the hit.
fn overflow_ruleset(attacker_extra: &str, first: &str, second: &str) -> DeclarativeRuleset {
    let action = format!(
        "  - id: attack
    params:
      - name: attacker
        zone: field
        card_type: creature
        where:
          and:
            - {{ eq: [{{ ref: attacker.tapped }}, false] }}
            - {{ eq: [{{ ref: attacker.recently_played }}, false] }}
            - {attacker_extra}
      - name: target
        one_of:
          - {first}
          - {second}
    effects:
      - set_state: {{ card: {{ ref: attacker }}, state: tapped, value: true }}
      - add_state: {{ card: {{ ref: target }}, state: current_life, amount: -1 }}

"
    );
    let from = RULESET
        .find("  - id: attack\n")
        .expect("the ruleset has `attack`");
    let to = RULESET
        .find("  - id: pass_turn")
        .expect("the ruleset has `pass_turn`");
    let yaml = format!("{}{action}{}", &RULESET[..from], &RULESET[to..]);
    ruleset_with(&yaml, &catalog_yaml())
}

const OVERFLOWS: &str = "{ gte: [{ ref: attacker.power }, 0] }";
const ALWAYS: &str = "{ eq: [true, true] }";
const LEADER: &str = "zone: leader_zone, card_type: leader, owner: opponent";

/// The overflow Game at the Turn where the goblin attacks: its power is `1 + i64::MAX`.
fn overflow_scenario(ruleset: &DeclarativeRuleset) -> (GameState, CardInstanceId, CardInstanceId) {
    let creation = create(ruleset, OVERFLOW_SEED, "captain_ardel");
    let opening = creation.opening.state.clone();
    let aura = find(&opening, 0, "hand", "titan_aura");
    let goblin = find(&opening, 0, "hand", "goblin_scout");
    let leader_1 = find(&opening, 1, "leader_zone", "captain_ardel");
    let played = play_all(
        ruleset,
        creation,
        &[play(0, aura), play(0, goblin), pass(0), pass(1)],
    );
    (played.final_state().clone(), goblin, leader_1)
}

#[test]
fn an_overflowing_where_in_one_alternative_tries_the_next_alternative() {
    // The first alternative's `where` overflows on the goblin's power; the second fits the Leader.
    let ruleset = overflow_ruleset(
        ALWAYS,
        &format!("{{ {LEADER}, where: {OVERFLOWS} }}"),
        &format!("{{ {LEADER} }}"),
    );
    let (state, goblin, leader_1) = overflow_scenario(&ruleset);
    let before = state.clone();

    let attack = attack(0, goblin, leader_1);
    let accepted = ruleset
        .transition(&state, &attack)
        .expect("the Leader fits the second alternative");
    assert_eq!(accepted.trace.events(), [tap(goblin), hit(leader_1, 4)]);
    assert_eq!(state, before);

    // Listing goes through the same binding.
    assert!(ruleset.legal_actions(&state).contains(&attack));
}

#[test]
fn an_overflowing_where_with_no_alternative_fitting_is_no_alternative_matched() {
    // The first overflows and the second's `where` does not hold: the usual Rejection, not the
    // overflow, and nothing changed.
    let ruleset = overflow_ruleset(
        ALWAYS,
        &format!("{{ {LEADER}, where: {OVERFLOWS} }}"),
        &format!("{{ {LEADER}, where: {{ eq: [{{ ref: attacker.recently_played }}, true] }} }}"),
    );
    let (state, goblin, leader_1) = overflow_scenario(&ruleset);
    let before = state.clone();

    let attack = attack(0, goblin, leader_1);
    assert_eq!(
        ruleset.transition(&state, &attack),
        Err(Rejection::NoAlternativeMatched {
            param: ParamName::new("target"),
            card: leader_1,
        })
    );
    assert_eq!(state, before);
    assert!(!ruleset.legal_actions(&state).contains(&attack));
}

#[test]
fn a_single_alternative_param_whose_where_overflows_is_still_derived_value_overflow() {
    // `attacker` has one alternative, and its `where` reads the goblin's power: the overflow is the
    // Rejection, as before, whatever `target` has.
    let ruleset = overflow_ruleset(
        OVERFLOWS,
        &format!("{{ {LEADER} }}"),
        &format!("{{ {LEADER} }}"),
    );
    let (state, goblin, leader_1) = overflow_scenario(&ruleset);
    let before = state.clone();

    let attack = attack(0, goblin, leader_1);
    assert_eq!(
        ruleset.transition(&state, &attack),
        Err(Rejection::DerivedValueOverflow {
            card: goblin,
            field: FieldName::new("power"),
        })
    );
    assert_eq!(state, before);
    assert!(!ruleset.legal_actions(&state).contains(&attack));
}
