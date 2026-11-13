//! The YAML loader for Ruleset Definitions, card catalogs and deck lists.
//!
//! Loading is two-stage (ADR-0004): first the ruleset, then the card catalog validated against it.
//! A deck list is Game input, validated against both. This crate is the syntax half of each step.
//! It turns YAML text into a plain-data draft ([`RulesetDraft`], [`CardCatalogDraft`],
//! [`DeckListDraft`]) and nothing more; every semantic check (ids, references, types, sizes) lives
//! in the engine's `validate` functions, so a code or script frontend gets identical errors.
//!
//! The pipeline is, in order:
//! 1. an event pass that rejects anchors, aliases and every tag, and tracks the path;
//! 2. `serde-saphyr`, with strict options, into private data-transfer types that reject unknown
//!    keys;
//! 3. conversion into the draft.
//!
//! The three file kinds share steps 1 and 2; each has its own data-transfer types. The first error
//! wins. See ADR-0008 and ADR-0009.

mod catalog_dto;
mod deck_dto;
mod dto;
mod error;
mod event_pass;
mod scalar;

use serde::de::DeserializeOwned;
use serde_saphyr::Error as SaphyrError;
use tcg_engine::card_catalog::{CardCatalog, CardCatalogDraft};
use tcg_engine::deck_list::{DeckList, DeckListDraft};
use tcg_engine::definition_path::{DefinitionPath, PathSegment};
use tcg_engine::ruleset_definition::{RulesetDefinition, RulesetDraft};

pub use error::{
    CatalogLoadError, DeckLoadError, LoadError, Location, YamlError, YamlErrorKind, YamlFeature,
};

use catalog_dto::CatalogDto;
use deck_dto::DeckListDto;
use dto::RulesetDto;

/// Loads a Ruleset Definition: stage 1 of ADR-0004's two-stage loading.
pub fn load_ruleset_definition(yaml: &str) -> Result<RulesetDefinition, LoadError> {
    let draft = parse_ruleset_draft(yaml)?;
    Ok(RulesetDefinition::validate(draft)?)
}

/// Parses YAML text into a draft without checking its meaning: syntax only.
pub fn parse_ruleset_draft(yaml: &str) -> Result<RulesetDraft, YamlError> {
    Ok(parse::<RulesetDto>(yaml)?.into_draft())
}

/// Loads a card catalog and validates it against the ruleset it targets: stage 2 of ADR-0004's
/// two-stage loading.
pub fn load_card_catalog(
    ruleset: &RulesetDefinition,
    yaml: &str,
) -> Result<CardCatalog, CatalogLoadError> {
    let draft = parse_card_catalog_draft(yaml)?;
    Ok(CardCatalog::validate(ruleset, draft)?)
}

/// Parses YAML text into a card catalog draft without checking its meaning: syntax only.
pub fn parse_card_catalog_draft(yaml: &str) -> Result<CardCatalogDraft, YamlError> {
    Ok(parse::<CatalogDto>(yaml)?.into_draft())
}

/// Loads a deck list and validates it against the ruleset's deck inputs and the card catalog.
pub fn load_deck_list(
    ruleset: &RulesetDefinition,
    catalog: &CardCatalog,
    yaml: &str,
) -> Result<DeckList, DeckLoadError> {
    let draft = parse_deck_list_draft(yaml)?;
    Ok(DeckList::validate(ruleset, catalog, draft)?)
}

/// Parses YAML text into a deck list draft without checking its meaning: syntax only.
pub fn parse_deck_list_draft(yaml: &str) -> Result<DeckListDraft, YamlError> {
    Ok(parse::<DeckListDto>(yaml)?.into_draft())
}

/// The event pass, then the strict deserialization into a data-transfer type.
fn parse<T: DeserializeOwned>(yaml: &str) -> Result<T, YamlError> {
    event_pass::check(yaml)?;
    deserialize(yaml)
}

/// The parser options that enforce ADR-0004's restricted subset (docs/research/yaml-parser.md).
/// `duplicate_keys` is deliberately left at its default, which is an error.
fn strict_options() -> serde_saphyr::Options {
    serde_saphyr::options! {
        strict_booleans: true,
        merge_keys: serde_saphyr::options::MergeKeyPolicy::Error,
        reject_unsupported_tags: true,
        reject_tagged_field_names: true,
        budget: serde_saphyr::budget! { max_anchors: 0, max_aliases: 0 },
        with_snippet: false,
    }
}

