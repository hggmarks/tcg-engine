//! Acceptance tests for ticket #21: the declarative Ruleset's GameState, `apply`, and Game creation,
//! built from code the way a code frontend would build them.
//!
//! The behavior on the real sample files (shuffle, draw, instance state, determinism) is tested in
//! the loader crate, in `crates/tcg-loader/tests/create_game.rs`.

mod common;

use common::{
    card, catalog_meta, creature_id, path, small_catalog, small_catalog_draft, small_definition,
    small_draft,
};
use tcg_engine::card_catalog::{CardCatalog, CardCatalogDraft, CardId};
use tcg_engine::deck_list::{DeckEntryDraft, DeckInputCardsDraft, DeckList, DeckListDraft};
use tcg_engine::declarative::{
    ApplyError, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameCreationError,
    GameState,
};
use tcg_engine::player::{PlayerId, PlayerOrder};
use tcg_engine::random::{RandomState, Seed};
use tcg_engine::ruleset::Ruleset;
use tcg_engine::ruleset_definition::{
    DefinitionError, DefinitionErrorKind, FieldTypeDraft, FieldValue, InitializerDraft,
    InstanceStateDraft, MetaDraft, RulesetDefinition, RulesetDraft, SetupStepDraft, StateName,
    ZoneId,
};
use tcg_engine::runtime::Runtime;

// ---- builders

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn instance(id: u32) -> CardInstanceId {
    CardInstanceId(id)
}

fn entry(card: &str, count: u32) -> DeckEntryDraft {
    DeckEntryDraft {
        card: card.to_string(),
        count,
    }
}

fn provided(input: &str, cards: Vec<DeckEntryDraft>) -> DeckInputCardsDraft {
    DeckInputCardsDraft {
        input: input.to_string(),
        cards,
    }
}

/// 50 creatures: `creature_01` to `creature_12` four times each, then `last` twice.
fn main_deck(last: &str) -> DeckInputCardsDraft {
    let mut cards: Vec<DeckEntryDraft> = (1..=12).map(|n| entry(&creature_id(n), 4)).collect();
    cards.push(entry(last, 2));
    provided("main_deck", cards)
}

fn deck_draft(leader: &str, last_creature: &str) -> DeckListDraft {
    DeckListDraft {
        inputs: vec![
            provided("leader", vec![entry(leader, 1)]),
            main_deck(last_creature),
            provided("mana_deck", vec![entry("mana", 10)]),
        ],
    }
}

/// A legal deck for the code-built Small Ruleset.
fn deck(leader: &str) -> DeckList {
    DeckList::validate(
        &small_definition(),
        &small_catalog(),
        deck_draft(leader, &creature_id(13)),
    )
    .expect("a legal deck")
}

/// One deck per seat: seat 0 plays `hero`, seat 1 plays `second_hero`.
fn decks() -> Vec<DeckList> {
    vec![deck("hero"), deck("second_hero")]
}

fn ruleset() -> DeclarativeRuleset {
    DeclarativeRuleset::new(small_definition(), small_catalog())
}

fn create() -> GameCreation {
    ruleset()
        .create_game(&decks(), Seed(1))
        .expect("the sample decks create a game")
}

fn creation_error(ruleset: &DeclarativeRuleset, decks: &[DeckList]) -> GameCreationError {
    match ruleset.create_game(decks, Seed(1)) {
        Err(error) => error,
        Ok(_) => panic!("expected Game creation to fail"),
    }
}

/// A ruleset whose draft is the code-built Small Ruleset changed by `edit`, with the code-built
/// catalog (which only needs the same id and version).
fn ruleset_from(edit: impl FnOnce(&mut RulesetDraft)) -> DeclarativeRuleset {
    let mut draft = small_draft();
    edit(&mut draft);
    let definition = RulesetDefinition::validate(draft).expect("a valid edited draft");
    DeclarativeRuleset::new(definition, small_catalog())
}

