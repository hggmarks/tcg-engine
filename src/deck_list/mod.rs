//! The typed Deck List: what one Player provides for each Deck Input of a ruleset, validated against
//! the ruleset and a Card Catalog. Only [`DeckList::validate`] builds one, so later Game creation
//! can require a legal deck.
//!
//! A frontend fills a plain-data [`DeckListDraft`] and calls [`DeckList::validate`].
//!
//! Validation reports the first error only, in a fixed order: the catalog against the ruleset
//! (id, then version), then each provided input in file order, then every ruleset input provided.
//! Within an input: it is known, it is not provided twice, then each entry in file order (count at
//! least 1, card known, card not listed twice, card type allowed, count within the copy limit),
//! then the exact total size. A duplicate is rejected, never summed.
//!
//! A [`DeckList`] stores its inputs in ruleset order and the entries of each input in file order.
//! The order of the entries is what a seeded shuffle will start from, so it is deterministic.

mod draft;
mod error;
mod model;
mod validate;

pub use draft::{DeckEntryDraft, DeckInputCardsDraft, DeckListDraft};
pub use error::{DeckError, DeckErrorKind};
pub use model::{DeckEntry, DeckList, DeckListInput};