fn deserialize<T: DeserializeOwned>(yaml: &str) -> Result<T, YamlError> {
    let mut path = DefinitionPath::root();
    let result =
        serde_saphyr::with_deserializer_from_str_with_options(yaml, strict_options(), |de| {
            serde_path_to_error::deserialize::<_, T>(de).map_err(|error| {
                path = positional_path(error.path());
                error.into_inner()
            })
        });
    result.map_err(|error| classify(error, path))
}

/// The positional path that `serde_path_to_error` tracked. A segment whose key it could not name
/// (`?`) is dropped.
fn positional_path(path: &serde_path_to_error::Path) -> DefinitionPath {
    use serde_path_to_error::Segment;
    path.iter()
        .fold(DefinitionPath::root(), |path, segment| match segment {
            Segment::Seq { index } => path.index(*index),
            Segment::Map { key } => path.key(key.as_str()),
            Segment::Enum { variant } => path.key(variant.as_str()),
            Segment::Unknown => path,
        })
}

/// Classifies by the error's variant, never by its message text.
fn classify(error: SaphyrError, path: DefinitionPath) -> YamlError {
    let mut error = &error;
    while let SaphyrError::WithSnippet { error: inner, .. } = error {
        error = inner;
    }
    let (kind, path) = match error {
        SaphyrError::SerdeUnknownField { field, .. } => {
            (YamlErrorKind::UnknownKey { key: field.clone() }, path)
        }
        // An unknown primitive (of a step, an effect or a condition), at exactly the map that
        // names it: an unknown key of that map. Any other unknown enum value (`visibility`,
        // `type`) stays `Malformed`.
        SaphyrError::SerdeUnknownVariant { variant, .. } if names_a_primitive(&path) => (
            YamlErrorKind::UnknownKey {
                key: variant.clone(),
            },
            path,
        ),
        SaphyrError::DuplicateMappingKey { key, .. } => {
            let path = match key {
                Some(key) => path.key(key.as_str()),
                None => path,
            };
            (forbidden(YamlFeature::DuplicateKey), path)
        }
        SaphyrError::MergeKeyNotAllowed { .. } => {
            (forbidden(YamlFeature::MergeKey), path.key("<<"))
        }
        SaphyrError::MultipleDocuments { .. } => (forbidden(YamlFeature::MultipleDocuments), path),
        SaphyrError::InvalidBooleanStrict { .. } | SaphyrError::NullIntoString { .. } => {
            (forbidden(YamlFeature::ImplicitTyping), path)
        }
        _ => (
            YamlErrorKind::Malformed {
                message: error.to_string(),
            },
            path,
        ),
    };
    YamlError {
        path,
        location: error.location().map(|location| Location {
            line: usize::try_from(location.line()).unwrap_or(usize::MAX),
            column: usize::try_from(location.column()).unwrap_or(usize::MAX),
        }),
        kind,
    }
}

/// Whether `path` is a single-key map `{ <primitive>: ... }` whose primitive is chosen by name: an
/// element of `setup` (`setup[1]`), of an automatic phase or an `if` step
/// (`turn.phases[0].automatic[2]`, `turn.phases[0].automatic[2].else[0]`) or of an action's
/// `effects` or `cost` (`actions[0].effects[0]`, `actions[0].cost[0]`), the condition of an `if`
/// step or effect (`turn.phases[0].automatic[2].if`), the `where` of a param, the `when` of a win
/// condition (`win_conditions[0].when`), or an operand of `eq`, `gte`, `lte` or `and`
/// (`actions[0].params[0].where.and[1]`).
fn names_a_primitive(path: &DefinitionPath) -> bool {
    match path.segments() {
        [PathSegment::Key(key), PathSegment::Index(_)] if key == "setup" => true,
        [.., PathSegment::Key(key), PathSegment::Index(_)] => {
            matches!(
                key.as_str(),
                "automatic"
                    | "then"
                    | "else"
                    | "effects"
                    | "cost"
                    | "do"
                    | "eq"
                    | "gte"
                    | "lte"
                    | "and"
            )
        }
        [.., PathSegment::Key(key)] => matches!(key.as_str(), "if" | "when" | "where" | "while"),
        _ => false,
    }
}

fn forbidden(feature: YamlFeature) -> YamlErrorKind {
    YamlErrorKind::ForbiddenFeature { feature }
}
