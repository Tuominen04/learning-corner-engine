//! The Learning Corner engine.
//!
//! Everything that touches the user's library lives here: reading and writing files,
//! maintaining the derived index, and the operations the interface calls. The engine is usable
//! with no interface at all, which is what makes it testable and what keeps the boundary in
//! `backend-contract.md` honest.
//!
//! The specification this implements is in the `learning-corner` repository under
//! `docs/technical/`. Where this code and those documents disagree, the documents are
//! normative and this is a bug.

pub mod api;
pub mod atomic;
pub mod backup;
pub mod citation;
pub mod error;
pub mod export;
pub mod id;
pub mod index;
pub mod library;
pub mod model;
pub mod store;
pub mod timestamp;

pub use api::Engine;
pub use error::{Error, Result};
pub use id::Id;
pub use library::Library;
pub use model::{Document, Topic};
pub use timestamp::Timestamp;