fn some_card(card: u32, owner: u32, zone_id: &str) -> Event {
    Event::CardCreated {
        card: instance(card),
        owner: p(owner),
        definition: CardId::new(creature_id(1)),
        zone: zone(zone_id),
        state: vec![(StateName::new("tapped"), FieldValue::Bool(false))],
    }
}

fn moved(card: u32, player: u32, from: &str, to: &str) -> Event {
    Event::CardMoved {
        card: instance(card),
        player: p(player),
        from: zone(from),
        to: zone(to),
    }
}

fn shuffled(player: u32, zone_id: &str, order: &[u32]) -> Event {
    Event::ZoneShuffled {
        player: p(player),
        zone: zone(zone_id),
        order: order.iter().copied().map(instance).collect(),
        random_state: RandomState::from_seed(Seed(3)),
    }
}

/// The empty state with `events` applied, each of which must be accepted.
fn state_after(events: &[Event]) -> GameState {
    let mut state = create().empty;
    for event in events {
        state.apply(event).expect("the event is accepted");
    }
    state
}

fn cards_in(state: &GameState, player: u32, zone_id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(player), &zone(zone_id))
        .expect("the zone exists")
        .to_vec()
}

fn ids(values: &[u32]) -> Vec<CardInstanceId> {
    values.iter().copied().map(instance).collect()
}

/// Asserts that applying `event` fails with `expected` and leaves the state as it was.
fn assert_rejected_atomically(mut state: GameState, event: &Event, expected: ApplyError) {
    let before = state.clone();
    assert_eq!(state.apply(event), Err(expected));
    assert_eq!(
        state, before,
        "a rejected Event must leave the state intact"
    );
}

// ---- the GameState: Players, Active Player, zones

#[test]
fn players_are_ordered_by_seat() {
    let creation = create();
    let seats = [p(0), p(1)];
    assert_eq!(creation.empty.player_order().players(), seats);
    assert_eq!(creation.opening.state.player_order().players(), seats);

    // `by_seat` is the order the engine builds for any Player Count.
    let count = small_definition().parameters().player_count();
    assert_eq!(PlayerOrder::by_seat(count).players(), seats);
}

#[test]
fn active_player_is_the_starting_player() {
    // The sample has `starting_player: 0`.
    assert_eq!(create().opening.state.active_player(), p(0));
    assert_eq!(create().empty.active_player(), p(0));

    // A ruleset that starts at seat 1.
    let ruleset = ruleset_from(|draft| draft.parameters.starting_player = 1);
    let creation = ruleset
        .create_game(&decks(), Seed(1))
        .expect("the decks create a game");
    assert_eq!(creation.empty.active_player(), p(1));
    assert_eq!(creation.opening.state.active_player(), p(1));
}

#[test]
fn each_player_has_every_zone_of_the_ruleset_definition() {
    let definition = small_definition();
    let declared: Vec<&ZoneId> = definition.zones().iter().map(|zone| zone.id()).collect();
    let creation = create();
    for seat in [p(0), p(1)] {
        let held: Vec<&ZoneId> = creation
            .empty
            .zones(seat)
            .expect("the seat has zones")
            .iter()
            .map(|zone_cards| zone_cards.zone())
            .collect();
        assert_eq!(held, declared, "zones of {seat:?}, in declared order");
        assert!(creation.empty.zone(seat, &zone("trash")).is_some());
        assert!(creation.empty.zone(seat, &zone("nowhere")).is_none());
    }
    assert!(creation.empty.zones(p(2)).is_none());
    assert!(creation.empty.zone(p(2), &zone("deck")).is_none());
}

// ---- apply

