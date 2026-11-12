//! Acceptance tests for ticket #27: listing the Legal Actions of a GameState of the declarative
//! Ruleset (ADR-0018), on the whole real Small Ruleset, the real catalog and the real sample decks.
//!
//! A Legal Action is exactly an Action `transition` accepts. The oracle of these tests is brute
//! force: `brute_force` tries every Player of the Player Order, every declared action, every arity
//! up to two (no Small Ruleset action has more params) and every tuple of Card Instance Ids, and
//! keeps the Actions `transition` accepts. It never calls `legal_actions`.
//!
//! A few tests edit the text of the catalog (a cost, a Leader's life) or of the ruleset (a `where`,
//! a phase's `actions`, the whole `turn`), and say so.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameState, Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, FieldName, FieldValue, PhaseId, StateName, ZoneId};
use tcg_engine::runtime::Runtime;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// With this Seed seat 0 opens with, in hand order: `goblin_scout` (card 1, cost 1, power 1),
/// `river_otter` (13, cost 2, power 3), `moss_sprite` (7, cost 1, power 2), `thorn_boar` (35, cost
/// 4) and `ash_drake` (43, cost 5); seat 1 holds `moss_sprite` (63), `sun_knight` (104),
/// `stone_golem` (75), `river_otter` (72) and `ash_drake` (101) and its Leader is card 61. The hand
/// order differs from the order of the ids on purpose. Each test asserts the facts it relies on.
const SEED: u64 = 9;

const GOBLIN_0: CardInstanceId = CardInstanceId(1);
const MOSS_0: CardInstanceId = CardInstanceId(7);
const OTTER_0: CardInstanceId = CardInstanceId(13);
const BOAR_0: CardInstanceId = CardInstanceId(35);
const DRAKE_0: CardInstanceId = CardInstanceId(43);
const MOSS_1: CardInstanceId = CardInstanceId(63);
const OTTER_1: CardInstanceId = CardInstanceId(72);
const LEADER_1: CardInstanceId = CardInstanceId(61);

/// The second `thorn_boar` of seat 0, which it draws at the start of Turn 3.
const DRAWN_BOAR_0: CardInstanceId = CardInstanceId(34);

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
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

