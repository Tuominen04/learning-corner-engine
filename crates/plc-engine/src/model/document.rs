//! Documents: a page you write, inside a topic.
//!
//! Markdown with a `+++` front matter block. The two extensions to plain Markdown —
//! `[^cite:<id>]` for a citation and `[[<id>]]` for a link — are stored as written and
//! rendered at display time, so a source corrected once is correct everywhere and the file
//! stays readable in any text editor.

use serde::{Deserialize, Serialize};

use crate::id::Id;
use crate::timestamp::Timestamp;

/// Front matter of a document file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub id: Id,
    pub title: String,
    pub created: Timestamp,
    pub modified: Timestamp,
}

/// A document and its body.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub meta: Meta,
    pub body: String,
}

impl Document {
    pub fn new(title: impl Into<String>) -> Self {
        let now = Timestamp::now();
        Document {
            meta: Meta {
                id: Id::new(),
                title: title.into(),
                created: now,
                modified: now,
            },
            body: String::new(),
        }
    }

    pub fn id(&self) -> Id {
        self.meta.id
    }

    pub fn title(&self) -> &str {
        &self.meta.title
    }

    /// Identifiers this document links to, in `[[id]]` form.
    ///
    /// Derived on demand and never stored: no file records what points at it, so a backlink
    /// cannot go stale.
    pub fn links(&self) -> Vec<Id> {
        extract(&self.body, "[[", "]]")
    }

    /// Sources this document cites, each once.
    ///
    /// Answers "what does this document draw on", so a source cited eleven times appears once.
    /// For the eleven places themselves, see [`Self::citation_refs`].
    pub fn citations(&self) -> Vec<Id> {
        let mut found: Vec<Id> = Vec::new();
        for reference in self.citation_refs() {
            if !found.contains(&reference.source) {
                found.push(reference.source);
            }
        }
        found
    }

    /// Every citation in the body, in order, keeping repeats.
    ///
    /// Each one is a place in the text that has to be rendered, and two citations of the same
    /// source can differ — one narrative, one with a page number.
    pub fn citation_refs(&self) -> Vec<crate::citation::CitationRef> {
        crate::citation::reference::parse_all(&self.body)
    }
}

/// Pull well-formed identifiers out of `open … close` pairs, ignoring anything malformed.
///
/// Deliberately forgiving: a user typing `[[` mid-sentence is writing prose, not a broken
/// link, and must not produce an error.
fn extract(body: &str, open: &str, close: &str) -> Vec<Id> {
    let mut found = Vec::new();
    let mut rest = body;

    while let Some(start) = rest.find(open) {
        rest = &rest[start + open.len()..];
        let Some(end) = rest.find(close) else { break };
        let inner = &rest[..end];
        if let Ok(id) = inner.trim().parse::<Id>() {
            if !found.contains(&id) {
                found.push(id);
            }
        }
        rest = &rest[end + close.len()..];
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_links() {
        let mut doc = Document::new("Protein");
        let a = Id::new();
        let b = Id::new();
        doc.body = format!("See [[{a}]] and also [[{b}]].");
        assert_eq!(doc.links(), vec![a, b]);
    }

    #[test]
    fn finds_citations_with_and_without_locators() {
        let mut doc = Document::new("Protein");
        let a = Id::new();
        let b = Id::new();
        doc.body = format!("Nine acids.[^cite:{a}@44-46] And more.[^cite:{b}]");
        assert_eq!(doc.citations(), vec![a, b]);
    }

    #[test]
    fn deduplicates() {
        let mut doc = Document::new("Protein");
        let a = Id::new();
        doc.body = format!("[[{a}]] and again [[{a}]]");
        assert_eq!(doc.links(), vec![a]);
    }

    #[test]
    fn ignores_prose_that_looks_like_syntax() {
        let mut doc = Document::new("Protein");
        doc.body = "An unclosed [[ and a [[not-an-id]] and a bare ] bracket.".into();
        assert!(doc.links().is_empty());
        assert!(doc.citations().is_empty());
    }

    #[test]
    fn an_empty_body_has_nothing() {
        let doc = Document::new("Protein");
        assert!(doc.links().is_empty());
        assert!(doc.citations().is_empty());
    }
}