#[test]
fn apply_places_cards_on_top_and_moves_a_card_from_anywhere_in_its_zone() {
    // The top of a Zone is the end of its list.
    let state = state_after(&[
        some_card(0, 0, "deck"),
        some_card(1, 0, "deck"),
        some_card(2, 0, "deck"),
        some_card(3, 1, "deck"),
    ]);
    assert_eq!(cards_in(&state, 0, "deck"), ids(&[0, 1, 2]));
    assert_eq!(cards_in(&state, 1, "deck"), ids(&[3]));
    assert_eq!(state.cards().len(), 4);
    assert_eq!(state.card(instance(2)).map(|card| card.owner()), Some(p(0)));

    // The middle card moves; the others keep their order; it lands on top of the destination.
    let state = state_after(&[
        some_card(0, 0, "deck"),
        some_card(1, 0, "deck"),
        some_card(2, 0, "deck"),
        moved(1, 0, "deck", "hand"),
        moved(2, 0, "deck", "hand"),
    ]);
    assert_eq!(cards_in(&state, 0, "deck"), ids(&[0]));
    assert_eq!(cards_in(&state, 0, "hand"), ids(&[1, 2]));
}

#[test]
fn apply_zone_shuffled_replaces_the_order_and_the_random_state() {
    let mut state = state_after(&[
        some_card(0, 0, "deck"),
        some_card(1, 0, "deck"),
        some_card(2, 0, "deck"),
    ]);
    let event = shuffled(0, "deck", &[2, 0, 1]);
    state.apply(&event).expect("a permutation is accepted");
    assert_eq!(cards_in(&state, 0, "deck"), ids(&[2, 0, 1]));
    assert_eq!(state.random_state(), &RandomState::from_seed(Seed(3)));

    // A zone of one card is a valid Event too.
    let mut state = state_after(&[some_card(0, 0, "deck")]);
    state
        .apply(&shuffled(0, "deck", &[0]))
        .expect("a one-card zone shuffles");
    assert_eq!(cards_in(&state, 0, "deck"), ids(&[0]));
    assert_eq!(state.random_state(), &RandomState::from_seed(Seed(3)));

    // So is an empty zone.
    let mut state = create().empty;
    assert_ne!(state.random_state(), &RandomState::from_seed(Seed(3)));
    state
        .apply(&shuffled(1, "deck", &[]))
        .expect("an empty zone shuffles");
    assert_eq!(cards_in(&state, 1, "deck"), ids(&[]));
    assert_eq!(state.random_state(), &RandomState::from_seed(Seed(3)));
}

#[test]
fn apply_trace_folds_events_over_a_clone_and_leaves_the_original_untouched() {
    let empty = create().empty;
    let before = empty.clone();
    let events = [some_card(0, 0, "deck"), moved(0, 0, "deck", "hand")];
    let next = empty.apply_trace(&events).expect("the trace applies");
    assert_eq!(empty, before);
    assert_eq!(cards_in(&next, 0, "hand"), ids(&[0]));

    // The first rejected Event ends the fold with its error.
    let events = [some_card(0, 0, "deck"), some_card(5, 0, "deck")];
    assert_eq!(
        empty.apply_trace(&events),
        Err(ApplyError::CardIdOutOfSequence { found: instance(5) })
    );
    assert_eq!(empty, before);
}

#[test]
fn apply_rejects_unknown_player() {
    let state = state_after(&[some_card(0, 0, "deck")]);
    let unknown = ApplyError::UnknownPlayer { player: p(9) };
    assert_rejected_atomically(state.clone(), &some_card(1, 9, "deck"), unknown.clone());
    assert_rejected_atomically(state.clone(), &moved(0, 9, "deck", "hand"), unknown.clone());
    assert_rejected_atomically(state, &shuffled(9, "deck", &[0]), unknown);
}

#[test]
fn apply_rejects_unknown_zone() {
    let state = state_after(&[some_card(0, 0, "deck")]);
    let unknown = |zone_id: &str| ApplyError::UnknownZone {
        player: p(0),
        zone: zone(zone_id),
    };
    assert_rejected_atomically(
        state.clone(),
        &some_card(1, 0, "nowhere"),
        unknown("nowhere"),
    );
    // The destination is unknown, though the card is where the Event says: nothing moves.
    assert_rejected_atomically(
        state.clone(),
        &moved(0, 0, "deck", "nowhere"),
        unknown("nowhere"),
    );
    assert_rejected_atomically(
        state.clone(),
        &moved(0, 0, "nowhere", "hand"),
        unknown("nowhere"),
    );
    assert_rejected_atomically(state, &shuffled(0, "nowhere", &[0]), unknown("nowhere"));
}

