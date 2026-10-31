//! The library on disk.
//!
//! Reading and writing entities, and walking the tree. The folder structure mirrors what the
//! user sees: a topic is a directory holding `topic.toml`, its documents as `.md` files, and
//! any nested topics as subdirectories.
//!
//! Identity is the ULID inside each file, never the path — so this module can rename and move
//! freely, and nothing that references an entity has to be updated when it does.

pub mod slug;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::atomic;
use crate::error::{Error, IoContext, Result};
use crate::id::Id;
use crate::library::Library;
use crate::model::{document, frontmatter, topic, Document, Topic};
use crate::timestamp::Timestamp;
use crate::works::{self, Work, WorkRevision, WORKS_DIR};

/// The directory holding all root topics.
pub const TOPICS_DIR: &str = "topics";

/// The directory holding every source in the library.
///
/// Flat rather than nested: a source belongs to the library, not to a topic, which is what
/// lets one book be cited from fifty documents across it.
pub const SOURCES_DIR: &str = "sources";

/// Where the library keeps its own copies of source files.
///
/// Inside `sources/` rather than beside it, so a folder listing puts a source's record and
/// its paper in the same place, and so moving the library moves both.
pub const SOURCE_FILES_DIR: &str = "sources/files";

/// An opaque token identifying the on-disk state of a file.
///
/// Returned by every read and required by every write: if the file changed underneath —
/// because the user edited it in a text editor, as they are explicitly permitted to — the
/// write is refused rather than silently discarding their edit.
///
/// Derived from the filesystem rather than from the document's own `modified` field, because
/// an external editor changes the body without touching the front matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision {
    modified_nanos: u128,
    len: u64,
}

/// Serialised as the opaque string `"<nanos>:<len>"`, never as a pair of numbers.
///
/// This is not a style choice. A revision carries nanoseconds since the epoch — around
/// 1.8e18 — and the interface reaches this type across a JSON boundary where numbers are
/// IEEE-754 doubles. Anything above 2^53 (about 9.0e15) is rounded on the way through, so a
/// revision serialised as a number comes back *different* from the one that was issued, every
/// comparison fails, and no document can ever be saved.
///
/// A string crosses unchanged, and it matches what the contract already says a revision is: a
/// token the caller stores and hands back, never one it inspects.
impl serde::Serialize for Revision {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.token())
    }
}

impl<'de> serde::Deserialize<'de> for Revision {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        use serde::de::Error;
        let raw = String::deserialize(deserializer)?;
        Revision::from_token(&raw)
            .ok_or_else(|| D::Error::custom(format!("{raw:?} is not a revision token")))
    }
}

impl Revision {
    /// A compact form for recording in the index and comparing later.
    pub fn token(&self) -> String {
        format!("{}:{}", self.modified_nanos, self.len)
    }

    /// Recover a revision from the form [`Revision::token`] produces.
    pub fn from_token(raw: &str) -> Option<Self> {
        let (nanos, len) = raw.split_once(':')?;
        Some(Revision {
            modified_nanos: nanos.parse().ok()?,
            len: len.parse().ok()?,
        })
    }

    fn of(path: &Path) -> Result<Self> {
        let meta = fs::metadata(path).ctx(format!("inspecting {}", path.display()))?;
        let modified_nanos = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Ok(Revision {
            modified_nanos,
            len: meta.len(),
        })
    }
}

/// One entity found by a scan, with the path it was found at, relative to the library root.
#[derive(Debug, Clone)]
pub struct Found<T> {
    pub entity: T,
    pub path: PathBuf,
    pub revision: Revision,
}

/// Something wrong with the library that the user needs told about.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub path: PathBuf,
    pub message: String,
    /// The revision of the offending file, where there is one. Recorded so a permanently
    /// broken document does not make the index look stale on every open.
    pub revision: Option<String>,
}

