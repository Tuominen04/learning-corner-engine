//! Back up, bring back and restore — `backup-and-sync.md`, implemented.
//!
//! The library leads and the drive follows. *Back up* makes the drive exactly like the library,
//! warning once when the drive holds changes of its own. *Bring back* copies the drive's changes
//! into the library and asks only about a file changed in both. *Restore* is bring back into an
//! empty folder.
//!
//! Both are two-phase: a [`Plan`] is everything the user is shown, and nothing is written until
//! it is applied with their answers. Applying re-checks every file against the plan before
//! touching it, keeps every version it replaces in the library (and every change on the drive it
//! overwrites) under `.conflicts/`, and records what the two sides now share, so an interrupted
//! run resumes from a fresh plan.

mod classify;
mod scan;
mod state;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use classify::{classify, Action, Outcome};
pub use scan::Side;

use crate::atomic;
use crate::error::{Error, Result};
use crate::id::Id;
use crate::library::{Library, CONFLICTS_DIR, MANIFEST};
use crate::timestamp::Timestamp;
use state::Record;

/// Which way a plan copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Library to drive.
    Backup,
    /// Drive to library: a bring-back, or a restore into an empty folder.
    Restore,
}

/// One path that differs between the library and the drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Relative to the library, `/`-separated.
    pub path: String,
    pub outcome: Outcome,
    pub library: Option<Side>,
    pub drive: Option<Side>,
    pub action: Action,
    /// Whether the drive's own change is involved — what a backup warns about.
    pub drive_change: bool,
}

/// Everything the user is shown before anything is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub direction: Direction,
    pub library_id: Id,
    pub library_name: String,
    /// The library's folder; on a restore, the folder being restored into.
    pub library_root: PathBuf,
    pub drive_root: PathBuf,
    /// The drive's copy identifier, or none for a destination never backed up to before.
    pub copy: Option<Id>,
    /// Paths the two sides already have in common, with their digest. Recorded on apply.
    pub unchanged: BTreeMap<String, String>,
    pub entries: Vec<Entry>,
}

impl Plan {
    /// How many entries are changes on the drive. A backup with any needs `overwrite`.
    pub fn drive_changes(&self) -> usize {
        self.entries.iter().filter(|e| e.drive_change).count()
    }

    /// How many entries need an answer.
    pub fn asks(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.action == Action::Ask)
            .count()
    }

    /// Whether applying it would change nothing on either side.
    pub fn is_empty(&self) -> bool {
        self.entries.iter().all(|e| e.action == Action::Leave)
    }
}

/// The user's answer for a file changed in both, on a bring-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Answer {
    /// The library's version stays; the drive's goes at the next backup.
    KeepLibrary,
    /// The drive's version replaces the library's.
    TakeDrive,
    /// The drive's version is saved beside the library's. Only where both have the file.
    KeepBoth,
}

/// What an apply did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    /// Paths written, to either side.
    pub copied: Vec<String>,
    /// Paths removed, from either side.
    pub removed: Vec<String>,
    /// Every version kept aside, as its path in the library under `.conflicts/`.
    pub kept: Vec<String>,
    /// How many files are now recorded as in step with the copy.
    pub recorded: usize,
}

/// How often the record is written during an apply, in files. A lost record costs, at worst,
/// a question that need not have been asked — never a file — so it need not be per file.
const RECORD_EVERY: usize = 64;

// ---- planning ------------------------------------------------------------------------------

/// Plan a backup of `library` to `destination`.
pub fn plan_backup(library: &Library, destination: &Path) -> Result<Plan> {
    let library_root = library.root();
    refuse_overlap(library_root, destination)?;
    accept_destination(destination, library.id())?;

    let copy = if destination.exists() {
        state::read_copy(destination)?
    } else {
        None
    };
    let record = match copy {
        Some(copy) => Record::read(library_root, copy)?,
        None => Record::default(),
    };
    let here = scan::scan(library_root)?;
    let there = scan::scan(destination)?;
    Ok(build(
        Direction::Backup,
        library.id(),
        library.name().to_string(),
        library_root.to_path_buf(),
        destination.to_path_buf(),
        copy,
        &record,
        here,
        there,
    ))
}