fn ruleset_from(ruleset_yaml: &str, catalog_yaml: &str) -> DeclarativeRuleset {
    let definition = load_ruleset_definition(ruleset_yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, catalog_yaml).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The real Small Ruleset and the real catalog.
fn small() -> DeclarativeRuleset {
    ruleset_from(SMALL_RULESET, SMALL_CATALOG)
}

/// The real Small Ruleset with the ruleset text edited.
fn small_with_ruleset(from: &str, to: &str) -> DeclarativeRuleset {
    ruleset_from(&edit(SMALL_RULESET, from, to), SMALL_CATALOG)
}

/// The real Small Ruleset with the catalog text edited.
fn small_with_catalog(from: &str, to: &str) -> DeclarativeRuleset {
    ruleset_from(SMALL_RULESET, &edit(SMALL_CATALOG, from, to))
}

/// The Deck Lists of the seats, each one of the real sample decks.
fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    [DECK_A, DECK_B]
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

/// Deck A for seat 0 and deck B for seat 1.
fn opening(ruleset: &DeclarativeRuleset) -> GameState {
    ruleset
        .create_game(&decks(ruleset), Seed(SEED))
        .expect("the sample decks create a game")
        .opening
        .state
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

/// Accepts every Action in turn, each on the state the previous one produced.
fn accept_all(ruleset: &DeclarativeRuleset, state: &GameState, actions: &[Action]) -> GameState {
    let mut state = state.clone();
    for action in actions {
        state = ruleset
            .transition(&state, action)
            .expect("the Action is accepted")
            .state;
    }
    state
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

fn cost_of(ruleset: &DeclarativeRuleset, state: &GameState, card: CardInstanceId) -> i64 {
    let instance = state.card(card).expect("the card exists");
    let definition = ruleset
        .catalog()
        .card(instance.definition())
        .expect("the card is in the catalog");
    match definition.field(&FieldName::new("cost")) {
        Some(FieldValue::Int(value)) => *value,
        other => panic!("expected an int `cost`, got {other:?}"),
    }
}

/// Applies Events that set a scenario up; each must be accepted.
fn apply_all(state: &mut GameState, events: &[Event]) {
    for event in events {
        state.apply(event).expect("the scenario Event is accepted");
    }
}

/// The Event of a creature becoming tapped.
fn tapped(card: CardInstanceId) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new("tapped"),
        value: FieldValue::Bool(true),
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

/// Turn 3 of the Game, with seat 0 to act. Turn 1: seat 0 plays `goblin_scout` and `moss_sprite`
/// (2 mana) and passes. Turn 2: seat 1 plays `moss_sprite`; the scenario then moves `river_otter`
/// onto its field and taps it, so seat 1 has a tapped and an untapped creature (and no Action of
/// its own is needed). Seat 0 has 4 mana, two ready attackers (1 and 7), `river_otter` (cost 2),
/// two `thorn_boar` (cost 4) it can play, and `ash_drake` (cost 5) it cannot.
fn mid_game(ruleset: &DeclarativeRuleset) -> GameState {
    let mut state = accept_all(
        ruleset,
        &opening(ruleset),
        &[
            play(0, GOBLIN_0),
            play(0, MOSS_0),
            pass(0),
            play(1, MOSS_1),
            pass(1),
        ],
    );
    apply_all(
        &mut state,
        &[moved(OTTER_1, 1, "hand", "field"), tapped(OTTER_1)],
    );
    state
}

/// The Turn of seat 1: Turn 3 passed with seat 0's `moss_sprite` tapped by an attack on the Leader
/// of seat 1, so seat 1 has a ready `moss_sprite` and a tapped enemy creature.
fn seat_1_turn(ruleset: &DeclarativeRuleset) -> GameState {
    let turn_3 = accept_all(
        ruleset,
        &opening(ruleset),
        &[play(0, MOSS_0), pass(0), play(1, MOSS_1), pass(1)],
    );
    accept_all(ruleset, &turn_3, &[attack(0, MOSS_0, LEADER_1), pass(0)])
}

/// Every Action `transition` accepts, found by trying every Player of the Player Order, every
/// declared action, every arity from 0 to 2 and every tuple of Card Instance Ids of the state.
/// In definition order of the actions, then by arity and ascending ids: not the Listing Order.
fn brute_force(ruleset: &DeclarativeRuleset, state: &GameState) -> Vec<Action> {
    let ids: Vec<CardInstanceId> = state.cards().iter().map(|card| card.id()).collect();
    let mut accepted = Vec::new();
    for &player in state.player_order().players() {
        for declared in ruleset.definition().actions() {
            let mut tuples: Vec<Vec<CardInstanceId>> = vec![vec![]];
            tuples.extend(ids.iter().map(|&first| vec![first]));
            for &first in &ids {
                tuples.extend(ids.iter().map(|&second| vec![first, second]));
            }
            for tuple in tuples {
                let candidate = Action {
                    player,
                    action: declared.id().clone(),
                    arguments: tuple.into_iter().map(Argument::Card).collect(),
                };
                if ruleset.transition(state, &candidate).is_ok() {
                    accepted.push(candidate);
                }
            }
        }
    }
    accepted
}

/// The Actions of `list` that are of the action `id`.
fn of_action<'a>(list: &'a [Action], id: &str) -> Vec<&'a Action> {
    list.iter()
        .filter(|action| action.action == ActionId::new(id))
        .collect()
}

/// Asserts that `listed` and `expected` are the same set: equal lengths, each in the other and no
/// duplicate.
fn assert_same_set(listed: &[Action], expected: &[Action], context: &str) {
    assert_eq!(
        listed.len(),
        expected.len(),
        "{context}: listed {} Actions, expected {}",
        listed.len(),
        expected.len()
    );
    for (index, action) in listed.iter().enumerate() {
        assert!(
            expected.contains(action),
            "{context}: listed but not accepted: {action:?}"
        );
        assert!(
            !listed[..index].contains(action),
            "{context}: listed twice: {action:?}"
        );
    }
    for action in expected {
        assert!(
            listed.contains(action),
            "{context}: accepted but not listed: {action:?}"
        );
    }
}

/// One ruleset with one GameState to list from.
struct Scenario {
    name: &'static str,
    ruleset: DeclarativeRuleset,
    state: GameState,
}

/// The ruleset where an attack on a creature also needs the attacker's power to be at least the
/// target's: the `where` of the target reads the earlier param `attacker`.
fn power_gated_attack() -> DeclarativeRuleset {
    small_with_ruleset(
        "where: { eq: [{ ref: target.tapped }, true] } }",
        "where: { and: [ { eq: [{ ref: target.tapped }, true] }, { gte: [{ ref: attacker.power }, { ref: target.power }] } ] } }",
    )
}

/// The ruleset where attacking a Leader adds 1 to its life (instead of taking 1), with a Leader of
/// seat 1 at the top of what an int holds.
fn overflowing_leader() -> DeclarativeRuleset {
    let ruleset = edit(SMALL_RULESET, "amount: -1", "amount: 1");
    let catalog = edit(
        SMALL_CATALOG,
        "life: 6 } }",
        "life: 9223372036854775807 } }",
    );
    ruleset_from(&ruleset, &catalog)
}

/// Seat 0 plays `goblin_scout` and `moss_sprite` on Turn 1: no mana is left and nothing can attack,
/// so only `pass_turn` is legal.
fn only_pass(ruleset: &DeclarativeRuleset) -> GameState {
    accept_all(
        ruleset,
        &opening(ruleset),
        &[play(0, GOBLIN_0), play(0, MOSS_0)],
    )
}

/// The states every soundness and completeness test runs over.
fn scenarios() -> Vec<Scenario> {
    let canonical = small();
    let gated = power_gated_attack();
    let overflowing = overflowing_leader();
    let mut gated_state = mid_game(&gated);
    apply_all(&mut gated_state, &[tapped(MOSS_1)]);
    let overflowing_state = mid_game(&overflowing);
    vec![
        Scenario {
            name: "the Opening Position",
            state: opening(&canonical),
            ruleset: small(),
        },
        Scenario {
            name: "mid-game, seat 0 to act",
            state: mid_game(&canonical),
            ruleset: small(),
        },
        Scenario {
            name: "the Turn of seat 1",
            state: seat_1_turn(&canonical),
            ruleset: small(),
        },
        Scenario {
            name: "nothing but pass_turn",
            state: only_pass(&canonical),
            ruleset: small(),
        },
        Scenario {
            name: "mid-game, attacks gated on the power of an earlier param",
            state: gated_state,
            ruleset: gated,
        },
        Scenario {
            name: "mid-game, attacks on a Leader that would overflow",
            state: overflowing_state,
            ruleset: overflowing,
        },
    ]
}

// ---- built from the Action Definitions

#[test]
fn legal_actions_fill_every_param_of_every_action_of_the_phase() {
    let ruleset = small();
    let state = mid_game(&ruleset);

    // The facts the expected list relies on.
    assert_eq!(state.active_player(), p(0));
    assert_eq!(cards_in(&state, 0, "field"), vec![GOBLIN_0, MOSS_0]);
    assert_eq!(
        cards_in(&state, 0, "hand"),
        vec![OTTER_0, BOAR_0, DRAKE_0, DRAWN_BOAR_0]
    );
    assert_eq!(cards_in(&state, 0, "mana_zone").len(), 4);
    assert_eq!(
        cards_in(&state, 0, "hand")
            .iter()
            .map(|&card| cost_of(&ruleset, &state, card))
            .collect::<Vec<_>>(),
        vec![2, 4, 5, 4]
    );
    assert_eq!(cards_in(&state, 1, "field"), vec![MOSS_1, OTTER_1]);
    assert_eq!(cards_in(&state, 1, "leader_zone"), vec![LEADER_1]);

    assert_eq!(
        ruleset.legal_actions(&state),
        vec![
            play(0, OTTER_0),
            play(0, DRAWN_BOAR_0),
            play(0, BOAR_0),
            attack(0, GOBLIN_0, LEADER_1),
            attack(0, GOBLIN_0, OTTER_1),
            attack(0, MOSS_0, LEADER_1),
            attack(0, MOSS_0, OTTER_1),
            pass(0),
        ]
    );
}

// ---- conditions hold

#[test]
fn a_card_that_fails_a_where_condition_is_not_listed() {
    let ruleset = small();
    // The goblin attacks (and is tapped), then `river_otter` is played (and was just played).
    let state = accept_all(
        &ruleset,
        &mid_game(&ruleset),
        &[attack(0, GOBLIN_0, LEADER_1), play(0, OTTER_0)],
    );
    let listed = ruleset.legal_actions(&state);

    let attacks = of_action(&listed, "attack");
    assert!(!attacks.is_empty(), "the moss sprite can still attack");
    for attack in &attacks {
        assert_eq!(
            attack.arguments[0],
            Argument::Card(MOSS_0),
            "only the ready creature attacks: {attack:?}"
        );
    }
    assert!(!listed.contains(&attack(0, GOBLIN_0, LEADER_1)), "tapped");
    assert!(!listed.contains(&attack(0, GOBLIN_0, OTTER_1)), "tapped");
    assert!(
        !listed.contains(&attack(0, OTTER_0, LEADER_1)),
        "just played"
    );
    assert!(
        !listed.contains(&attack(0, OTTER_0, OTTER_1)),
        "just played"
    );

    // The untapped enemy creature and the own cards are never a target.
    assert!(
        !listed.contains(&attack(0, MOSS_0, MOSS_1)),
        "untapped enemy"
    );
    for own in [GOBLIN_0, MOSS_0, OTTER_0] {
        assert!(!listed.contains(&attack(0, MOSS_0, own)), "an own card");
    }
    assert_eq!(
        attacks
            .iter()
            .map(|attack| attack.arguments[1])
            .collect::<Vec<_>>(),
        vec![Argument::Card(LEADER_1), Argument::Card(OTTER_1)],
        "the tapped enemy creature and the Leader are the only targets"
    );
}

// ---- costs can be paid

#[test]
fn a_creature_the_player_cannot_pay_for_is_not_listed() {
    let ruleset = small();
    let plays = |ruleset: &DeclarativeRuleset, state: &GameState| -> Vec<Action> {
        ruleset
            .legal_actions(state)
            .into_iter()
            .filter(|action| action.action == ActionId::new("play_creature"))
            .collect()
    };

    // Opening position: 2 mana, so exactly the hand creatures of cost 2 or less, by id.
    let start = opening(&ruleset);
    assert_eq!(cards_in(&start, 0, "mana_zone").len(), 2);
    let affordable: Vec<CardInstanceId> = {
        let mut hand = cards_in(&start, 0, "hand");
        hand.sort();
        hand.into_iter()
            .filter(|&card| cost_of(&ruleset, &start, card) <= 2)
            .collect()
    };
    assert_eq!(affordable, vec![GOBLIN_0, MOSS_0, OTTER_0]);
    assert_eq!(
        plays(&ruleset, &start),
        vec![play(0, GOBLIN_0), play(0, MOSS_0), play(0, OTTER_0)]
    );

    // After a play taps the mana, the others disappear: 1 mana is left, then 0.
    let after_one = accept_all(&ruleset, &start, &[play(0, GOBLIN_0)]);
    assert_eq!(plays(&ruleset, &after_one), vec![play(0, MOSS_0)]);
    let after_two = accept_all(&ruleset, &start, &[play(0, OTTER_0)]);
    assert_eq!(plays(&ruleset, &after_two), Vec::<Action>::new());
    let all_tapped = only_pass(&ruleset);
    assert_eq!(ruleset.legal_actions(&all_tapped), vec![pass(0)]);

    // A cost of 0 (an edited catalog) is listed with all mana tapped.
    let free = small_with_catalog("\"Ash Drake\",    cost: 5", "\"Ash Drake\",    cost: 0");
    let free_state = only_pass(&free);
    assert_eq!(cost_of(&free, &free_state, DRAKE_0), 0);
    assert_eq!(
        free.legal_actions(&free_state),
        vec![play(0, DRAKE_0), pass(0)]
    );

    // A negative cost is never listed, with all the mana untapped or not.
    let negative = small_with_catalog("\"Ash Drake\",    cost: 5", "\"Ash Drake\",    cost: -1");
    let negative_state = opening(&negative);
    assert_eq!(cost_of(&negative, &negative_state, DRAKE_0), -1);
    assert_eq!(
        plays(&negative, &negative_state),
        vec![play(0, GOBLIN_0), play(0, MOSS_0), play(0, OTTER_0)]
    );
    assert_eq!(
        plays(&negative, &only_pass(&negative)),
        Vec::<Action>::new()
    );
}

// ---- earlier params

#[test]
fn a_where_condition_that_reads_an_earlier_param_filters_per_binding() {
    let gated = power_gated_attack();
    let mut state = mid_game(&gated);
    // Both enemy creatures are tapped: `moss_sprite` (power 2) and `river_otter` (power 3). The
    // goblin has power 1 and the moss sprite 2.
    apply_all(&mut state, &[tapped(MOSS_1)]);

    let listed = gated.legal_actions(&state);
    assert_eq!(
        of_action(&listed, "attack")
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            attack(0, GOBLIN_0, LEADER_1),
            attack(0, MOSS_0, LEADER_1),
            attack(0, MOSS_0, MOSS_1),
        ],
        "only the pairs where the attacker is at least as strong as the creature, and the Leader"
    );
    assert_same_set(&listed, &brute_force(&gated, &state), "the gated ruleset");

    // Without the extra condition the same state lists every pair.
    let canonical = small();
    let all = canonical.legal_actions(&state);
    assert_eq!(of_action(&all, "attack").len(), 6);
    assert!(all.contains(&attack(0, GOBLIN_0, OTTER_1)));
    assert!(!listed.contains(&attack(0, GOBLIN_0, OTTER_1)));
}

