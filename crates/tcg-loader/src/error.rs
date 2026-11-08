//! Errors of the YAML loader. They never expose `serde_saphyr` types.

use std::fmt;

use tcg_engine::card_catalog::CatalogError;
use tcg_engine::deck_list::DeckError;
use tcg_engine::definition_path::DefinitionPath;
use tcg_engine::ruleset_definition::DefinitionError;

/// A position in the YAML text, 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

/// A YAML feature that the ruleset format forbids (ADR-0004's restricted subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YamlFeature {
    Anchor,
    Alias,
    Tag,
    MergeKey,
    DuplicateKey,
    MultipleDocuments,
    /// A scalar that YAML would read as another type, such as `yes` for a bool or `~` for a text.
    ImplicitTyping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlErrorKind {
    ForbiddenFeature {
        feature: YamlFeature,
    },
    UnknownKey {
        key: String,
    },
    /// A behavior section (`setup`, `turn`, `actions`, `win_conditions`) that is not supported yet.
    UnsupportedSection {
        section: String,
    },
    /// Anything else that makes the text not a ruleset: bad syntax, a missing key, a wrong type.
    Malformed {
        message: String,
    },
}

/// A syntax-stage error. The path is positional (`card_types[1].fields[0]`) because ids are not
/// checked yet; `location` is the line and column of the problem, when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlError {
    pub path: DefinitionPath,
    pub location: Option<Location>,
    pub kind: YamlErrorKind,
}

/// Either stage of loading failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The text is not a ruleset in the supported YAML subset.
    Yaml(YamlError),
    /// The text parsed into a draft that the engine rejected.
    Definition(DefinitionError),
}

impl fmt::Display for YamlFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Anchor => "anchors are not allowed",
            Self::Alias => "aliases are not allowed",
            Self::Tag => "tags are not allowed",
            Self::MergeKey => "merge keys are not allowed",
            Self::DuplicateKey => "duplicate keys are not allowed",
            Self::MultipleDocuments => "only one document is allowed",
            Self::ImplicitTyping => "the value must be written as its declared type",
        })
    }
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            YamlErrorKind::ForbiddenFeature { feature } => write!(f, "{feature}")?,
            YamlErrorKind::UnknownKey { key } => write!(f, "unknown key `{key}`")?,
            YamlErrorKind::UnsupportedSection { section } => {
                write!(f, "the section `{section}` is not supported yet")?
            }
            YamlErrorKind::Malformed { message } => write!(f, "{message}")?,
        }
        if !self.path.segments().is_empty() {
            write!(f, " at `{}`", self.path)?;
        }
        if let Some(location) = self.location {
            write!(f, " (line {}, column {})", location.line, location.column)?;
        }
        Ok(())
    }
}

impl std::error::Error for YamlError {}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yaml(error) => error.fmt(f),
            Self::Definition(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<YamlError> for LoadError {
    fn from(error: YamlError) -> Self {
        Self::Yaml(error)
    }
}

impl From<DefinitionError> for LoadError {
    fn from(error: DefinitionError) -> Self {
        Self::Definition(error)
    }
}

/// Either stage of loading a card catalog failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogLoadError {
    /// The text is not a card catalog in the supported YAML subset.
    Yaml(YamlError),
    /// The text parsed into a draft that the engine rejected.
    Catalog(CatalogError),
}

impl fmt::Display for CatalogLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yaml(error) => error.fmt(f),
            Self::Catalog(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CatalogLoadError {}

impl From<YamlError> for CatalogLoadError {
    fn from(error: YamlError) -> Self {
        Self::Yaml(error)
    }
}

impl From<CatalogError> for CatalogLoadError {
    fn from(error: CatalogError) -> Self {
        Self::Catalog(error)
    }
}

/// Either stage of loading a deck list failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckLoadError {
    /// The text is not a deck list in the supported YAML subset.
    Yaml(YamlError),
    /// The text parsed into a draft that the engine rejected.
    Deck(DeckError),
}

impl fmt::Display for DeckLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yaml(error) => error.fmt(f),
            Self::Deck(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DeckLoadError {}

impl From<YamlError> for DeckLoadError {
    fn from(error: YamlError) -> Self {
        Self::Yaml(error)
    }
}

impl From<DeckError> for DeckLoadError {
    fn from(error: DeckError) -> Self {
        Self::Deck(error)
    }
}