/// Everything a full walk of the library found.
#[derive(Debug, Default)]
pub struct Scan {
    pub topics: Vec<Found<Topic>>,
    pub documents: Vec<Found<Document>>,
    pub sources: Vec<Found<crate::citation::Source>>,
    pub works: Vec<FoundWork>,
    pub problems: Vec<Problem>,
}

/// A work found by a scan: its folder, relative to the library root, and its revision.
#[derive(Debug, Clone)]
pub struct FoundWork {
    pub work: Work,
    pub path: PathBuf,
    pub revision: WorkRevision,
}

pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(library: &Library) -> Self {
        Store {
            root: library.root().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn absolute(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }

    // ---- topics ----------------------------------------------------------------

    pub fn read_topic(&self, dir: &Path) -> Result<Found<Topic>> {
        let manifest = self.absolute(dir).join(topic::MANIFEST);
        let text = fs::read_to_string(&manifest).ctx(format!("reading {}", manifest.display()))?;
        let entity: Topic = toml::from_str(&text)
            .map_err(|e| Error::library(format!("{}/{}: {e}", dir.display(), topic::MANIFEST)))?;
        Ok(Found {
            entity,
            path: dir.to_path_buf(),
            revision: Revision::of(&manifest)?,
        })
    }

    pub fn write_topic(&self, dir: &Path, entity: &Topic) -> Result<()> {
        let text = toml::to_string_pretty(entity)
            .map_err(|e| Error::library(format!("cannot serialise topic: {e}")))?;
        atomic::write(&self.absolute(dir).join(topic::MANIFEST), &text)
    }

    // ---- documents -------------------------------------------------------------

    pub fn read_document(&self, path: &Path) -> Result<Found<Document>> {
        let absolute = self.absolute(path);
        let text = fs::read_to_string(&absolute).ctx(format!("reading {}", absolute.display()))?;
        let (meta, body): (document::Meta, String) =
            frontmatter::parse(&text, &path.display().to_string())?;
        Ok(Found {
            entity: Document { meta, body },
            path: path.to_path_buf(),
            revision: Revision::of(&absolute)?,
        })
    }

    pub fn write_document(&self, path: &Path, entity: &Document) -> Result<Revision> {
        let text = frontmatter::render(&entity.meta, &entity.body)?;
        let absolute = self.absolute(path);
        atomic::write(&absolute, &text)?;
        Revision::of(&absolute)
    }

    /// Refuse to write if the file changed since `expected` was taken.
    pub fn check_revision(&self, path: &Path, expected: Revision) -> Result<()> {
        let absolute = self.absolute(path);
        if !absolute.exists() {
            return Err(Error::NotFound(path.display().to_string()));
        }
        if Revision::of(&absolute)? == expected {
            Ok(())
        } else {
            Err(Error::Conflict {
                path: path.to_path_buf(),
            })
        }
    }

    // ---- sources ---------------------------------------------------------------

    pub fn read_source(&self, path: &Path) -> Result<Found<crate::citation::Source>> {
        let absolute = self.absolute(path);
        let text = fs::read_to_string(&absolute).ctx(format!("reading {}", absolute.display()))?;
        let entity = toml::from_str(&text)
            .map_err(|e| Error::library(format!("{}: {e}", path.display())))?;
        Ok(Found {
            entity,
            path: path.to_path_buf(),
            revision: Revision::of(&absolute)?,
        })
    }

    pub fn write_source(&self, path: &Path, entity: &crate::citation::Source) -> Result<Revision> {
        let text = toml::to_string_pretty(entity)
            .map_err(|e| Error::library(format!("cannot serialise source: {e}")))?;
        let absolute = self.absolute(path);
        atomic::write(&absolute, &text)?;
        Revision::of(&absolute)
    }

    pub fn remove_source(&self, path: &Path) -> Result<()> {
        atomic::remove(&self.absolute(path))
    }

    // ---- source files ----------------------------------------------------------

    /// Where a stored source file lives, as an absolute path.
    ///
    /// The name is what the source record holds, and is treated as a name and not as a path:
    /// a record carrying `../../etc/passwd` — hand-edited, or written by something else —
    /// must not reach outside the library.
    pub fn source_file_path(&self, name: &str) -> Option<PathBuf> {
        let mut parts = Path::new(name).components();
        match (parts.next(), parts.next()) {
            (Some(std::path::Component::Normal(one)), None) => {
                Some(self.absolute(Path::new(SOURCE_FILES_DIR)).join(one))
            }
            _ => None,
        }
    }

    /// Copy a file into the library, under a name derived from `stem`.
    ///
    /// Returns the name to record. The extension comes from the file being copied so the
    /// stored copy opens in the same application the original would.
    pub fn copy_source_file(&self, stem: &str, from: &Path) -> Result<String> {
        let extension = from
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("pdf")
            .to_ascii_lowercase();
        let name = format!("{stem}.{extension}");
        let to = self
            .source_file_path(&name)
            .ok_or_else(|| Error::invalid("file", "that name cannot be stored"))?;
        atomic::copy_in(from, &to)?;
        Ok(name)
    }

    // ---- images beside a document ---------------------------------------------

    /// Copy a file in as `name` inside `dir`, atomically.
    pub fn copy_beside(&self, dir: &Path, name: &str, from: &Path) -> Result<()> {
        let to = self.beside(dir, name)?;
        atomic::copy_in(from, &to)
    }

    /// Move `name` from one folder to another, avoiding names already taken there.
    ///
    /// A name that cannot be moved is skipped rather than refused, and the move goes ahead.
    ///
    /// Two cases, one answer. The picture may be gone — somebody deleted it by hand. Or the
    /// name may not be a name at all: a body is plain text, and a hand-edited
    /// `![x](../../elsewhere.png)` must never be followed out of the folder it belongs to.
    /// Either way the reference is already broken, and refusing to move somebody's document
    /// over a broken picture would be the worse answer — it would strand the document too.
    pub fn move_beside(
        &self,
        from_dir: &Path,
        to_dir: &Path,
        name: &str,
        taken: &HashSet<String>,
    ) -> Result<()> {
        let Ok(from) = self.beside(from_dir, name) else {
            return Ok(());
        };
        if !from.is_file() {
            return Ok(());
        }
        let stem = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let extension = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let settled = format!("{}.{extension}", slug::unique(stem, taken));
        atomic::rename(&from, &self.beside(to_dir, &settled)?)
    }

    /// A picture beside a document, if it is actually there.
    ///
    /// `None` covers both ways a name can lead nowhere: it is not a name that may be resolved
    /// at all, or it is and the file has been deleted by hand. Neither is an error — a body is
    /// plain text somebody may type anything into, and a picture is theirs to remove.
    pub fn beside_file(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        self.beside(dir, name).ok().filter(|path| path.is_file())
    }

    /// A file beside a document, as an absolute path.
    ///
    /// The name is treated as a name: a body naming `../../elsewhere.png` — typed by hand, or
    /// written by something else — must not reach outside the folder it belongs to.
    fn beside(&self, dir: &Path, name: &str) -> Result<PathBuf> {
        let mut parts = Path::new(name).components();
        match (parts.next(), parts.next()) {
            (Some(std::path::Component::Normal(one)), None) => Ok(self.absolute(dir).join(one)),
            _ => Err(Error::invalid("image", "that name cannot be stored")),
        }
    }

    pub fn remove_source_file(&self, name: &str) -> Result<()> {
        match self.source_file_path(name) {
            Some(path) => atomic::remove(&path),
            // A name that cannot be resolved cannot have been written by us, so there is
            // nothing of ours to remove and nothing to report.
            None => Ok(()),
        }
    }

    // ---- moving and removing ---------------------------------------------------

    pub fn move_path(&self, from: &Path, to: &Path) -> Result<()> {
        atomic::rename(&self.absolute(from), &self.absolute(to))
    }

    pub fn remove_document(&self, path: &Path) -> Result<()> {
        atomic::remove(&self.absolute(path))
    }

    /// Remove a folder and everything inside it.
    ///
    /// The one operation here that is not reversible by any amount of care, which is why
    /// nothing calls it without the user having been shown what is in the folder first.
    pub fn remove_tree(&self, dir: &Path) -> Result<()> {
        let absolute = self.absolute(dir);
        if !absolute.exists() {
            return Ok(());
        }
        fs::remove_dir_all(&absolute).ctx(format!("removing {}", absolute.display()))
    }

    /// Remove an empty folder, and say nothing if it is not there.
    pub fn remove_empty(&self, dir: &Path) -> Result<()> {
        let absolute = self.absolute(dir);
        if !absolute.exists() {
            return Ok(());
        }
        fs::remove_dir(&absolute).ctx(format!("removing {}", absolute.display()))
    }

    /// Slugs already used inside a directory, so a new one can avoid them.
    pub fn taken_names(&self, dir: &Path) -> Result<HashSet<String>> {
        let absolute = self.absolute(dir);
        if !absolute.exists() {
            return Ok(HashSet::new());
        }
        let mut taken = HashSet::new();
        for entry in fs::read_dir(&absolute).ctx(format!("listing {}", absolute.display()))? {
            let entry = entry.ctx(format!("listing {}", absolute.display()))?;
            // Compare stems, not filenames. Trimming one known extension meant a `.toml`
            // source was recorded as "nieminen-2021.toml" while a new one asked about
            // "nieminen-2021" — so the collision went unseen and the second source
            // overwrote the first.
            let name = entry.file_name();
            let stem = Path::new(&name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| name.to_string_lossy().to_string());
            taken.insert(stem);
        }
        Ok(taken)
    }

    // ---- walking ---------------------------------------------------------------

    /// Walk the whole library.
    ///
    /// This is what a rebuild runs, and what makes the index derived rather than
    /// authoritative: everything the index knows can be recovered from here.
    pub fn scan(&self) -> Result<Scan> {
        let mut scan = Scan::default();
        if self.root.join(TOPICS_DIR).exists() {
            self.walk(&PathBuf::from(TOPICS_DIR), &mut scan)?;
        }
        if self.root.join(SOURCES_DIR).exists() {
            self.walk_sources(&mut scan)?;
        }
        if self.root.join(WORKS_DIR).exists() {
            self.walk_works(&mut scan)?;
        }
        detect_duplicate_ids(&mut scan);
        Ok(scan)
    }

    /// Every file the scan would parse, with its revision, without parsing any of them.
    ///
    /// This is the cheap pass run on open: stat-only, so it costs a directory walk rather than
    /// a read of every document, and it is enough to notice anything that changed while the
    /// application was closed.
    pub fn signature(&self) -> Result<BTreeMap<PathBuf, String>> {
        let mut out = BTreeMap::new();
        if self.root.join(TOPICS_DIR).exists() {
            self.walk_signature(&PathBuf::from(TOPICS_DIR), &mut out)?;
        }
        if self.root.join(SOURCES_DIR).exists() {
            self.walk_signature(&PathBuf::from(SOURCES_DIR), &mut out)?;
        }
        for dir in self.work_dirs()? {
            out.insert(
                dir.clone(),
                works::revision(&self.root, &dir)?.token().to_string(),
            );
        }
        Ok(out)
    }

    /// Every folder in `works/` holding a `work.toml`, in name order.
    pub fn work_dirs(&self) -> Result<Vec<PathBuf>> {
        let absolute = self.absolute(Path::new(WORKS_DIR));
        if !absolute.exists() {
            return Ok(Vec::new());
        }
        let mut dirs = Vec::new();
        for entry in fs::read_dir(&absolute).ctx(format!("listing {}", absolute.display()))? {
            let entry = entry.ctx(format!("listing {}", absolute.display()))?;
            let relative = PathBuf::from(WORKS_DIR).join(entry.file_name());
            if entry.file_type().ctx("reading a directory entry")?.is_dir()
                && self.absolute(&relative).join(works::MANIFEST).exists()
            {
                dirs.push(relative);
            }
        }
        dirs.sort();
        Ok(dirs)
    }

    fn walk_works(&self, scan: &mut Scan) -> Result<()> {
        for dir in self.work_dirs()? {
            match works::read(&self.root, &dir) {
                Ok((work, revision)) => {
                    for problem in work_problems(&work) {
                        scan.problems.push(Problem {
                            path: dir.clone(),
                            message: problem,
                            revision: None,
                        });
                    }
                    scan.works.push(FoundWork {
                        work,
                        path: dir,
                        revision,
                    });
                }
                Err(e) => scan.problems.push(Problem {
                    message: e.to_string(),
                    revision: works::revision(&self.root, &dir)
                        .ok()
                        .map(|r| r.token().to_string()),
                    path: dir,
                }),
            }
        }
        Ok(())
    }

    fn walk_signature(&self, dir: &Path, out: &mut BTreeMap<PathBuf, String>) -> Result<()> {
        let absolute = self.absolute(dir);
        for entry in fs::read_dir(&absolute).ctx(format!("listing {}", absolute.display()))? {
            let entry = entry.ctx(format!("listing {}", absolute.display()))?;
            let relative = dir.join(entry.file_name());
            if Library::is_reserved(&relative) {
                continue;
            }
            let file_type = entry.file_type().ctx("reading a directory entry")?;
            if file_type.is_dir() {
                let manifest = relative.join(topic::MANIFEST);
                if self.absolute(&manifest).exists() {
                    out.insert(
                        manifest.clone(),
                        Revision::of(&self.absolute(&manifest))?.token(),
                    );
                }
                self.walk_signature(&relative, out)?;
            } else if relative.extension().and_then(|e| e.to_str()) == Some("md") {
                out.insert(
                    relative.clone(),
                    Revision::of(&self.absolute(&relative))?.token(),
                );
            }
        }
        Ok(())
    }

    fn walk_sources(&self, scan: &mut Scan) -> Result<()> {
        let dir = PathBuf::from(SOURCES_DIR);
        let absolute = self.absolute(&dir);
        let mut entries: Vec<_> = fs::read_dir(&absolute)
            .ctx(format!("listing {}", absolute.display()))?
            .collect::<std::io::Result<Vec<_>>>()
            .ctx(format!("listing {}", absolute.display()))?;
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let relative = dir.join(entry.file_name());
            if relative.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            match self.read_source(&relative) {
                Ok(found) => scan.sources.push(found),
                Err(e) => scan.problems.push(Problem {
                    path: relative.clone(),
                    message: e.to_string(),
                    revision: revision_token(&self.absolute(&relative)),
                }),
            }
        }
        Ok(())
    }

    fn walk(&self, dir: &Path, scan: &mut Scan) -> Result<()> {
        let absolute = self.absolute(dir);
        let mut entries: Vec<_> = fs::read_dir(&absolute)
            .ctx(format!("listing {}", absolute.display()))?
            .collect::<std::io::Result<Vec<_>>>()
            .ctx(format!("listing {}", absolute.display()))?;
        // Sorted so a scan is deterministic, which the rebuild-equivalence test relies on.
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let name = entry.file_name();
            let relative = dir.join(&name);
            if Library::is_reserved(&relative) {
                continue;
            }

            let file_type = entry.file_type().ctx("reading a directory entry")?;
            if file_type.is_dir() {
                if self.absolute(&relative).join(topic::MANIFEST).exists() {
                    match self.read_topic(&relative) {
                        Ok(found) => scan.topics.push(found),
                        Err(e) => scan.problems.push(Problem {
                            path: relative.join(topic::MANIFEST),
                            message: e.to_string(),
                            revision: revision_token(
                                &self.absolute(&relative.join(topic::MANIFEST)),
                            ),
                        }),
                    }
                }
                self.walk(&relative, scan)?;
            } else if relative.extension().and_then(|e| e.to_str()) == Some("md") {
                match self.read_document(&relative) {
                    Ok(found) => scan.documents.push(found),
                    Err(e) => scan.problems.push(Problem {
                        path: relative.clone(),
                        message: e.to_string(),
                        revision: revision_token(&self.absolute(&relative)),
                    }),
                }
            }
        }
        Ok(())
    }
}

