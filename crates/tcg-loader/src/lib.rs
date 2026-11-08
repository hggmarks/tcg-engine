//! The YAML loader for Ruleset Definitions.
//!
//! Loading is two-stage (ADR-0004): first the ruleset, then the card catalog validated against it.
//! This crate is the syntax half of stage 1. It turns YAML text into a plain-data
//! [`RulesetDraft`] and nothing more; every semantic check (ids, references, types, parameters)
//! lives in the engine's [`RulesetDefinition::validate`], so a code or script frontend gets
//! identical errors. The catalog stage ([06]) will take the resulting [`RulesetDefinition`].
//!
//! The pipeline is, in order:
//! 1. an event pass that rejects anchors, aliases and every tag, and tracks the path;
//! 2. `serde-saphyr`, with strict options, into private data-transfer types that reject unknown
//!    keys;
//! 3. a check that no unsupported behavior section (`setup`, `turn`, `actions`, `win_conditions`)
//!    is present;
//! 4. conversion into the draft.
//!
//! The first error wins. See ADR-0008.

mod dto;
mod error;
mod event_pass;

use serde_saphyr::Error as SaphyrError;
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::{RulesetDefinition, RulesetDraft};

pub use error::{LoadError, Location, YamlError, YamlErrorKind, YamlFeature};

use dto::RulesetDto;

/// Loads a Ruleset Definition: stage 1 of ADR-0004's two-stage loading. The catalog stage will
/// take `&RulesetDefinition`.
pub fn load_ruleset_definition(yaml: &str) -> Result<RulesetDefinition, LoadError> {
    let draft = parse_ruleset_draft(yaml)?;
    Ok(RulesetDefinition::validate(draft)?)
}

/// Parses YAML text into a draft without checking its meaning: syntax only.
pub fn parse_ruleset_draft(yaml: &str) -> Result<RulesetDraft, YamlError> {
    event_pass::check(yaml)?;
    let dto = deserialize(yaml)?;
    if let Some(section) = dto.unsupported_section() {
        return Err(YamlError {
            path: DefinitionPath::root().key(section),
            location: None,
            kind: YamlErrorKind::UnsupportedSection {
                section: section.to_string(),
            },
        });
    }
    Ok(dto.into_draft())
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

fn deserialize(yaml: &str) -> Result<RulesetDto, YamlError> {
    let mut path = DefinitionPath::root();
    let result =
        serde_saphyr::with_deserializer_from_str_with_options(yaml, strict_options(), |de| {
            serde_path_to_error::deserialize::<_, RulesetDto>(de).map_err(|error| {
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

fn forbidden(feature: YamlFeature) -> YamlErrorKind {
    YamlErrorKind::ForbiddenFeature { feature }
}
