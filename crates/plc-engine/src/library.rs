//! Library identity, creation, and opening.
//!
//! A library is one folder the user chose. `library.toml` at its root says what it is and
//! which version of the storage format it uses. That version is the library's, not the
//! application's: restoring a two-year-old backup into a current build has to work, and it
//! only works if the two version lines are independent.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::atomic;
use crate::error::{Error, IoContext, Result};
use crate::id::Id;
use crate::timestamp::Timestamp;

/// The storage format this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;

/// The manifest file at the root of every library.
pub const MANIFEST: &str = "library.toml";

/// Derived data. Safe to delete; rebuilt on demand.
pub const WORK_DIR: &str = ".plc";

/// Copies kept when the user resolved a conflict. Never deleted automatically.
pub const CONFLICTS_DIR: &str = ".conflicts";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: Id,
    pub name: String,
    pub format_version: u32,
    pub created: Timestamp,
}

/// An open library.
#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
    manifest: Manifest,
    writable: bool,
}

impl Library {
    /// Create a library at `root`, creating the directory if it does not exist.
    pub fn create(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let name = name.into();

        if name.trim().is_empty() {
            return Err(Error::invalid("name", "a library needs a name"));
        }
        if root.join(MANIFEST).exists() {
            return Err(Error::invalid(
                "path",
                format!("{} already holds a library", root.display()),
            ));
        }

        fs::create_dir_all(&root).ctx(format!("creating {}", root.display()))?;

        let manifest = Manifest {
            id: Id::new(),
            name,
            format_version: FORMAT_VERSION,
            created: Timestamp::now(),
        };
        let library = Library {
            root,
            manifest,
            writable: true,
        };
        library.write_manifest()?;
        Ok(library)
    }

