//! The operations the interface calls.
//!
//! This is the M0 subset of `backend-contract.md`. Operations belonging to later milestones
//! are present and return [`Error::Unimplemented`], so the shape of the surface is fixed now
//! and the interface can be written against the whole of it.
//!
//! Every operation takes identifiers, never paths inside a library: where an entity lives is
//! this crate's business, and keeping it that way is what lets files be renamed and moved
//! without anything else having to know.

use std::path::{Path, PathBuf};

use crate::citation::disambiguate::Disambiguation;
use crate::citation::format::{self, Form, Locator};
use crate::citation::{Field, Source};
use crate::error::{Error, Result};
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

    pub fn create(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self> {
        let library = Library::create(root, name)?;
        Self::from_library(library)
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
        found.entity.name = name;
        self.store.write_topic(&dir, &found.entity)?;
        // The folder keeps its old slug: renaming it would be cosmetic, and moving a whole
        // subtree to follow a rename risks far more than a stale-looking folder name costs.
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

    pub fn document_rename(&mut self, id: Id, title: impl Into<String>) -> Result<DocumentView> {
        self.library.ensure_writable()?;
        let title = title.into();
        if title.trim().is_empty() {
            return Err(Error::invalid("title", "a document needs a title"));
        }

        let entry = self.expect(id, Kind::Document)?;
        let mut found = self.store.read_document(&entry.path)?;
        found.entity.meta.title = title;
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
            self.store.move_path(&entry.path, &to)?;
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

    // ---- later milestones ------------------------------------------------------

    pub fn work_create(&mut self) -> Result<()> {
        Err(Error::Unimplemented("work.create"))
    }

    pub fn map_read(&self) -> Result<()> {
        Err(Error::Unimplemented("map.read"))
    }

    pub fn backup_plan(&self, _destination: &Path) -> Result<()> {
        Err(Error::Unimplemented("backup.plan"))
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
