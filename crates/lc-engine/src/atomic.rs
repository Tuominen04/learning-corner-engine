//! Durable file writes.
//!
//! The guarantee the whole storage design rests on: a crash, a killed process, or a pulled
//! power cable leaves either the previous complete file or the new complete file on disk, and
//! never a truncated one. Without it, "files are the source of truth"
//! (ADR 0003) is a wish rather than a property.
//!
//! The sequence is: write a temporary file **in the same directory**, fsync it, rename it over
//! the target, then fsync the directory. Same-directory rename is atomic on the filesystems
//! Fedora uses. The final directory fsync is the step that is easy to omit and that decides
//! whether the rename itself survives a power loss.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{IoContext, Result};
use crate::id::Id;

/// Write `contents` to `path`, atomically, creating parent directories as needed.
pub fn write(path: &Path, contents: &str) -> Result<()> {
    write_bytes(path, contents.as_bytes())
}

/// [`write`], for contents that are not text: an exported document in a format made of bytes.
pub fn write_bytes(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).ctx(format!("creating {}", parent.display()))?;

    let temp = temp_path_beside(path);

    // Scoped so the handle is closed before the rename, which Windows requires and which
    // costs nothing here.
    {
        let mut file =
            File::create(&temp).ctx(format!("creating temporary file {}", temp.display()))?;
        file.write_all(contents)
            .ctx(format!("writing {}", temp.display()))?;
        file.sync_all()
            .ctx(format!("flushing {}", temp.display()))?;
    }

    if let Err(e) = fs::rename(&temp, path) {
        // Leaving a stray temporary file behind would look like library corruption later.
        let _ = fs::remove_file(&temp);
        return Err(crate::error::Error::io(
            format!("replacing {}", path.display()),
            e,
        ));
    }

    sync_dir(parent)
}

/// Copy a file from outside the library to `path`, atomically.
///
/// The same temp-fsync-rename sequence as [`write`], for the same reason: an interrupted copy
/// must leave the previous file or the new one, never half a PDF. The bytes are streamed
/// rather than read into a `Vec` first — a scanned book is not a small file, and there is no
/// reason to hold one in memory to put it on disk.
pub fn copy_in(from: &Path, to: &Path) -> Result<()> {
    let parent = to.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).ctx(format!("creating {}", parent.display()))?;

    let temp = temp_path_beside(to);

    {
        let mut source = File::open(from).ctx(format!("opening {}", from.display()))?;
        let mut file =
            File::create(&temp).ctx(format!("creating temporary file {}", temp.display()))?;
        std::io::copy(&mut source, &mut file).ctx(format!("copying {}", from.display()))?;
        file.sync_all()
            .ctx(format!("flushing {}", temp.display()))?;
    }

    if let Err(e) = fs::rename(&temp, to) {
        let _ = fs::remove_file(&temp);
        return Err(crate::error::Error::io(
            format!("replacing {}", to.display()),
            e,
        ));
    }

    sync_dir(parent)
}

/// Remove a file, tolerating its already being gone.
pub fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => sync_dir(path.parent().unwrap_or_else(|| Path::new("."))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(crate::error::Error::io(
            format!("removing {}", path.display()),
            e,
        )),
    }
}

/// Rename within the library, atomically, creating the destination's parent as needed.
pub fn rename(from: &Path, to: &Path) -> Result<()> {
    let parent = to.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).ctx(format!("creating {}", parent.display()))?;
    fs::rename(from, to).ctx(format!("renaming {} to {}", from.display(), to.display()))?;
    sync_dir(parent)?;
    if let Some(old_parent) = from.parent() {
        if old_parent != parent {
            sync_dir(old_parent)?;
        }
    }
    Ok(())
}

/// fsync a directory, so a rename into it survives a power loss.
fn sync_dir(dir: &Path) -> Result<()> {
    // Opening a directory for reading and syncing it is the portable-enough idiom on Unix.
    // On platforms where it is not permitted, a failure here must not fail the write: the
    // data is already durable, only the directory entry's ordering is not guaranteed.
    match File::open(dir) {
        Ok(handle) => {
            let _ = handle.sync_all();
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(()),
        Err(e) => Err(crate::error::Error::io(
            format!("opening {} to flush it", dir.display()),
            e,
        )),
    }
}

/// A temporary name beside `path`, unique enough that two writers never collide.
///
/// It must share a directory with the target: `rename` is only atomic within one filesystem,
/// and a temp directory is frequently a different one.
fn temp_path_beside(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{}.tmp", Id::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a/b/c/note.md");
        write(&target, "hello").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn bytes_are_written_exactly_and_replace_wholly() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("export.bin");
        write_bytes(&target, &[0xff, 0x00, 0xfe, 0x01, 0x02]).unwrap();
        write_bytes(&target, &[0x00, 0xff]).unwrap();
        assert_eq!(fs::read(&target).unwrap(), [0x00, 0xff]);
    }

    #[test]
    fn replaces_an_existing_file_wholly() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("note.md");
        write(&target, "first, and rather long").unwrap();
        write(&target, "second").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "second");
    }

    #[test]
    fn leaves_no_temporary_files_behind() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("note.md"), "hello").unwrap();
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn temp_name_is_in_the_target_directory() {
        // If this ever moves to a temp dir, the rename stops being atomic across filesystems
        // and the guarantee silently disappears.
        let path = Path::new("/some/library/topics/protein/note.md");
        assert_eq!(temp_path_beside(path).parent(), path.parent());
    }

    #[test]
    fn remove_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("note.md");
        write(&target, "hello").unwrap();
        remove(&target).unwrap();
        remove(&target).unwrap();
        assert!(!target.exists());
    }
}