#[test]
fn apply_rejects_card_id_out_of_sequence() {
    // Ids are dense from 0, in creation order.
    let empty = create().empty;
    assert_rejected_atomically(
        empty,
        &some_card(5, 0, "deck"),
        ApplyError::CardIdOutOfSequence { found: instance(5) },
    );
    let one = state_after(&[some_card(0, 0, "deck")]);
    assert_rejected_atomically(
        one.clone(),
        &some_card(0, 0, "deck"),
        ApplyError::CardIdOutOfSequence { found: instance(0) },
    );
    assert_rejected_atomically(
        one,
        &some_card(2, 0, "deck"),
        ApplyError::CardIdOutOfSequence { found: instance(2) },
    );
}

#[test]
fn apply_rejects_card_not_in_zone() {
    let state = state_after(&[some_card(0, 0, "deck"), some_card(1, 1, "deck")]);
    let not_in = |card: u32, player: u32, zone_id: &str| ApplyError::CardNotInZone {
        card: instance(card),
        player: p(player),
        zone: zone(zone_id),
    };
    // The card is in another zone of the Player.
    assert_rejected_atomically(
        state.clone(),
        &moved(0, 0, "hand", "field"),
        not_in(0, 0, "hand"),
    );
    // The card does not exist.
    assert_rejected_atomically(
        state.clone(),
        &moved(7, 0, "deck", "hand"),
        not_in(7, 0, "deck"),
    );
    // The card belongs to another Player's zone.
    assert_rejected_atomically(state, &moved(1, 0, "deck", "hand"), not_in(1, 0, "deck"));
}

#[test]
fn apply_rejects_not_a_permutation() {
    let state = state_after(&[
        some_card(0, 0, "deck"),
        some_card(1, 0, "deck"),
        some_card(2, 0, "deck"),
        some_card(3, 1, "deck"),
    ]);
    let not_a_permutation = ApplyError::NotAPermutation {
        player: p(0),
        zone: zone("deck"),
    };
    for order in [
        &[0, 1][..],       // a card is missing
        &[0, 1, 1][..],    // a card is repeated
        &[0, 1, 5][..],    // a card is not in the zone
        &[0, 1, 2, 2][..], // a card too many
        &[0, 1, 3][..],    // a card of another Player's zone
        &[][..],           // nothing
    ] {
        assert_rejected_atomically(
            state.clone(),
            &shuffled(0, "deck", order),
            not_a_permutation.clone(),
        );
    }
}

// ---- Game creation: errors, in the documented order

/// A catalog validated against a definition whose id is `id` and version `version`, with the cards
/// of the code-built catalog. Returns it with that definition.
fn catalog_for(id: &str, version: u32) -> (CardCatalog, RulesetDefinition) {
    let mut draft = small_draft();
    draft.meta = MetaDraft {
        id: id.to_string(),
        version,
    };
    let definition = RulesetDefinition::validate(draft).expect("a valid draft");
    let catalog = CardCatalog::validate(
        &definition,
        CardCatalogDraft {
            meta: catalog_meta(id, version),
            cards: small_catalog_draft().cards,
        },
    )
    .expect("a valid catalog");
    (catalog, definition)
}

#[test]
fn game_creation_rejects_catalog_for_another_ruleset() {
    // The error names the ruleset and version the catalog was written for.
    let (catalog, other) = catalog_for("other", 1);
    let ruleset = DeclarativeRuleset::new(small_definition(), catalog);
    assert_eq!(
        creation_error(&ruleset, &decks()),
        GameCreationError::CatalogForAnotherRuleset {
            ruleset: other.meta().id().clone(),
            ruleset_version: other.meta().version(),
        }
    );

    let (catalog, newer) = catalog_for("small", 2);
    let ruleset = DeclarativeRuleset::new(small_definition(), catalog);
    assert_eq!(
        creation_error(&ruleset, &decks()),
        GameCreationError::CatalogForAnotherRuleset {
            ruleset: newer.meta().id().clone(),
            ruleset_version: newer.meta().version(),
        }
    );

    // It is checked first: the wrong number of decks does not hide it.
    let (catalog, other) = catalog_for("other", 1);
    let ruleset = DeclarativeRuleset::new(small_definition(), catalog);
    assert_eq!(
        creation_error(&ruleset, &[]),
        GameCreationError::CatalogForAnotherRuleset {
            ruleset: other.meta().id().clone(),
            ruleset_version: other.meta().version(),
        }
    );
}

