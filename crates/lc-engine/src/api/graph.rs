//! `library.graph`: the whole library as nodes and lines, for the graph screen (ADR 0010).
//!
//! Everything here is computed from the index on every call and nothing is written. The
//! engine says what is connected to what; where anything goes on screen is the interface's.

use std::collections::HashSet;

use super::*;

/// The library as a graph: every page, and every source if asked for, and the lines between
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryGraph {
    /// Pages first, in the order of the page tree (each page before the pages under it), then
    /// sources, by title.
    pub nodes: Vec<GraphNode>,
    pub lines: Vec<GraphLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: Id,
    pub kind: NodeKind,
    pub title: String,
    /// The page at the top of the tree that this page is under, or the page itself when it is
    /// at the top. What the interface colours by, so a subject reads as one colour. `None`
    /// for a source, which is under nothing.
    pub top: Option<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Page,
    Source,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphLine {
    pub from: Id,
    pub to: Id,
    pub kind: LineKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// `[[to]]` written in the page `from`.
    LinksTo,
    /// The page `from` is directly under the page `to`.
    Under,
    /// The page `from` cites the source `to`.
    Cites,
}

impl Engine {
    /// Every page, and with `sources` every source, with the lines between them.
    ///
    /// A link or citation whose other end is not on the graph — a link to an id that names no
    /// page, a source left out — has no line, so every line joins two nodes that are there.
    /// A page linking to itself has no line either: it would be drawn as nothing.
    pub fn library_graph(&self, sources: bool) -> Result<LibraryGraph> {
        let mut nodes = Vec::new();
        let mut lines = Vec::new();
        for top in self.page_tree()? {
            let id = top.id;
            add_pages(&top, None, id, &mut nodes, &mut lines);
        }
        let pages: HashSet<Id> = nodes.iter().map(|n| n.id).collect();

        for (from, to) in self.index.links()? {
            if from != to && pages.contains(&from) && pages.contains(&to) {
                lines.push(GraphLine {
                    from,
                    to,
                    kind: LineKind::LinksTo,
                });
            }
        }

        if sources {
            let mut found = self.index.entries(Kind::Source)?;
            found.sort_by_key(|s| s.title.to_lowercase());
            let listed: HashSet<Id> = found.iter().map(|s| s.id).collect();
            nodes.extend(found.into_iter().map(|s| GraphNode {
                id: s.id,
                kind: NodeKind::Source,
                title: s.title,
                top: None,
            }));
            // Works cite sources too; they are not on the graph yet, so their citations are
            // not either.
            for (citing, source) in self.index.citations()? {
                if pages.contains(&citing) && listed.contains(&source) {
                    lines.push(GraphLine {
                        from: citing,
                        to: source,
                        kind: LineKind::Cites,
                    });
                }
            }
        }

        Ok(LibraryGraph { nodes, lines })
    }
}

fn add_pages(
    page: &PageNode,
    parent: Option<Id>,
    top: Id,
    nodes: &mut Vec<GraphNode>,
    lines: &mut Vec<GraphLine>,
) {
    nodes.push(GraphNode {
        id: page.id,
        kind: NodeKind::Page,
        title: page.title.clone(),
        top: Some(top),
    });
    if let Some(parent) = parent {
        lines.push(GraphLine {
            from: page.id,
            to: parent,
            kind: LineKind::Under,
        });
    }
    for child in &page.children {
        add_pages(child, Some(page.id), top, nodes, lines);
    }
}
