//! The private data-transfer types that mirror the deck list file, and their conversion into the
//! engine's plain-data draft. The same rules as in `dto.rs` apply.

use serde::Deserialize;
use tcg_engine::deck_list::{DeckEntryDraft, DeckInputCardsDraft, DeckListDraft};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeckListDto {
    inputs: Vec<DeckInputDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckInputDto {
    input: String,
    cards: Vec<DeckEntryDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeckEntryDto {
    card: String,
    count: u32,
}

impl DeckListDto {
    pub(crate) fn into_draft(self) -> DeckListDraft {
        DeckListDraft {
            inputs: self
                .inputs
                .into_iter()
                .map(|input| DeckInputCardsDraft {
                    input: input.input,
                    cards: input
                        .cards
                        .into_iter()
                        .map(|entry| DeckEntryDraft {
                            card: entry.card,
                            count: entry.count,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
