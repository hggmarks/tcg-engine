//! The identifier grammar shared by rulesets, card catalogs and deck lists.

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

pub(crate) use id_type;

/// The words that YAML reads as a bool or a null, so a quoted and an unquoted spelling would
/// otherwise differ.
const RESERVED: [&str; 3] = ["true", "false", "null"];

/// Whether `id` is an Identifier: it matches `[a-z][a-z0-9_]*` and is not `true`, `false` or
/// `null`.
pub(crate) fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    starts_with_letter
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !RESERVED.contains(&id)
}