// ---- soundness and completeness

#[test]
fn every_listed_action_is_accepted() {
    for scenario in scenarios() {
        let Scenario {
            name,
            ruleset,
            state,
        } = scenario;
        let listed = ruleset.legal_actions(&state);
        assert!(!listed.is_empty(), "{name}: pass_turn is always legal");
        for action in &listed {
            assert!(
                ruleset.transition(&state, action).is_ok(),
                "{name}: listed but rejected: {action:?}"
            );
            assert_eq!(
                action.player,
                state.active_player(),
                "{name}: every listed Action is the Active Player's"
            );
        }
    }
}

#[test]
fn every_accepted_action_is_listed() {
    for scenario in scenarios() {
        let Scenario {
            name,
            ruleset,
            state,
        } = scenario;
        let oracle = brute_force(&ruleset, &state);
        assert!(!oracle.is_empty(), "{name}: the oracle finds pass_turn");
        assert_same_set(&ruleset.legal_actions(&state), &oracle, name);
    }
}

// ---- effects that fail

#[test]
fn an_action_whose_effects_would_be_rejected_is_not_listed() {
    let ruleset = overflowing_leader();
    let state = mid_game(&ruleset);
    let doomed = attack(0, GOBLIN_0, LEADER_1);

    // The attack is a legal pick for its params and costs, but its effect overflows the life.
    assert_eq!(
        ruleset.transition(&state, &doomed),
        Err(Rejection::StateOverflow {
            card: LEADER_1,
            state: StateName::new("current_life"),
        })
    );
    let listed = ruleset.legal_actions(&state);
    assert!(!listed.contains(&doomed));
    assert!(!listed.contains(&attack(0, MOSS_0, LEADER_1)));
    // An attack on the tapped creature has no such effect and stays.
    assert!(listed.contains(&attack(0, GOBLIN_0, OTTER_1)));
    assert!(listed.contains(&attack(0, MOSS_0, OTTER_1)));
    assert_same_set(
        &listed,
        &brute_force(&ruleset, &state),
        "the overflowing Leader",
    );
}

