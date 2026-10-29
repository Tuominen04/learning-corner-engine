//! The operations the interface calls.
//!
//! This is the M0 subset of `backend-contract.md`. Operations belonging to later milestones
//! are present and return [`Error::Unimplemented`], so the shape of the surface is fixed now
//! and the interface can be written against the whole of it.
//!
//! Every operation takes identifiers, never paths inside a library: where an entity lives is
//! this crate's business, and keeping it that way is what lets files be renamed and moved
//! without anything else having to know.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::atomic;
use crate::backup;
use crate::citation::disambiguate::Disambiguation;
use crate::citation::format::{self, Form, Locator};
use crate::citation::{CitationRef, Field, Source};
use crate::error::{Error, IoContext, Result};
use crate::export;
use crate::id::Id;
use crate::index::{Entry, Hit, Index, Kind, RebuildSummary};
use crate::library::{Library, Manifest};
use crate::model::{topic, Document, Topic};
use crate::store::{slug, Problem, Revision, Store, SOURCES_DIR, TOPICS_DIR};
use crate::timestamp::Timestamp;

/// An open library, with its store and index.
pub struct Engine {
    library: Library,
    store: Store,
    index: Index,
}

/// What renaming a library did with its folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryFolder {
    /// It follows the name: renamed to match, or already called what the name would call it.
    Follows,
    /// Left as it is, because it holds things that are not the library's, and renaming it
    /// would move them too.
    Shared,
    /// Left as it is, because it is the top of a drive, which is named for the drive.
    Drive,
}

/// A document as the interface sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentView {
    pub id: Id,
    pub title: String,
    pub body: String,
    pub topic: Id,
    pub created: Timestamp,
    pub modified: Timestamp,
    /// Pass back to [`Engine::document_write`]; a mismatch means the file changed underneath.
    pub revision: Revision,
}

/// One node of the topic tree.
#[derive(Debug, Clone, PartialEq)]
pub struct TopicNode {
    pub id: Id,
    pub name: String,
    pub children: Vec<TopicNode>,
    pub documents: Vec<DocumentSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSummary {
    pub id: Id,
    pub title: String,
}

/// What a topic holds, so that deleting it can be a decision rather than a surprise.
///
/// The documents are named rather than counted. "3 documents" is a number somebody agrees to;
/// a list of titles is a thing they recognise, and one of them being called "Thesis notes" is
/// how a mistake gets caught before it happens.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TopicContents {
    /// Topics nested anywhere below this one.
    pub topics: usize,
    /// Every document anywhere below this one, in path order.
    pub documents: Vec<DocumentSummary>,
}

/// What becomes of what is inside a topic being deleted.
///
/// There is no default. `backend-contract.md` has said since M0 that deleting a topic never
/// deletes documents implicitly, and an operation that picks for you is exactly that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contents {
    /// Delete the topic and everything under it.
    Delete,
    /// Move what is directly inside up to the topic's parent, then delete the empty topic.
    MoveUp,
}

/// The two documents a split leaves behind.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    /// The document that was split, now carrying a link where the text used to be.
    pub original: DocumentView,
    /// The new document, holding the text that was taken out.
    pub taken: DocumentView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub name: String,
    pub topics: usize,
    pub documents: usize,
    pub problems: Vec<Problem>,
    pub writable: bool,
}

impl Engine {
    // ---- library ---------------------------------------------------------------

    /// Make a library in exactly the folder `root`.
    ///
    /// For tests, and for putting a library back where it was. A person making a new one goes
    /// through [`Engine::create_in`], which makes the folder too.
    pub fn create(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self> {
        let library = Library::create(root, name)?;
        Self::from_library(library)
    }

    /// Make a new library in a folder of its own inside `location`, named after it.
    ///
    /// The folder somebody picks is where the library goes, not the library: choosing
    /// `Documents` makes `Documents/learning/`, rather than filling `Documents` with a
    /// library's files. The folder is named the way a topic's is, with a number on the end if
    /// the name is already taken there.
    pub fn create_in(location: impl AsRef<Path>, name: impl Into<String>) -> Result<Self> {
        let location = location.as_ref();
        let name = name.into();
        let folder = slug::unique(&name, &names_in(location)?);
        Self::create(location.join(folder), name)
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let library = Library::open(root)?;
        Self::from_library(library)
    }

    fn from_library(library: Library) -> Result<Self> {
        let store = Store::new(&library);
        let index = Index::open(&library, &store)?;
        Ok(Engine {
            library,
            store,
            index,
        })
    }

    pub fn manifest(&self) -> &Manifest {
        self.library.manifest()
    }

    pub fn root(&self) -> &Path {
        self.library.root()
    }

    /// Give the library another name, and its folder with it when the folder is its own.
    ///
    /// A name follows what it holds, here as for a topic: a library called "Nutrition" in a
    /// folder called `testing` is the kind of mismatch that makes a folder useless to read. But
    /// only a folder holding nothing but the library is renamed. One that holds other things
    /// too — somebody's whole `Documents`, or other libraries beside this one — is theirs, and
    /// so is a drive's; those keep their names, and the answer says which it was.
    pub fn library_rename(&mut self, name: impl Into<String>) -> Result<LibraryFolder> {
        let name = name.into();
        // The name first, in the folder where it already is. If moving the folder fails, the
        // library is still called what it was asked to be — the old outcome, only untidy.
        self.library.rename(name.clone())?;

        let from = std::path::absolute(self.library.root())
            .ctx(format!("finding {}", self.library.root().display()))?;
        let Some(parent) = from.parent() else {
            return Ok(LibraryFolder::Drive);
        };
        if is_top_of_a_drive(&from, parent) {
            return Ok(LibraryFolder::Drive);
        }
        if !self.library.has_its_folder_to_itself()? {
            return Ok(LibraryFolder::Shared);
        }

        let mut taken = names_in(parent)?;
        if let Some(own) = from.file_name() {
            taken.remove(&own.to_string_lossy().to_lowercase());
        }
        let to = parent.join(slug::unique(&name, &taken));
        if to == from {
            return Ok(LibraryFolder::Follows);
        }

        // The index lives inside the folder about to move. Let go of it first — Windows will
        // not move a folder with a file open inside it, and SQLite finds its journal by path —
        // and open everything again from wherever the library is afterwards.
        self.index.release();
        let moved = atomic::rename(&from, &to);
        *self = Engine::open(if moved.is_ok() { &to } else { &from })?;
        moved?;
        Ok(LibraryFolder::Follows)
    }

    pub fn status(&self) -> Result<Status> {
        Ok(Status {
            name: self.library.name().to_string(),
            topics: self.index.entries(Kind::Topic)?.len(),
            documents: self.index.entries(Kind::Document)?.len(),
            problems: self.index.problems()?,
            writable: self.library.is_writable(),
        })
    }

    /// Discard the index and rebuild it from the files.
    ///
    /// Always safe: the index holds nothing that is not derivable from a scan.
    pub fn rebuild_index(&mut self) -> Result<RebuildSummary> {
        self.index.rebuild(&self.store)
    }

    // ---- topics ----------------------------------------------------------------

    pub fn topic_create(&mut self, parent: Option<Id>, name: impl Into<String>) -> Result<Id> {
        self.library.ensure_writable()?;
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Error::invalid("name", "a topic needs a name"));
        }

