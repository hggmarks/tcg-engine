//! Acceptance tests for ticket #99 (ADR-0044): the Duel Ruleset's tribute summons and normal spells
//! (`rulesets/duel-ruleset/`), loaded from its real files with its real catalog and sample decks, and
//! distinct card parameters on `tribute_summon_two`. The rule in any ruleset is tested in
//! `distinct_card_parameters.rs`.
//!
//! Every scenario starts from the Opening Position of Seed 0, where seat 0 is the Active Player in
//! Main of Turn 1, and sets itself up by moving cards between zones with Events (`Scene::take`), so
//! no test depends on what the opening hands happen to hold. Cards are picked by level band, or by
//! spell id.

use std::ops::RangeInclusive;

use tcg_engine::declarative::{
    Action, Answer, Argument, CardInstanceId, DecisionKind, DeclarativeRuleset, Event, GameState,
    Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, CardTypeId, FieldName, FieldValue, ParamName, PlayerStateId, StateName, ZoneId,
};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const RULESET: &str = include_str!("../../../rulesets/duel-ruleset/duel-ruleset.yaml");
const CATALOG: &str = include_str!("../../../rulesets/duel-ruleset/duel-catalog.yaml");
const DECK_A: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-a.yaml");
const DECK_B: &str = include_str!("../../../rulesets/duel-ruleset/decks/deck-b.yaml");

/// The level bands of the monsters: normal summoned, one tribute, two tributes.
const LOW: RangeInclusive<i64> = 1..=4;
const MID: RangeInclusive<i64> = 5..=6;
const HIGH: RangeInclusive<i64> = 7..=8;

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

fn is_legal(ruleset: &DeclarativeRuleset, state: &GameState, action: &Action) -> bool {
    ruleset.legal_actions(state).contains(action)
}

/// Whether some Legal Action is the declared action `id`.
fn offers(ruleset: &DeclarativeRuleset, state: &GameState, id: &str) -> bool {
    ruleset
        .legal_actions(state)
        .iter()
        .any(|action| matches!(action, Action::Declared { action, .. } if action.as_str() == id))
}

fn take(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
) -> Transition<GameState, Event> {
    ruleset
        .transition(state, action)
        .unwrap_or_else(|rejection| panic!("{action:?} is rejected: {rejection:?}"))
}

/// Asserts that the Action is not a Legal Action, that it is refused with exactly `expected`, and
/// that the GameState is what it was before the call. A Rejection has no Event Trace.
fn assert_refused(
    ruleset: &DeclarativeRuleset,
    state: &GameState,
    action: &Action,
    expected: Rejection,
) {
    assert!(!is_legal(ruleset, state, action), "{action:?} is listed");
    let before = state.clone();
    assert_eq!(ruleset.transition(state, action), Err(expected));
    assert_eq!(state, &before, "a rejected Action changes nothing");
}