// ---- order

#[test]
fn legal_actions_are_listed_in_phase_order_then_by_card_instance_id() {
    let ruleset = small();

    // The opening: the hand order is 1, 13, 7, ..., the listing is by id.
    let start = opening(&ruleset);
    assert_eq!(
        cards_in(&start, 0, "hand")[..3],
        [GOBLIN_0, OTTER_0, MOSS_0]
    );
    assert_eq!(
        ruleset.legal_actions(&start),
        vec![
            play(0, GOBLIN_0),
            play(0, MOSS_0),
            play(0, OTTER_0),
            pass(0)
        ]
    );

    // Mid-game: the plays by id, then each attacker by id with its targets by id. The Leader (61)
    // comes before the creature (72) although its alternative is listed second.
    let mid = mid_game(&ruleset);
    assert_eq!(
        ruleset.legal_actions(&mid),
        vec![
            play(0, OTTER_0),
            play(0, DRAWN_BOAR_0),
            play(0, BOAR_0),
            attack(0, GOBLIN_0, LEADER_1),
            attack(0, GOBLIN_0, OTTER_1),
            attack(0, MOSS_0, LEADER_1),
            attack(0, MOSS_0, OTTER_1),
            pass(0),
        ]
    );

    // The phase lists `pass_turn, attack, play_creature`: that is the order of the listing.
    let reordered = small_with_ruleset(
        "actions: [play_creature, attack, pass_turn]",
        "actions: [pass_turn, attack, play_creature]",
    );
    let reordered_state = mid_game(&reordered);
    assert_eq!(
        reordered.legal_actions(&reordered_state),
        vec![
            pass(0),
            attack(0, GOBLIN_0, LEADER_1),
            attack(0, GOBLIN_0, OTTER_1),
            attack(0, MOSS_0, LEADER_1),
            attack(0, MOSS_0, OTTER_1),
            play(0, OTTER_0),
            play(0, DRAWN_BOAR_0),
            play(0, BOAR_0),
        ]
    );

    // The listing is stable and changes nothing.
    let before = mid.clone();
    let first = ruleset.legal_actions(&mid);
    let second = ruleset.legal_actions(&mid);
    assert_eq!(first, second);
    assert_eq!(mid, before, "listing does not change the GameState");

    // The order is the ruleset's, not an accident of one instance: a second ruleset, loaded on
    // its own, lists the same Vec, and so does the Runtime around a ruleset.
    let reloaded = small();
    assert_eq!(reloaded.legal_actions(&mid), first);
    assert_eq!(Runtime::new(small()).legal_actions(&mid), first);
}

