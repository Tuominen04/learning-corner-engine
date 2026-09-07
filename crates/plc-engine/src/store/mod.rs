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

/// The directory holding all root topics.
pub const TOPICS_DIR: &str = "topics";

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
    pub problems: Vec<Problem>,
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

    // ---- moving and removing ---------------------------------------------------

    pub fn move_path(&self, from: &Path, to: &Path) -> Result<()> {
        atomic::rename(&self.absolute(from), &self.absolute(to))
    }

    pub fn remove_document(&self, path: &Path) -> Result<()> {
        atomic::remove(&self.absolute(path))
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
            let name = entry.file_name().to_string_lossy().to_string();
            taken.insert(name.trim_end_matches(".md").to_string());
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
        let topics_root = self.root.join(TOPICS_DIR);
        if topics_root.exists() {
            self.walk(&PathBuf::from(TOPICS_DIR), &mut scan)?;
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
        let topics_root = self.root.join(TOPICS_DIR);
        if topics_root.exists() {
            self.walk_signature(&PathBuf::from(TOPICS_DIR), &mut out)?;
        }
        Ok(out)
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

fn revision_token(path: &Path) -> Option<String> {
    Revision::of(path).ok().map(|r| r.token())
}

/// The timestamp to record as a document's `modified` field.
pub fn now() -> Timestamp {
    Timestamp::now()
}
