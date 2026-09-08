//! Timestamps, stored as native TOML datetimes.
//!
//! `created = 2026-09-06T10:14:00Z` — unquoted, so a hand-editing user sees a date rather than
//! a string, and a TOML-aware editor can validate it.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(toml::value::Datetime);

impl Timestamp {
    /// The current moment, in UTC, truncated to whole seconds.
    ///
    /// Sub-second precision would be noise: these timestamps are shown to people and compared
    /// against filesystem times that round differently on every filesystem anyway.
    pub fn now() -> Self {
        Self::from_offset(OffsetDateTime::now_utc())
    }

    pub fn from_offset(t: OffsetDateTime) -> Self {
        Timestamp(toml::value::Datetime {
            date: Some(toml::value::Date {
                year: t.year() as u16,
                month: t.month() as u8,
                day: t.day(),
            }),
            time: Some(toml::value::Time {
                hour: t.hour(),
                minute: t.minute(),
                second: Some(t.second()),
                // Explicitly none rather than zero: toml 1 distinguishes "no sub-second part
                // was given" from "exactly zero nanoseconds", and these timestamps are
                // second-resolution by design.
                nanosecond: None,
            }),
            offset: Some(toml::value::Offset::Z),
        })
    }

    /// Parse from the RFC 3339 form a hand-editing user is most likely to type.
    pub fn parse(s: &str) -> Result<Self> {
        s.parse::<toml::value::Datetime>()
            .map(Timestamp)
            .map_err(|e| Error::invalid("timestamp", e.to_string()))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Holder {
        created: Timestamp,
    }

    #[test]
    fn serialises_unquoted_as_a_toml_datetime() {
        let h = Holder {
            created: Timestamp::parse("2026-09-06T10:14:00Z").unwrap(),
        };
        let text = toml::to_string(&h).unwrap();
        assert_eq!(text.trim(), "created = 2026-09-06T10:14:00Z");
        assert!(
            !text.contains('"'),
            "a timestamp must not be quoted: {text}"
        );
    }

    #[test]
    fn round_trips() {
        let h = Holder {
            created: Timestamp::now(),
        };
        let text = toml::to_string(&h).unwrap();
        assert_eq!(h, toml::from_str(&text).unwrap());
    }

    #[test]
    fn rejects_nonsense() {
        assert!(Timestamp::parse("yesterday").is_err());
    }
}