        let parent_dir = match parent {
            Some(id) => self.topic_dir(id)?,
            None => PathBuf::from(TOPICS_DIR),
        };
        let taken = self.store.taken_names(&parent_dir)?;
        let dir = parent_dir.join(slug::unique(&name, &taken));

        let entity = Topic::new(name);
        let id = entity.id;
        self.store.write_topic(&dir, &entity)?;
        self.index.rebuild(&self.store)?;
        Ok(id)
    }

    pub fn topic_rename(&mut self, id: Id, name: impl Into<String>) -> Result<()> {
        self.library.ensure_writable()?;
        let name = name.into();
        if name.trim().is_empty() {
            return Err(Error::invalid("name", "a topic needs a name"));
        }

        let dir = self.topic_dir(id)?;
        let mut found = self.store.read_topic(&dir)?;
        found.entity.name = name.clone();

        // The name goes in first, in the folder that already exists. If the move below fails,
        // the topic still knows what it is called — the old outcome, which was never wrong,
        // only untidy.
        self.store.write_topic(&dir, &found.entity)?;

        // And then the folder follows it. Files are the source of truth, and a user is told
        // they may open their library in a file manager — so a folder called `new-topic`
        // holding a topic called "Protein" is this application failing its own promise, not a
        // cosmetic detail. One `rename` moves the subtree, which is what `topic_move` already
        // relies on; nothing inside is touched or rewritten.
        let to = self.renamed_beside(&dir, &name)?;
        if to != dir {
            self.store.move_path(&dir, &to)?;
        }

        // Every document underneath has a new path now, and the index is where paths live.
        self.index.rebuild(&self.store)?;
        Ok(())
    }

    pub fn topic_move(&mut self, id: Id, new_parent: Option<Id>) -> Result<()> {
        self.library.ensure_writable()?;
        let from = self.topic_dir(id)?;

        let parent_dir = match new_parent {
            Some(parent) => {
                let parent_dir = self.topic_dir(parent)?;
                if parent_dir.starts_with(&from) {
                    return Err(Error::invalid(
                        "parent",
                        "a topic cannot be moved inside itself",
                    ));
                }
                parent_dir
            }
            None => PathBuf::from(TOPICS_DIR),
        };

        let name = from
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let taken = self.store.taken_names(&parent_dir)?;
        let to = parent_dir.join(slug::unique(&name, &taken));
        if from == to {
            return Ok(());
        }

        self.store.move_path(&from, &to)?;
        self.index.rebuild(&self.store)?;
        Ok(())
    }

    /// Where something belongs once it has been renamed, beside where it sits now.
    ///
    /// The extension is the caller's business — a topic is a folder and a document is a
    /// `.md`, and only the caller knows which it is holding.
    ///
    /// Its own name is not counted as taken. Without that, renaming a topic to the name it
    /// already has would turn `protein` into `protein-2`, and a user who opened the rename
    /// box and pressed Enter without changing anything would watch their folder get a number
    /// on the end for no reason.
    fn renamed_beside(&self, current: &Path, name: &str) -> Result<PathBuf> {
        let parent = current
            .parent()
            .ok_or_else(|| Error::library("nothing in a library sits outside it"))?;
        let mut taken = self.store.taken_names(parent)?;
        if let Some(stem) = current.file_stem().and_then(|s| s.to_str()) {
            taken.remove(stem);
        }
        Ok(parent.join(slug::unique(name, &taken)))
    }

    /// What deleting this topic would reach.
    ///
    /// Everything below it, not only what is directly inside: a topic with one empty-looking
    /// sub-topic can hold a year of writing two levels down, and a plan that stopped at the
    /// first level would be the most misleading possible answer.
    pub fn topic_delete_plan(&self, id: Id) -> Result<TopicContents> {
        let dir = self.topic_dir(id)?;

        let mut contents = TopicContents::default();
        for entry in self.index.entries(Kind::Topic)? {
            if entry.path.starts_with(&dir) && entry.path != dir {
                contents.topics += 1;
            }
        }
        for entry in self.index.entries(Kind::Document)? {
            if entry.path.starts_with(&dir) {
                contents.documents.push(DocumentSummary {
                    id: entry.id,
                    title: entry.title,
                });
            }
        }
        Ok(contents)
    }

    /// Delete a topic, having been told what to do with what is inside it.
    pub fn topic_delete(&mut self, id: Id, contents: Contents) -> Result<()> {
        self.library.ensure_writable()?;
        let dir = self.topic_dir(id)?;

        if contents == Contents::MoveUp {
            // The parent, as a topic. `topics/protein` has none — its parent is the root,
            // and a document cannot live there.
            let parent_dir = dir
                .parent()
                .ok_or_else(|| Error::library("a topic outside the library"))?
                .to_path_buf();
            let parent = if parent_dir == Path::new(TOPICS_DIR) {
                None
            } else {
                Some(self.store.read_topic(&parent_dir)?.entity.id)
            };

            // Only what is *directly* inside. A nested topic carries its own contents with it,
            // and moving those one at a time would be moving the same documents twice.
            let documents: Vec<Id> = self
                .index
                .entries(Kind::Document)?
                .into_iter()
                .filter(|e| e.path.parent() == Some(dir.as_path()))
                .map(|e| e.id)
                .collect();
            let topics: Vec<Id> = self
                .index
                .entries(Kind::Topic)?
                .into_iter()
                .filter(|e| e.path.parent() == Some(dir.as_path()))
                .map(|e| e.id)
                .collect();

            match parent {
                Some(parent) => {
                    for document in documents {
                        self.document_move(document, parent)?;
                    }
                }
                None if documents.is_empty() => {}
                // Said plainly rather than guessed at. Inventing a topic to hold them, or
                // deleting them because there is nowhere to put them, are both worse than
                // telling somebody that this particular choice is not available here.
                None => return Err(Error::invalid(
                    "contents",
                    "a document cannot sit outside a topic, so these have nowhere to move up to",
                )),
            }

            for topic in topics {
                self.topic_move(topic, parent)?;
            }

            // Whatever is left is the topic's own `topic.toml` and anything a user put there
            // by hand, so this is a tree removal and not an empty-folder one.
            self.store.remove_tree(&dir)?;
        } else {
            self.store.remove_tree(&dir)?;
        }

        // Everything under the folder has gone or moved, and the index is where paths live.
        self.index.rebuild(&self.store)?;
        Ok(())
    }

    pub fn topic_tree(&self) -> Result<Vec<TopicNode>> {
        let topics = self.index.entries(Kind::Topic)?;
        let documents = self.index.entries(Kind::Document)?;
        let root = Path::new(TOPICS_DIR);
        Ok(build_tree(root, &topics, &documents))
    }

    // ---- documents -------------------------------------------------------------

    pub fn document_create(&mut self, topic: Id, title: impl Into<String>) -> Result<Id> {
        self.library.ensure_writable()?;
        let title = title.into();
        if title.trim().is_empty() {
            return Err(Error::invalid("title", "a document needs a title"));
        }

        let dir = self.topic_dir(topic)?;
        let taken = self.store.taken_names(&dir)?;
        let path = dir.join(format!("{}.md", slug::unique(&title, &taken)));

        let document = Document::new(title);
        let id = document.id();
        let revision = self.store.write_document(&path, &document)?;
        self.index.upsert_document(&crate::store::Found {
            entity: document,
            path,
            revision,
        })?;
        Ok(id)
    }

    pub fn document_read(&self, id: Id) -> Result<DocumentView> {
        let entry = self.expect(id, Kind::Document)?;
        let found = self.store.read_document(&entry.path)?;
        self.view(found)
    }

    /// Replace a document's body.
    ///
    /// `expected` comes from the last read. If the file changed underneath — because the user
    /// edited it in a text editor, which they are explicitly told they may — the write is
    /// refused with [`Error::Conflict`] rather than discarding their edit.
    pub fn document_write(
        &mut self,
        id: Id,
        body: impl Into<String>,
        expected: Revision,
    ) -> Result<DocumentView> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Document)?;
        self.store.check_revision(&entry.path, expected)?;

        let mut found = self.store.read_document(&entry.path)?;
        found.entity.body = body.into();
        found.entity.meta.modified = Timestamp::now();
        let revision = self.store.write_document(&entry.path, &found.entity)?;

        let found = crate::store::Found {
            entity: found.entity,
            path: entry.path,
            revision,
        };
        self.index.upsert_document(&found)?;
        self.view(found)
    }

    /// Take part of a document out into a new one, leaving a link behind.
    ///
    /// The reason this is one operation rather than three calls from the interface: a crash
    /// between writing the new document and rewriting the old one must not be able to lose
    /// somebody's writing. The order here makes the worst outcome a duplicate rather than a
    /// hole — the new document is written first, so a failure afterwards leaves the text in
    /// two places, which is visible and fixable. The other order loses it.
    ///
    /// `range` is a byte range into the body the caller last read, which is why `expected` is
    /// required: offsets into a file that has changed underneath address different text, and
    /// splitting on them would cut somebody's document in the wrong place.
    ///
    /// Citations and links inside the range move because they are *in* the text — there is no
    /// separate record of them to update. Pictures need nothing either: the new document is
    /// written beside the original, so the bare filenames its body carries still resolve.
    pub fn document_split(
        &mut self,
        id: Id,
        range: std::ops::Range<usize>,
        title: impl Into<String>,
        expected: Revision,
    ) -> Result<Split> {
        self.library.ensure_writable()?;
        let title = title.into();
        if title.trim().is_empty() {
            return Err(Error::invalid("title", "a document needs a title"));
        }

        let entry = self.expect(id, Kind::Document)?;
        self.store.check_revision(&entry.path, expected)?;
        let mut found = self.store.read_document(&entry.path)?;
        let body = found.entity.body.clone();

        if range.start > range.end || range.end > body.len() {
            return Err(Error::invalid("range", "that is not part of this document"));
        }
        // Checked rather than assumed. A byte offset that lands inside a character would
        // panic on the slice below, and this application is for people writing Finnish and
        // Russian, where multi-byte characters are the ordinary case rather than the exotic
        // one.
        if !body.is_char_boundary(range.start) || !body.is_char_boundary(range.end) {
            return Err(Error::invalid(
                "range",
                "that would cut a character in half",
            ));
        }

        // And nor may either end sit inside a citation, a link or a picture. Cutting a
        // character in half takes the process down and is caught above; cutting one of these
        // in half is quieter and worse, because it succeeds — the file keeps `3R4QFEM]` where
        // an identifier used to be, and nothing ever says so.
        //
        // Every construct is checked, not only the ones a client renders. A client renders
        // what it understands, so the markers it does *not* understand are the ones its user
        // can put a selection boundary inside, which makes them exactly the dangerous ones.
        if let Some(what) = cut_in_half(&body, range.start, range.end) {
            return Err(Error::invalid(
                "range",
                format!("that would cut {what} in half"),
            ));
        }

        // Trimmed, because a selection that reaches one line further than intended would
        // otherwise open the new document on a blank paragraph. Only the ends of what was
        // taken; the text left behind is spliced exactly where it was.
        let taken_text = body[range.clone()].trim();
        if taken_text.is_empty() {
            return Err(Error::invalid("range", "there is nothing there to take"));
        }

        // Beside the original rather than through `document_create`, which would need the
        // topic's identifier for a folder this already has in hand — and the folder is what
        // keeps the pictures working.
        let dir = entry
            .path
            .parent()
            .ok_or_else(|| Error::library("a document outside any folder"))?
            .to_path_buf();
        let taken_names = self.store.taken_names(&dir)?;
        let path = dir.join(format!("{}.md", slug::unique(&title, &taken_names)));

        let mut taken = Document::new(title);
        taken.body = taken_text.to_string();
        let taken_id = taken.id();
        let revision = self.store.write_document(&path, &taken)?;
        let taken = crate::store::Found {
            entity: taken,
            path,
            revision,
        };

        // Only now is the original rewritten, and only now can anything be lost.
        found.entity.body = format!(
            "{}[[{taken_id}]]{}",
            &body[..range.start],
            &body[range.end..]
        );
        found.entity.meta.modified = Timestamp::now();
        let revision = self.store.write_document(&entry.path, &found.entity)?;
        let original = crate::store::Found {
            entity: found.entity,
            path: entry.path,
            revision,
        };

        self.index.upsert_document(&taken)?;
        self.index.upsert_document(&original)?;

        Ok(Split {
            original: self.view(original)?,
            taken: self.view(taken)?,
        })
    }

    pub fn document_rename(&mut self, id: Id, title: impl Into<String>) -> Result<DocumentView> {
        self.library.ensure_writable()?;
        let title = title.into();
        if title.trim().is_empty() {
            return Err(Error::invalid("title", "a document needs a title"));
        }

        let entry = self.expect(id, Kind::Document)?;
        let mut found = self.store.read_document(&entry.path)?;
        found.entity.meta.title = title.clone();
        found.entity.meta.modified = Timestamp::now();

        // The title first, at the name the file already has, so a failure below leaves the
        // document saying what the user just called it.
        self.store.write_document(&entry.path, &found.entity)?;

        // Then the file follows the title. Identity is the ULID inside the file, so this
        // breaks no citation and no link — and leaving it behind is how a folder fills up
        // with `untitled-2.md`, which is the promise of a readable library going quietly
        // unkept.
        //
        // The pictures stay where they are: they sit beside the document in the same folder,
        // which this does not change.
        let path = self
            .renamed_beside(&entry.path, &title)?
            .with_extension("md");
        if path != entry.path {
            self.store.move_path(&entry.path, &path)?;
        }

        // Re-read rather than reuse: the revision has to be the one this file has now.
        let found = self.store.read_document(&path)?;
        self.index.upsert_document(&found)?;
        self.view(found)
    }

    pub fn document_move(&mut self, id: Id, topic: Id) -> Result<DocumentView> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Document)?;
        let dir = self.topic_dir(topic)?;

        let name = entry
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let taken = self.store.taken_names(&dir)?;
        let to = dir.join(format!("{}.md", slug::unique(&name, &taken)));

        if to != entry.path {
            // The images go with it. They are referenced by bare filename, so a document that
            // moved without them would keep pointing at names that are no longer beside it —
            // broken pictures in the app and in every other Markdown reader.
            let from_dir = entry
                .path
                .parent()
                .ok_or_else(|| Error::library("a document outside any folder"))?
                .to_path_buf();
            let images = self.store.read_document(&entry.path)?.entity.images();

            self.store.move_path(&entry.path, &to)?;

            let taken = self.store.taken_names(&dir)?;
            for image in images {
                self.store.move_beside(&from_dir, &dir, &image, &taken)?;
            }
        }
        self.index.rebuild(&self.store)?;
        let found = self.store.read_document(&to)?;
        self.view(found)
    }

    pub fn document_delete(&mut self, id: Id) -> Result<()> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Document)?;
        self.store.remove_document(&entry.path)?;
        self.index.forget(id)
    }

    /// Documents linking here, derived from `[[…]]` rather than stored.
    pub fn backlinks(&self, id: Id) -> Result<Vec<DocumentSummary>> {
        self.index
            .backlinks(id)?
            .into_iter()
            .map(|from| {
                self.index.resolve(from).map(|e| DocumentSummary {
                    id: e.id,
                    title: e.title,
                })
            })
            .collect()
    }

    pub fn search(&self, query: &str) -> Result<Vec<Hit>> {
        self.index.search(query)
    }

    // ---- sources ---------------------------------------------------------------

    /// Store a source.
    ///
    /// A source missing a required field is **saved anyway** and reported as incomplete.
    /// Somebody who cannot find a publisher should end up with an incomplete source, never
    /// with no source at all, so nothing here rejects and nothing is defaulted or invented.
    pub fn source_create(&mut self, source: Source) -> Result<(Id, Vec<Field>)> {
        self.library.ensure_writable()?;
        if source.title.trim().is_empty() {
            return Err(Error::invalid("title", "a source needs a title"));
        }

        let dir = PathBuf::from(SOURCES_DIR);
        let taken = self.store.taken_names(&dir)?;
        let stem = format!("{} {}", source.sort_key(), source.date.year_text());
        let path = dir.join(format!("{}.toml", slug::unique(&stem, &taken)));

        let id = source.id;
        let missing = source.missing_fields();
        self.store.write_source(&path, &source)?;
        self.index.rebuild(&self.store)?;
        Ok((id, missing))
    }

    pub fn source_read(&self, id: Id) -> Result<Source> {
        let entry = self.expect(id, Kind::Source)?;
        Ok(self.store.read_source(&entry.path)?.entity)
    }

    pub fn source_update(&mut self, source: Source) -> Result<Vec<Field>> {
        self.library.ensure_writable()?;
        let entry = self.expect(source.id, Kind::Source)?;

        // The attached file is not the caller's to change here, and is carried over from what
        // is already stored. An interface editing a title sends the fields its form knows
        // about, and one that had not heard of attachments would otherwise detach the paper
        // every time somebody fixed a typo. Attaching and detaching are their own operations.
        let mut source = source;
        source.file = self.store.read_source(&entry.path)?.entity.file;

        let missing = source.missing_fields();
        self.store.write_source(&entry.path, &source)?;
        self.index.rebuild(&self.store)?;
        Ok(missing)
    }

    /// Every source in the library, in reference-list order.
    pub fn source_list(&self) -> Result<Vec<Source>> {
        let mut sources = Vec::new();
        for entry in self.index.entries(Kind::Source)? {
            sources.push(self.store.read_source(&entry.path)?.entity);
        }
        sources.sort_by(format::reference_order);
        Ok(sources)
    }

    /// What would break if this source were deleted.
    ///
    /// Deleting a source orphans every citation of it, so this is two-phase without
    /// exception: the plan names the documents, and only then may `source_delete` run.
    pub fn source_delete_plan(&self, id: Id) -> Result<Vec<DocumentSummary>> {
        self.expect(id, Kind::Source)?;
        self.source_citing_documents(id)
    }

    pub fn source_delete(&mut self, id: Id) -> Result<()> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Source)?;
        // Its copy goes with it. Leaving the file behind would accumulate papers nobody can
        // reach from inside the app and nobody can identify from outside it.
        if let Some(name) = self.store.read_source(&entry.path)?.entity.file {
            self.store.remove_source_file(&name)?;
        }
        self.store.remove_source(&entry.path)?;
        self.index.rebuild(&self.store)?;
        Ok(())
    }

    /// Documents citing this source.
    ///
    /// The feature you are glad of on a bad day: a study is retracted, and this is how you
    /// find the eleven places you leaned on it.
    pub fn source_citing_documents(&self, id: Id) -> Result<Vec<DocumentSummary>> {
        self.index
            .citing_documents(id)?
            .into_iter()
            .map(|doc| {
                self.index.resolve(doc).map(|e| DocumentSummary {
                    id: e.id,
                    title: e.title,
                })
            })
            .collect()
    }

    /// Keep a file with a source, so the source has its paper for good.
    ///
    /// The file is **brought into the library**, not referenced. That is the entire point: the
    /// original can be deleted out of a downloads folder, or the memory stick it came from
    /// unplugged, and the library still has it. What is recorded is a filename inside the
    /// library, so moving the library folder moves the file with it.
    ///
    /// `keep` decides what becomes of the original — see [`Keep`]. Either way the library gets
    /// its own copy first, and a `Move` only removes the original once that copy is written,
    /// fsynced and recorded. There is no moment where the library's copy is the one at risk.
    ///
    /// Attaching a second time replaces the first, taking the old copy with it.
    pub fn source_attach(&mut self, id: Id, from: &Path, keep: Keep) -> Result<Source> {
        self.library.ensure_writable()?;
        ensure_pdf(from)?;

        let entry = self.expect(id, Kind::Source)?;
        let mut found = self.store.read_source(&entry.path)?;

        // Named after the source's own record, so a folder listing pairs them by eye:
        // `nieminen-2021.toml` beside `nieminen-2021.pdf`.
        let stem = entry
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::library("a source without a filename"))?;

        let name = self.store.copy_source_file(stem, from)?;

        // Only once the new copy is safely in place, and only if it did not overwrite it.
        if let Some(previous) = found.entity.file.take() {
            if previous != name {
                self.store.remove_source_file(&previous)?;
            }
        }

        found.entity.file = Some(name);
        self.store.write_source(&entry.path, &found.entity)?;
        self.index.rebuild(&self.store)?;

        if keep == Keep::Move {
            // Last of all, and deliberately so: the copy is on disk, fsynced, and named in a
            // record that has been written. Removing the original any earlier — even between
            // the copy and the record — risks taking away the only one anybody can find.
            //
            // A removal that fails is not a failed attach. A read-only disc or a file the user
            // cannot delete leaves the original where it was, which is exactly the `Copy`
            // outcome: the source has its paper and nothing has been lost. Turning that into
            // an error would report a failure for an operation that succeeded.
            let _ = std::fs::remove_file(from);
        }

        Ok(found.entity)
    }

    /// Keep a copy of an image with a document, beside it on disk.
    ///
    /// Returns the **filename** to write into the body as `![alt](name)`. Beside the document
    /// and named plainly, because that is what keeps the `.md` readable outside this
    /// application: a Markdown viewer, a text editor or a forge shows the picture, since the
    /// file is exactly where the document says it is.
    ///
    /// `keep` decides what becomes of the original, as for a source's paper. The library's own
    /// copy is written first either way.
    pub fn document_attach_image(&mut self, id: Id, from: &Path, keep: Keep) -> Result<String> {
        self.library.ensure_writable()?;
        ensure_image(from)?;

        let entry = self.expect(id, Kind::Document)?;
        let dir = entry
            .path
            .parent()
            .ok_or_else(|| Error::library("a document outside any folder"))?
            .to_path_buf();

        let stem = from.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let extension = from
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();

        // Unique against everything already in the folder, documents included: two pictures
        // both called `diagram.png` are two pictures, and one must not land on the other.
        let taken = self.store.taken_names(&dir)?;
        let name = format!("{}.{extension}", slug::unique(stem, &taken));
        self.store.copy_beside(&dir, &name, from)?;

        if keep == Keep::Move {
            // Last, once the library's copy is on disk — the same ordering a source's paper
            // gets, and for the same reason.
            let _ = std::fs::remove_file(from);
        }

        Ok(name)
    }

    /// Where a document's picture is, for showing it.
    ///
    /// The document is named by id and the picture by the bare name its body carries, so the
    /// interface never says where to look — the same arrangement a source's paper has, and
    /// the reason the window needs no filesystem permission of its own.
    ///
    /// `None` when there is nothing to show: a name that may not be resolved, or one that may
    /// and names a file no longer there. Bodies are plain text, so both are ordinary rather
    /// than exceptional, and a missing picture must not fail the document around it.
    pub fn document_image(&self, id: Id, name: &str) -> Result<Option<PathBuf>> {
        let entry = self.expect(id, Kind::Document)?;
        let dir = entry
            .path
            .parent()
            .ok_or_else(|| Error::library("a document outside any folder"))?;
        Ok(self.store.beside_file(dir, name))
    }

    /// Forget a source's file, and delete the library's copy of it.
    pub fn source_detach(&mut self, id: Id) -> Result<Source> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Source)?;
        let mut found = self.store.read_source(&entry.path)?;

        if let Some(previous) = found.entity.file.take() {
            self.store.remove_source_file(&previous)?;
        }
        self.store.write_source(&entry.path, &found.entity)?;
        self.index.rebuild(&self.store)?;
        Ok(found.entity)
    }

    /// Where a source's file is, for opening it.
    ///
    /// `None` when the source has no file, and also when the record names one that is not
    /// there — a user can delete anything in their own folder, and the honest answer to
    /// "where is it" is then that there is nothing to open.
    pub fn source_file(&self, id: Id) -> Result<Option<PathBuf>> {
        let entry = self.expect(id, Kind::Source)?;
        let Some(name) = self.store.read_source(&entry.path)?.entity.file else {
            return Ok(None);
        };
        Ok(self.store.source_file_path(&name).filter(|p| p.is_file()))
    }

    pub fn source_required_fields(&self, source_type: crate::citation::SourceType) -> Vec<Field> {
        source_type.required_fields().to_vec()
    }

    // ---- citations ---------------------------------------------------------------

    /// The in-text citation for one source, disambiguated against the whole library.
    pub fn citation_format(
        &self,
        source: Id,
        form: Form,
        locator: Option<Locator>,
    ) -> Result<String> {
        let sources = self.source_list()?;
        let target = sources
            .iter()
            .find(|s| s.id == source)
            .ok_or_else(|| Error::NotFound(source.to_string()))?;
        let disambiguation = Disambiguation::compute(&sources);
        Ok(format::in_text(
            target,
            &disambiguation,
            form,
            locator.as_ref(),
        ))
    }

    /// In-text citations for many at once, disambiguated against the whole library.
    ///
    /// The reason this exists rather than looping over [`Self::citation_format`]: each of those
    /// calls reads every source in the library and recomputes the disambiguation pass. A
    /// document with fifteen citations over a hundred sources would read fifteen hundred files
    /// to draw one screen. Here the library is read once and the pass runs once, whatever the
    /// document asks for.
    ///
    /// A request naming a source that no longer exists yields `None` in its place rather than
    /// failing the batch. One deleted source must not blank an entire document — the citation
    /// that points nowhere is the one thing the reader needs to see.
    pub fn citation_format_many(
        &self,
        requests: &[(Id, Form, Option<Locator>)],
    ) -> Result<Vec<Option<String>>> {
        let sources = self.source_list()?;
        let disambiguation = Disambiguation::compute(&sources);
        Ok(requests
            .iter()
            .map(|(source, form, locator)| {
                sources
                    .iter()
                    .find(|s| s.id == *source)
                    .map(|target| format::in_text(target, &disambiguation, *form, locator.as_ref()))
            })
            .collect())
    }

    /// The reference list for one document, built from the sources it actually cites.
    ///
    /// Disambiguation is computed over the **whole library**, not over this document's
    /// sources, so the same source cites identically in every document that uses it.
    pub fn citation_reference_list(&self, document: Id) -> Result<Vec<String>> {
        let view = self.document_read(document)?;
        let cited: Vec<Id> = crate::model::Document {
            meta: crate::model::document::Meta {
                id: view.id,
                title: view.title.clone(),
                created: view.created,
                modified: view.modified,
            },
            body: view.body.clone(),
        }
        .citations();

        let all = self.source_list()?;
        let disambiguation = Disambiguation::compute(&all);
        let used: Vec<Source> = all.into_iter().filter(|s| cited.contains(&s.id)).collect();
        Ok(format::reference_list(&used, &disambiguation))
    }

    // ---- export ----------------------------------------------------------------

    /// A document as a file somebody else can read, in `format`.
    ///
    /// Every citation is written out against the whole library, exactly as the editor shows
    /// it, and the reference list follows the writing. Links read as the titles of the
    /// documents they lead to, and pictures go inside the file wherever the format can carry
    /// them. Nothing is written anywhere: where the bytes go is the caller's choice.
    pub fn export_document(&self, id: Id, format: export::Format) -> Result<Vec<u8>> {
        let view = self.document_read(id)?;
        let sources = self.source_list()?;
        let disambiguation = Disambiguation::compute(&sources);

        let cited = crate::citation::reference::parse_all(&view.body);
        let used: Vec<Source> = sources
            .iter()
            .filter(|s| cited.iter().any(|c| c.source == s.id))
            .cloned()
            .collect();
        let references = format::reference_list(&used, &disambiguation);

        let resolver = Resolver {
            engine: self,
            document: id,
            sources: &sources,
            disambiguation: &disambiguation,
        };
        let model = export::build(&view.title, &view.body, &references, &resolver);

        let together = |e: &dyn std::fmt::Display| {
            Error::library(format!(
                "putting the {} file together: {e}",
                format.extension()
            ))
        };
        Ok(match format {
            export::Format::Text => export::text::write(&model).into_bytes(),
            export::Format::Html => export::html::write(&model).into_bytes(),
            export::Format::Docx => export::docx::write(&model).map_err(|e| together(&e))?,
            export::Format::Odt => export::odt::write(&model).map_err(|e| together(&e))?,
            export::Format::Pdf => export::pdf::write(&model).map_err(|e| together(&e))?,
        })
    }

    // ---- later milestones ------------------------------------------------------

    pub fn work_create(&mut self) -> Result<()> {
        Err(Error::Unimplemented("work.create"))
    }

    pub fn map_read(&self) -> Result<()> {
        Err(Error::Unimplemented("map.read"))
    }

    // ---- backup ----------------------------------------------------------------

    /// Plan a backup of this library to `destination`: empty, or a copy of this library.
    pub fn backup_plan(&self, destination: &Path) -> Result<backup::Plan> {
        backup::plan_backup(&self.library, destination)
    }

    /// Apply a backup plan. `overwrite` is required when the drive has changes of its own.
    pub fn backup_apply(
        &mut self,
        plan: &backup::Plan,
        overwrite: bool,
    ) -> Result<backup::Applied> {
        backup::apply_backup(&self.library, plan, overwrite)
    }

    /// Plan bringing `drive` back into `destination`: a library's own folder, or an empty one.
    ///
    /// Needs no open library. Bringing back into the library that is open means closing it
    /// first, so nothing is written to it from two places at once.
    pub fn restore_plan(drive: &Path, destination: &Path) -> Result<backup::Plan> {
        backup::plan_restore(drive, destination)
    }

    /// Apply a restore plan, with an answer for every entry that asks.
    pub fn restore_apply(
        plan: &backup::Plan,
        answers: &std::collections::BTreeMap<String, backup::Answer>,
    ) -> Result<backup::Applied> {
        backup::apply_restore(plan, answers)
    }

    // ---- internals -------------------------------------------------------------

    fn expect(&self, id: Id, kind: Kind) -> Result<Entry> {
        let entry = self.index.resolve(id)?;
        if entry.kind == kind {
            Ok(entry)
        } else {
            Err(Error::NotFound(id.to_string()))
        }
    }

    fn topic_dir(&self, id: Id) -> Result<PathBuf> {
        Ok(self.expect(id, Kind::Topic)?.path)
    }

    fn view(&self, found: crate::store::Found<Document>) -> Result<DocumentView> {
        let topic_dir = found.path.parent().unwrap_or(Path::new(TOPICS_DIR));
        let topic = self.store.read_topic(topic_dir)?.entity.id;
        Ok(DocumentView {
            id: found.entity.id(),
            title: found.entity.title().to_string(),
            body: found.entity.body.clone(),
            topic,
            created: found.entity.meta.created,
            modified: found.entity.meta.modified,
            revision: found.revision,
        })
    }
}

