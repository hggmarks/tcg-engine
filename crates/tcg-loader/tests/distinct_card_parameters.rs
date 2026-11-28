//! Acceptance tests for ticket #99 (ADR-0044): distinct card parameters, in any ruleset. Binding one
//! Card Instance to two card params of one Declared Action is the typed Rejection `CardBoundTwice`,
//! and the Legal Action listing never offers such an Action.
//!
//! The ruleset, catalog and deck are the fixtures `distinct-*.yaml`: a card type `unit` and the
//! actions `pair`, `triple` and `pair_checked`, whose params all take cards of the acting Player's
//! hand, so one card could fill several of them. Seat 0 opens with three `alpha` in hand. The same
//! rule on the Duel Ruleset's `tribute_summon_two` is tested in `tributes_and_spells.rs`.

use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, GameCreation, GameState, Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, ParamName, ZoneId};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const RULESET: &str = include_str!("fixtures/distinct-ruleset.yaml");
const CATALOG: &str = include_str!("fixtures/distinct-catalog.yaml");
const DECK: &str = include_str!("fixtures/distinct-deck.yaml");

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn act(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn game() -> (DeclarativeRuleset, GameCreation) {
    let definition = load_ruleset_definition(RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, CATALOG).expect("the catalog loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let decks: Vec<_> = (0..2)
        .map(|_| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), DECK)
                .expect("the deck is legal")
        })
        .collect();
    let creation = ruleset
        .create_game(&decks, Seed(1))
        .expect("the decks create a game");
    (ruleset, creation)
}

/// The three cards of seat 0's hand, in ascending id.
fn hand_of_seat_0(state: &GameState) -> [CardInstanceId; 3] {
    let mut hand = state
        .zone(p(0), &ZoneId::new("hand"))
        .expect("the hand exists")
        .to_vec();
    hand.sort();
    <[CardInstanceId; 3]>::try_from(hand).expect("seat 0 holds three cards")
}

fn arguments_of(action: &Action) -> Vec<CardInstanceId> {
    match action {
        Action::Declared { arguments, .. } => arguments
            .iter()
            .map(|argument| {
                let Argument::Card(card) = argument;
                *card
            })
            .collect(),
        other => panic!("not a Declared Action: {other:?}"),
    }
}

fn is_action(action: &Action, id: &str) -> bool {
    matches!(action, Action::Declared { action, .. } if action.as_str() == id)
}