#[test]
fn game_creation_rejects_wrong_deck_count() {
    let ruleset = ruleset();
    for given in [vec![], vec![deck("hero")], {
        let mut three = decks();
        three.push(deck("hero"));
        three
    }] {
        assert_eq!(
            creation_error(&ruleset, &given),
            GameCreationError::WrongDeckCount {
                players: 2,
                decks: given.len(),
            }
        );
    }
}

#[test]
fn game_creation_rejects_deck_list_mismatch() {
    let ruleset = ruleset();

    // A deck validated against a ruleset that declares fewer inputs.
    let mut draft = small_draft();
    draft.inputs.truncate(2);
    draft.setup.clear();
    let fewer = RulesetDefinition::validate(draft).expect("a valid draft");
    let catalog = CardCatalog::validate(&fewer, small_catalog_draft()).expect("a valid catalog");
    let short_deck = DeckList::validate(
        &fewer,
        &catalog,
        DeckListDraft {
            inputs: vec![
                provided("leader", vec![entry("second_hero", 1)]),
                main_deck(&creature_id(13)),
            ],
        },
    )
    .expect("a legal deck for the smaller ruleset");
    assert_eq!(
        creation_error(&ruleset, &[deck("hero"), short_deck]),
        GameCreationError::DeckListMismatch { player: p(1) }
    );

    // A deck validated against a ruleset that declares the same inputs in another order.
    let mut draft = small_draft();
    draft.inputs.reverse();
    draft.setup.clear();
    let reversed = RulesetDefinition::validate(draft).expect("a valid draft");
    let catalog = CardCatalog::validate(&reversed, small_catalog_draft()).expect("a catalog");
    let reordered = DeckList::validate(&reversed, &catalog, deck_draft("hero", &creature_id(13)))
        .expect("a legal deck for the reordered ruleset");
    assert_eq!(
        creation_error(&ruleset, &[reordered, deck("second_hero")]),
        GameCreationError::DeckListMismatch { player: p(0) }
    );

    // A deck that lists a card the ruleset's catalog does not have.
    let mut extended = small_catalog_draft();
    extended.cards.push(card(
        "creature_99",
        "creature",
        vec![
            ("name", FieldValue::Text("Creature 99".to_string())),
            ("cost", FieldValue::Int(1)),
            ("power", FieldValue::Int(1)),
        ],
    ));
    let extended = CardCatalog::validate(&small_definition(), extended).expect("a catalog");
    let unknown_card = DeckList::validate(
        &small_definition(),
        &extended,
        deck_draft("hero", "creature_99"),
    )
    .expect("a legal deck for the extended catalog");
    assert_eq!(
        creation_error(&ruleset, &[unknown_card, deck("second_hero")]),
        GameCreationError::DeckListMismatch { player: p(0) }
    );
}