/// What an exported document's citations, links and pictures lead to, answered from the
/// library.
struct Resolver<'a> {
    engine: &'a Engine,
    document: Id,
    sources: &'a [Source],
    disambiguation: &'a Disambiguation,
}

impl Resolver<'_> {
    fn source(&self, id: Id) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }
}

impl export::Resolve for Resolver<'_> {
    fn known(&self, source: Id) -> bool {
        self.source(source).is_some()
    }

    fn introduces(&self, source: Id) -> Option<String> {
        let (group, _) = format::short_form(self.source(source)?)?;
        Some(group.to_string())
    }

    fn parenthetical(&self, together: &[(CitationRef, bool)]) -> String {
        let cited: Vec<format::Cite> = together
            .iter()
            .filter_map(|(c, introduced)| {
                Some(format::Cite {
                    source: self.source(c.source)?,
                    locator: c.locator.as_ref(),
                    introduced: *introduced,
                })
            })
            .collect();
        format::parenthetical_together(&cited, self.disambiguation)
    }

    fn narrative(&self, citation: &CitationRef, introduced: bool) -> Option<String> {
        let cite = format::Cite {
            source: self.source(citation.source)?,
            locator: citation.locator.as_ref(),
            introduced,
        };
        Some(format::in_text_cite(
            cite,
            self.disambiguation,
            Form::Narrative,
        ))
    }

    fn link(&self, document: Id) -> Option<String> {
        self.engine
            .document_read(document)
            .ok()
            .map(|view| view.title)
    }

    fn picture(&self, name: &str) -> Option<Vec<u8>> {
        let path = self.engine.document_image(self.document, name).ok()??;
        fs::read(path).ok()
    }
}