/// Plan bringing `drive` back into `destination`: the library's own folder, or an empty one.
///
/// The drive's format is checked here, before anything is written, exactly as opening checks
/// it: a newer format is `FormatTooNew`, an older one `MigrationRequired`.
pub fn plan_restore(drive: &Path, destination: &Path) -> Result<Plan> {
    refuse_overlap(drive, destination)?;
    let copied = Library::open(drive)?;
    accept_destination(destination, copied.id())?;
    if destination.join(MANIFEST).is_file() {
        // Bringing back into a library: it must be one this build may write.
        Library::open(destination)?;
    }

    let copy = state::read_copy(drive)?;
    let record = match copy {
        Some(copy) if destination.exists() => Record::read(destination, copy)?,
        _ => Record::default(),
    };
    let here = scan::scan(destination)?;
    let there = scan::scan(drive)?;
    Ok(build(
        Direction::Restore,
        copied.id(),
        copied.name().to_string(),
        destination.to_path_buf(),
        drive.to_path_buf(),
        copy,
        &record,
        here,
        there,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build(
    direction: Direction,
    library_id: Id,
    library_name: String,
    library_root: PathBuf,
    drive_root: PathBuf,
    copy: Option<Id>,
    record: &Record,
    library: BTreeMap<String, Side>,
    drive: BTreeMap<String, Side>,
) -> Plan {
    let paths: BTreeSet<&String> = library.keys().chain(drive.keys()).collect();
    let mut unchanged = BTreeMap::new();
    let mut entries = Vec::new();
    for path in paths {
        let l = library.get(path);
        let d = drive.get(path);
        let outcome = classify(
            l.map(|s| s.digest.as_str()),
            d.map(|s| s.digest.as_str()),
            record.get(path),
        );
        let Some(outcome) = outcome else {
            if let Some(l) = l {
                unchanged.insert(path.clone(), l.digest.clone());
            }
            continue;
        };
        let action = match direction {
            Direction::Backup => outcome.backup_action(),
            Direction::Restore => outcome.restore_action(),
        };
        entries.push(Entry {
            path: path.clone(),
            outcome,
            library: l.cloned(),
            drive: d.cloned(),
            action,
            drive_change: outcome.is_drive_change(),
        });
    }
    // `library.toml` first, so a destination interrupted partway is already a copy of this
    // library, and a fresh plan accepts it.
    entries.sort_by_key(|e| (e.path != MANIFEST, e.path.clone()));
    Plan {
        direction,
        library_id,
        library_name,
        library_root,
        drive_root,
        copy,
        unchanged,
        entries,
    }
}

/// A destination is empty, or a copy of the library named by `id`.
fn accept_destination(destination: &Path, id: Id) -> Result<()> {
    if destination.join(MANIFEST).is_file() {
        let there = Library::open_read_only(destination)?;
        if there.id() == id {
            return Ok(());
        }
        return Err(Error::invalid(
            "destination",
            format!(
                "{} holds a different library, “{}”",
                destination.display(),
                there.name()
            ),
        ));
    }
    if scan::is_empty(destination)? {
        return Ok(());
    }
    Err(Error::invalid(
        "destination",
        format!(
            "{} is not empty, and is not a copy of this library",
            destination.display()
        ),
    ))
}

/// Neither folder may be inside the other, nor the same.
fn refuse_overlap(a: &Path, b: &Path) -> Result<()> {
    let a = resolved(a);
    let b = resolved(b);
    if a.starts_with(&b) || b.starts_with(&a) {
        return Err(Error::invalid(
            "destination",
            format!(
                "{} and {} overlap; a backup must be somewhere else",
                a.display(),
                b.display()
            ),
        ));
    }
    Ok(())
}

/// The canonical form of `path`, resolving as much of it as exists.
fn resolved(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_owned());
                existing = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut out = fs::canonicalize(&existing).unwrap_or(existing);
    for name in rest.into_iter().rev() {
        out.push(name);
    }
    out
}

// ---- applying ------------------------------------------------------------------------------

/// Apply a backup plan. `overwrite` answers the one question a backup can ask: whether to
/// overwrite changes on the drive. It is required when the plan has any.
pub fn apply_backup(library: &Library, plan: &Plan, overwrite: bool) -> Result<Applied> {
    library.ensure_writable()?;
    if plan.direction != Direction::Backup {
        return Err(Error::invalid("plan", "not a backup plan"));
    }
    if resolved(&plan.library_root) != resolved(library.root()) || plan.library_id != library.id() {
        return Err(Error::invalid("plan", "planned for a different library"));
    }
    if plan.drive_changes() > 0 && !overwrite {
        return Err(Error::invalid(
            "overwrite",
            format!(
                "the drive has {} change(s) of its own; bring them back first, or overwrite them",
                plan.drive_changes()
            ),
        ));
    }
    accept_destination(&plan.drive_root, plan.library_id)?;
    apply(plan, &BTreeMap::new())
}

/// Apply a bring-back or restore plan, with an answer for every entry that asks.
pub fn apply_restore(plan: &Plan, answers: &BTreeMap<String, Answer>) -> Result<Applied> {
    if plan.direction != Direction::Restore {
        return Err(Error::invalid("plan", "not a restore plan"));
    }
    let copied = Library::open(&plan.drive_root)?;
    if copied.id() != plan.library_id {
        return Err(Error::Conflict {
            path: plan.drive_root.join(MANIFEST),
        });
    }
    accept_destination(&plan.library_root, plan.library_id)?;
    for entry in &plan.entries {
        match (entry.action, answers.get(&entry.path)) {
            (Action::Ask, None) => {
                return Err(Error::invalid(
                    "answers",
                    format!("{} needs an answer", entry.path),
                ))
            }
            (Action::Ask, Some(Answer::KeepBoth))
                if entry.library.is_none() || entry.drive.is_none() =>
            {
                return Err(Error::invalid(
                    "answers",
                    format!("{}: keep both needs a file on both sides", entry.path),
                ))
            }
            (Action::Ask, Some(_)) => {}
            (_, Some(_)) => {
                return Err(Error::invalid(
                    "answers",
                    format!("{} does not need an answer", entry.path),
                ))
            }
            (_, None) => {}
        }
    }
    if let Some(path) = answers
        .keys()
        .find(|p| !plan.entries.iter().any(|e| &e.path == *p))
    {
        return Err(Error::invalid(
            "answers",
            format!("{path} is not in the plan"),
        ));
    }
    apply(plan, answers)
}

fn apply(plan: &Plan, answers: &BTreeMap<String, Answer>) -> Result<Applied> {
    let library_root = &plan.library_root;
    let drive_root = &plan.drive_root;

    // The drive must still be the copy the plan was made against — a swapped drive is a
    // surprise like any other.
    let copy_now = if drive_root.exists() {
        state::read_copy(drive_root)?
    } else {
        None
    };
    if copy_now != plan.copy {
        return Err(Error::Conflict {
            path: drive_root.join(".plc").join("backup.toml"),
        });
    }
    let copy = match plan.copy {
        Some(copy) => copy,
        None => {
            let copy = Id::new();
            fs::create_dir_all(drive_root)
                .map_err(|e| Error::io(format!("creating {}", drive_root.display()), e))?;
            state::write_copy(drive_root, copy)?;
            copy
        }
    };

    let mut record = if library_root.exists() {
        Record::read(library_root, copy)?
    } else {
        Record::default()
    };
    record.files.extend(plan.unchanged.clone());

    let written_side = match plan.direction {
        Direction::Backup => drive_root,
        Direction::Restore => library_root,
    };
    for leftover in scan::leftovers(written_side)? {
        atomic::remove(&leftover)?;
    }

    let mut run = Run {
        library_root,
        drive_root,
        kept_under: kept_folder(library_root),
        applied: Applied::default(),
    };
    let mut since_written = 0;
    for entry in &plan.entries {
        if entry.action == Action::Leave {
            continue;
        }
        let step = run
            .verify(entry)
            .and_then(|()| run.act(entry, answers.get(&entry.path).copied(), &mut record));
        if let Err(e) = step {
            // What was done stays done, and is recorded, so a fresh plan shows only the rest.
            record.write(library_root, copy)?;
            return Err(e);
        }
        since_written += 1;
        if since_written >= RECORD_EVERY {
            record.write(library_root, copy)?;
            since_written = 0;
        }
    }
    record.write(library_root, copy)?;
    run.applied.recorded = record.files.len();
    Ok(run.applied)
}

struct Run<'a> {
    library_root: &'a Path,
    drive_root: &'a Path,
    /// `.conflicts/<stamp>`, relative to the library: where this run keeps what it replaces.
    kept_under: String,
    applied: Applied,
}

impl Run<'_> {
    /// Both sides still hold what the plan saw.
    fn verify(&self, entry: &Entry) -> Result<()> {
        for (root, side) in [
            (self.library_root, &entry.library),
            (self.drive_root, &entry.drive),
        ] {
            let path = scan::absolute(root, &entry.path);
            let now = scan::digest_of(&path)?;
            if now.as_deref() != side.as_ref().map(|s| s.digest.as_str()) {
                return Err(Error::Conflict { path });
            }
        }
        Ok(())
    }

    fn act(&mut self, entry: &Entry, answer: Option<Answer>, record: &mut Record) -> Result<()> {
        let path = entry.path.as_str();
        match entry.action {
            Action::Leave => Ok(()),
            Action::CopyToDrive => {
                if entry.drive_change && entry.drive.is_some() {
                    self.keep(self.drive_root, path)?;
                }
                self.copy(self.library_root, self.drive_root, path, path)?;
                record.files.insert(path.into(), digest(&entry.library));
                Ok(())
            }
            Action::RemoveFromDrive => {
                if entry.drive_change {
                    self.keep(self.drive_root, path)?;
                }
                self.remove(self.drive_root, path)?;
                record.files.remove(path);
                Ok(())
            }
            Action::CopyToLibrary => self.take_drive(entry, record),
            Action::RemoveFromLibrary => self.take_drive(entry, record),
            Action::Ask => match answer {
                Some(Answer::KeepLibrary) => {
                    // Nothing moves. Recording the drive's file as the common ground means the
                    // next backup sees a change in the library, and simply copies it out.
                    match &entry.drive {
                        Some(d) => record.files.insert(path.into(), d.digest.clone()),
                        None => record.files.remove(path),
                    };
                    Ok(())
                }
                Some(Answer::TakeDrive) => self.take_drive(entry, record),
                Some(Answer::KeepBoth) => {
                    let beside = self.beside(path, entry.drive.as_ref())?;
                    self.copy(self.drive_root, self.library_root, path, &beside)?;
                    record.files.insert(path.into(), digest(&entry.drive));
                    Ok(())
                }
                None => Err(Error::invalid("answers", format!("{path} needs an answer"))),
            },
        }
    }

    /// Make the library's file at `entry.path` the drive's, keeping what it replaces.
    fn take_drive(&mut self, entry: &Entry, record: &mut Record) -> Result<()> {
        let path = entry.path.as_str();
        if entry.library.is_some() {
            self.keep(self.library_root, path)?;
        }
        match &entry.drive {
            Some(d) => {
                self.copy(self.drive_root, self.library_root, path, path)?;
                record.files.insert(path.into(), d.digest.clone());
            }
            None => {
                self.remove(self.library_root, path)?;
                record.files.remove(path);
            }
        }
        Ok(())
    }

    /// Keep the file at `path` under `root` in the library's `.conflicts/`.
    fn keep(&mut self, root: &Path, path: &str) -> Result<()> {
        let kept = format!("{}/{path}", self.kept_under);
        atomic::copy_in(
            &scan::absolute(root, path),
            &scan::absolute(self.library_root, &kept),
        )?;
        self.applied.kept.push(kept);
        Ok(())
    }

    fn copy(&mut self, from: &Path, to: &Path, path: &str, as_path: &str) -> Result<()> {
        atomic::copy_in(&scan::absolute(from, path), &scan::absolute(to, as_path))?;
        self.applied.copied.push(as_path.to_string());
        Ok(())
    }

    fn remove(&mut self, root: &Path, path: &str) -> Result<()> {
        let absolute = scan::absolute(root, path);
        atomic::remove(&absolute)?;
        prune(root, absolute.parent());
        self.applied.removed.push(path.to_string());
        Ok(())
    }

    /// `<name> (from backup YYYY-MM-DD).<ext>` beside `path`, numbered if that is taken.
    fn beside(&self, path: &str, drive: Option<&Side>) -> Result<String> {
        let date = drive
            .and_then(|d| d.modified)
            .unwrap_or_else(Timestamp::now)
            .to_string();
        let date = date.get(..10).unwrap_or(&date);
        let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
        let (stem, ext) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
            _ => (name, String::new()),
        };
        for n in 1.. {
            let suffix = if n == 1 {
                format!(" (from backup {date})")
            } else {
                format!(" (from backup {date}, {n})")
            };
            let candidate = format!("{stem}{suffix}{ext}");
            let candidate = if dir.is_empty() {
                candidate
            } else {
                format!("{dir}/{candidate}")
            };
            if !scan::absolute(self.library_root, &candidate).exists() {
                return Ok(candidate);
            }
        }
        unreachable!("an unbounded range always yields a free name")
    }
}

