//! Topics, as format 1 had them — read only by the conversion to format 2.
//!
//! In format 1 a topic was a directory holding `topic.toml`, its documents, and any nested
//! topics. Format 2 has no topics: each became a page beside its folder, keeping its id, name and
//! date (ADR 0009, `crate::migrate`). Nothing else in the engine reads or writes one.

use serde::{Deserialize, Serialize};

use crate::id::Id;
use crate::timestamp::Timestamp;

/// The file at the root of every topic directory.
pub const MANIFEST: &str = "topic.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Topic {
    pub id: Id,
    pub name: String,
    pub created: Timestamp,
}

impl Topic {
    pub fn new(name: impl Into<String>) -> Self {
        Topic {
            id: Id::new(),
            name: name.into(),
            created: Timestamp::now(),
        }
    }
}