fn where_not_met(param: &str, card: CardInstanceId) -> Rejection {
    Rejection::WhereConditionNotMet {
        param: ParamName::new(param),
        card,
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

fn card_state(card: CardInstanceId, state: &str, value: FieldValue) -> Event {
    Event::CardStateChanged {
        card,
        state: StateName::new(state),
        value,
    }
}

fn normal_summoned(seat: u32, value: bool) -> Event {
    Event::PlayerStateChanged {
        player: p(seat),
        state: PlayerStateId::new("normal_summoned"),
        value: FieldValue::Bool(value),
    }
}

fn life_changed(seat: u32, value: i64) -> Event {
    Event::PlayerStateChanged {
        player: p(seat),
        state: PlayerStateId::new("life_points"),
        value: FieldValue::Int(value),
    }
}

fn cards_in(state: &GameState, seat: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(seat), &zone(zone_id))
        .unwrap_or_else(|| panic!("the zone `{zone_id}` exists"))
        .to_vec()
}

fn life_points(state: &GameState, seat: u32) -> i64 {
    match state.player_state_value(p(seat), &PlayerStateId::new("life_points")) {
        Some(FieldValue::Int(value)) => *value,
        other => panic!("life_points is {other:?}"),
    }
}

fn instance_state(state: &GameState, card: CardInstanceId, name: &str) -> FieldValue {
    state
        .card(card)
        .expect("the card exists")
        .state_value(&StateName::new(name))
        .unwrap_or_else(|| panic!("the card has no `{name}`"))
        .clone()
}

fn position(state: &GameState, card: CardInstanceId) -> FieldValue {
    instance_state(state, card, "position")
}

fn position_fixed(state: &GameState, card: CardInstanceId) -> bool {
    instance_state(state, card, "position_fixed") == FieldValue::Bool(true)
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

fn to_battle(player: u32) -> Action {
    declared(player, "to_battle", &[])
}

fn end_turn(player: u32) -> Action {
    declared(player, "end_turn", &[])
}

/// The real Duel Ruleset with its two sample decks, at the Opening Position of Seed 0, and the
/// cards a scenario has taken so far.
struct Scene {
    ruleset: DeclarativeRuleset,
    state: GameState,
    taken: Vec<CardInstanceId>,
}

impl Scene {
    fn new() -> Self {
        let definition = load_ruleset_definition(RULESET).expect("the Duel Ruleset loads");
        let catalog = load_card_catalog(&definition, CATALOG).expect("the catalog loads");
        let ruleset = DeclarativeRuleset::new(definition, catalog);
        let decks: Vec<_> = [DECK_A, DECK_B]
            .iter()
            .map(|yaml| {
                load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                    .expect("the sample deck is legal")
            })
            .collect();
        let state = ruleset
            .create_game(&decks, Seed(0))
            .expect("the decks create a game")
            .opening
            .state;
        Self {
            ruleset,
            state,
            taken: Vec::new(),
        }
    }

    /// The Card Definition id and the card type of `card`.
    fn kind(&self, card: CardInstanceId) -> (String, CardTypeId) {
        let definition = self
            .state
            .card(card)
            .expect("the card exists")
            .definition()
            .clone();
        let card_type = self
            .ruleset
            .catalog()
            .card(&definition)
            .expect("the definition is in the catalog")
            .card_type()
            .clone();
        (definition.as_str().to_string(), card_type)
    }

    /// The level of a monster.
    fn level(&self, card: CardInstanceId) -> i64 {
        let instance = self.state.card(card).expect("the card exists");
        let definition = self
            .ruleset
            .catalog()
            .card(instance.definition())
            .expect("the definition is in the catalog");
        match definition.field(&FieldName::new("level")) {
            Some(FieldValue::Int(level)) => *level,
            other => panic!("level is {other:?}"),
        }
    }

    /// Moves `card` to the `to` zone of its owner, by an Event that sets the scenario up.
    fn put(&mut self, card: CardInstanceId, to: &str) {
        let from = self
            .state
            .zone_of(card)
            .expect("the card is in a zone")
            .clone();
        if from == zone(to) {
            return;
        }
        let player = self.state.card(card).expect("the card exists").owner();
        self.state
            .apply(&Event::CardMoved {
                card,
                player,
                from,
                to: zone(to),
            })
            .expect("the scenario Event is accepted");
    }

    /// The first monster of `seat` in the level band `levels` that the scenario has not taken, put
    /// in the `to` zone.
    fn monster(&mut self, seat: u32, levels: RangeInclusive<i64>, to: &str) -> CardInstanceId {
        let card = self
            .state
            .cards()
            .iter()
            .map(|card| card.id())
            .filter(|&id| self.state.card(id).map(|card| card.owner()) == Some(p(seat)))
            .filter(|id| !self.taken.contains(id))
            .find(|&id| {
                self.kind(id).1 == CardTypeId::new("monster") && levels.contains(&self.level(id))
            })
            .unwrap_or_else(|| panic!("seat {seat} holds a monster of level {levels:?}"));
        self.taken.push(card);
        self.put(card, to);
        card
    }

    /// `count` monsters of `seat` in the band, each in the `to` zone.
    fn monsters(
        &mut self,
        seat: u32,
        levels: RangeInclusive<i64>,
        count: usize,
        to: &str,
    ) -> Vec<CardInstanceId> {
        (0..count)
            .map(|_| self.monster(seat, levels.clone(), to))
            .collect()
    }

    /// The first spell of `seat` made from `definition` (any spell for `None`) that the scenario has
    /// not taken, put in the `to` zone.
    fn spell(&mut self, seat: u32, definition: Option<&str>, to: &str) -> CardInstanceId {
        let card = self
            .state
            .cards()
            .iter()
            .map(|card| card.id())
            .filter(|&id| self.state.card(id).map(|card| card.owner()) == Some(p(seat)))
            .filter(|id| !self.taken.contains(id))
            .find(|&id| {
                let (id_of_definition, card_type) = self.kind(id);
                card_type == CardTypeId::new("spell")
                    && definition.is_none_or(|wanted| wanted == id_of_definition)
            })
            .unwrap_or_else(|| panic!("seat {seat} holds the spell {definition:?}"));
        self.taken.push(card);
        self.put(card, to);
        card
    }

    fn offers(&self, id: &str) -> bool {
        offers(&self.ruleset, &self.state, id)
    }

    fn legal(&self, action: &Action) -> bool {
        is_legal(&self.ruleset, &self.state, action)
    }

    fn take(&self, action: &Action) -> Transition<GameState, Event> {
        take(&self.ruleset, &self.state, action)
    }

    fn refused(&self, action: &Action, expected: Rejection) {
        assert_refused(&self.ruleset, &self.state, action, expected);
    }
}

fn summon_one(card: CardInstanceId, tribute: CardInstanceId) -> Action {
    declared(0, "tribute_summon_one", &[card, tribute])
}

fn summon_two(card: CardInstanceId, first: CardInstanceId, second: CardInstanceId) -> Action {
    declared(0, "tribute_summon_two", &[card, first, second])
}

fn set_one(card: CardInstanceId, tribute: CardInstanceId) -> Action {
    declared(0, "tribute_set_one", &[card, tribute])
}

fn set_two(card: CardInstanceId, first: CardInstanceId, second: CardInstanceId) -> Action {
    declared(0, "tribute_set_two", &[card, first, second])
}

fn play_spell(card: CardInstanceId) -> Action {
    declared(0, "play_spell", &[card])
}

// ---- tribute summons

#[test]
fn a_tribute_summon_with_one_tribute_sends_it_to_the_graveyard() {
    let mut scene = Scene::new();
    let tribute = scene.monster(0, LOW, "monster_zone");
    let big = scene.monster(0, MID, "hand");
    let action = summon_one(big, tribute);
    assert!(scene.legal(&action));

    let summoned = scene.take(&action);
    // The tribute is the cost, so it leaves first; then the summon: the Monster Zone, the once-per-
    // Turn flag and the monster arriving in attack position, which a position change cannot undo
    // this Turn. The position is "attack" already, so it emits no Event.
    assert_eq!(
        summoned.trace.events().to_vec(),
        vec![
            moved(tribute, 0, "monster_zone", "graveyard"),
            moved(big, 0, "hand", "monster_zone"),
            normal_summoned(0, true),
            card_state(big, "position_fixed", FieldValue::Bool(true)),
        ]
    );
    assert_eq!(cards_in(&summoned.state, 0, "graveyard"), vec![tribute]);
    assert_eq!(cards_in(&summoned.state, 0, "monster_zone"), vec![big]);
    assert_eq!(position(&summoned.state, big), text("attack"));
    assert!(position_fixed(&summoned.state, big));
    assert!(!cards_in(&summoned.state, 0, "hand").contains(&big));
}

#[test]
fn a_tribute_summon_with_two_tributes_sends_both_to_the_graveyard() {
    let mut scene = Scene::new();
    let tributes = scene.monsters(0, LOW, 2, "monster_zone");
    let big = scene.monster(0, HIGH, "hand");
    let action = summon_two(big, tributes[0], tributes[1]);
    assert!(scene.legal(&action));

    let summoned = scene.take(&action);
    assert_eq!(
        summoned.trace.events().to_vec(),
        vec![
            moved(tributes[0], 0, "monster_zone", "graveyard"),
            moved(tributes[1], 0, "monster_zone", "graveyard"),
            moved(big, 0, "hand", "monster_zone"),
            normal_summoned(0, true),
            card_state(big, "position_fixed", FieldValue::Bool(true)),
        ]
    );
    // Both go to the Graveyard, in param order.
    assert_eq!(cards_in(&summoned.state, 0, "graveyard"), tributes);
    assert_eq!(cards_in(&summoned.state, 0, "monster_zone"), vec![big]);
    assert_eq!(position(&summoned.state, big), text("attack"));
}

#[test]
fn a_tribute_set_puts_the_monster_in_defence_position() {
    // One tribute.
    let mut scene = Scene::new();
    let tribute = scene.monster(0, LOW, "monster_zone");
    let mid = scene.monster(0, MID, "hand");
    let action = set_one(mid, tribute);
    assert!(scene.legal(&action));
    let set = scene.take(&action);
    assert_eq!(
        set.trace.events().to_vec(),
        vec![
            moved(tribute, 0, "monster_zone", "graveyard"),
            moved(mid, 0, "hand", "monster_zone"),
            normal_summoned(0, true),
            card_state(mid, "position", text("defence")),
            card_state(mid, "position_fixed", FieldValue::Bool(true)),
        ]
    );
    assert_eq!(cards_in(&set.state, 0, "graveyard"), vec![tribute]);
    assert_eq!(cards_in(&set.state, 0, "monster_zone"), vec![mid]);
    assert_eq!(position(&set.state, mid), text("defence"));
    assert!(position_fixed(&set.state, mid));

    // Two tributes.
    let mut scene = Scene::new();
    let tributes = scene.monsters(0, LOW, 2, "monster_zone");
    let high = scene.monster(0, HIGH, "hand");
    let action = set_two(high, tributes[0], tributes[1]);
    assert!(scene.legal(&action));
    let set = scene.take(&action);
    assert_eq!(
        set.trace.events().to_vec(),
        vec![
            moved(tributes[0], 0, "monster_zone", "graveyard"),
            moved(tributes[1], 0, "monster_zone", "graveyard"),
            moved(high, 0, "hand", "monster_zone"),
            normal_summoned(0, true),
            card_state(high, "position", text("defence")),
            card_state(high, "position_fixed", FieldValue::Bool(true)),
        ]
    );
    assert_eq!(cards_in(&set.state, 0, "graveyard"), tributes);
    assert_eq!(position(&set.state, high), text("defence"));
}

#[test]
fn a_tribute_summon_with_too_few_tributes_is_rejected() {
    // A level 7 monster needs two tributes: with one it is not a level 5-6 summon.
    let mut scene = Scene::new();
    let tribute = scene.monster(0, LOW, "monster_zone");
    let high = scene.monster(0, HIGH, "hand");
    scene.refused(&summon_one(high, tribute), where_not_met("card", high));
    scene.refused(&set_one(high, tribute), where_not_met("card", high));
    // With one monster on the field no two tributes exist, so the two-tribute summon is never
    // listed, and the only way to name one monster twice is refused.
    assert!(!scene.offers("tribute_summon_two"));
    assert!(!scene.offers("tribute_set_two"));
    scene.refused(
        &summon_two(high, tribute, tribute),
        Rejection::CardBoundTwice {
            param: ParamName::new("tribute_2"),
            card: tribute,
            first: ParamName::new("tribute_1"),
        },
    );
    // No tribute at all: the Action has too few arguments.
    scene.refused(
        &declared(0, "tribute_summon_one", &[high]),
        Rejection::ArgumentCountMismatch {
            expected: 2,
            found: 1,
        },
    );

    // The other way round: a level 5-6 monster does not take two tributes.
    let mut scene = Scene::new();
    let tributes = scene.monsters(0, LOW, 2, "monster_zone");
    let mid = scene.monster(0, MID, "hand");
    scene.refused(
        &summon_two(mid, tributes[0], tributes[1]),
        where_not_met("card", mid),
    );
    scene.refused(
        &set_two(mid, tributes[0], tributes[1]),
        where_not_met("card", mid),
    );
    assert!(scene.legal(&summon_one(mid, tributes[0])));

    // An empty Monster Zone: no tribute summon is listed at all.
    let mut scene = Scene::new();
    scene.monster(0, MID, "hand");
    scene.monster(0, HIGH, "hand");
    for id in [
        "tribute_summon_one",
        "tribute_summon_two",
        "tribute_set_one",
        "tribute_set_two",
    ] {
        assert!(!scene.offers(id), "{id} is listed with no tribute");
    }
}

#[test]
fn a_tribute_summon_with_a_tribute_the_summoner_does_not_control_is_rejected() {
    let mut scene = Scene::new();
    let own = scene.monster(0, LOW, "monster_zone");
    let foreign = scene.monster(1, LOW, "monster_zone");
    let mid = scene.monster(0, MID, "hand");
    let high = scene.monster(0, HIGH, "hand");

    scene.refused(
        &summon_one(mid, foreign),
        Rejection::NotCardOwner {
            param: ParamName::new("tribute"),
            card: foreign,
            owner: p(1),
        },
    );
    scene.refused(
        &set_one(mid, foreign),
        Rejection::NotCardOwner {
            param: ParamName::new("tribute"),
            card: foreign,
            owner: p(1),
        },
    );
    // For two tributes the refused one is the second param.
    scene.refused(
        &summon_two(high, own, foreign),
        Rejection::NotCardOwner {
            param: ParamName::new("tribute_2"),
            card: foreign,
            owner: p(1),
        },
    );
    scene.refused(
        &set_two(high, foreign, own),
        Rejection::NotCardOwner {
            param: ParamName::new("tribute_1"),
            card: foreign,
            owner: p(1),
        },
    );
    // The summoner's own monster is a fine tribute.
    assert!(scene.legal(&summon_one(mid, own)));
}

#[test]
fn a_tribute_summon_of_a_level_1_to_4_monster_is_rejected() {
    let mut scene = Scene::new();
    let tributes = scene.monsters(0, LOW, 2, "monster_zone");
    let low = scene.monster(0, LOW, "hand");

    scene.refused(&summon_one(low, tributes[0]), where_not_met("card", low));
    scene.refused(&set_one(low, tributes[0]), where_not_met("card", low));
    scene.refused(
        &summon_two(low, tributes[0], tributes[1]),
        where_not_met("card", low),
    );
    scene.refused(
        &set_two(low, tributes[0], tributes[1]),
        where_not_met("card", low),
    );
    // It can be normal summoned, with no tribute.
    assert!(scene.legal(&declared(0, "normal_summon", &[low])));
}

#[test]
fn a_tribute_summon_with_two_tributes_from_a_full_monster_zone_is_legal() {
    // Three monsters on the field: a summon with two tributes leaves room, because the capacity is
    // checked after the tributes leave.
    let mut scene = Scene::new();
    let field = scene.monsters(0, LOW, 3, "monster_zone");
    let high = scene.monster(0, HIGH, "hand");
    assert_eq!(cards_in(&scene.state, 0, "monster_zone"), field);
    let action = summon_two(high, field[0], field[1]);
    assert!(scene.legal(&action));
    // Without tributes a full Monster Zone takes no summon.
    assert!(!scene.offers("normal_summon"));

    let summoned = scene.take(&action);
    // The third monster stays and the summoned one arrives: the zone holds 2.
    assert_eq!(
        cards_in(&summoned.state, 0, "monster_zone"),
        vec![field[2], high]
    );
    assert_eq!(
        cards_in(&summoned.state, 0, "graveyard"),
        vec![field[0], field[1]]
    );

    // One tribute from a full Monster Zone, level 5-6.
    let mut scene = Scene::new();
    let field = scene.monsters(0, LOW, 3, "monster_zone");
    let mid = scene.monster(0, MID, "hand");
    let action = summon_one(mid, field[0]);
    assert!(scene.legal(&action));
    let summoned = scene.take(&action);
    assert_eq!(
        cards_in(&summoned.state, 0, "monster_zone"),
        vec![field[1], field[2], mid]
    );
    assert_eq!(cards_in(&summoned.state, 0, "graveyard"), vec![field[0]]);
    // The set is as legal.
    assert!(scene.legal(&set_one(mid, field[0])));
}

#[test]
fn a_tribute_summon_after_the_normal_summon_is_rejected() {
    let mut scene = Scene::new();
    let tributes = scene.monsters(0, LOW, 2, "monster_zone");
    let low = scene.monster(0, LOW, "hand");
    let mid = scene.monster(0, MID, "hand");
    let high = scene.monster(0, HIGH, "hand");

    // The Turn's one summon is spent by a normal summon: no tribute summon or set is legal.
    let after_normal = scene.take(&declared(0, "normal_summon", &[low])).state;
    for (action, card) in [
        (summon_one(mid, tributes[0]), mid),
        (set_one(mid, tributes[0]), mid),
        (summon_two(high, tributes[0], tributes[1]), high),
        (set_two(high, tributes[0], tributes[1]), high),
    ] {
        assert_refused(
            &scene.ruleset,
            &after_normal,
            &action,
            where_not_met("card", card),
        );
    }

    // And the other way round: a tribute summon spends it for the normal summon and the set.
    let after_tribute = scene.take(&summon_one(mid, tributes[0])).state;
    assert_refused(
        &scene.ruleset,
        &after_tribute,
        &declared(0, "normal_summon", &[low]),
        where_not_met("card", low),
    );
    assert_refused(
        &scene.ruleset,
        &after_tribute,
        &declared(0, "set_monster", &[low]),
        where_not_met("card", low),
    );
    assert_refused(
        &scene.ruleset,
        &after_tribute,
        &set_one(high, tributes[1]),
        where_not_met("card", high),
    );
}

#[test]
fn a_tribute_summon_is_never_listed_with_one_card_as_both_tributes() {
    let mut scene = Scene::new();
    let tributes = scene.monsters(0, LOW, 2, "monster_zone");
    let (m1, m2) = (tributes[0], tributes[1]);
    let high = scene.monster(0, HIGH, "hand");

    let listed = scene.ruleset.legal_actions(&scene.state);
    // Both orders of the two tributes are listed, none repeats a card.
    assert!(listed.contains(&summon_two(high, m1, m2)));
    assert!(listed.contains(&summon_two(high, m2, m1)));
    for id in ["tribute_summon_two", "tribute_set_two"] {
        let twos: Vec<Action> = listed
            .iter()
            .filter(
                |action| matches!(action, Action::Declared { action, .. } if action.as_str() == id),
            )
            .cloned()
            .collect();
        assert_eq!(twos.len(), 2, "{id}: the two orders of the two tributes");
    }
    assert!(!listed.contains(&summon_two(high, m1, m1)));
    assert!(!listed.contains(&summon_two(high, m2, m2)));

    // The candidates that repeat a tribute are in `listing_with_rejections`, each refused with
    // `CardBoundTwice`.
    let walked = scene.ruleset.listing_with_rejections(&scene.state);
    for card in [m1, m2] {
        let entry = walked
            .iter()
            .find(|(action, _)| action == &summon_two(high, card, card))
            .expect("the repeat is a candidate");
        assert_eq!(
            entry.1,
            Some(Rejection::CardBoundTwice {
                param: ParamName::new("tribute_2"),
                card,
                first: ParamName::new("tribute_1"),
            })
        );
    }

    // The two orders differ in the Graveyard order: the moves follow param order.
    let forward = scene.take(&summon_two(high, m1, m2)).state;
    let backward = scene.take(&summon_two(high, m2, m1)).state;
    assert_eq!(cards_in(&forward, 0, "graveyard"), vec![m1, m2]);
    assert_eq!(cards_in(&backward, 0, "graveyard"), vec![m2, m1]);
}

#[test]
fn declaring_one_monster_as_both_tributes_is_rejected() {
    let mut scene = Scene::new();
    let tributes = scene.monsters(0, LOW, 2, "monster_zone");
    let high = scene.monster(0, HIGH, "hand");

    for action in [
        summon_two(high, tributes[0], tributes[0]),
        set_two(high, tributes[1], tributes[1]),
    ] {
        let Action::Declared { arguments, .. } = &action else {
            unreachable!("a Declared Action");
        };
        let Argument::Card(card) = arguments[1];
        scene.refused(
            &action,
            Rejection::CardBoundTwice {
                param: ParamName::new("tribute_2"),
                card,
                first: ParamName::new("tribute_1"),
            },
        );
    }
}

// ---- normal spells

#[test]
fn breaking_bolt_destroys_a_chosen_opponent_monster() {
    let mut scene = Scene::new();
    let victims = scene.monsters(1, LOW, 2, "monster_zone");
    let bolt = scene.spell(0, Some("breaking_bolt"), "hand");
    let action = play_spell(bolt);
    assert!(scene.legal(&action));

    // The cost moves the spell to the Spell/Trap Zone, its `enter` trigger sends it to the Graveyard
    // first, and then asks its controller for one of the opponent's monsters.
    let played = scene.take(&action);
    let events = played.trace.events();
    assert_eq!(
        events[..2].to_vec(),
        vec![
            moved(bolt, 0, "hand", "spell_trap_zone"),
            moved(bolt, 0, "spell_trap_zone", "graveyard"),
        ]
    );
    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[2],
        Event::DecisionPending {
            decider: PlayerId(0),
            kind: DecisionKind::Card,
            ..
        }
    ));
    let pending = played
        .state
        .pending_decision()
        .expect("a Choice is pending");
    assert_eq!(pending.decider(), p(0));
    assert_eq!(pending.kind(), DecisionKind::Card);
    // The Candidates are the opponent's monsters, in ascending Card Instance Id.
    assert_eq!(
        scene.ruleset.legal_actions(&played.state),
        vec![
            Action::Answer {
                player: p(0),
                answer: Answer::Card(victims[0]),
            },
            Action::Answer {
                player: p(0),
                answer: Answer::Card(victims[1]),
            },
        ]
    );

    let chosen = take(
        &scene.ruleset,
        &played.state,
        &Action::Answer {
            player: p(0),
            answer: Answer::Card(victims[1]),
        },
    );
    assert_eq!(
        chosen.trace.events().last(),
        Some(&moved(victims[1], 1, "monster_zone", "graveyard"))
    );
    let after = chosen.state;
    assert!(after.pending_decision().is_none());
    assert_eq!(cards_in(&after, 1, "graveyard"), vec![victims[1]]);
    assert_eq!(cards_in(&after, 1, "monster_zone"), vec![victims[0]]);
    assert_eq!(cards_in(&after, 0, "graveyard"), vec![bolt]);
    assert!(cards_in(&after, 0, "spell_trap_zone").is_empty());
}