#[test]
fn a_listing_whose_candidates_come_from_several_zones_is_in_listing_order_along_whole_games() {
    // Listing visits only the cards of the zones a param names (ADR-0021). Along two whole Games
    // the listing is, at every GameState, the brute-force oracle sorted into Listing Order: the
    // actions in the order of the phase's list, each one's argument lists by ascending Card
    // Instance Ids. The `attack` targets come from the field and from the leader zone, and a hand
    // card and a field card interleave by id.
    let ruleset = small();
    let decks = decks(&ruleset);
    let phase_order = ["play_creature", "attack", "pass_turn"];
    let mut states_with_attacks = 0;
    for seed in [3_u64, 4] {
        let mut state = ruleset
            .create_game(&decks, Seed(seed))
            .expect("the sample decks create a game")
            .opening
            .state;
        for step in 0..40_usize {
            if state.outcome().is_some() {
                break;
            }
            let listed = ruleset.legal_actions(&state);
            let mut oracle = brute_force(&ruleset, &state);
            oracle.sort_by_key(|action| {
                let cards: Vec<CardInstanceId> = action
                    .arguments
                    .iter()
                    .map(|argument| {
                        let Argument::Card(card) = argument;
                        *card
                    })
                    .collect();
                (
                    phase_order
                        .iter()
                        .position(|id| ActionId::new(*id) == action.action),
                    cards,
                )
            });
            assert_eq!(listed, oracle, "seed {seed}, step {step}");
            if !of_action(&listed, "attack").is_empty() {
                states_with_attacks += 1;
            }
            // Advance along a path that mixes the kinds of Action.
            let pick = (step * 5 + usize::try_from(seed).expect("a small Seed")) % listed.len();
            state = ruleset
                .transition(&state, &listed[pick])
                .expect("a listed Action is accepted")
                .state;
        }
    }
    assert!(
        states_with_attacks > 0,
        "the walk reaches states with attacks"
    );
}

