use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::LocalhostError;
use crate::fold::fold;

/// The short, human-readable, stable handle of a Project or Service
/// (e.g. `my-saas`, `api`).
///
/// A Key is a slug: lowercase ASCII letters and digits in groups separated by
/// single hyphens, at most 64 characters. A `Key` value is always valid.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(String);

impl Key {
    pub const MAX_LENGTH: usize = 64;

    pub fn new(raw: impl Into<String>) -> Result<Key, LocalhostError> {
        let raw = raw.into();
        if !Key::is_valid(&raw) {
            return Err(LocalhostError::InvalidKey(raw));
        }
        Ok(Key(raw))
    }

    /// A Key derived from a display name, e.g. `"My SaaS"` → `my-saas`.
    /// `None` when the name has no usable characters.
    pub fn suggested_from(name: &str) -> Option<Key> {
        let mut slug = String::new();
        for c in fold(name).chars() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                slug.push(c);
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        }
        slug.truncate(Key::MAX_LENGTH);
        let slug = slug.trim_end_matches('-');
        Key::new(slug).ok()
    }

    /// A Key suggested from `name` (or `fallback`, itself a valid Key, if
    /// the name has no usable characters) that isn't taken: `-2`, `-3`, … is
    /// appended until `is_taken` says no.
    pub fn suggested_unique(name: &str, fallback: &str, is_taken: impl Fn(&str) -> bool) -> Key {
        debug_assert!(Key::is_valid(fallback));
        let base = Key::suggested_from(name).unwrap_or_else(|| Key(fallback.into()));
        if !is_taken(base.as_str()) {
            return base;
        }
        // Room for the suffix, without leaving a trailing hyphen before it.
        let stem: String = base.0.chars().take(Key::MAX_LENGTH - 4).collect();
        let stem = stem.trim_end_matches('-');
        (2..)
            .map(|suffix| Key(format!("{stem}-{suffix}")))
            .find(|candidate| !is_taken(candidate.as_str()))
            .expect("a free suffix")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_valid(raw: &str) -> bool {
        !raw.is_empty()
            && raw.chars().count() <= Key::MAX_LENGTH
            && raw
                .split('-')
                .all(|group| !group.is_empty() && group.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Key({})", self.0)
    }
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Key::new(raw.clone()).map_err(|_| serde::de::Error::custom(format!("Invalid key \"{raw}\"")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_slugs() {
        for raw in ["api", "my-saas", "web2", "a-b-c", "0", &"a".repeat(64)] {
            assert_eq!(Key::new(raw).unwrap().as_str(), raw);
        }
    }

    #[test]
    fn rejects_anything_that_is_not_a_slug() {
        for raw in ["", "API", "my saas", "-api", "api-", "my--saas", "api_v2", "café", &"a".repeat(65)] {
            assert_eq!(Key::new(raw), Err(LocalhostError::InvalidKey(raw.to_string())));
        }
    }

    #[test]
    fn suggests_a_key_from_a_name() {
        for (name, expected) in [
            ("My SaaS", "my-saas"),
            ("  Café API  ", "cafe-api"),
            ("web_frontend (v2)", "web-frontend-v2"),
            ("Docker: Postgres!", "docker-postgres"),
        ] {
            assert_eq!(Key::suggested_from(name).unwrap().as_str(), expected);
        }
    }

    #[test]
    fn suggestion_is_none_when_the_name_has_no_usable_characters() {
        assert_eq!(Key::suggested_from("!!! "), None);
    }

    #[test]
    fn suggestion_is_cut_to_the_maximum_length() {
        let key = Key::suggested_from(&"ab ".repeat(40)).unwrap();
        assert!(key.as_str().len() <= Key::MAX_LENGTH);
        assert!(!key.as_str().ends_with('-'));
    }

    #[test]
    fn unique_suggestion_appends_a_number_until_free() {
        let taken = ["my-saas", "my-saas-2"];
        let key = Key::suggested_unique("My SaaS", "project", |key| taken.contains(&key));
        assert_eq!(key.as_str(), "my-saas-3");
        assert_eq!(Key::suggested_unique("My SaaS", "project", |_| false).as_str(), "my-saas");
    }

    #[test]
    fn unique_suggestion_falls_back_when_the_name_has_no_usable_characters() {
        assert_eq!(Key::suggested_unique("!!!", "service", |_| false).as_str(), "service");
        assert_eq!(Key::suggested_unique("", "service", |key| key == "service").as_str(), "service-2");
    }

    #[test]
    fn unique_suggestion_stays_valid_at_the_maximum_length() {
        // 64 characters; cut to 60 for the suffix, it would end in a hyphen.
        let name = format!("{} bbbb", "a".repeat(59));
        let key = Key::suggested_unique(&name, "project", |key| key.len() == Key::MAX_LENGTH);
        assert_eq!(key.as_str(), format!("{}-2", "a".repeat(59)));
        assert!(Key::new(key.as_str()).is_ok());
    }
}
