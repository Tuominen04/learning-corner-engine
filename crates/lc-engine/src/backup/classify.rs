//! The comparison table of `backup-and-sync.md`, as a pure function.
//!
//! Kept apart from everything that touches a disk, so that every row of the table — and the
//! action each direction gives it — is tested without a single file being written.

use serde::{Deserialize, Serialize};

/// How one path differs between the library and the drive.
///
/// `L`, `D` and `R` below are the library's digest, the drive's, and the one recorded at the
/// last run with this copy. A path that is the same on both sides has no outcome at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// Only in the library, and never recorded.
    NewInLibrary,
    /// Only on the drive, and never recorded.
    NewOnDrive,
    /// Both differ, and the drive still has what was recorded.
    ChangedInLibrary,
    /// Both differ, and the library still has what was recorded.
    ChangedOnDrive,
    /// Both differ from each other and from the record — or there is no record.
    ChangedInBoth,
    /// Gone from the drive; the library still has what was recorded.
    RemovedOnDrive,
    /// Gone from the drive; the library has changed it since.
    RemovedOnDriveChangedInLibrary,
    /// Gone from the library; the drive still has what was recorded.
    RemovedInLibrary,
    /// Gone from the library; the drive has changed it since.
    RemovedInLibraryChangedOnDrive,
}

/// What applying a plan does with one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    CopyToDrive,
    RemoveFromDrive,
    CopyToLibrary,
    RemoveFromLibrary,
    /// Nothing now: a change made only in the library goes to the drive on the next backup.
    Leave,
    /// A file changed in both places, on a bring-back. The user answers.
    Ask,
}

/// The row of the table for one path, or `None` when both sides hold the same file.
pub fn classify(
    library: Option<&str>,
    drive: Option<&str>,
    recorded: Option<&str>,
) -> Option<Outcome> {
    use Outcome::*;
    Some(match (library, drive) {
        (None, None) => return None,
        (Some(l), Some(d)) if l == d => return None,
        (Some(l), Some(d)) => match recorded {
            Some(r) if r == d => ChangedInLibrary,
            Some(r) if r == l => ChangedOnDrive,
            _ => ChangedInBoth,
        },
        (Some(l), None) => match recorded {
            None => NewInLibrary,
            Some(r) if r == l => RemovedOnDrive,
            Some(_) => RemovedOnDriveChangedInLibrary,
        },
        (None, Some(d)) => match recorded {
            None => NewOnDrive,
            Some(r) if r == d => RemovedInLibrary,
            Some(_) => RemovedInLibraryChangedOnDrive,
        },
    })
}

impl Outcome {
    /// Whether the drive's own change is involved: its file differs from the record, or there is
    /// no record to tell old from changed. These are the only outcomes a backup warns about.
    pub fn is_drive_change(self) -> bool {
        !matches!(
            self,
            Outcome::NewInLibrary | Outcome::ChangedInLibrary | Outcome::RemovedInLibrary
        )
    }

    /// Whether the library has a file at this path.
    pub fn library_has_file(self) -> bool {
        !matches!(
            self,
            Outcome::NewOnDrive
                | Outcome::RemovedInLibrary
                | Outcome::RemovedInLibraryChangedOnDrive
        )
    }

    /// Whether the drive has a file at this path.
    pub fn drive_has_file(self) -> bool {
        !matches!(
            self,
            Outcome::NewInLibrary
                | Outcome::RemovedOnDrive
                | Outcome::RemovedOnDriveChangedInLibrary
        )
    }

    /// Back up: the drive becomes exactly like the library.
    pub fn backup_action(self) -> Action {
        if self.library_has_file() {
            Action::CopyToDrive
        } else {
            Action::RemoveFromDrive
        }
    }

    /// Bring back: the drive's changes come in; the library's own changes are left for the next
    /// backup; a file changed in both is asked about.
    pub fn restore_action(self) -> Action {
        use Outcome::*;
        match self {
            NewOnDrive | ChangedOnDrive => Action::CopyToLibrary,
            RemovedOnDrive => Action::RemoveFromLibrary,
            NewInLibrary | ChangedInLibrary | RemovedInLibrary => Action::Leave,
            ChangedInBoth | RemovedOnDriveChangedInLibrary | RemovedInLibraryChangedOnDrive => {
                Action::Ask
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Outcome::*;

    const A: Option<&str> = Some("a");
    const B: Option<&str> = Some("b");
    const C: Option<&str> = Some("c");

    #[test]
    fn every_row_of_the_table() {
        // (library, drive, recorded) -> outcome, in the order backup-and-sync.md lists them.
        let rows = [
            ((A, A, None), None),
            ((A, A, B), None),
            ((A, None, None), Some(NewInLibrary)),
            ((None, A, None), Some(NewOnDrive)),
            ((A, B, B), Some(ChangedInLibrary)),
            ((A, B, A), Some(ChangedOnDrive)),
            ((A, B, C), Some(ChangedInBoth)),
            ((A, B, None), Some(ChangedInBoth)),
            ((A, None, A), Some(RemovedOnDrive)),
            ((A, None, B), Some(RemovedOnDriveChangedInLibrary)),
            ((None, A, A), Some(RemovedInLibrary)),
            ((None, A, B), Some(RemovedInLibraryChangedOnDrive)),
            ((None, None, A), None),
        ];
        for ((l, d, r), expected) in rows {
            assert_eq!(
                classify(l, d, r),
                expected,
                "library {l:?}, drive {d:?}, recorded {r:?}"
            );
        }
    }

    #[test]
    fn a_backup_makes_the_drive_like_the_library_whatever_the_outcome() {
        for outcome in all() {
            let action = outcome.backup_action();
            assert_eq!(
                action == Action::CopyToDrive,
                outcome.library_has_file(),
                "{outcome:?}"
            );
            assert!(matches!(
                action,
                Action::CopyToDrive | Action::RemoveFromDrive
            ));
        }
    }

    #[test]
    fn only_the_library_s_own_changes_are_not_drive_changes() {
        let quiet: Vec<_> = all().into_iter().filter(|o| !o.is_drive_change()).collect();
        assert_eq!(quiet, [NewInLibrary, ChangedInLibrary, RemovedInLibrary]);
    }

    #[test]
    fn a_bring_back_asks_only_about_a_file_changed_in_both() {
        let asked: Vec<_> = all()
            .into_iter()
            .filter(|o| o.restore_action() == Action::Ask)
            .collect();
        assert_eq!(
            asked,
            [
                ChangedInBoth,
                RemovedOnDriveChangedInLibrary,
                RemovedInLibraryChangedOnDrive
            ]
        );
        assert_eq!(NewOnDrive.restore_action(), Action::CopyToLibrary);
        assert_eq!(ChangedOnDrive.restore_action(), Action::CopyToLibrary);
        assert_eq!(RemovedOnDrive.restore_action(), Action::RemoveFromLibrary);
        assert_eq!(ChangedInLibrary.restore_action(), Action::Leave);
    }

    fn all() -> Vec<Outcome> {
        vec![
            NewInLibrary,
            NewOnDrive,
            ChangedInLibrary,
            ChangedOnDrive,
            ChangedInBoth,
            RemovedOnDrive,
            RemovedOnDriveChangedInLibrary,
            RemovedInLibrary,
            RemovedInLibraryChangedOnDrive,
        ]
    }
}