/// Two files claiming the same identifier is a library error, not something to resolve
/// silently — it usually means a user copied a file to duplicate it.
fn detect_duplicate_ids(scan: &mut Scan) {
    let mut seen: Vec<(Id, PathBuf)> = Vec::new();
    let mut problems = Vec::new();

    let all = scan
        .topics
        .iter()
        .map(|f| (f.entity.id, f.path.clone()))
        .chain(
            scan.documents
                .iter()
                .map(|f| (f.entity.id(), f.path.clone())),
        )
        .chain(
            scan.works
                .iter()
                .map(|f| (f.work.manifest.id, f.path.clone())),
        );

    for (id, path) in all {
        if let Some((_, first)) = seen.iter().find(|(seen_id, _)| *seen_id == id) {
            problems.push(Problem {
                path: path.clone(),
                message: format!(
                    "duplicate identifier {id}, already used by {}",
                    first.display()
                ),
                revision: None,
            });
        } else {
            seen.push((id, path));
        }
    }

    scan.problems.extend(problems);
}

/// What is wrong with a work that still reads: a missing edition file, an edition out of line
/// with the original, a repeat that no longer matches.
pub(crate) fn work_problems(work: &Work) -> Vec<String> {
    let mut out = Vec::new();
    for id in &work.missing {
        if let Some(e) = work.manifest.editions.iter().find(|e| e.id == *id) {
            out.push(format!(
                "the file for {}, {}, is missing",
                works::describe(e),
                e.file
            ));
        }
    }
    for m in work.misalignments() {
        if work.missing.contains(&m.edition) {
            continue;
        }
        if let Some(e) = work.manifest.editions.iter().find(|e| e.id == m.edition) {
            out.push(format!(
                "{} has {} rows and the original has {}, so they do not line up",
                works::describe(e),
                m.rows,
                m.original_rows
            ));
        }
    }
    out.extend(work.repeat_problems());
    out
}

