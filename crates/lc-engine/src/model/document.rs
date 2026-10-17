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

    /// Images this document shows, in order, as written in the body.
    ///
    /// Each is a plain filename beside the document — `![Diagram](muscle-repair.png)` — never a
    /// path. That is what keeps the `.md` readable outside this application: a Markdown viewer,
    /// a text editor or a forge renders the picture because it is exactly where the file says
    /// it is. Resolving a name to a path is the store's job, not the reader's.
    pub fn images(&self) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = self.body.as_str();

        while let Some(start) = rest.find("](") {
            // `![` before the `](`, with the alt text between — anything else is an ordinary
            // link, which is not ours to collect.
            let before = &rest[..start];
            rest = &rest[start + 2..];
            let Some(end) = rest.find(')') else { break };
            let name = &rest[..end];
            rest = &rest[end + 1..];

            let is_image = before
                .rfind('[')
                .is_some_and(|open| before[..open].ends_with('!'));
            if is_image && !name.is_empty() && !found.contains(&name.to_string()) {
                found.push(name.to_string());
            }
        }

        found
    }

    /// The body as a reader sees it, with its markers taken out.
    ///
    /// What the [index](crate::index) holds, and therefore what a search matches and what an
    /// excerpt shows. The file on disk keeps `[^cite:01J8…]` and `[[01J8…]]`; the index has no
    /// reason to. Those markers are addresses, not words — indexing them means an excerpt of
    /// somebody's writing is mostly identifiers, and it means searching `cite` finds every
    /// document that cites anything, because `[^cite:` tokenises to the word `cite`.
    ///
    /// Each marker becomes a single space rather than nothing, so `tissue.[^cite:01J8…] The`
    /// does not come back as `tissue.The`.
    pub fn searchable_text(&self) -> String {
        strip_between(&strip_between(&self.body, "[^cite:", "]"), "[[", "]]")
    }
}

/// Pull well-formed identifiers out of `open … close` pairs, ignoring anything malformed.
///
/// Deliberately forgiving: a user typing `[[` mid-sentence is writing prose, not a broken
/// link, and must not produce an error.
/// Everything from each `open` to its next `close`, replaced by one space.
///
/// An unterminated marker is left alone: a stray `[[` somebody typed is text they wrote, and
/// swallowing the rest of the document because of it would lose far more than it tidied.
fn strip_between(body: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;

    while let Some(start) = rest.find(open) {
        let after_open = &rest[start + open.len()..];
        let Some(end) = after_open.find(close) else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push(' ');
        rest = &after_open[end + close.len()..];
    }

    out.push_str(rest);
    out
}

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
    fn body(text: &str) -> Document {
        let mut doc = Document::new("Probe");
        doc.body = text.to_string();
        doc
    }

    #[test]
    fn finds_images_and_leaves_ordinary_links_alone() {
        let doc = body(
            "Text ![Diagram](muscle-repair.png) more.\n\n             A [link](https://example.com) is not an image.\n\n             ![](no-alt.jpg) and ![Again](muscle-repair.png)",
        );
        // Ordinary links stay out, an empty alt is still an image, and a repeat is one file.
        assert_eq!(doc.images(), vec!["muscle-repair.png", "no-alt.jpg"]);
    }

    #[test]
    fn an_unterminated_image_is_not_collected() {
        assert!(body("![Diagram](muscle-repair.png").images().is_empty());
        assert!(body("![Diagram]").images().is_empty());
    }

    #[test]
    fn markers_leave_the_words_around_them_intact() {
        // Spacing is not normalised: a marker that already had a space after it leaves two.
        // That is deliberate — FTS5 ignores runs of whitespace when tokenising, and HTML
        // collapses them when an excerpt is drawn, so tidying it would be work with no reader
        // to notice. The words are what matter, and they are untouched.
        assert_eq!(
            body("Protein repairs tissue.[^cite:01J8ZQ8D4Q6S8W0W2Y4A6C8E0G@44] The rest.")
                .searchable_text(),
            "Protein repairs tissue.  The rest."
        );
    }

    #[test]
    fn a_marker_becomes_a_space_and_not_nothing() {
        // Otherwise the words on either side glue into one that nobody will ever search for.
        let text = body("tissue.[^cite:01J8ZQ8D4Q6S8W0W2Y4A6C8E0G]The").searchable_text();
        assert!(!text.contains("tissue.The"), "words were glued: {text:?}");
    }

    #[test]
    fn links_go_too() {
        assert_eq!(
            body("See [[01J8ZQ8D4Q6S8W0W2Y4A6C8E0G]] for the kinds.").searchable_text(),
            "See   for the kinds."
        );
    }

    #[test]
    fn an_unterminated_marker_is_left_as_written() {
        // Somebody typing `[[` is writing, not addressing. Swallowing the rest of the document
        // over it would lose far more than it tidied.
        for text in ["a [[ b", "a [^cite: b", "[[", "[^cite:"] {
            assert_eq!(body(text).searchable_text(), text, "changed {text:?}");
        }
    }

    #[test]
    fn a_body_without_markers_is_returned_unchanged() {
        let text = "Protein is built from amino acids. Nothing to strip here.";
        assert_eq!(body(text).searchable_text(), text);
    }

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
