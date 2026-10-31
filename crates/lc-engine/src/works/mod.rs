//! Works and their editions — `parallel-text-model.md`, implemented.
//!
//! A work is a folder in `works/`: `work.toml`, and one plain text file per edition. Every
//! operation here reads the whole work, changes it in memory, and writes it back — the
//! editions first and `work.toml` last, so an interruption leaves at worst a note one row from
//! where it was, never words lost. Works are small; reading one whole is cheaper than being
//! clever about it.

pub mod text;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use text::{Cell, Unit};

use crate::atomic;
use crate::error::{Error, IoContext, Result};
use crate::id::Id;
use crate::store::slug;
use crate::timestamp::Timestamp;

/// The directory holding every work in the library.
pub const WORKS_DIR: &str = "works";

/// The file at the root of every work's folder.
pub const MANIFEST: &str = "work.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditionKind {
    Original,
    Pronunciation,
    Translation,
}

/// One edition, as `work.toml` lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edition {
    pub id: Id,
    /// The engine's business: made once from the kind and language, never renamed.
    pub file: String,
    pub kind: EditionKind,
    /// BCP 47.
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// Tells two editions of one kind and language apart: whose translation, which year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Where this version came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Id>,
}

/// A note on a row. Rows count from 1, in the original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub row: usize,
    pub text: String,
}

/// `row` is the same text as the earlier row `of`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repeat {
    pub row: usize,
    pub of: usize,
}

/// `work.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: Id,
    pub title: String,
    pub unit: Unit,
    pub created: Timestamp,
    #[serde(default)]
    pub sources: Vec<Id>,
    pub editions: Vec<Edition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeats: Vec<Repeat>,
}

/// An opaque token for the state of a work's whole folder.
///
/// One for the folder rather than one per file, because one change — a row inserted — writes
/// every edition at once, and an edit made to any of them in a text editor must stop a write
/// made from what was read before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkRevision(String);

impl WorkRevision {
    pub fn token(&self) -> &str {
        &self.0
    }

    pub fn from_token(token: impl Into<String>) -> Self {
        WorkRevision(token.into())
    }
}

/// A work in memory: its manifest, and each edition's rows in the manifest's order.
#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    pub manifest: Manifest,
    pub cells: Vec<Vec<Cell>>,
    /// Editions whose file was not there when the work was read. Read as having no rows.
    pub missing: Vec<Id>,
}

/// An edition whose row count differs from the original's.
///
/// Where the two stop corresponding is not something the engine can know — only a person
/// reading them can — so it says which edition and by how much, and the editions are shown
/// unaligned until the counts agree again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Misalignment {
    pub edition: Id,
    pub rows: usize,
    pub original_rows: usize,
}