    /// Open an existing library for reading and writing.
    ///
    /// A library written by a newer format is refused rather than opened: guessing at a format
    /// we do not understand is how an old build silently corrupts a newer library. Use
    /// [`Library::open_read_only`] to show it to the user instead.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let library = Self::read(root.as_ref(), true)?;
        match library.manifest.format_version {
            v if v == FORMAT_VERSION => Ok(library),
            v if v > FORMAT_VERSION => Err(Error::FormatTooNew {
                found: v,
                supported: FORMAT_VERSION,
            }),
            v => Err(Error::MigrationRequired {
                found: v,
                supported: FORMAT_VERSION,
            }),
        }
    }

    /// Open any library, including one written by a newer format, without permitting writes.
    ///
    /// Every write through this handle fails with [`Error::FormatTooNew`], so a future
    /// library can be shown to its owner without being damaged by an old build.
    pub fn open_read_only(root: impl AsRef<Path>) -> Result<Self> {
        Self::read(root.as_ref(), false)
    }

    fn read(root: &Path, writable: bool) -> Result<Self> {
        let manifest_path = root.join(MANIFEST);
        if !manifest_path.exists() {
            return Err(Error::library(format!(
                "{} is not a library: no {MANIFEST}",
                root.display()
            )));
        }
        let text = fs::read_to_string(&manifest_path)
            .ctx(format!("reading {}", manifest_path.display()))?;
        let manifest: Manifest = toml::from_str(&text)
            .map_err(|e| Error::library(format!("{MANIFEST} is not readable: {e}")))?;

        let writable = writable && manifest.format_version <= FORMAT_VERSION;
        Ok(Library {
            root: root.to_path_buf(),
            manifest,
            writable,
        })
    }

    fn write_manifest(&self) -> Result<()> {
        let text = toml::to_string_pretty(&self.manifest)
            .map_err(|e| Error::library(format!("cannot serialise {MANIFEST}: {e}")))?;
        atomic::write(&self.root.join(MANIFEST), &text)
    }

    /// Fail if this handle may not write. Every mutating operation calls this first.
    pub fn ensure_writable(&self) -> Result<()> {
        if self.writable {
            Ok(())
        } else {
            Err(Error::FormatTooNew {
                found: self.manifest.format_version,
                supported: FORMAT_VERSION,
            })
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn id(&self) -> Id {
        self.manifest.id
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn is_writable(&self) -> bool {
        self.writable
    }

    /// The derived-data directory. Deleting it is always safe.
    pub fn work_dir(&self) -> PathBuf {
        self.root.join(WORK_DIR)
    }

    /// Where copies rejected during conflict resolution are kept.
    pub fn conflicts_dir(&self) -> PathBuf {
        self.root.join(CONFLICTS_DIR)
    }

    /// Change the name in `library.toml`. The folder is not touched.
    pub fn rename(&mut self, name: impl Into<String>) -> Result<()> {
        self.ensure_writable()?;
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Error::invalid("name", "a library needs a name"));
        }
        self.manifest.name = name;
        self.write_manifest()
    }

    /// True for paths the engine owns. Nothing outside these is ever deleted implicitly.
    pub fn is_reserved(path: &Path) -> bool {
        path.components()
            .any(|c| matches!(c.as_os_str().to_str(), Some(WORK_DIR) | Some(CONFLICTS_DIR)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn creates_and_reopens() {
        let dir = temp();
        let created = Library::create(dir.path(), "Learning").unwrap();
        let opened = Library::open(dir.path()).unwrap();
        assert_eq!(created.id(), opened.id());
        assert_eq!(opened.name(), "Learning");
        assert_eq!(opened.manifest().format_version, FORMAT_VERSION);
    }

    #[test]
    fn manifest_is_readable_toml_with_an_unquoted_date() {
        let dir = temp();
        Library::create(dir.path(), "Learning").unwrap();
        let text = fs::read_to_string(dir.path().join(MANIFEST)).unwrap();
        assert!(text.contains("name = \"Learning\""), "{text}");
        assert!(text.contains("format_version = 1"), "{text}");
        assert!(
            text.contains("created = 2"),
            "created must be an unquoted TOML datetime: {text}"
        );
    }

    #[test]
    fn refuses_to_create_over_an_existing_library() {
        let dir = temp();
        Library::create(dir.path(), "Learning").unwrap();
        assert!(matches!(
            Library::create(dir.path(), "Again"),
            Err(Error::Invalid { .. })
        ));
    }

    #[test]
    fn requires_a_name() {
        let dir = temp();
        assert!(matches!(
            Library::create(dir.path(), "   "),
            Err(Error::Invalid { .. })
        ));
    }

    #[test]
    fn an_ordinary_folder_is_not_a_library() {
        let dir = temp();
        assert!(matches!(
            Library::open(dir.path()),
            Err(Error::LibraryError(_))
        ));
    }

    #[test]
    fn a_corrupt_manifest_is_diagnosed_not_ignored() {
        let dir = temp();
        fs::write(dir.path().join(MANIFEST), "this is not toml {{{").unwrap();
        let message = Library::open(dir.path()).unwrap_err().to_string();
        assert!(message.contains(MANIFEST), "{message}");
    }

    #[test]
    fn a_newer_format_is_refused_for_writing() {
        let dir = temp();
        Library::create(dir.path(), "Learning").unwrap();
        bump_format_version(dir.path(), FORMAT_VERSION + 1);

        assert!(matches!(
            Library::open(dir.path()),
            Err(Error::FormatTooNew { found, supported })
                if found == FORMAT_VERSION + 1 && supported == FORMAT_VERSION
        ));
    }

    #[test]
    fn a_newer_format_can_still_be_shown_read_only() {
        let dir = temp();
        Library::create(dir.path(), "Learning").unwrap();
        bump_format_version(dir.path(), FORMAT_VERSION + 1);

        let library = Library::open_read_only(dir.path()).unwrap();
        assert_eq!(library.name(), "Learning");
        assert!(!library.is_writable());
        assert!(matches!(
            library.ensure_writable(),
            Err(Error::FormatTooNew { .. })
        ));
    }

    #[test]
    fn a_library_open_read_only_cannot_be_renamed() {
        let dir = temp();
        Library::create(dir.path(), "Learning").unwrap();
        bump_format_version(dir.path(), FORMAT_VERSION + 1);
        let before = fs::read_to_string(dir.path().join(MANIFEST)).unwrap();

        let mut library = Library::open_read_only(dir.path()).unwrap();
        assert!(matches!(
            library.rename("Nutrition"),
            Err(Error::FormatTooNew { .. })
        ));
        assert_eq!(library.name(), "Learning");
        assert_eq!(
            fs::read_to_string(dir.path().join(MANIFEST)).unwrap(),
            before,
            "a future library's manifest must not be rewritten by an old build"
        );
    }

    #[test]
    fn an_older_format_asks_for_migration() {
        let dir = temp();
        Library::create(dir.path(), "Learning").unwrap();
        bump_format_version(dir.path(), 0);

        assert!(matches!(
            Library::open(dir.path()),
            Err(Error::MigrationRequired {
                found: 0,
                supported: FORMAT_VERSION
            })
        ));
    }

    #[test]
    fn reserved_paths_are_recognised_at_any_depth() {
        assert!(Library::is_reserved(Path::new(".plc/index.sqlite")));
        assert!(Library::is_reserved(Path::new("a/b/.conflicts/x.md")));
        assert!(!Library::is_reserved(Path::new("topics/protein/note.md")));
        assert!(
            !Library::is_reserved(Path::new("topics/plc-notes/note.md")),
            "a similarly-named user folder must not be treated as reserved"
        );
    }

    fn bump_format_version(root: &Path, to: u32) {
        let path = root.join(MANIFEST);
        let text = fs::read_to_string(&path).unwrap();
        let bumped = text.replace(
            &format!("format_version = {FORMAT_VERSION}"),
            &format!("format_version = {to}"),
        );
        assert_ne!(text, bumped, "test helper failed to rewrite the version");
        fs::write(&path, bumped).unwrap();
    }
}
