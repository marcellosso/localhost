use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// The identity of a Project, Service, Profile or Run.
///
/// Written uppercase, as earlier (Swift) versions wrote UUIDs: in file names, in JSON
/// and in the Secret accounts (`<ID>/<NAME>`), so existing data keeps working.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Id(Uuid);

impl Id {
    pub fn new() -> Self {
        Id(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for Id {
    fn default() -> Self {
        Id::new()
    }
}

impl From<Uuid> for Id {
    fn from(uuid: Uuid) -> Self {
        Id(uuid)
    }
}

impl FromStr for Id {
    type Err = uuid::Error;

    /// Accepts upper- and lowercase.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Id)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buffer = Uuid::encode_buffer();
        f.write_str(self.0.hyphenated().encode_upper(&mut buffer))
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(|_| serde::de::Error::custom(format!("Invalid UUID \"{raw}\"")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_uppercase_and_reads_either_case() {
        let id: Id = "6f9a2c1e-0000-4000-8000-00000000000a".parse().unwrap();
        assert_eq!(id.to_string(), "6F9A2C1E-0000-4000-8000-00000000000A");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"6F9A2C1E-0000-4000-8000-00000000000A\"");
        assert_eq!(id, "6F9A2C1E-0000-4000-8000-00000000000A".parse().unwrap());
    }
}