#[test]
fn breaking_bolt_with_no_opponent_monster_goes_to_the_graveyard() {
    let mut scene = Scene::new();
    // The summoner's own monster is not a Candidate.
    scene.monster(0, LOW, "monster_zone");
    let bolt = scene.spell(0, Some("breaking_bolt"), "hand");
    let action = play_spell(bolt);
    assert!(scene.legal(&action));

    // With no Candidate nothing is asked: the spell has left before the Choice, so it still goes
    // to the Graveyard, and the Game does not wait.
    let played = scene.take(&action);
    assert_eq!(
        played.trace.events().to_vec(),
        vec![
            moved(bolt, 0, "hand", "spell_trap_zone"),
            moved(bolt, 0, "spell_trap_zone", "graveyard"),
        ]
    );
    assert!(played.state.pending_decision().is_none());
    assert_eq!(cards_in(&played.state, 0, "graveyard"), vec![bolt]);
    assert!(cards_in(&played.state, 0, "spell_trap_zone").is_empty());
}

#[test]
fn lucky_find_draws_two_cards() {
    let mut scene = Scene::new();
    let find = scene.spell(0, Some("lucky_find"), "hand");
    let deck = cards_in(&scene.state, 0, "deck");
    let (first, second) = (deck[deck.len() - 1], deck[deck.len() - 2]);
    let hand_before = cards_in(&scene.state, 0, "hand").len();

    let played = scene.take(&play_spell(find));
    assert_eq!(
        played.trace.events().to_vec(),
        vec![
            moved(find, 0, "hand", "spell_trap_zone"),
            moved(find, 0, "spell_trap_zone", "graveyard"),
            moved(first, 0, "deck", "hand"),
            moved(second, 0, "deck", "hand"),
        ]
    );
    // The spell left the hand and two cards arrived.
    assert_eq!(cards_in(&played.state, 0, "hand").len(), hand_before + 1);
    assert_eq!(cards_in(&played.state, 0, "deck").len(), deck.len() - 2);
    assert_eq!(cards_in(&played.state, 0, "graveyard"), vec![find]);
    assert!(cards_in(&played.state, 0, "spell_trap_zone").is_empty());
}

