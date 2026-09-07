//! Entity identifiers.
//!
//! Every entity carries a ULID: 26 characters, generated locally with no coordination, and
//! lexicographically sortable by creation time to millisecond resolution (within a single
//! millisecond the trailing bits are random, so order there is arbitrary). The ID lives *in the file*, never in the filename —
//! filenames are human-chosen slugs and may be renamed freely, by the app or by the user in a
//! file manager, without breaking a single link or citation.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(ulid::Ulid);

impl Id {
    /// A new identifier, timestamped now.
    pub fn new() -> Self {
        Id(ulid::Ulid::new())
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Id {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        ulid::Ulid::from_string(s)
            .map(Id)
            .map_err(|_| Error::invalid("id", format!("{s:?} is not a valid identifier")))
    }
}

impl TryFrom<String> for Id {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        s.parse()
    }
}

impl From<Id> for String {
    fn from(id: Id) -> String {
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_a_string() {
        let id = Id::new();
        assert_eq!(id, id.to_string().parse().unwrap());
    }

    #[test]
    fn rejects_nonsense() {
        assert!("not-an-id".parse::<Id>().is_err());
        assert!("".parse::<Id>().is_err());
    }

    #[test]
    fn sorts_by_creation_time_across_milliseconds() {
        // ULIDs carry a millisecond timestamp followed by random bits, so ordering holds
        // between milliseconds but not within one. That is the guarantee the storage format
        // relies on, and the only one worth asserting.
        let mut ids = Vec::new();
        for _ in 0..10 {
            ids.push(Id::new());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let created = ids.clone();
        ids.sort();
        assert_eq!(
            ids, created,
            "ULIDs minted in distinct milliseconds must sort in order"
        );
    }
}
