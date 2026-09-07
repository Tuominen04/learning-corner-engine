//! The error causes named in `backend-contract.md`.
//!
//! Every operation can fail, and every failure names one of these. The interface is expected
//! to handle each, so the set is deliberately small and closed.

use std::path::PathBuf;

/// The result type every public operation returns.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No such entity. The interface should refresh — it may have been deleted externally.
    #[error("no entity with id {0}")]
    NotFound(String),

    /// No library is open. The interface should send the user to the library picker.
    #[error("no library is open")]
    LibraryNotOpen,

    /// The library was written by a newer version of the format than this build understands.
    ///
    /// Opening read-only is permitted; writing never is. Guessing at a format we do not know
    /// is how a future version's library gets silently corrupted by an old build.
    #[error("library format version {found} is newer than supported version {supported}")]
    FormatTooNew { found: u32, supported: u32 },

    /// The library predates this build and must be migrated before it can be opened.
    #[error("library format version {found} requires migration to version {supported}")]
    MigrationRequired { found: u32, supported: u32 },

    /// Content changed underneath this operation. Both versions are available; the user decides.
    #[error("{path} changed on disk since it was last read")]
    Conflict { path: PathBuf },

    /// Accepted and stored, but required fields are missing.
    ///
    /// Not a rejection: a user who cannot find a publisher should end up with an incomplete
    /// source, never with no source at all.
    #[error("saved, but missing required fields: {}", .missing.join(", "))]
    Incomplete { missing: Vec<String> },

    /// Rejected. Names the field and why.
    #[error("{field}: {reason}")]
    Invalid { field: String, reason: String },

    /// The library on disk is inconsistent. Retrying will not help.
    #[error("library error: {0}")]
    LibraryError(String),

    /// Disk full, permissions, a removed drive.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// Named in the contract, not yet built. Present so the operation surface is fixed early.
    #[error("{0} is not implemented in this milestone")]
    Unimplemented(&'static str),
}

impl Error {
    /// Attach context to an I/O failure. `Io` errors without context are unhelpful in a
    /// program whose whole job is touching files.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Error::Io {
            context: context.into(),
            source,
        }
    }

    pub fn invalid(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Error::Invalid {
            field: field.into(),
            reason: reason.into(),
        }
    }

    pub fn library(message: impl Into<String>) -> Self {
        Error::LibraryError(message.into())
    }
}

/// Adds context to a `std::io::Result` without a closure at every call site.
pub(crate) trait IoContext<T> {
    fn ctx(self, context: impl Into<String>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn ctx(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|e| Error::io(context, e))
    }
}