// ---- the phase's list

#[test]
fn an_action_the_phase_does_not_list_is_not_listed() {
    let ruleset = small_with_ruleset(
        "actions: [play_creature, attack, pass_turn]",
        "actions: [play_creature, pass_turn]",
    );
    let state = mid_game(&ruleset);

    // Attackers exist: `attack` is declared, and the real ruleset lists it for this state.
    assert!(small()
        .legal_actions(&state)
        .contains(&attack(0, GOBLIN_0, LEADER_1)));

    let listed = ruleset.legal_actions(&state);
    assert_eq!(of_action(&listed, "attack").len(), 0);
    assert_eq!(
        listed,
        vec![
            play(0, OTTER_0),
            play(0, DRAWN_BOAR_0),
            play(0, BOAR_0),
            pass(0)
        ]
    );
    assert_same_set(&listed, &brute_force(&ruleset, &state), "the shorter phase");
}

// ---- the end of the Game

/// The Small Ruleset without a `turn`: its text from `turn:` up to the `actions:` section left out.
fn without_a_turn() -> DeclarativeRuleset {
    let start = SMALL_RULESET
        .find("\nturn:")
        .expect("the Small Ruleset has a turn");
    let end = SMALL_RULESET
        .find("\nactions:")
        .expect("the Small Ruleset has an actions section");
    assert_eq!(SMALL_RULESET.matches("\nturn:").count(), 1);
    assert_eq!(SMALL_RULESET.matches("\nactions:").count(), 1);
    assert!(start < end);
    let text = format!("{}{}", &SMALL_RULESET[..start], &SMALL_RULESET[end..]);
    ruleset_from(&text, SMALL_CATALOG)
}

