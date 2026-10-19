//! Walking a library, or a copy of one, into the files a backup compares.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{Error, IoContext, Result};
use crate::library::{FOLDER_NOTES, WORK_DIR};
use crate::timestamp::Timestamp;

/// One side's file at a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Side {
    /// BLAKE3 of the content, as hex.
    pub digest: String,
    pub size: u64,
    /// Shown to the user; never used to decide anything.
    pub modified: Option<Timestamp>,
}

/// Every compared file under `root`, keyed by `/`-separated path relative to it.
///
/// Left out, as `backup-and-sync.md` says: `.plc/` at the root, the notes a file manager leaves
/// in a folder, and the temporary files of a write in progress. Symbolic links are not
/// followed: the engine never makes one, and following one could copy something outside the
/// library.
pub(crate) fn scan(root: &Path) -> Result<BTreeMap<String, Side>> {
    let mut files = BTreeMap::new();
    if !root.exists() {
        return Ok(files);
    }
    walk(root, root, &mut |relative, path, metadata| {
        let side = Side {
            digest: digest(path)?,
            size: metadata.len(),
            modified: metadata
                .modified()
                .ok()
                .map(|t| Timestamp::from_offset(OffsetDateTime::from(t))),
        };
        files.insert(relative, side);
        Ok(())
    })?;
    Ok(files)
}

/// The temporary files an interrupted write left under `root`, for the next run to remove.
pub(crate) fn leftovers(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if root.exists() {
        visit(root, root, &mut |path, name, _| {
            if is_temporary(name) {
                found.push(path.to_path_buf());
            }
            Ok(())
        })?;
    }
    Ok(found)
}

/// The digest of the file at `path`, or `None` if there is none.
pub(crate) fn digest_of(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => digest(path).map(Some),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(format!("reading {}", path.display()), e)),
    }
}

/// Streamed, so a scanned book is not read into memory to be hashed.
fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path).ctx(format!("opening {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher).ctx(format!("reading {}", path.display()))?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Whether `root` holds nothing a backup would compare — what an empty destination is.
pub(crate) fn is_empty(root: &Path) -> Result<bool> {
    if !root.exists() {
        return Ok(true);
    }
    let mut empty = true;
    visit(root, root, &mut |_, name, _| {
        if !is_temporary(name) {
            empty = false;
        }
        Ok(())
    })?;
    Ok(empty)
}

/// Every compared file, as its relative path, absolute path and metadata.
fn walk(
    root: &Path,
    dir: &Path,
    found: &mut dyn FnMut(String, &Path, &fs::Metadata) -> Result<()>,
) -> Result<()> {
    visit(root, dir, &mut |path, name, metadata| {
        if is_temporary(name) {
            return Ok(());
        }
        found(relative(root, path)?, path, metadata)
    })
}

/// Every file under `dir` that is not excluded by location or name, temporary files included.
fn visit(
    root: &Path,
    dir: &Path,
    found: &mut dyn FnMut(&Path, &str, &fs::Metadata) -> Result<()>,
) -> Result<()> {
    let listing = format!("listing {}", dir.display());
    let mut entries: Vec<_> = fs::read_dir(dir)
        .ctx(listing.clone())?
        .collect::<std::io::Result<_>>()
        .ctx(listing)?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(Error::library(format!(
                "{} has a name that is not valid text, and cannot be backed up",
                path.display()
            )));
        };
        if dir == root && name == WORK_DIR {
            continue;
        }
        if FOLDER_NOTES.contains(&name) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).ctx(format!("reading {}", path.display()))?;
        if metadata.is_dir() {
            visit(root, &path, found)?;
        } else if metadata.is_file() {
            found(&path, name, &metadata)?;
        }
    }
    Ok(())
}

/// `atomic`'s temporary names: `.<name>.<ULID>.tmp`.
fn is_temporary(name: &str) -> bool {
    let Some(inner) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".tmp")) else {
        return false;
    };
    inner
        .rsplit_once('.')
        .is_some_and(|(_, id)| id.len() == 26 && id.parse::<ulid::Ulid>().is_ok())
}

fn relative(root: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| Error::library(format!("{} is outside {}", path.display(), root.display())))?;
    let parts: Vec<&str> = rel
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or_default())
        .collect();
    Ok(parts.join("/"))
}

/// The absolute path of a `/`-separated relative path under `root`.
pub(crate) fn absolute(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |p, part| p.join(part))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_names_are_exactly_atomic_s() {
        assert!(is_temporary(".note.md.01J8XKQ4V9C3Z6E2N5T7W0R1YB.tmp"));
        assert!(!is_temporary(".note.md.tmp"));
        assert!(!is_temporary("note.md.01J8XKQ4V9C3Z6E2N5T7W0R1YB.tmp"));
        assert!(!is_temporary(".hidden-notes.tmp"));
    }

    #[test]
    fn leaves_out_the_work_folder_folder_notes_and_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".plc")).unwrap();
        fs::write(root.join(".plc/index.sqlite"), "x").unwrap();
        fs::create_dir_all(root.join("topics/a/.plc")).unwrap();
        fs::write(
            root.join("topics/a/.plc/kept.md"),
            "a user folder of that name",
        )
        .unwrap();
        fs::write(root.join("topics/a/.directory"), "[Dolphin]").unwrap();
        fs::write(root.join("topics/a/note.md"), "hello").unwrap();
        fs::write(
            root.join("topics/a/.note.md.01J8XKQ4V9C3Z6E2N5T7W0R1YB.tmp"),
            "half",
        )
        .unwrap();

        let files = scan(root).unwrap();
        let paths: Vec<_> = files.keys().map(String::as_str).collect();
        assert_eq!(paths, ["topics/a/.plc/kept.md", "topics/a/note.md"]);
        assert_eq!(files["topics/a/note.md"].size, 5);
        assert_eq!(leftovers(root).unwrap().len(), 1);
    }

    #[test]
    fn an_empty_destination_may_hold_folder_notes_and_the_work_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_empty(dir.path()).unwrap());
        assert!(is_empty(&dir.path().join("not-there-yet")).unwrap());
        fs::write(dir.path().join(".directory"), "x").unwrap();
        fs::create_dir_all(dir.path().join(".plc")).unwrap();
        fs::write(dir.path().join(".plc/backup.toml"), "x").unwrap();
        assert!(is_empty(dir.path()).unwrap());
        fs::write(dir.path().join("holiday.jpg"), "x").unwrap();
        assert!(!is_empty(dir.path()).unwrap());
    }
}
