//! The `diagram.*` operations of `backend-contract.md` (ADR 0011).
//!
//! A diagram is a file of its own in `maps/`, written whole with the revision from the last
//! read — the editor holds the diagram and saves it all at once, so undo is the editor's, and a
//! diagram changed in a text editor meanwhile is a `Conflict` rather than being overwritten.
//!
//! A page shows a diagram with `![alt](diagram:<id>)`. Deleting a diagram takes those lines
//! out of the pages that show it; deleting a page leaves every diagram alone — a box that
//! pointed at it stays where it was, saying its page is gone, so a family tree does not fall
//! apart because one page went.

use std::path::Path;

use super::*;
use crate::model::diagram::{self, Diagram, Direction, Layout, Line};
use crate::store::{Found, MAPS_DIR};

/// A diagram in the list of diagrams.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramSummary {
    pub id: Id,
    pub title: String,
    pub layout: Layout,
}

/// A diagram as the interface draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramView {
    pub id: Id,
    pub title: String,
    pub layout: Layout,
    pub direction: Direction,
    pub created: Timestamp,
    pub nodes: Vec<NodeView>,
    pub lines: Vec<Line>,
    /// Hand it back unchanged to `diagram_write`.
    pub revision: Revision,
}

/// One box, with what its page is called now.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeView {
    pub id: Id,
    pub page: Option<PageLink>,
    pub label: Option<String>,
    pub when: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
}

/// The page a box points at, and what it is called now. A `title` of `None` means the page is
/// gone: deleted, inside the application or by hand.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLink {
    pub id: Id,
    pub title: Option<String>,
}

/// The diagrams that have to do with a page.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PageDiagrams {
    /// Diagrams the page shows: `![…](diagram:<id>)` in its text.
    pub shown: Vec<DiagramSummary>,
    /// Diagrams with a box that points at the page.
    pub pointing_here: Vec<DiagramSummary>,
}

impl Engine {
    /// Every diagram, by title.
    pub fn diagram_list(&self) -> Result<Vec<DiagramSummary>> {
        let mut out = self.diagram_summaries(
            self.index
                .entries(Kind::Diagram)?
                .into_iter()
                .map(|e| e.id)
                .collect(),
        )?;
        out.sort_by_key(|d| d.title.to_lowercase());
        Ok(out)
    }

    /// A new, empty diagram.
    pub fn diagram_create(
        &mut self,
        title: impl Into<String>,
        layout: Layout,
    ) -> Result<DiagramView> {
        self.library.ensure_writable()?;
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err(Error::invalid("title", "a diagram needs a title"));
        }
        let diagram = Diagram::new(title, layout);