impl Work {
    /// A new work with only its original.
    pub fn new(
        title: String,
        unit: Unit,
        text: &str,
        language: String,
        source: Option<Id>,
    ) -> Result<Self> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(Error::invalid("title", "a work needs a title"));
        }
        check_language(&language)?;
        let cells = text::divide(text, unit, &language);
        Ok(Work {
            manifest: Manifest {
                id: Id::new(),
                title,
                unit,
                created: Timestamp::now(),
                sources: Vec::new(),
                editions: vec![Edition {
                    id: Id::new(),
                    file: "original.txt".into(),
                    kind: EditionKind::Original,
                    language,
                    scheme: None,
                    label: None,
                    source,
                }],
                notes: Vec::new(),
                repeats: Vec::new(),
            },
            cells: vec![cells],
            missing: Vec::new(),
        })
    }

    pub fn original(&self) -> usize {
        self.manifest
            .editions
            .iter()
            .position(|e| e.kind == EditionKind::Original)
            .unwrap_or(0)
    }

    /// Rows, counted in the original.
    pub fn rows(&self) -> usize {
        self.cells.get(self.original()).map_or(0, Vec::len)
    }

    pub fn edition(&self, id: Id) -> Result<usize> {
        self.manifest
            .editions
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| Error::NotFound(id.to_string()))
    }

    pub fn misalignments(&self) -> Vec<Misalignment> {
        let original_rows = self.rows();
        self.manifest
            .editions
            .iter()
            .zip(&self.cells)
            .filter(|(_, cells)| cells.len() != original_rows)
            .map(|(e, cells)| Misalignment {
                edition: e.id,
                rows: cells.len(),
                original_rows,
            })
            .collect()
    }

    /// Every source the work cites: its own, and its editions'.
    pub fn cited_sources(&self) -> Vec<Id> {
        let mut out = self.manifest.sources.clone();
        for e in &self.manifest.editions {
            if let Some(s) = e.source {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
        out
    }

    /// Stop citing `source`, anywhere in the work. Whether anything changed.
    pub fn forget_source(&mut self, source: Id) -> bool {
        let before = self.manifest.clone();
        self.manifest.sources.retain(|s| *s != source);
        for e in &mut self.manifest.editions {
            if e.source == Some(source) {
                e.source = None;
            }
        }
        before != self.manifest
    }

    /// Repeated rows whose text differs from their first occurrence in some edition.
    pub fn repeat_problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for r in &self.manifest.repeats {
            for (e, cells) in self.manifest.editions.iter().zip(&self.cells) {
                let (Some(a), Some(b)) = (cells.get(r.of - 1), cells.get(r.row - 1)) else {
                    continue;
                };
                if a != b {
                    out.push(format!(
                        "row {} repeats row {} but differs from it in {}",
                        r.row,
                        r.of,
                        describe(e)
                    ));
                }
            }
        }
        out
    }

    /// The text to index: every edition's words and every note, markers left out.
    pub fn searchable_text(&self) -> String {
        let mut out = String::new();
        for cells in &self.cells {
            for cell in cells {
                for line in &cell.lines {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        for note in &self.manifest.notes {
            out.push_str(&note.text);
            out.push('\n');
        }
        out
    }

    // ---- editions ------------------------------------------------------------

    pub fn add_edition(&mut self, new: NewEdition, text: &str) -> Result<Id> {
        if new.kind == EditionKind::Original {
            return Err(Error::invalid("kind", "a work has exactly one original"));
        }
        check_language(&new.language)?;
        let scheme = clean(new.scheme);
        if scheme.is_some() && new.kind != EditionKind::Pronunciation {
            return Err(Error::invalid(
                "scheme",
                "only a pronunciation is written in a scheme",
            ));
        }
        let stem = match new.kind {
            EditionKind::Pronunciation => "pronunciation".to_string(),
            _ => format!("translation.{}", new.language),
        };
        let taken: std::collections::HashSet<String> = self
            .manifest
            .editions
            .iter()
            .map(|e| e.file.trim_end_matches(".txt").to_string())
            .collect();
        let mut file = stem.clone();
        let mut n = 2;
        while taken.contains(&file) {
            file = format!("{stem}-{n}");
            n += 1;
        }
        let cells = if text.trim().is_empty() {
            vec![Cell::empty(); self.rows()]
        } else {
            text::divide(text, self.manifest.unit, &new.language)
        };
        let id = Id::new();
        self.manifest.editions.push(Edition {
            id,
            file: format!("{file}.txt"),
            kind: new.kind,
            language: new.language,
            scheme,
            label: clean(new.label),
            source: new.source,
        });
        self.cells.push(cells);
        Ok(id)
    }

    pub fn update_edition(
        &mut self,
        edition: Id,
        label: Option<String>,
        scheme: Option<String>,
        source: Option<Id>,
    ) -> Result<()> {
        let i = self.edition(edition)?;
        let e = &mut self.manifest.editions[i];
        let scheme = clean(scheme);
        if scheme.is_some() && e.kind != EditionKind::Pronunciation {
            return Err(Error::invalid(
                "scheme",
                "only a pronunciation is written in a scheme",
            ));
        }
        e.label = clean(label);
        e.scheme = scheme;
        e.source = source;
        Ok(())
    }

    /// Move an edition to `position` among the panes, counting from 1. The others keep their
    /// order around it. Only the list in `work.toml` changes; no row moves.
    pub fn move_edition(&mut self, edition: Id, position: usize) -> Result<()> {
        let from = self.edition(edition)?;
        let count = self.manifest.editions.len();
        if position == 0 || position > count {
            return Err(Error::invalid(
                "position",
                format!("a position from 1 to {count}"),
            ));
        }
        let e = self.manifest.editions.remove(from);
        let cells = self.cells.remove(from);
        self.manifest.editions.insert(position - 1, e);
        self.cells.insert(position - 1, cells);
        Ok(())
    }

    /// Take an edition out, returning its file name. The original stays.
    pub fn remove_edition(&mut self, edition: Id) -> Result<String> {
        let i = self.edition(edition)?;
        if self.manifest.editions[i].kind == EditionKind::Original {
            return Err(Error::invalid(
                "edition",
                "the original cannot be removed; a work without one is not a work",
            ));
        }
        self.cells.remove(i);
        Ok(self.manifest.editions.remove(i).file)
    }

    // ---- rows ------------------------------------------------------------------

    /// Write one row of one edition. A repeated row writes every row sharing its text.
    pub fn write_row(&mut self, edition: Id, row: usize, text: &str) -> Result<()> {
        let e = self.edition(edition)?;
        self.check_row(row, self.cells[e].len())?;
        let cell = Cell::from_text(text);
        for r in self.repeat_group(row) {
            if let Some(slot) = self.cells[e].get_mut(r - 1) {
                *slot = cell.clone();
            }
        }
        Ok(())
    }

    /// An empty row at `row` in every edition; `row` may be one past the end, to append.
    pub fn insert_row(&mut self, row: usize) -> Result<()> {
        let longest = self.cells.iter().map(Vec::len).max().unwrap_or(0);
        if row == 0 || row > longest + 1 {
            return Err(out_of_range(row));
        }
        for cells in &mut self.cells {
            let at = (row - 1).min(cells.len());
            cells.insert(at, Cell::empty());
        }
        self.shift_from(row, 1);
        Ok(())
    }

    /// Remove `row` from every edition, while editing `edition`.
    ///
    /// Refused while another edition still has text there — that text is somebody's work — or
    /// while the row carries a note.
    pub fn remove_row(&mut self, edition: Id, row: usize) -> Result<()> {
        let e = self.edition(edition)?;
        let longest = self.cells.iter().map(Vec::len).max().unwrap_or(0);
        if row == 0 || row > longest {
            return Err(out_of_range(row));
        }
        for (i, cells) in self.cells.iter().enumerate() {
            if i != e && cells.get(row - 1).is_some_and(|c| !c.is_empty()) {
                return Err(Error::invalid(
                    "row",
                    format!(
                        "row {row} still has text in {}; clear it there first, or keep the row",
                        describe(&self.manifest.editions[i])
                    ),
                ));
            }
        }
        if self.manifest.notes.iter().any(|n| n.row == row) {
            return Err(Error::invalid(
                "row",
                format!("row {row} has a note; remove the note first, or keep the row"),
            ));
        }
        for cells in &mut self.cells {
            if row <= cells.len() {
                cells.remove(row - 1);
            }
        }
        self.manifest
            .repeats
            .retain(|r| r.row != row && r.of != row);
        self.shift_from(row + 1, -1);
        Ok(())
    }

    /// A deliberately empty row at `row` in one edition, moving its later rows down.
    pub fn mark_empty(&mut self, edition: Id, row: usize) -> Result<()> {
        let e = self.edition(edition)?;
        self.check_row(row, self.cells[e].len() + 1)?;
        self.cells[e].insert(row - 1, Cell::empty());
        if e == self.original() {
            self.shift_from(row, 1);
        }
        Ok(())
    }

    /// Join `row` of one edition onto the row above, moving its later rows up.
    pub fn join(&mut self, edition: Id, row: usize) -> Result<()> {
        let e = self.edition(edition)?;
        self.check_row(row, self.cells[e].len())?;
        if row < 2 {
            return Err(Error::invalid(
                "row",
                "the first row has no row above to join",
            ));
        }
        let joined = self.cells[e].remove(row - 1);
        self.cells[e][row - 2].lines.extend(joined.lines);
        if e == self.original() {
            self.manifest
                .repeats
                .retain(|r| ![row, row - 1].contains(&r.row) && ![row, row - 1].contains(&r.of));
            // The joined row's note goes with its words, onto the row above.
            if let Some(i) = self.manifest.notes.iter().position(|n| n.row == row) {
                let moved = self.manifest.notes.remove(i);
                match self.manifest.notes.iter_mut().find(|n| n.row == row - 1) {
                    Some(above) => above.text = format!("{}\n\n{}", above.text, moved.text),
                    None => self.manifest.notes.push(Note {
                        row: row - 1,
                        text: moved.text,
                    }),
                }
            }
            self.shift_from(row + 1, -1);
        }
        Ok(())
    }

    /// Split `row` of one edition before its line `line` (counting from 1), moving its later
    /// rows down.
    pub fn split(&mut self, edition: Id, row: usize, line: usize) -> Result<()> {
        let e = self.edition(edition)?;
        self.check_row(row, self.cells[e].len())?;
        let cell = &mut self.cells[e][row - 1];
        if line < 2 || line > cell.lines.len() {
            return Err(Error::invalid(
                "line",
                format!(
                    "row {row} can be split before lines 2 to {}",
                    cell.lines.len()
                ),
            ));
        }
        let rest = cell.lines.split_off(line - 1);
        self.cells[e].insert(row, Cell { lines: rest });
        if e == self.original() {
            self.manifest
                .repeats
                .retain(|r| r.row != row && r.of != row);
            self.shift_from(row + 1, 1);
        }
        Ok(())
    }

    // ---- repeats and notes -----------------------------------------------------

    /// Make `row` the same text as the earlier row `of`, or with `None` stop it being one.
    /// Returns how many rows now share the text.
    pub fn set_repeat(&mut self, row: usize, of: Option<usize>) -> Result<usize> {
        let rows = self.rows();
        if row == 0 || row > rows {
            return Err(out_of_range(row));
        }
        self.manifest.repeats.retain(|r| r.row != row);
        let Some(of) = of else {
            return Ok(1);
        };
        // A repeat of a repeat is a repeat of the first occurrence: never a chain.
        let root = self
            .manifest
            .repeats
            .iter()
            .find(|r| r.row == of)
            .map_or(of, |r| r.of);
        if of == 0 || root >= row {
            return Err(Error::invalid("of", "a row repeats an earlier row"));
        }
        if self.manifest.repeats.iter().any(|r| r.of == row) {
            return Err(Error::invalid(
                "row",
                format!("later rows repeat row {row}; it can only be a first occurrence"),
            ));
        }
        // Agree the text first, refusing rather than choosing between two different texts.
        for (e, cells) in self.cells.iter().enumerate() {
            let (Some(a), Some(b)) = (cells.get(root - 1), cells.get(row - 1)) else {
                continue;
            };
            if !a.is_empty() && !b.is_empty() && a != b {
                return Err(Error::invalid(
                    "row",
                    format!(
                        "row {row} and row {root} differ in {}; make them the same, or empty one of them, first",
                        describe(&self.manifest.editions[e])
                    ),
                ));
            }
        }
        self.manifest.repeats.push(Repeat { row, of: root });
        self.manifest.repeats.sort_by_key(|r| r.row);
        let group = self.repeat_group(row);
        for cells in &mut self.cells {
            let text = group
                .iter()
                .filter_map(|r| cells.get(r - 1))
                .find(|c| !c.is_empty())
                .cloned();
            if let Some(text) = text {
                for r in &group {
                    if let Some(slot) = cells.get_mut(r - 1) {
                        *slot = text.clone();
                    }
                }
            }
        }
        Ok(group.len())
    }

    /// The rows sharing `row`'s text: itself, its first occurrence and every repeat of that.
    pub fn repeat_group(&self, row: usize) -> Vec<usize> {
        let root = self
            .manifest
            .repeats
            .iter()
            .find(|r| r.row == row)
            .map_or(row, |r| r.of);
        let mut group = vec![root];
        group.extend(
            self.manifest
                .repeats
                .iter()
                .filter(|r| r.of == root)
                .map(|r| r.row),
        );
        group
    }

    pub fn set_note(&mut self, row: usize, text: &str) -> Result<()> {
        if row == 0 || row > self.rows() {
            return Err(out_of_range(row));
        }
        self.manifest.notes.retain(|n| n.row != row);
        let text = text.trim();
        if !text.is_empty() {
            self.manifest.notes.push(Note {
                row,
                text: text.to_string(),
            });
            self.manifest.notes.sort_by_key(|n| n.row);
        }
        Ok(())
    }

    // ---- re-dividing -----------------------------------------------------------

    /// What dividing every edition again by `unit` would do, without doing it.
    pub fn resegment(&self, unit: Unit) -> Resegmented {
        let mut work = self.clone();
        let mut editions = Vec::new();
        for (i, e) in self.manifest.editions.iter().enumerate() {
            let before = &self.cells[i];
            let raw = raw_text(before, self.manifest.unit);
            let after = text::divide(&raw, unit, &e.language);
            editions.push(ResegmentEdition {
                edition: e.id,
                rows_before: before.len(),
                rows_after: after.len(),
                empty_lost: before.iter().filter(|c| c.is_empty()).count(),
                joined_before: before.iter().filter(|c| c.is_joined()).count(),
                joined_after: after.iter().filter(|c| c.is_joined()).count(),
            });
            work.cells[i] = after;
        }
        work.manifest.unit = unit;

        // A row still exists when the original has the same words at the same place.
        let o = self.original();
        let same = |row: usize| {
            row >= 1
                && self.cells[o].get(row - 1).is_some()
                && self.cells[o].get(row - 1) == work.cells[o].get(row - 1)
        };
        let (notes_kept, notes_moved): (Vec<Note>, Vec<Note>) = self
            .manifest
            .notes
            .iter()
            .cloned()
            .partition(|n| same(n.row));
        let (repeats_kept, repeats_dropped): (Vec<Repeat>, Vec<Repeat>) = self
            .manifest
            .repeats
            .iter()
            .copied()
            .partition(|r| same(r.row) && same(r.of));
        work.manifest.notes = notes_kept;
        work.manifest.repeats = repeats_kept;
        Resegmented {
            work,
            editions,
            notes_moved,
            repeats_dropped,
        }
    }

    // ---- internals -------------------------------------------------------------

    fn check_row(&self, row: usize, last: usize) -> Result<()> {
        if row == 0 || row > last {
            return Err(out_of_range(row));
        }
        Ok(())
    }

    /// Renumber notes and repeats at or after `from` by `delta`.
    fn shift_from(&mut self, from: usize, delta: isize) {
        let shift = |n: usize| {
            if n >= from {
                n.saturating_add_signed(delta)
            } else {
                n
            }
        };
        for note in &mut self.manifest.notes {
            note.row = shift(note.row);
        }
        for r in &mut self.manifest.repeats {
            r.row = shift(r.row);
            r.of = shift(r.of);
        }
        self.manifest.notes.sort_by_key(|n| n.row);
        self.manifest.repeats.sort_by_key(|r| r.row);
    }
}

/// What [`Work::resegment`] found: the work as it would be, and what is lost on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Resegmented {
    pub work: Work,
    pub editions: Vec<ResegmentEdition>,
    /// Notes on rows that no longer exist; kept in `.conflicts/` when applied.
    pub notes_moved: Vec<Note>,
    pub repeats_dropped: Vec<Repeat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResegmentEdition {
    pub edition: Id,
    pub rows_before: usize,
    pub rows_after: usize,
    /// Deliberately empty rows, which dividing again cannot keep.
    pub empty_lost: usize,
    pub joined_before: usize,
    pub joined_after: usize,
}

/// An edition to add.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEdition {
    pub kind: EditionKind,
    pub language: String,
    pub label: Option<String>,
    pub scheme: Option<String>,
    pub source: Option<Id>,
}

/// An edition's rows as text that dividing again can start from.
fn raw_text(cells: &[Cell], unit: Unit) -> String {
    let separator = if unit == Unit::Paragraph {
        "\n\n"
    } else {
        "\n"
    };
    cells
        .iter()
        .filter(|c| !c.is_empty())
        .map(Cell::text)
        .collect::<Vec<_>>()
        .join(separator)
}

/// An edition as a person would name it: its label, or its kind and language.
pub fn describe(e: &Edition) -> String {
    if let Some(label) = &e.label {
        return label.clone();
    }
    let kind = match e.kind {
        EditionKind::Original => "the original",
        EditionKind::Pronunciation => "the pronunciation",
        EditionKind::Translation => "the translation",
    };
    format!("{kind} ({})", e.language)
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn out_of_range(row: usize) -> Error {
    Error::invalid("row", format!("there is no row {row}"))
}

/// BCP 47, loosely: letters, digits and hyphens, starting with a letter. Enough to make a file
/// name of and to recognise a primary subtag; not a registry check.
fn check_language(language: &str) -> Result<()> {
    let ok = !language.is_empty()
        && language.len() <= 35
        && language.starts_with(|c: char| c.is_ascii_alphabetic())
        && language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(Error::invalid(
            "language",
            "a language is a code such as fi, en or ru",
        ))
    }
}

// ---- on disk -----------------------------------------------------------------

/// A work's folder for a new title, inside `works/`.
pub fn new_folder(root: &Path, title: &str) -> Result<PathBuf> {
    let dir = root.join(WORKS_DIR);
    let mut taken = std::collections::HashSet::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir).ctx(format!("listing {}", dir.display()))? {
            let entry = entry.ctx(format!("listing {}", dir.display()))?;
            taken.insert(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(PathBuf::from(WORKS_DIR).join(slug::unique(title, &taken)))
}

/// Read the work in `dir`, relative to the library root.
pub fn read(root: &Path, dir: &Path) -> Result<(Work, WorkRevision)> {
    let manifest_path = root.join(dir).join(MANIFEST);
    let raw =
        fs::read_to_string(&manifest_path).ctx(format!("reading {}", manifest_path.display()))?;
    let mut manifest: Manifest = toml::from_str(&raw)
        .map_err(|e| Error::library(format!("{}/{MANIFEST}: {e}", dir.display())))?;
    // A hand edit can name a repeat no row could be: of nothing, or of a later row. Such a
    // repeat cannot be honoured, and is read as not there rather than trusted.
    manifest.repeats.retain(|r| r.of >= 1 && r.of < r.row);
    let originals = manifest
        .editions
        .iter()
        .filter(|e| e.kind == EditionKind::Original)
        .count();
    if originals != 1 {
        return Err(Error::library(format!(
            "{}/{MANIFEST}: a work has exactly one original, and this one has {originals}",
            dir.display()
        )));
    }
    let mut cells = Vec::new();
    let mut missing = Vec::new();
    for e in &manifest.editions {
        let path = edition_path(root, dir, &e.file)?;
        match fs::read_to_string(&path) {
            Ok(text) => cells.push(text::parse(&text)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(e.id);
                cells.push(Vec::new());
            }
            Err(err) => return Err(Error::io(format!("reading {}", path.display()), err)),
        }
    }
    let revision = revision(root, dir)?;
    Ok((
        Work {
            manifest,
            cells,
            missing,
        },
        revision,
    ))
}

/// Write a work: every edition, then `work.toml`.
pub fn write(root: &Path, dir: &Path, work: &Work) -> Result<WorkRevision> {
    let folder = root.join(dir);
    fs::create_dir_all(&folder).ctx(format!("creating {}", folder.display()))?;
    for (e, cells) in work.manifest.editions.iter().zip(&work.cells) {
        atomic::write(&edition_path(root, dir, &e.file)?, &text::render(cells))?;
    }
    let manifest = toml::to_string_pretty(&work.manifest)
        .map_err(|e| Error::library(format!("cannot write {MANIFEST}: {e}")))?;
    atomic::write(&folder.join(MANIFEST), &manifest)?;
    revision(root, dir)
}

/// Refuse with `Conflict` if the work's folder changed since `expected`.
pub fn check(root: &Path, dir: &Path, expected: &WorkRevision) -> Result<()> {
    if revision(root, dir)? == *expected {
        Ok(())
    } else {
        Err(Error::Conflict {
            path: dir.to_path_buf(),
        })
    }
}

/// The token for a work's folder: every file in it, by name, size and modification time.
pub fn revision(root: &Path, dir: &Path) -> Result<WorkRevision> {
    let folder = root.join(dir);
    let mut entries: Vec<(String, u64, u128)> = Vec::new();
    for entry in fs::read_dir(&folder).ctx(format!("listing {}", folder.display()))? {
        let entry = entry.ctx(format!("listing {}", folder.display()))?;
        let meta = entry
            .metadata()
            .ctx(format!("inspecting {}", entry.path().display()))?;
        if !meta.is_file() {
            continue;
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        entries.push((
            entry.file_name().to_string_lossy().to_string(),
            meta.len(),
            modified,
        ));
    }
    entries.sort();
    let mut hasher = blake3::Hasher::new();
    for (name, len, modified) in entries {
        hasher.update(format!("{name}\0{len}\0{modified}\n").as_bytes());
    }
    Ok(WorkRevision(hasher.finalize().to_hex()[..32].to_string()))
}

/// An edition's file, treated as a name: a hand-edited `../../elsewhere` must not reach outside
/// the work's folder.
fn edition_path(root: &Path, dir: &Path, file: &str) -> Result<PathBuf> {
    let mut parts = Path::new(file).components();
    match (parts.next(), parts.next()) {
        (Some(std::path::Component::Normal(one)), None) => Ok(root.join(dir).join(one)),
        _ => Err(Error::library(format!(
            "{}/{MANIFEST}: {file:?} is not a file name",
            dir.display()
        ))),
    }
}

/// Keep notes that re-dividing would lose, under `.conflicts/`. Returns where, relative to the
/// library root.
pub fn keep_notes(root: &Path, dir: &Path, notes: &[Note]) -> Result<Option<PathBuf>> {
    if notes.is_empty() {
        return Ok(None);
    }
    #[derive(Serialize)]
    struct Kept<'a> {
        notes: &'a [Note],
    }
    let stamp = Timestamp::now().to_string().replace(':', "-");
    let mut base = PathBuf::from(crate::library::CONFLICTS_DIR).join(&stamp);
    let mut n = 2;
    while root.join(&base).exists() {
        base = PathBuf::from(crate::library::CONFLICTS_DIR).join(format!("{stamp}-{n}"));
        n += 1;
    }
    let relative = base.join(dir).join("notes.toml");
    let path = root.join(&relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ctx(format!("creating {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(&Kept { notes })
        .map_err(|e| Error::library(format!("cannot write kept notes: {e}")))?;
    atomic::write(&path, &text)?;
    Ok(Some(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(original: &str) -> Work {
        Work::new(
            "Я вас любил".into(),
            Unit::Line,
            original,
            "ru".into(),
            None,
        )
        .unwrap()
    }

    fn fi(w: &mut Work, text: &str) -> Id {
        w.add_edition(
            NewEdition {
                kind: EditionKind::Translation,
                language: "fi".into(),
                label: None,
                scheme: None,
                source: None,
            },
            text,
        )
        .unwrap()
    }

    fn texts(w: &Work, e: usize) -> Vec<String> {
        w.cells[e].iter().map(|c| c.text()).collect()
    }

    #[test]
    fn an_empty_edition_starts_with_a_row_for_every_row() {
        let mut w = work("a\nb\nc");
        let f = fi(&mut w, "");
        assert_eq!(w.cells[1].len(), 3);
        assert!(w.misalignments().is_empty());
        w.write_row(f, 2, "bee").unwrap();
        assert_eq!(texts(&w, 1), ["", "bee", ""]);
    }

    #[test]
    fn inserting_a_row_inserts_it_everywhere_and_moves_notes() {
        let mut w = work("a\nb");
        fi(&mut w, "A\nB");
        w.set_note(2, "on b").unwrap();
        w.insert_row(2).unwrap();
        assert_eq!(texts(&w, 0), ["a", "", "b"]);
        assert_eq!(texts(&w, 1), ["A", "", "B"]);
        assert_eq!(
            w.manifest.notes,
            [Note {
                row: 3,
                text: "on b".into()
            }]
        );
    }

    #[test]
    fn removing_a_row_is_refused_while_another_edition_has_text_there() {
        let mut w = work("a\nb");
        let f = fi(&mut w, "A\nB");
        let o = w.manifest.editions[0].id;
        assert!(matches!(w.remove_row(o, 2), Err(Error::Invalid { .. })));
        w.write_row(f, 2, "").unwrap();
        w.remove_row(o, 2).unwrap();
        assert_eq!(texts(&w, 0), ["a"]);
        assert_eq!(texts(&w, 1), ["A"]);
    }

    #[test]
    fn join_split_and_mark_empty_act_on_one_edition() {
        let mut w = work("a\nb\nc");
        let f = fi(&mut w, "A\nB1\nB2\nC");
        assert_eq!(w.misalignments().len(), 1);
        w.join(f, 3).unwrap();
        assert_eq!(texts(&w, 1), ["A", "B1\nB2", "C"]);
        assert!(w.misalignments().is_empty());
        w.split(f, 2, 2).unwrap();
        assert_eq!(texts(&w, 1), ["A", "B1", "B2", "C"]);
        w.join(f, 3).unwrap();
        w.mark_empty(f, 2).unwrap();
        assert_eq!(texts(&w, 1), ["A", "", "B1\nB2", "C"]);
        assert_eq!(texts(&w, 0), ["a", "b", "c"]);
    }

    #[test]
    fn joining_the_original_takes_its_note_up_with_it() {
        let mut w = work("a\nb\nc");
        let o = w.manifest.editions[0].id;
        w.set_note(2, "on b").unwrap();
        w.set_note(3, "on c").unwrap();
        w.join(o, 2).unwrap();
        assert_eq!(
            w.manifest.notes,
            [
                Note {
                    row: 1,
                    text: "on b".into()
                },
                Note {
                    row: 2,
                    text: "on c".into()
                }
            ]
        );
    }

    #[test]
    fn a_repeat_shares_its_text_and_writing_one_writes_all() {
        let mut w = work("chorus\nverse\nchorus\nverse 2\nchorus");
        let f = fi(&mut w, "");
        w.write_row(f, 1, "kertosäe").unwrap();
        assert_eq!(w.set_repeat(3, Some(1)).unwrap(), 2);
        assert_eq!(
            w.set_repeat(5, Some(3)).unwrap(),
            3,
            "a repeat of a repeat is of the first"
        );
        assert_eq!(texts(&w, 1), ["kertosäe", "", "kertosäe", "", "kertosäe"]);
        w.write_row(f, 5, "kertosäe!").unwrap();
        assert_eq!(texts(&w, 1)[0], "kertosäe!");
        assert_eq!(texts(&w, 1)[2], "kertosäe!");
        assert!(w.repeat_problems().is_empty());
    }

    #[test]
    fn a_repeat_of_two_different_texts_is_refused() {
        let mut w = work("one\ntwo");
        assert!(matches!(
            w.set_repeat(2, Some(1)),
            Err(Error::Invalid { .. })
        ));
    }

    #[test]
    fn a_hand_edit_that_breaks_a_repeat_is_a_problem() {
        let mut w = work("x\nx");
        w.set_repeat(2, Some(1)).unwrap();
        w.cells[0][1] = Cell::from_text("y");
        assert_eq!(w.repeat_problems().len(), 1);
    }

    #[test]
    fn re_dividing_keeps_notes_where_the_words_did_not_move() {
        let mut w = Work::new(
            "Prose".into(),
            Unit::Line,
            "First sentence. Second one.\nThird.",
            "en".into(),
            None,
        )
        .unwrap();
        w.set_note(2, "on third").unwrap();
        let plan = w.resegment(Unit::Sentence);
        assert_eq!(plan.editions[0].rows_after, 3);
        assert_eq!(plan.notes_moved.len(), 1, "row 2 now holds other words");
        assert_eq!(plan.work.manifest.unit, Unit::Sentence);
    }

    #[test]
    fn editions_move_among_the_panes_with_their_rows() {
        let mut w = work("a\nb");
        let f = fi(&mut w, "A\nB");
        w.move_edition(f, 1).unwrap();
        assert_eq!(w.manifest.editions[0].id, f);
        assert_eq!(texts(&w, 0), ["A", "B"]);
        assert_eq!(texts(&w, 1), ["a", "b"]);
        assert_eq!(
            w.rows(),
            2,
            "rows are still counted in the original, wherever it is"
        );
        assert!(w.move_edition(f, 3).is_err());
    }

    #[test]
    fn the_original_cannot_be_removed() {
        let mut w = work("a");
        let o = w.manifest.editions[0].id;
        assert!(w.remove_edition(o).is_err());
    }

    #[test]
    fn a_second_translation_into_one_language_gets_a_file_of_its_own() {
        let mut w = work("a");
        fi(&mut w, "");
        fi(&mut w, "");
        let files: Vec<_> = w
            .manifest
            .editions
            .iter()
            .map(|e| e.file.as_str())
            .collect();
        assert_eq!(
            files,
            ["original.txt", "translation.fi.txt", "translation.fi-2.txt"]
        );
    }

    #[test]
    fn an_impossible_repeat_written_by_hand_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let w = work("a\nb");
        let folder = Path::new("works/x");
        write(dir.path(), folder, &w).unwrap();
        let manifest = dir.path().join(folder).join(MANIFEST);
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            format!("{text}\n[[repeats]]\nrow = 1\nof = 0\n\n[[repeats]]\nrow = 1\nof = 2\n"),
        )
        .unwrap();
        let (read, _) = read(dir.path(), folder).unwrap();
        assert!(read.manifest.repeats.is_empty());
    }

    #[test]
    fn languages_are_codes() {
        assert!(check_language("fi").is_ok());
        assert!(check_language("zh-Hant").is_ok());
        assert!(check_language("../x").is_err());
        assert!(check_language("").is_err());
    }
}
