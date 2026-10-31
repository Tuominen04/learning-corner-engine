//! The `work.*` operations of `backend-contract.md`.
//!
//! Every change names the work's revision from the last read and is refused with `Conflict`
//! if any of its files changed since — the user may edit them in a text editor, and a write
//! made from an older read must not undo that.

use std::path::{Path, PathBuf};

use super::*;
use crate::store::FoundWork;
use crate::works::{
    self, Cell, Edition, EditionKind, Misalignment, NewEdition, Note, Repeat, ResegmentEdition,
    Unit, Work, WorkRevision,
};

/// A work in the list of works.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkSummary {
    pub id: Id,
    pub title: String,
    /// Each edition's language, original first, in pane order.
    pub languages: Vec<String>,
    pub misaligned: bool,
}

/// A work as the interface sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkView {
    pub id: Id,
    pub title: String,
    pub unit: Unit,
    pub created: Timestamp,
    pub sources: Vec<Id>,
    /// In pane order, each with its rows. Row *n* of every edition is the same piece of text
    /// while the work is aligned.
    pub editions: Vec<EditionView>,
    pub notes: Vec<Note>,
    pub repeats: Vec<Repeat>,
    /// Editions whose row count differs from the original's. Never hidden.
    pub misalignments: Vec<Misalignment>,
    /// Pass back with the next change; a mismatch means a file changed underneath.
    pub revision: WorkRevision,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditionView {
    pub id: Id,
    pub kind: EditionKind,
    pub language: String,
    pub scheme: Option<String>,
    pub label: Option<String>,
    pub source: Option<Id>,
    pub cells: Vec<Cell>,
    /// Its file was not there; it reads as having no rows.
    pub missing: bool,
}

/// What removing an edition would take with it.
#[derive(Debug, Clone, PartialEq)]
pub struct EditionContents {
    pub edition: Id,
    /// "the translation (fi)", or its label.
    pub name: String,
    pub rows_with_text: usize,
}

/// What deleting a work would take with it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkContents {
    pub title: String,
    pub editions: Vec<EditionContents>,
    pub notes: usize,
}

/// What dividing a work again would do.
#[derive(Debug, Clone, PartialEq)]
pub struct ResegmentPlan {
    pub work: Id,
    pub unit: Unit,
    pub editions: Vec<ResegmentEdition>,
    /// Notes on rows that would no longer exist; kept in `.conflicts/` when applied.
    pub notes_moved: Vec<Note>,
    pub repeats_dropped: Vec<Repeat>,
    /// The revision the plan was made from. Applying refuses if the work changed since.
    pub revision: WorkRevision,
}

/// Where a re-division kept the notes it could not keep in place, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Resegmented {
    pub work: WorkView,
    pub notes_kept_at: Option<PathBuf>,
}

impl Engine {
    /// Every work, by folder name.
    pub fn work_list(&self) -> Result<Vec<WorkSummary>> {
        let mut out = Vec::new();
        for entry in self.index.entries(Kind::Work)? {
            let (work, _) = works::read(self.store.root(), &entry.path)?;
            out.push(WorkSummary {
                id: work.manifest.id,
                title: work.manifest.title.clone(),
                languages: work
                    .manifest
                    .editions
                    .iter()
                    .map(|e| e.language.clone())
                    .collect(),
                misaligned: !work.misalignments().is_empty(),
            });
        }
        Ok(out)
    }

    /// A new work, its original divided by `unit`.
    pub fn work_create(
        &mut self,
        title: impl Into<String>,
        unit: Unit,
        text: &str,
        language: impl Into<String>,
        source: Option<Id>,
    ) -> Result<WorkView> {
        self.library.ensure_writable()?;
        if let Some(source) = source {
            self.expect(source, Kind::Source)?;
        }
        let work = Work::new(title.into(), unit, text, language.into(), source)?;
        let dir = works::new_folder(self.store.root(), &work.manifest.title)?;
        works::write(self.store.root(), &dir, &work)?;
        self.index.rebuild(&self.store)?;
        self.work_view_at(&dir, &[])
    }

    /// A work with the editions asked for — all of them when `editions` is empty.
    pub fn work_read(&self, id: Id, editions: &[Id]) -> Result<WorkView> {
        let entry = self.expect(id, Kind::Work)?;
        self.work_view_at(&entry.path, editions)
    }