/// What becomes of the original when a file is attached to a source.
///
/// The library always ends up with its own copy; this only decides whether the file the user
/// picked stays where it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Leave the original alone. The safe answer, and the right one for a file that lives
    /// somewhere deliberate — a folder of papers, a shared drive, a memory stick.
    Copy,
    /// Remove the original once the library's copy is safe. What a downloads folder wants:
    /// the file has been filed, and leaving a second copy behind is how that folder becomes
    /// a place nobody can find anything in.
    Move,
}

/// The syntax in a body that must survive a split whole, and what to call each kind.
const CONSTRUCTS: &[(&str, &str, &str)] = &[
    ("[^cite:", "]", "a citation"),
    ("[[", "]]", "a link"),
    ("![", ")", "a picture"),
];

/// Which construct, if any, one of these offsets lands inside.
///
/// A boundary touching one is not cutting it: a range ending exactly where a citation begins
/// does not include the citation, and pushing it out would take one nobody asked to move.
fn cut_in_half(body: &str, start: usize, end: usize) -> Option<&'static str> {
    for (opens, closes, what) in CONSTRUCTS {
        let mut from = 0;
        while let Some(at) = body[from..].find(opens) {
            let opened = from + at;
            let after = opened + opens.len();
            let Some(length) = body[after..].find(closes) else {
                // Unterminated, so there is nothing whole here to break. A stray `[[` is
                // text somebody typed.
                break;
            };
            let closed = after + length + closes.len();

            if (start > opened && start < closed) || (end > opened && end < closed) {
                return Some(what);
            }
            from = closed;
        }
    }
    None
}