        let dir = PathBuf::from(MAPS_DIR);
        let taken = self.store.taken_names(&dir)?;
        let path = dir.join(format!("{}.toml", slug::unique(&diagram.title, &taken)));
        let revision = self.store.write_diagram(&path, &diagram)?;
        self.index.rebuild(&self.store)?;
        Ok(self.diagram_view(diagram, revision))
    }

    pub fn diagram_read(&self, id: Id) -> Result<DiagramView> {
        let entry = self.expect(id, Kind::Diagram)?;
        let found = self.store.read_diagram(&entry.path)?;
        Ok(self.diagram_view(found.entity, found.revision))
    }

    /// Save a diagram, whole.
    ///
    /// `diagram.id` must be `id`, and `created` is kept from what is stored. A changed title
    /// renames the file to match, as a page's does. A box may point at a page that is gone —
    /// that is how a deleted page leaves a diagram — but not at something that is not a page.
    pub fn diagram_write(
        &mut self,
        id: Id,
        diagram: Diagram,
        expected: Revision,
    ) -> Result<DiagramView> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Diagram)?;
        if diagram.id != id {
            return Err(Error::invalid("id", "that is a different diagram"));
        }
        let diagram = diagram.tidied();
        diagram.check()?;
        for page in diagram.pages() {
            if let Ok(found) = self.index.resolve(page) {
                if found.kind != Kind::Document {
                    return Err(Error::invalid(
                        "nodes",
                        format!("{page} is not a page, so a box cannot point at it"),
                    ));
                }
            }
        }
        self.store.check_revision(&entry.path, expected)?;

        let mut diagram = diagram;
        diagram.created = self.store.read_diagram(&entry.path)?.entity.created;

        let mut path = entry.path.clone();
        let mut revision = self.store.write_diagram(&path, &diagram)?;
        if diagram.title != entry.title {
            let mut taken = self.store.taken_names(Path::new(MAPS_DIR))?;
            taken.remove(&stem_of(&path));
            let stem = slug::unique(&diagram.title, &taken);
            if stem != stem_of(&path) {
                let to = Path::new(MAPS_DIR).join(format!("{stem}.toml"));
                self.store.move_path(&path, &to)?;
                path = to;
                revision = self.store.read_diagram(&path)?.revision;
            }
        }
        self.index.rebuild(&self.store)?;
        Ok(self.diagram_view(diagram, revision))
    }

    /// Pages that show a diagram, by title.
    pub fn diagram_shown_in(&self, id: Id) -> Result<Vec<DocumentSummary>> {
        self.expect(id, Kind::Diagram)?;
        let mut out = Vec::new();
        for page in self.index.pages_showing(id)? {
            let e = self.index.resolve(page)?;
            out.push(DocumentSummary {
                id: e.id,
                title: e.title,
            });
        }
        out.sort_by_key(|d| d.title.to_lowercase());
        Ok(out)
    }

    /// What deleting a diagram would change: the pages that show it, whose lines for it would
    /// go. Their other writing is not touched. Ask before deleting, never after.
    pub fn diagram_delete_plan(&self, id: Id) -> Result<Vec<DocumentSummary>> {
        self.diagram_shown_in(id)
    }

    /// Delete a diagram, and the lines that show it from the pages that do.
    ///
    /// The pages first and the file last, so an interruption leaves a diagram that some pages
    /// no longer show — whole, and still there — rather than pages showing a diagram that is
    /// gone.
    pub fn diagram_delete(&mut self, id: Id) -> Result<()> {
        self.library.ensure_writable()?;
        let entry = self.expect(id, Kind::Diagram)?;
        for page in self.index.pages_showing(id)? {
            let current = self.document_read(page)?;
            if let Some(body) = diagram::without(&current.body, id) {
                self.document_write(page, body, current.revision)?;
            }
        }
        self.store.remove_diagram(&entry.path)?;
        self.index.rebuild(&self.store)?;
        Ok(())
    }

    /// The diagrams a page shows and the diagrams that point at it.
    pub fn document_diagrams(&self, page: Id) -> Result<PageDiagrams> {
        self.expect(page, Kind::Document)?;
        Ok(PageDiagrams {
            shown: self.diagram_summaries(self.index.diagrams_shown_on(page)?)?,
            pointing_here: self.diagram_summaries(self.index.diagrams_pointing_at(page)?)?,
        })
    }

    /// A diagram that a page shows may not exist: the line was typed, or the diagram was
    /// deleted outside the application. Such an id is simply not listed.
    fn diagram_summaries(&self, ids: Vec<Id>) -> Result<Vec<DiagramSummary>> {
        let mut out = Vec::new();
        for id in ids {
            let Ok(entry) = self.index.resolve(id) else {
                continue;
            };
            if entry.kind != Kind::Diagram {
                continue;
            }
            let found: Found<Diagram> = self.store.read_diagram(&entry.path)?;
            out.push(DiagramSummary {
                id,
                title: found.entity.title,
                layout: found.entity.layout,
            });
        }
        out.sort_by_key(|d| d.title.to_lowercase());
        Ok(out)
    }

    fn diagram_view(&self, diagram: Diagram, revision: Revision) -> DiagramView {
        let nodes = diagram
            .nodes
            .into_iter()
            .map(|n| NodeView {
                page: n.page.map(|id| PageLink {
                    id,
                    title: self
                        .index
                        .resolve(id)
                        .ok()
                        .filter(|e| e.kind == Kind::Document)
                        .map(|e| e.title),
                }),
                id: n.id,
                label: n.label,
                when: n.when,
                x: n.x,
                y: n.y,
            })
            .collect();
        DiagramView {
            id: diagram.id,
            title: diagram.title,
            layout: diagram.layout,
            direction: diagram.direction,
            created: diagram.created,
            nodes,
            lines: diagram.lines,
            revision,
        }
    }
}
