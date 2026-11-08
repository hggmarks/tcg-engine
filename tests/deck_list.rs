//! Acceptance tests for ticket #20: the typed `DeckList` and its validation against a ruleset's
//! deck inputs and a card catalog.
//!
//! Drafts are built in Rust, the way a code frontend would build them. Every error test asserts the
//! whole `DeckError`: kind and path.

mod common;

use common::{
    card, catalog_meta, creature_id, path, small_catalog, small_catalog_draft, small_definition,
    small_draft,
};
use tcg_engine::card_catalog::{CardCatalog, CardCatalogDraft, CardId};
use tcg_engine::deck_list::{
    DeckEntryDraft, DeckError, DeckErrorKind, DeckInputCardsDraft, DeckList, DeckListDraft,
};
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{
    CardTypeId, DeckInputId, ReferenceTarget, RulesetDefinition, RulesetId,
};

// ---- draft builders

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

/// 50 creatures: `creature_01` to `creature_12` four times each, `creature_13` twice.
fn main_deck() -> Vec<DeckEntryDraft> {
    let mut cards: Vec<DeckEntryDraft> = (1..=12).map(|n| entry(&creature_id(n), 4)).collect();
    cards.push(entry(&creature_id(13), 2));
    cards
}

fn leader() -> DeckInputCardsDraft {
    provided("leader", vec![entry("hero", 1)])
}

fn main_deck_input() -> DeckInputCardsDraft {
    provided("main_deck", main_deck())
}

fn mana_deck() -> DeckInputCardsDraft {
    provided("mana_deck", vec![entry("mana", 10)])
}

/// A legal deck, with the inputs in ruleset order.
fn legal_draft() -> DeckListDraft {
    DeckListDraft {
        inputs: vec![leader(), main_deck_input(), mana_deck()],
    }
}

fn validate(draft: DeckListDraft) -> Result<DeckList, DeckError> {
    DeckList::validate(&small_definition(), &small_catalog(), draft)
}

fn error_of(draft: DeckListDraft) -> DeckError {
    match validate(draft) {
        Err(error) => error,
        Ok(deck) => panic!("expected a validation error, got {deck:?}"),
    }
}

fn input_path(index: usize) -> DefinitionPath {
    path().key("inputs").index(index)
}

fn entry_path(input: usize, entry: usize) -> DefinitionPath {
    input_path(input).key("cards").index(entry)
}

/// A deck whose main deck is the given entries, the other inputs being legal.
fn with_main_deck(cards: Vec<DeckEntryDraft>) -> DeckListDraft {
    DeckListDraft {
        inputs: vec![leader(), provided("main_deck", cards), mana_deck()],
    }
}

/// A deck whose Leader input is the given entries, the other inputs being legal.
fn with_leaders(cards: Vec<DeckEntryDraft>) -> DeckListDraft {
    DeckListDraft {
        inputs: vec![provided("leader", cards), main_deck_input(), mana_deck()],
    }
}

fn described(deck: &DeckList, input: &str) -> Vec<(String, u32)> {
    deck.input(&DeckInputId::new(input))
        .expect("the input is in the deck list")
        .entries()
        .iter()
        .map(|entry| (entry.card().as_str().to_string(), entry.count().get()))
        .collect()
}

// ---- the model

#[test]
fn deck_list_accepts_a_legal_deck() {
    let deck = validate(legal_draft()).expect("legal deck");

    let inputs: Vec<&str> = deck.inputs().iter().map(|i| i.input().as_str()).collect();
    assert_eq!(inputs, vec!["leader", "main_deck", "mana_deck"]);

    assert_eq!(described(&deck, "leader"), vec![("hero".to_string(), 1)]);
    assert_eq!(
        described(&deck, "mana_deck"),
        vec![("mana".to_string(), 10)]
    );
    let mut expected: Vec<(String, u32)> = (1..=12).map(|n| (creature_id(n), 4)).collect();
    expected.push((creature_id(13), 2));
    assert_eq!(described(&deck, "main_deck"), expected);

    assert!(deck.input(&DeckInputId::new("sideboard")).is_none());
}