/// Refuse anything that is not an image this application can show.
///
/// Checked by content as well as by extension, for the same reason a PDF is: the extension is
/// a claim, and a picture that will not render is worth catching while the user is still
/// looking at the file they chose.
fn ensure_image(from: &Path) -> Result<()> {
    if !from.is_file() {
        return Err(Error::invalid("image", "that file is not there"));
    }

    let mut head = [0u8; 16];
    let read = {
        use std::io::Read;
        let mut file = std::fs::File::open(from)
            .map_err(|e| Error::io(format!("opening {}", from.display()), e))?;
        file.read(&mut head)
            .map_err(|e| Error::io(format!("reading {}", from.display()), e))?
    };
    let head = &head[..read];

    let known = head.starts_with(b"\x89PNG\r\n\x1a\n")
        || head.starts_with(b"\xff\xd8\xff")
        || head.starts_with(b"GIF87a")
        || head.starts_with(b"GIF89a")
        || (head.starts_with(b"RIFF") && head.len() >= 12 && &head[8..12] == b"WEBP");
    if !known {
        return Err(Error::invalid(
            "image",
            "that file is not a PNG, JPEG, GIF or WebP image",
        ));
    }
    Ok(())
}

/// Refuse anything that is not a PDF, by extension **and** by content.
///
/// The extension alone is a claim rather than a fact — a renamed `.docx` carries one just as
/// well — and a file that will not open is worth catching while the user is still looking at
/// the file they chose, rather than months later when they go back to check a claim.
fn ensure_pdf(from: &Path) -> Result<()> {
    if !from.is_file() {
        return Err(Error::invalid("file", "that file is not there"));
    }

    let named_pdf = from
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if !named_pdf {
        return Err(Error::invalid(
            "file",
            "only PDF files can be kept with a source",
        ));
    }

    // `%PDF-` normally opens the file, but the specification tolerates junk ahead of it and
    // readers do too, so the header is looked for rather than demanded at byte zero.
    let mut head = [0u8; 1024];
    let read = {
        use std::io::Read;
        let mut file = std::fs::File::open(from)
            .map_err(|e| Error::io(format!("opening {}", from.display()), e))?;
        file.read(&mut head)
            .map_err(|e| Error::io(format!("reading {}", from.display()), e))?
    };
    if !head[..read].windows(5).any(|w| w == b"%PDF-") {
        return Err(Error::invalid(
            "file",
            "that file is named .pdf but is not a PDF",
        ));
    }
    Ok(())
}