#[test]
fn nothing_is_listed_after_the_game_has_ended() {
    let ruleset = small();
    // Both Players only pass: seat 1 draws from an empty deck at the start of Turn 92.
    let mut state = opening(&ruleset);
    for turn in 1..=90 {
        let active = state.active_player();
        assert_eq!(active, p((turn + 1) % 2), "Turn {turn}");
        state = accept_all(&ruleset, &state, &[pass(active.0)]);
    }
    assert!(state.outcome().is_none());
    assert!(ruleset.legal_actions(&state).contains(&pass(0)));

    let ended = accept_all(&ruleset, &state, &[pass(0)]);
    assert!(ended.outcome().is_some(), "the Game has ended");
    assert_eq!(ruleset.legal_actions(&ended), Vec::<Action>::new());
    assert_eq!(brute_force(&ruleset, &ended), Vec::<Action>::new());
    assert_eq!(
        ruleset.transition(&ended, &pass(ended.active_player().0)),
        Err(Rejection::GameOver)
    );

    // A ruleset without a `turn` has no current phase: nothing is listed.
    let timeless = without_a_turn();
    let created = opening(&timeless);
    assert_eq!(created.phase(), None);
    assert!(created.outcome().is_none());
    assert_eq!(timeless.legal_actions(&created), Vec::<Action>::new());
    assert_eq!(brute_force(&timeless, &created), Vec::<Action>::new());
}

#[test]
fn nothing_is_listed_outside_an_action_phase() {
    let ruleset = small();
    // The Game is back in the Automatic Phase `start` of the Turn in progress.
    let mut state = mid_game(&ruleset);
    assert!(!ruleset.legal_actions(&state).is_empty());
    apply_all(
        &mut state,
        &[Event::PhaseBegan {
            phase: PhaseId::new("start"),
        }],
    );
    assert_eq!(state.phase(), Some(&PhaseId::new("start")));
    assert!(state.outcome().is_none());

    assert_eq!(ruleset.legal_actions(&state), Vec::<Action>::new());
    assert_eq!(brute_force(&ruleset, &state), Vec::<Action>::new());
    assert_eq!(
        ruleset.transition(&state, &pass(0)),
        Err(Rejection::ActionNotInCurrentPhase {
            action: ActionId::new("pass_turn")
        })
    );
}