#[test]
fn deck_list_orders_inputs_by_ruleset() {
    let canonical = validate(legal_draft()).expect("legal deck");

    // The same inputs written in another order give the same deck list.
    let shuffled = DeckListDraft {
        inputs: vec![mana_deck(), main_deck_input(), leader()],
    };
    assert_eq!(validate(shuffled).expect("legal deck"), canonical);

    // The entries of an input keep the order of the file: it feeds the seeded shuffle.
    let mut reversed_cards = main_deck();
    reversed_cards.reverse();
    let reversed = validate(with_main_deck(reversed_cards)).expect("legal deck");
    assert_ne!(reversed, canonical);
    let mut expected: Vec<(String, u32)> = vec![(creature_id(13), 2)];
    expected.extend((1..=12).rev().map(|n| (creature_id(n), 4)));
    assert_eq!(described(&reversed, "main_deck"), expected);
}

// ---- inputs

#[test]
fn deck_list_rejects_missing_input() {
    // Without the mana deck.
    assert_eq!(
        error_of(DeckListDraft {
            inputs: vec![leader(), main_deck_input()],
        }),
        DeckError {
            path: path().key("inputs"),
            kind: DeckErrorKind::MissingInput {
                input: DeckInputId::new("mana_deck"),
            },
        }
    );

    // With no inputs at all, the first input of the ruleset is the one reported.
    assert_eq!(
        error_of(DeckListDraft { inputs: vec![] }).kind,
        DeckErrorKind::MissingInput {
            input: DeckInputId::new("leader"),
        }
    );

    // An input that is provided with no cards is provided: it fails on its size, not as missing.
    assert_eq!(
        error_of(with_main_deck(vec![])).kind,
        DeckErrorKind::WrongDeckSize {
            expected: 50,
            actual: 0,
        }
    );
}

#[test]
fn deck_list_rejects_duplicate_input() {
    let mut draft = legal_draft();
    draft
        .inputs
        .push(provided("leader", vec![entry("hero", 1)]));
    assert_eq!(
        error_of(draft),
        DeckError {
            path: input_path(3),
            kind: DeckErrorKind::DuplicateInput {
                input: DeckInputId::new("leader"),
                first: input_path(0),
            },
        }
    );
}

#[test]
fn deck_list_rejects_unknown_input() {
    let mut draft = legal_draft();
    draft
        .inputs
        .push(provided("sideboard", vec![entry("hero", 1)]));
    assert_eq!(
        error_of(draft),
        DeckError {
            path: input_path(3).key("input"),
            kind: DeckErrorKind::DanglingReference {
                target: ReferenceTarget::DeckInput,
                id: "sideboard".to_string(),
            },
        }
    );
}

// ---- entries

#[test]
fn deck_list_rejects_zero_count() {
    let mut cards = main_deck();
    cards[2] = entry(&creature_id(3), 0);
    assert_eq!(
        error_of(with_main_deck(cards)),
        DeckError {
            path: entry_path(1, 2).key("count"),
            kind: DeckErrorKind::ZeroCount,
        }
    );

    // The count is checked before the card is looked up.
    let cards = vec![entry("ghost", 0)];
    assert_eq!(
        error_of(with_main_deck(cards)).kind,
        DeckErrorKind::ZeroCount
    );
}

#[test]
fn deck_list_rejects_unknown_card() {
    let mut cards = main_deck();
    cards.insert(2, entry("ghost", 1));
    assert_eq!(
        error_of(with_main_deck(cards)),
        DeckError {
            path: entry_path(1, 2).key("card"),
            kind: DeckErrorKind::DanglingReference {
                target: ReferenceTarget::Card,
                id: "ghost".to_string(),
            },
        }
    );
}