fn twice(param: &str, card: CardInstanceId, first: &str) -> Rejection {
    Rejection::CardBoundTwice {
        param: ParamName::new(param),
        card,
        first: ParamName::new(first),
    }
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

// ---- the listing

#[test]
fn an_action_is_never_listed_with_one_card_in_two_parameters() {
    let (ruleset, creation) = game();
    let state = creation.opening.state;
    let [a, b, c] = hand_of_seat_0(&state);

    let listed = ruleset.legal_actions(&state);
    // Every Legal Action of `pair` names two different cards, and every ordered pair of different
    // cards is there once: 3 * 2.
    let pairs: Vec<Vec<CardInstanceId>> = listed
        .iter()
        .filter(|action| is_action(action, "pair"))
        .map(arguments_of)
        .collect();
    assert_eq!(pairs.len(), 6);
    for cards in &pairs {
        assert_ne!(cards[0], cards[1], "{cards:?} repeats a card");
    }
    for (first, second) in [(a, b), (b, a), (a, c), (c, a), (b, c), (c, b)] {
        assert!(
            pairs.contains(&vec![first, second]),
            "({first:?}, {second:?}) is listed"
        );
    }
    // The same with three params: the 6 orders of the 3 cards, none with a repeat.
    let triples: Vec<Vec<CardInstanceId>> = listed
        .iter()
        .filter(|action| is_action(action, "triple"))
        .map(arguments_of)
        .collect();
    assert_eq!(triples.len(), 6);
    for cards in &triples {
        assert!(
            cards[0] != cards[1] && cards[0] != cards[2] && cards[1] != cards[2],
            "{cards:?} repeats a card"
        );
    }

    // The listing is a filter through `transition`: the candidates that repeat a card are in
    // `listing_with_rejections`, each refused with `CardBoundTwice`, and the ones with `None` are
    // exactly the Legal Actions.
    let walked = ruleset.listing_with_rejections(&state);
    for card in [a, b, c] {
        let entry = walked
            .iter()
            .find(|(action, _)| action == &act(0, "pair", &[card, card]))
            .unwrap_or_else(|| panic!("({card:?}, {card:?}) is a candidate"));
        assert_eq!(entry.1, Some(twice("second", card, "first")));
    }
    let accepted: Vec<Action> = walked
        .iter()
        .filter(|(_, rejection)| rejection.is_none())
        .map(|(action, _)| action.clone())
        .collect();
    assert_eq!(accepted, listed);
}

#[test]
fn a_card_filling_two_parameters_is_refused_with_the_param_that_came_first() {
    let (ruleset, creation) = game();
    let state = creation.opening.state;
    let [a, b, _] = hand_of_seat_0(&state);

    // The Rejection names the later param, the card and the earlier param that holds it: for
    // `(a, b, b)` the third param repeats the second, not the first.
    for (cards, param, first) in [
        ([a, b, a], "third", "first"),
        ([a, b, b], "third", "second"),
        ([a, a, a], "second", "first"),
    ] {
        assert_rejected(
            &ruleset,
            &state,
            &act(0, "triple", &cards),
            twice(param, cards[param_index(param)], first),
        );
    }
}

fn param_index(param: &str) -> usize {
    match param {
        "first" => 0,
        "second" => 1,
        _ => 2,
    }
}

// ---- the Rejection

#[test]
fn declaring_one_card_in_two_parameters_is_rejected() {
    let (ruleset, creation) = game();
    let state = creation.opening.state;
    let [a, b, _] = hand_of_seat_0(&state);

    let action = act(0, "pair", &[a, a]);
    let before = state.clone();
    // The exact variant, the GameState unchanged, and no Events: a Rejection has no Event Trace.
    assert_eq!(
        ruleset.transition(&state, &action),
        Err(twice("second", a, "first"))
    );
    assert_eq!(state, before);

    // Two different cards are accepted, in both orders.
    assert!(ruleset.transition(&state, &act(0, "pair", &[a, b])).is_ok());
    assert!(ruleset.transition(&state, &act(0, "pair", &[b, a])).is_ok());
}

#[test]
fn the_distinct_check_comes_after_the_unknown_card_and_before_the_other_binding_checks() {
    let (ruleset, creation) = game();
    let state = creation.opening.state;
    let [a, _, _] = hand_of_seat_0(&state);
    let unknown = CardInstanceId(999);

    // The argument count is checked first, then the existence of each card in param order.
    assert_rejected(
        &ruleset,
        &state,
        &act(0, "pair", &[a]),
        Rejection::ArgumentCountMismatch {
            expected: 2,
            found: 1,
        },
    );
    assert_rejected(
        &ruleset,
        &state,
        &act(0, "pair", &[a, unknown]),
        Rejection::UnknownCard {
            param: ParamName::new("second"),
            card: unknown,
        },
    );
    // `pair_checked` refuses every second card by its `where`, but a card already bound to the first
    // param is refused before the `where` is read; a different card meets the `where`.
    assert_rejected(
        &ruleset,
        &state,
        &act(0, "pair_checked", &[a, a]),
        twice("second", a, "first"),
    );
    let [_, b, _] = hand_of_seat_0(&state);
    assert_rejected(
        &ruleset,
        &state,
        &act(0, "pair_checked", &[a, b]),
        Rejection::WhereConditionNotMet {
            param: ParamName::new("second"),
            card: b,
        },
    );
}