    pub fn work_update(
        &mut self,
        id: Id,
        title: impl Into<String>,
        sources: Vec<Id>,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err(Error::invalid("title", "a work needs a title"));
        }
        for source in &sources {
            self.expect(*source, Kind::Source)?;
        }
        self.change_work(id, revision, |w| {
            w.manifest.title = title;
            w.manifest.sources = sources;
            Ok(())
        })
        .map(|(view, ())| view)
    }

    /// Add a pronunciation or a translation. Its text is divided by the work's unit; empty
    /// text gives it an empty row for every row of the original.
    pub fn work_add_edition(
        &mut self,
        id: Id,
        edition: NewEdition,
        text: &str,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        if let Some(source) = edition.source {
            self.expect(source, Kind::Source)?;
        }
        self.change_work(id, revision, |w| w.add_edition(edition, text).map(|_| ()))
            .map(|(view, ())| view)
    }

    pub fn work_update_edition(
        &mut self,
        id: Id,
        edition: Id,
        label: Option<String>,
        scheme: Option<String>,
        source: Option<Id>,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        if let Some(source) = source {
            self.expect(source, Kind::Source)?;
        }
        self.change_work(id, revision, |w| {
            w.update_edition(edition, label, scheme, source)
        })
        .map(|(view, ())| view)
    }

    /// Move an edition to `position` among the panes, counting from 1.
    pub fn work_move_edition(
        &mut self,
        id: Id,
        edition: Id,
        position: usize,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.move_edition(edition, position))
            .map(|(view, ())| view)
    }

    /// What removing an edition deletes. Asked before [`Engine::work_remove_edition`].
    pub fn work_remove_edition_plan(&self, id: Id, edition: Id) -> Result<EditionContents> {
        let entry = self.expect(id, Kind::Work)?;
        let (work, _) = works::read(self.store.root(), &entry.path)?;
        let i = work.edition(edition)?;
        if work.manifest.editions[i].kind == EditionKind::Original {
            return Err(Error::invalid(
                "edition",
                "the original cannot be removed; a work without one is not a work",
            ));
        }
        Ok(contents(&work.manifest.editions[i], &work.cells[i]))
    }

    pub fn work_remove_edition(
        &mut self,
        id: Id,
        edition: Id,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        let entry = self.expect(id, Kind::Work)?;
        let (view, file) = self.change_work(id, revision, |w| w.remove_edition(edition))?;
        // The work no longer names the file, so removing it last leaves nothing dangling.
        let path = self.store.root().join(&entry.path).join(&file);
        if path.exists() {
            atomic::remove(&path)?;
        }
        self.refresh_work(&entry.path)?;
        self.work_view_at(&entry.path, &[]).or(Ok(view))
    }

    /// Write one row of one edition. Line breaks in `text` become lines within the row.
    pub fn work_write_row(
        &mut self,
        id: Id,
        edition: Id,
        row: usize,
        text: &str,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.write_row(edition, row, text))
            .map(|(view, ())| view)
    }

    /// An empty row at `row` in every edition.
    pub fn work_insert_row(
        &mut self,
        id: Id,
        row: usize,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.insert_row(row))
            .map(|(view, ())| view)
    }

    /// Remove `row` from every edition, while editing `edition`.
    pub fn work_remove_row(
        &mut self,
        id: Id,
        edition: Id,
        row: usize,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.remove_row(edition, row))
            .map(|(view, ())| view)
    }

    pub fn work_mark_empty(
        &mut self,
        id: Id,
        edition: Id,
        row: usize,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.mark_empty(edition, row))
            .map(|(view, ())| view)
    }

    pub fn work_join(
        &mut self,
        id: Id,
        edition: Id,
        row: usize,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.join(edition, row))
            .map(|(view, ())| view)
    }

    /// Split `row` of one edition before its line `line`, counting from 1.
    pub fn work_split(
        &mut self,
        id: Id,
        edition: Id,
        row: usize,
        line: usize,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.split(edition, row, line))
            .map(|(view, ())| view)
    }

    /// Make `row` a repeat of the earlier row `of`, or stop it being one. Answers with the work
    /// and how many rows now share the text.
    pub fn work_set_repeat(
        &mut self,
        id: Id,
        row: usize,
        of: Option<usize>,
        revision: &WorkRevision,
    ) -> Result<(WorkView, usize)> {
        self.change_work(id, revision, |w| w.set_repeat(row, of))
    }

    /// A note on `row`; empty text removes it.
    pub fn work_set_note(
        &mut self,
        id: Id,
        row: usize,
        text: &str,
        revision: &WorkRevision,
    ) -> Result<WorkView> {
        self.change_work(id, revision, |w| w.set_note(row, text))
            .map(|(view, ())| view)
    }

    /// What dividing the work again by `unit` would do. Nothing is written.
    pub fn work_resegment_plan(&self, id: Id, unit: Unit) -> Result<ResegmentPlan> {
        let entry = self.expect(id, Kind::Work)?;
        let (work, revision) = works::read(self.store.root(), &entry.path)?;
        let done = work.resegment(unit);
        Ok(ResegmentPlan {
            work: id,
            unit,
            editions: done.editions,
            notes_moved: done.notes_moved,
            repeats_dropped: done.repeats_dropped,
            revision,
        })
    }

    /// Divide the work again, as the plan showed. Refused with `Conflict` if it changed since.
    pub fn work_resegment(&mut self, plan: &ResegmentPlan) -> Result<Resegmented> {
        self.library.ensure_writable()?;
        let entry = self.expect(plan.work, Kind::Work)?;
        let root = self.store.root().to_path_buf();
        works::check(&root, &entry.path, &plan.revision)?;
        let (work, _) = works::read(&root, &entry.path)?;
        let done = work.resegment(plan.unit);
        // The notes go somewhere safe before the work stops naming them.
        let notes_kept_at = works::keep_notes(&root, &entry.path, &done.notes_moved)?;
        works::write(&root, &entry.path, &done.work)?;
        self.refresh_work(&entry.path)?;
        Ok(Resegmented {
            work: self.work_view_at(&entry.path, &[])?,
            notes_kept_at,
        })
    }

    /// What deleting a work takes with it. Asked before [`Engine::work_delete`].
    pub fn work_delete_plan(&self, id: Id) -> Result<WorkContents> {
        let entry = self.expect(id, Kind::Work)?;
        let (work, _) = works::read(self.store.root(), &entry.path)?;
        Ok(WorkContents {
            title: work.manifest.title.clone(),
            editions: work
                .manifest
                .editions
                .iter()
                .zip(&work.cells)
                .map(|(e, cells)| contents(e, cells))
                .collect(),
            notes: work.manifest.notes.len(),
        })
    }

    /// Delete a work's whole folder. The plan was the warning; nothing is kept.
    pub fn work_delete(&mut self, id: Id) -> Result<()> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Work)?;
        self.store.remove_tree(&entry.path)?;
        self.index.rebuild(&self.store)?;
        Ok(())
    }

    /// Works citing this source, as a work's source or as one of its editions'.
    pub fn source_citing_works(&self, id: Id) -> Result<Vec<WorkSummary>> {
        let mut out = Vec::new();
        for citing in self.index.citing_documents(id)? {
            let entry = self.index.resolve(citing)?;
            if entry.kind != Kind::Work {
                continue;
            }
            let (work, _) = works::read(self.store.root(), &entry.path)?;
            out.push(WorkSummary {
                id: work.manifest.id,
                title: work.manifest.title.clone(),
                languages: work
                    .manifest
                    .editions
                    .iter()
                    .map(|e| e.language.clone())
                    .collect(),
                misaligned: !work.misalignments().is_empty(),
            });
        }
        Ok(out)
    }

    /// Stop every work citing `source` from citing it — what deleting a source does to them.
    pub(super) fn forget_source_in_works(&mut self, source: Id) -> Result<()> {
        let root = self.store.root().to_path_buf();
        for citing in self.index.citing_documents(source)? {
            let entry = self.index.resolve(citing)?;
            if entry.kind != Kind::Work {
                continue;
            }
            let (mut work, _) = works::read(&root, &entry.path)?;
            if work.forget_source(source) {
                works::write(&root, &entry.path, &work)?;
            }
        }
        Ok(())
    }

    // ---- internals -------------------------------------------------------------

    /// Read a work, check its revision, change it, write it, and answer with it as it now is.
    fn change_work<T>(
        &mut self,
        id: Id,
        revision: &WorkRevision,
        change: impl FnOnce(&mut Work) -> Result<T>,
    ) -> Result<(WorkView, T)> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Work)?;
        let root = self.store.root().to_path_buf();
        works::check(&root, &entry.path, revision)?;
        let (mut work, _) = works::read(&root, &entry.path)?;
        let out = change(&mut work)?;
        works::write(&root, &entry.path, &work)?;
        self.refresh_work(&entry.path)?;
        Ok((self.work_view_at(&entry.path, &[])?, out))
    }

    fn refresh_work(&mut self, dir: &Path) -> Result<()> {
        let (work, revision) = works::read(self.store.root(), dir)?;
        self.index.upsert_work(&FoundWork {
            work,
            path: dir.to_path_buf(),
            revision,
        })
    }

    fn work_view_at(&self, dir: &Path, only: &[Id]) -> Result<WorkView> {
        let (work, revision) = works::read(self.store.root(), dir)?;
        let misalignments = work.misalignments();
        let mut editions = Vec::new();
        for (e, cells) in work.manifest.editions.iter().zip(&work.cells) {
            if !only.is_empty() && !only.contains(&e.id) {
                continue;
            }
            // A repeat shows its first occurrence's text, even where a hand edit has made the
            // two differ — that difference is listed as a problem until somebody chooses.
            let mut cells = cells.clone();
            for r in &work.manifest.repeats {
                if let Some(first) = cells.get(r.of - 1).cloned() {
                    if let Some(slot) = cells.get_mut(r.row - 1) {
                        *slot = first;
                    }
                }
            }
            editions.push(EditionView {
                id: e.id,
                kind: e.kind,
                language: e.language.clone(),
                scheme: e.scheme.clone(),
                label: e.label.clone(),
                source: e.source,
                cells,
                missing: work.missing.contains(&e.id),
            });
        }
        Ok(WorkView {
            id: work.manifest.id,
            title: work.manifest.title.clone(),
            unit: work.manifest.unit,
            created: work.manifest.created,
            sources: work.manifest.sources.clone(),
            editions,
            notes: work.manifest.notes.clone(),
            repeats: work.manifest.repeats.clone(),
            misalignments,
            revision,
        })
    }
}

fn contents(e: &Edition, cells: &[Cell]) -> EditionContents {
    EditionContents {
        edition: e.id,
        name: works::describe(e),
        rows_with_text: cells.iter().filter(|c| !c.is_empty()).count(),
    }
}