#[test]
fn mending_light_gains_1000_life_points() {
    let mut scene = Scene::new();
    let light = scene.spell(0, Some("mending_light"), "hand");
    assert_eq!(life_points(&scene.state, 0), 4000);

    let played = scene.take(&play_spell(light));
    assert_eq!(
        played.trace.events().to_vec(),
        vec![
            moved(light, 0, "hand", "spell_trap_zone"),
            moved(light, 0, "spell_trap_zone", "graveyard"),
            life_changed(0, 5000),
        ]
    );
    assert_eq!(life_points(&played.state, 0), 5000);
    assert_eq!(life_points(&played.state, 1), 4000);
    assert_eq!(cards_in(&played.state, 0, "graveyard"), vec![light]);
    assert!(cards_in(&played.state, 0, "spell_trap_zone").is_empty());
}

#[test]
fn playing_a_spell_with_a_full_spell_trap_zone_is_rejected() {
    let mut scene = Scene::new();
    // A hand-built state: three spells already in the Spell/Trap Zone (a transition leaves none
    // there), a fourth in hand.
    let zone_full = [
        scene.spell(0, None, "spell_trap_zone"),
        scene.spell(0, None, "spell_trap_zone"),
        scene.spell(0, None, "spell_trap_zone"),
    ];
    let fourth = scene.spell(0, None, "hand");
    assert_eq!(cards_in(&scene.state, 0, "spell_trap_zone"), zone_full);

    scene.refused(&play_spell(fourth), where_not_met("card", fourth));

    // With two spells there it is legal: the capacity is 3.
    let mut scene = Scene::new();
    scene.spell(0, None, "spell_trap_zone");
    scene.spell(0, None, "spell_trap_zone");
    let third = scene.spell(0, None, "hand");
    assert!(scene.legal(&play_spell(third)));
}

