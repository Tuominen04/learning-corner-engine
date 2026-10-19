//! Which copy a drive holds, and what the library last had in common with it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::error::{Error, IoContext, Result};
use crate::id::Id;
use crate::library::WORK_DIR;

const COPY_FILE: &str = "backup.toml";
const RECORDS_DIR: &str = "sync-state";

#[derive(Debug, Serialize, Deserialize)]
struct CopyFile {
    copy: Id,
}

/// The copy identifier in a drive's `.plc/backup.toml`, if it has one.
///
/// An unreadable file counts as none: the record it would select is derived data, and having
/// none is the conservative direction — every difference becomes something to show.
pub(crate) fn read_copy(drive: &Path) -> Result<Option<Id>> {
    let path = copy_path(drive);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(toml::from_str::<CopyFile>(&text).ok().map(|c| c.copy)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(format!("reading {}", path.display()), e)),
    }
}

pub(crate) fn write_copy(drive: &Path, copy: Id) -> Result<()> {
    let text = toml::to_string(&CopyFile { copy })
        .map_err(|e| Error::library(format!("cannot write {COPY_FILE}: {e}")))?;
    atomic::write(&copy_path(drive), &text)
}

/// What a library last had in common with one copy: path to digest.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Record {
    pub files: BTreeMap<String, String>,
}

impl Record {
    /// The library's record of `copy`; empty if there is none, or it cannot be read.
    pub fn read(library: &Path, copy: Id) -> Result<Record> {
        let path = record_path(library, copy);
        match fs::read_to_string(&path) {
            Ok(text) => Ok(toml::from_str(&text).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Record::default()),
            Err(e) => Err(Error::io(format!("reading {}", path.display()), e)),
        }
    }

    pub fn write(&self, library: &Path, copy: Id) -> Result<()> {
        let path = record_path(library, copy);
        let text = toml::to_string(self)
            .map_err(|e| Error::library(format!("cannot write {}: {e}", path.display())))?;
        let parent = path.parent().unwrap_or(library);
        fs::create_dir_all(parent).ctx(format!("creating {}", parent.display()))?;
        atomic::write(&path, &text)
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }
}

fn copy_path(drive: &Path) -> PathBuf {
    drive.join(WORK_DIR).join(COPY_FILE)
}

fn record_path(library: &Path, copy: Id) -> PathBuf {
    library
        .join(WORK_DIR)
        .join(RECORDS_DIR)
        .join(format!("{copy}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_identity_round_trips_and_its_absence_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_copy(dir.path()).unwrap(), None);
        let copy = Id::new();
        write_copy(dir.path(), copy).unwrap();
        assert_eq!(read_copy(dir.path()).unwrap(), Some(copy));
    }

    #[test]
    fn a_record_keys_any_path_and_an_unreadable_one_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let copy = Id::new();
        let mut record = Record::default();
        record
            .files
            .insert("topics/ruoka/mitä proteiini on.md".into(), "abc".into());
        record
            .files
            .insert("topics/язык/текст.md".into(), "def".into());
        record.write(dir.path(), copy).unwrap();
        assert_eq!(Record::read(dir.path(), copy).unwrap(), record);

        fs::write(record_path(dir.path(), copy), "not toml {{{").unwrap();
        assert_eq!(Record::read(dir.path(), copy).unwrap(), Record::default());
    }
}