fn revision_token(path: &Path) -> Option<String> {
    Revision::of(path).ok().map(|r| r.token())
}

/// The timestamp to record as a document's `modified` field.
pub fn now() -> Timestamp {
    Timestamp::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two entities whose names collide must get distinct files, whatever their extension.
    ///
    /// This failed for sources: the taken-name check trimmed only `.md`, so a second source
    /// by the same author in the same year was handed the filename of the first and wrote
    /// straight over it. Silent, and a lost source is a lost source.
    #[test]
    fn taken_names_compares_stems_across_extensions() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sources")).unwrap();
        fs::write(dir.path().join("sources/nieminen-2021.toml"), "").unwrap();
        fs::write(dir.path().join("topics/note.md"), "").unwrap_or_else(|_| {
            fs::create_dir_all(dir.path().join("topics")).unwrap();
            fs::write(dir.path().join("topics/note.md"), "").unwrap()
        });

        let library = Library::create(dir.path().join("lib"), "L").unwrap();
        let store = Store {
            root: dir.path().to_path_buf(),
        };
        let _ = library;

        let taken = store.taken_names(Path::new("sources")).unwrap();
        assert!(taken.contains("nieminen-2021"), "got {taken:?}");
        assert_eq!(slug::unique("Nieminen 2021", &taken), "nieminen-2021-2");

        let taken = store.taken_names(Path::new("topics")).unwrap();
        assert!(taken.contains("note"), "got {taken:?}");
    }
}