/// Assemble the tree from paths. Parent/child is the directory relation, so the tree the user
/// sees and the tree on disk cannot disagree.
fn build_tree(parent: &Path, topics: &[Entry], documents: &[Entry]) -> Vec<TopicNode> {
    topics
        .iter()
        .filter(|t| t.path.parent() == Some(parent))
        .map(|t| TopicNode {
            id: t.id,
            name: t.title.clone(),
            children: build_tree(&t.path, topics, documents),
            documents: documents
                .iter()
                .filter(|d| d.path.parent() == Some(t.path.as_path()))
                .map(|d| DocumentSummary {
                    id: d.id,
                    title: d.title.clone(),
                })
                .collect(),
        })
        .collect()
}

/// Re-exported so callers do not need the module path for the one constant they touch.
pub use topic::MANIFEST as TOPIC_MANIFEST;

/// What is already in `dir`, lowercased.
///
/// Lowercased so that a new folder is never a near-twin of one already there: `learning` beside
/// `Learning` is a mistake waiting to happen, and on some filesystems it is the same folder.
fn names_in(dir: &Path) -> Result<HashSet<String>> {
    let listing = format!("listing {}", dir.display());
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|entry| {
                entry
                    .map(|e| e.file_name().to_string_lossy().to_lowercase())
                    .ctx(listing.clone())
            })
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(e) => Err(Error::io(listing, e)),
    }
}

/// Whether `dir` is where a drive is mounted: on another device from the folder it sits in.
///
/// A drive's folder is named for the drive — its label, usually — and renaming it is not the
/// library's business even when the library is all that is on it. Looked at without following
/// a symlink, so a library reached through one renames the link rather than its target.
#[cfg(unix)]
fn is_top_of_a_drive(dir: &Path, parent: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::symlink_metadata(dir), fs::metadata(parent)) {
        (Ok(dir), Ok(parent)) => dir.dev() != parent.dev(),
        _ => false,
    }
}

/// Elsewhere the top of a drive has no parent at all, which is caught before this is asked.
#[cfg(not(unix))]
fn is_top_of_a_drive(_dir: &Path, _parent: &Path) -> bool {
    false
}