#[test]
fn playing_a_spell_outside_main_is_rejected() {
    let mut scene = Scene::new();
    let spell = scene.spell(0, None, "hand");
    assert!(scene.legal(&play_spell(spell)));

    // In Battle, a spell is not playable; it is listed in Main only.
    scene.state = scene.take(&to_battle(0)).state;
    scene.refused(
        &play_spell(spell),
        Rejection::ActionNotInCurrentPhase {
            action: ActionId::new("play_spell"),
        },
    );
    assert!(!scene.offers("play_spell"));
    // Ending the Turn leaves Battle too: the next Player's Main lists it again.
    let next = scene.take(&end_turn(0)).state;
    assert!(offers(&scene.ruleset, &next, "play_spell"));
}

// ---- the catalog and the decks

#[test]
fn the_sample_decks_hold_every_level_band_and_the_three_spells() {
    let scene = Scene::new();
    for seat in 0..2 {
        let mut bands = [0usize; 3];
        let mut spells = [0usize; 3];
        let mut total = 0usize;
        for card in scene
            .state
            .cards()
            .iter()
            .filter(|card| card.owner() == p(seat))
        {
            total += 1;
            let (definition, card_type) = scene.kind(card.id());
            if card_type == CardTypeId::new("spell") {
                match definition.as_str() {
                    "breaking_bolt" => spells[0] += 1,
                    "lucky_find" => spells[1] += 1,
                    "mending_light" => spells[2] += 1,
                    other => panic!("an unknown spell `{other}`"),
                }
            } else {
                let band = match scene.level(card.id()) {
                    1..=4 => 0,
                    5..=6 => 1,
                    7..=8 => 2,
                    other => panic!("a monster of level {other}"),
                };
                bands[band] += 1;
            }
        }
        assert_eq!(total, 40, "seat {seat}: a deck of 40");
        assert!(
            bands.iter().all(|&count| count > 0),
            "seat {seat}: {bands:?}"
        );
        assert!(
            spells.iter().all(|&count| count > 0),
            "seat {seat}: {spells:?}"
        );
    }
}
