//! Topics: a subject you are learning about.
//!
//! A topic is a directory holding `topic.toml`, its documents, and any nested topics. The
//! folder structure the user sees in the sidebar is the folder structure on disk, which is
//! worth a great deal on the day something goes wrong.

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
