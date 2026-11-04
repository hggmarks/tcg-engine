//! Paths that locate an element of a Ruleset Definition, such as
//! `card_types[creature].fields[cost]`.

use std::fmt;

/// One step of a [`DefinitionPath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// A key of the ruleset format, such as `zones` or `instance_state`.
    Key(String),
    /// An element addressed by its id.
    Id(String),
    /// An element addressed by its position in its list.
    Index(usize),
}

/// The location of an element inside a Ruleset Definition. It displays keys joined by `.`, ids and
/// indexes in brackets, and the root as the empty string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionPath {
    segments: Vec<PathSegment>,
}

impl DefinitionPath {
    /// The path of the whole definition.
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// This path followed by a key of the format.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.segments.push(PathSegment::Key(key.into()));
        self
    }

    /// This path followed by an element addressed by id.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.segments.push(PathSegment::Id(id.into()));
        self
    }

    /// This path followed by an element addressed by position.
    pub fn index(mut self, index: usize) -> Self {
        self.segments.push(PathSegment::Index(index));
        self
    }

    /// The segments from the root.
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }
}

impl fmt::Display for DefinitionPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, segment) in self.segments.iter().enumerate() {
            match segment {
                PathSegment::Key(key) => {
                    if position > 0 {
                        f.write_str(".")?;
                    }
                    f.write_str(key)?;
                }
                PathSegment::Id(id) => write!(f, "[{id}]")?,
                PathSegment::Index(index) => write!(f, "[{index}]")?,
            }
        }
        Ok(())
    }
}