fn digest(side: &Option<Side>) -> String {
    side.as_ref().map(|s| s.digest.clone()).unwrap_or_default()
}

/// Remove directories left empty by a removal, up to (not including) `root`.
fn prune(root: &Path, mut dir: Option<&Path>) {
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// This run's own folder under `.conflicts/`, relative to the library: a timestamp with no
/// colons in it, which some filesystems — the ones on most drives among them — refuse.
///
/// Numbered when that folder is taken. Two runs within one second would otherwise share it,
/// and the second would overwrite what the first had kept.
fn kept_folder(library_root: &Path) -> String {
    let stamp = Timestamp::now().to_string().replace(':', "-");
    let mut name = format!("{CONFLICTS_DIR}/{stamp}");
    let mut n = 2;
    while scan::absolute(library_root, &name).exists() {
        name = format!("{CONFLICTS_DIR}/{stamp}-{n}");
        n += 1;
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_run_keeps_into_a_folder_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let first = kept_folder(dir.path());
        fs::create_dir_all(scan::absolute(dir.path(), &first)).unwrap();
        let second = kept_folder(dir.path());
        assert_ne!(first, second);
        assert!(!scan::absolute(dir.path(), &second).exists());
        assert!(second.starts_with(".conflicts/"));
        assert!(!second.contains(':'), "{second}");
    }

    #[test]
    fn keep_both_names_the_copy_beside_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let run = Run {
            library_root: dir.path(),
            drive_root: dir.path(),
            kept_under: String::new(),
            applied: Applied::default(),
        };
        let drive = Side {
            digest: String::new(),
            size: 0,
            modified: Some(Timestamp::parse("2026-03-04T05:06:07Z").unwrap()),
        };
        let name = run.beside("topics/a/note.md", Some(&drive)).unwrap();
        assert_eq!(name, "topics/a/note (from backup 2026-03-04).md");
        fs::create_dir_all(dir.path().join("topics/a")).unwrap();
        fs::write(dir.path().join(&name), "taken").unwrap();
        assert_eq!(
            run.beside("topics/a/note.md", Some(&drive)).unwrap(),
            "topics/a/note (from backup 2026-03-04, 2).md"
        );
        assert_eq!(
            run.beside("library", Some(&drive)).unwrap(),
            "library (from backup 2026-03-04)"
        );
    }
}