#[test]
fn game_creation_rejects_card_that_does_not_fit_definition() {
    // The definition's leader card type has `health` instead of `life`, so `hero` (a catalog card
    // with `life`) has no value for the field that `current_life` copies.
    let mut draft = small_draft();
    draft.card_types[0].fields[2].name = "health".to_string();
    draft.card_types[0].instance_state.push(InstanceStateDraft {
        name: "current_life".to_string(),
        state_type: FieldTypeDraft::Int,
        init: InitializerDraft::FromField("health".to_string()),
    });
    let definition = RulesetDefinition::validate(draft).expect("a valid draft");
    let ruleset = DeclarativeRuleset::new(definition, small_catalog());
    assert_eq!(
        creation_error(&ruleset, &decks()),
        GameCreationError::CardDoesNotFitDefinition {
            card: CardId::new("hero")
        }
    );

    // The definition has no card type named like the card's.
    let mut draft = small_draft();
    draft.card_types[0].id = "champion".to_string();
    draft.inputs[0].card_types = vec!["champion".to_string()];
    let definition = RulesetDefinition::validate(draft).expect("a valid draft");
    let ruleset = DeclarativeRuleset::new(definition, small_catalog());
    assert_eq!(
        creation_error(&ruleset, &decks()),
        GameCreationError::CardDoesNotFitDefinition {
            card: CardId::new("hero")
        }
    );
}

#[test]
fn setup_draw_from_an_empty_zone_fails_game_creation() {
    let draw = |from: &str, to: &str, count: u32| SetupStepDraft::Draw {
        player: "each".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        count,
    };

    // The hand starts empty; the step index counts every step, shuffles included.
    let ruleset = ruleset_from(|draft| {
        draft.setup = vec![
            SetupStepDraft::Shuffle {
                player: "each".to_string(),
                zone: "deck".to_string(),
            },
            draw("hand", "deck", 1),
        ];
    });
    assert_eq!(
        creation_error(&ruleset, &decks()),
        GameCreationError::SetupDrawFromEmptyZone {
            step: 1,
            player: p(0),
            zone: zone("hand"),
        }
    );

    // The Leader Zone has one card: the second draw of the step finds it empty.
    let ruleset = ruleset_from(|draft| draft.setup = vec![draw("leader_zone", "hand", 2)]);
    assert_eq!(
        creation_error(&ruleset, &decks()),
        GameCreationError::SetupDrawFromEmptyZone {
            step: 0,
            player: p(0),
            zone: zone("leader_zone"),
        }
    );
}

#[test]
fn game_creation_without_setup_only_creates_cards() {
    let ruleset = ruleset_from(|draft| draft.setup.clear());
    let creation = ruleset
        .create_game(&decks(), Seed(1))
        .expect("the decks create a game");
    let events = creation.opening.trace.events();
    assert_eq!(events.len(), 122);
    assert!(events
        .iter()
        .all(|event| matches!(event, Event::CardCreated { .. })));
    // Nothing was shuffled, so the Random State is still the one of the Seed.
    assert_eq!(
        creation.opening.state.random_state(),
        &RandomState::from_seed(Seed(1))
    );
    assert_eq!(
        cards_in(&creation.opening.state, 0, "deck").len(),
        50,
        "the deck is whole and in creation order"
    );
}

#[test]
fn a_ruleset_definition_with_a_bad_setup_never_reaches_game_creation() {
    // `DeclarativeRuleset::new` takes a validated definition, so its setup was already checked.
    let mut draft = small_draft();
    draft.setup = vec![SetupStepDraft::Draw {
        player: "each".to_string(),
        from: "deck".to_string(),
        to: "hand".to_string(),
        count: 0,
    }];
    assert_eq!(
        RulesetDefinition::validate(draft),
        Err(DefinitionError {
            path: path().key("setup").index(0).key("draw").key("count"),
            kind: DefinitionErrorKind::InvalidDrawCount,
        })
    );
}

// ---- the seam

/// Accepts any Runtime over a Ruleset with the declarative GameState and Event types.
fn declarative_parts<R: Ruleset<State = GameState, Event = Event>>(_runtime: &Runtime<R>) {}

#[test]
fn declarative_ruleset_plugs_into_the_runtime() {
    // `Action` and `Rejection` have no variants until [10], so no Action can be written yet; the
    // Runtime is built and the Ruleset it holds is reachable.
    let runtime = Runtime::new(ruleset());
    declarative_parts(&runtime);
    assert_eq!(runtime.ruleset().definition(), &small_definition());
    assert_eq!(runtime.ruleset().catalog(), &small_catalog());
}