#[test]
fn deck_list_rejects_duplicate_card_entry() {
    // Two entries of 2 would add up to a legal 4 copies, but a card is listed once.
    let mut cards = vec![entry(&creature_id(1), 2), entry(&creature_id(2), 4)];
    cards.push(entry(&creature_id(1), 2));
    assert_eq!(
        error_of(with_main_deck(cards)),
        DeckError {
            path: entry_path(1, 2),
            kind: DeckErrorKind::DuplicateCard {
                card: CardId::new(creature_id(1)),
                first: entry_path(1, 0),
            },
        }
    );
}

#[test]
fn deck_list_rejects_card_type_not_allowed() {
    // A creature in the Leader input.
    let draft = with_leaders(vec![entry("hero", 1), entry(&creature_id(1), 1)]);
    assert_eq!(
        error_of(draft),
        DeckError {
            path: entry_path(0, 1).key("card"),
            kind: DeckErrorKind::CardTypeNotAllowed {
                card: CardId::new(creature_id(1)),
                card_type: CardTypeId::new("creature"),
            },
        }
    );

    // A Leader in the main deck.
    let mut cards = main_deck();
    cards.push(entry("hero", 1));
    assert_eq!(
        error_of(with_main_deck(cards)).kind,
        DeckErrorKind::CardTypeNotAllowed {
            card: CardId::new("hero"),
            card_type: CardTypeId::new("leader"),
        }
    );
}

#[test]
fn deck_list_rejects_too_many_copies() {
    // The copies are checked on the entry, before the total size of the input.
    let mut cards = main_deck();
    cards[0] = entry(&creature_id(1), 5);
    assert_eq!(
        error_of(with_main_deck(cards)),
        DeckError {
            path: entry_path(1, 0).key("count"),
            kind: DeckErrorKind::TooManyCopies {
                card: CardId::new(creature_id(1)),
                count: 5,
                max_copies: 4,
            },
        }
    );
}

#[test]
fn deck_list_accepts_max_copies_exactly() {
    // 4 copies of a card pass: every entry of the legal deck has 4 except the last.
    let deck = validate(legal_draft()).expect("4 copies are within the limit");
    let first = &deck
        .input(&DeckInputId::new("main_deck"))
        .expect("main deck")
        .entries()[0];
    assert_eq!(first.card(), &CardId::new(creature_id(1)));
    assert_eq!(first.count().get(), 4);

    // 5 fail, even when the total stays at 50.
    let mut cards = main_deck();
    cards[0] = entry(&creature_id(1), 5);
    cards[12] = entry(&creature_id(13), 1);
    assert_eq!(
        error_of(with_main_deck(cards)).kind,
        DeckErrorKind::TooManyCopies {
            card: CardId::new(creature_id(1)),
            count: 5,
            max_copies: 4,
        }
    );
}

// ---- sizes

#[test]
fn deck_list_rejects_wrong_size() {
    // Too few: 49.
    let mut cards = main_deck();
    cards[12] = entry(&creature_id(13), 1);
    assert_eq!(
        error_of(with_main_deck(cards)),
        DeckError {
            path: input_path(1).key("cards"),
            kind: DeckErrorKind::WrongDeckSize {
                expected: 50,
                actual: 49,
            },
        }
    );

    // Too many: 51.
    let mut cards = main_deck();
    cards[12] = entry(&creature_id(13), 3);
    assert_eq!(
        error_of(with_main_deck(cards)).kind,
        DeckErrorKind::WrongDeckSize {
            expected: 50,
            actual: 51,
        }
    );

    // The mana deck is checked like any input.
    let draft = DeckListDraft {
        inputs: vec![
            leader(),
            main_deck_input(),
            provided("mana_deck", vec![entry("mana", 9)]),
        ],
    };
    assert_eq!(
        error_of(draft),
        DeckError {
            path: input_path(2).key("cards"),
            kind: DeckErrorKind::WrongDeckSize {
                expected: 10,
                actual: 9,
            },
        }
    );
}

#[test]
fn deck_list_requires_exactly_one_leader() {
    let wrong = |actual: u64| DeckError {
        path: input_path(0).key("cards"),
        kind: DeckErrorKind::WrongDeckSize {
            expected: 1,
            actual,
        },
    };

    // No Leader.
    assert_eq!(error_of(with_leaders(vec![])), wrong(0));
    // Two copies of one Leader: the Leader input has no copy limit, only its size.
    assert_eq!(error_of(with_leaders(vec![entry("hero", 2)])), wrong(2));
    // Two different Leaders.
    assert_eq!(
        error_of(with_leaders(vec![
            entry("hero", 1),
            entry("second_hero", 1)
        ])),
        wrong(2)
    );
    // One Leader, whichever it is.
    assert!(validate(with_leaders(vec![entry("second_hero", 1)])).is_ok());
}

// ---- catalog and ruleset

#[test]
fn deck_list_rejects_catalog_from_other_ruleset() {
    let definition = small_definition();

    // A catalog written for another ruleset id.
    let mut other_ruleset = small_draft();
    other_ruleset.meta.id = "other".to_string();
    let other_definition = RulesetDefinition::validate(other_ruleset).expect("valid ruleset");
    let other_catalog = CardCatalog::validate(
        &other_definition,
        CardCatalogDraft {
            meta: catalog_meta("other", 1),
            cards: vec![card("mana", "mana", vec![])],
        },
    )
    .expect("valid catalog");
    assert_eq!(
        DeckList::validate(&definition, &other_catalog, legal_draft()),
        Err(DeckError {
            path: path(),
            kind: DeckErrorKind::CatalogFromOtherRuleset {
                expected_id: RulesetId::new("small"),
                expected_version: definition.meta().version(),
                found_id: RulesetId::new("other"),
                found_version: other_definition.meta().version(),
            },
        })
    );

    // A catalog written for another version of the same ruleset.
    let mut newer_ruleset = small_draft();
    newer_ruleset.meta.version = 2;
    let newer_definition = RulesetDefinition::validate(newer_ruleset).expect("valid ruleset");
    let newer_catalog = CardCatalog::validate(
        &newer_definition,
        CardCatalogDraft {
            meta: catalog_meta("small", 2),
            cards: small_catalog_draft().cards,
        },
    )
    .expect("valid catalog");
    assert_eq!(
        DeckList::validate(&definition, &newer_catalog, legal_draft()),
        Err(DeckError {
            path: path(),
            kind: DeckErrorKind::CatalogFromOtherRuleset {
                expected_id: RulesetId::new("small"),
                expected_version: definition.meta().version(),
                found_id: RulesetId::new("small"),
                found_version: newer_definition.meta().version(),
            },
        })
    );
}

// ---- first error wins

#[test]
fn deck_list_checks_inputs_in_file_order_and_missing_inputs_last() {
    // The main deck, listed first, is wrong; the leader, listed second, is wrong too; the mana
    // deck is missing. The first provided input in file order is reported.
    let draft = DeckListDraft {
        inputs: vec![
            provided("main_deck", vec![entry("ghost", 1)]),
            provided("leader", vec![]),
        ],
    };
    assert_eq!(
        error_of(draft),
        DeckError {
            path: entry_path(0, 0).key("card"),
            kind: DeckErrorKind::DanglingReference {
                target: ReferenceTarget::Card,
                id: "ghost".to_string(),
            },
        }
    );

    // The size of an input is checked after all its entries, so a bad entry that comes after a
    // short run of good ones wins over the size.
    let cards = vec![entry(&creature_id(1), 4), entry("ghost", 1)];
    assert_eq!(
        error_of(with_main_deck(cards)).path,
        entry_path(1, 1).key("card")
    );

    // A wrong size in one input is reported before a missing input.
    let draft = DeckListDraft {
        inputs: vec![provided("leader", vec![])],
    };
    assert_eq!(
        error_of(draft).kind,
        DeckErrorKind::WrongDeckSize {
            expected: 1,
            actual: 0,
        }
    );
}
