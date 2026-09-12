//! The citation syntax as it appears **inside a document**, and how to read it back.
//!
//! A citation in a file says three things: which source, where in it, and how the sentence uses
//! it. Only the first was expressible before, so a timestamp cited from a lecture was stored
//! indistinguishably from a page number, and `Nieminen (2021) found…` could not be written at
//! all.
//!
//! ```text
//! [^cite:01J8ZQ…]                  (Nieminen, 2021)
//! [^cite:01J8ZQ…@44]               (Nieminen, 2021, p. 44)
//! [^cite:01J8ZQ…@44-46]            (Nieminen, 2021, pp. 44–46)
//! [^cite:01J8ZQ…@para:7]           (Nieminen, 2021, para. 7)
//! [^cite:01J8ZQ…@chapter:3]        (Nieminen, 2021, Chapter 3)
//! [^cite:01J8ZQ…@time:1:04:22]     (Nieminen, 2021, 1:04:22)
//! [^cite:01J8ZQ…!narrative]        Nieminen (2021)
//! [^cite:01J8ZQ…@44!narrative]     Nieminen (2021, p. 44)
//! ```
//!
//! Three rules hold the design together.
//!
//! **A page is written bare.** It is what almost every citation carries, and it is what the
//! format already meant by `@44-46`, so every file written before this change reads the same
//! afterwards. The other kinds name themselves.
//!
//! **Parenthetical is never written.** It is the default, and spelling it out would put five
//! redundant characters into the overwhelming majority of citations.
//!
//! **Exactly one spelling per value.** `@page:44` is *not* accepted, because accepting it would
//! give pages two spellings and a file could not round-trip byte-identically through the editor.
//! That is not pedantry: a document opened and saved untouched must produce the same bytes, and
//! two spellings for one value is the cheapest way to break that promise.
//!
//! Anything that does not match stays literal text — a hand-typed `[^cite:whatever]` is prose
//! the user can see, never a citation quietly pointing nowhere.

use crate::citation::format::{Form, Locator};
use crate::id::Id;

/// One citation as written in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationRef {
    pub source: Id,
    pub locator: Option<Locator>,
    pub form: Form,
}

impl CitationRef {
    pub fn new(source: Id) -> Self {
        CitationRef {
            source,
            locator: None,
            form: Form::Parenthetical,
        }
    }

    /// Back to the text that produced it.
    ///
    /// `parse` and `render` are inverses, and the tests below hold them to it. Every citation
    /// the editor writes goes through here, so a drift between the two is silent corruption of
    /// somebody's file.
    pub fn render(&self) -> String {
        let mut out = format!("[^cite:{}", self.source);
        if let Some(locator) = &self.locator {
            out.push('@');
            out.push_str(&render_locator(locator));
        }
        if self.form == Form::Narrative {
            out.push_str("!narrative");
        }
        out.push(']');
        out
    }
}

fn render_locator(locator: &Locator) -> String {
    match locator {
        // Bare: the common case, and what the format already meant before kinds existed.
        Locator::Page(v) => v.clone(),
        Locator::Paragraph(v) => format!("para:{v}"),
        Locator::Chapter(v) => format!("chapter:{v}"),
        Locator::Timestamp(v) => format!("time:{v}"),
    }
}

/// Read the inside of a `[^cite:…]` — everything between the prefix and the closing bracket.
///
/// Returns `None` for anything malformed, which leaves the text alone rather than inventing a
/// citation from it.
fn parse_inner(inner: &str) -> Option<CitationRef> {
    // The form suffix is stripped first: a locator may contain anything, including an `!`.
    let (head, form) = match inner.strip_suffix("!narrative") {
        Some(head) => (head, Form::Narrative),
        None => (inner, Form::Parenthetical),
    };

    let (id_part, locator_part) = match head.split_once('@') {
        Some((id, locator)) => (id, Some(locator)),
        None => (head, None),
    };

    let source: Id = id_part.trim().parse().ok()?;
    let locator = match locator_part {
        None => None,
        Some("") => return None, // `@` with nothing after it is a typo
        Some(raw) => Some(parse_locator(raw)?),
    };

    Some(CitationRef {
        source,
        locator,
        form,
    })
}

fn parse_locator(raw: &str) -> Option<Locator> {
    // `split_once` and not `split`: a timestamp is `1:04:22`, so only the first colon separates
    // the kind from the value. Splitting on every colon turns 1:04:22 into a kind of "1".
    match raw.split_once(':') {
        Some(("para", value)) if !value.is_empty() => Some(Locator::Paragraph(value.to_string())),
        Some(("chapter", value)) if !value.is_empty() => Some(Locator::Chapter(value.to_string())),
        Some(("time", value)) if !value.is_empty() => Some(Locator::Timestamp(value.to_string())),
        // A kind nobody knows is not a page with a funny name — it is a mistake, and turning it
        // into a page would cite a chapter as though it were one.
        Some(_) => None,
        None => Some(Locator::Page(raw.to_string())),
    }
}

/// Every citation in a body, in the order it appears, keeping repeats.
///
/// Repeats are kept because each is a place in the text that needs rendering; `Document::
/// citations` is the one that deduplicates, because what it answers is "which sources does this
/// document draw on".
pub fn parse_all(body: &str) -> Vec<CitationRef> {
    const OPEN: &str = "[^cite:";
    let mut found = Vec::new();
    let mut rest = body;

    while let Some(start) = rest.find(OPEN) {
        rest = &rest[start + OPEN.len()..];
        let Some(end) = rest.find(']') else { break };
        if let Some(reference) = parse_inner(&rest[..end]) {
            found.push(reference);
        }
        rest = &rest[end + 1..];
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Id {
        "01J8ZQ8D4Q6S8W0W2Y4A6C8E0G".parse().unwrap()
    }

    #[test]
    fn a_bare_citation_is_parenthetical_with_no_locator() {
        let parsed = parse_all(&format!("Nine acids.[^cite:{}]", id()));
        assert_eq!(parsed, vec![CitationRef::new(id())]);
    }

    #[test]
    fn a_bare_locator_is_a_page() {
        // The format meant this before kinds existed, and every file already written says it
        // this way. Reading it as anything else would change what those files mean.
        let parsed = parse_all(&format!("[^cite:{}@44-46]", id()));
        assert_eq!(parsed[0].locator, Some(Locator::Page("44-46".to_string())));
    }

    #[test]
    fn a_timestamp_keeps_its_colons() {
        // The case that decides the parser: splitting on every colon makes the kind "1".
        let parsed = parse_all(&format!("[^cite:{}@time:1:04:22]", id()));
        assert_eq!(
            parsed[0].locator,
            Some(Locator::Timestamp("1:04:22".to_string()))
        );
    }

    #[test]
    fn paragraphs_and_chapters_name_themselves() {
        let parsed = parse_all(&format!(
            "[^cite:{a}@para:7] and [^cite:{a}@chapter:3]",
            a = id()
        ));
        assert_eq!(parsed[0].locator, Some(Locator::Paragraph("7".to_string())));
        assert_eq!(parsed[1].locator, Some(Locator::Chapter("3".to_string())));
    }

    #[test]
    fn narrative_form_is_written_and_read() {
        let parsed = parse_all(&format!("[^cite:{}!narrative]", id()));
        assert_eq!(parsed[0].form, Form::Narrative);
        assert_eq!(parsed[0].locator, None);
    }

    #[test]
    fn a_locator_and_a_form_together() {
        let parsed = parse_all(&format!("[^cite:{}@44!narrative]", id()));
        assert_eq!(parsed[0].form, Form::Narrative);
        assert_eq!(parsed[0].locator, Some(Locator::Page("44".to_string())));
    }

    #[test]
    fn parse_and_render_are_inverses() {
        // The guard. Every citation the editor writes passes through both, so a drift here is
        // silent corruption of a file the user believes they can read forever.
        let written = [
            format!("[^cite:{}]", id()),
            format!("[^cite:{}@44]", id()),
            format!("[^cite:{}@44-46]", id()),
            format!("[^cite:{}@para:7]", id()),
            format!("[^cite:{}@chapter:3]", id()),
            format!("[^cite:{}@time:1:04:22]", id()),
            format!("[^cite:{}!narrative]", id()),
            format!("[^cite:{}@44!narrative]", id()),
            format!("[^cite:{}@time:1:04:22!narrative]", id()),
        ];
        for text in written {
            let parsed = parse_all(&text);
            assert_eq!(parsed.len(), 1, "{text} should parse as one citation");
            assert_eq!(
                parsed[0].render(),
                text,
                "{text} did not survive a round trip"
            );
        }
    }

    #[test]
    fn an_explicit_page_kind_is_not_a_citation() {
        // Pages have one spelling, the bare one. Accepting `@page:44` as well would give the
        // same value two forms, and a file could not round-trip byte-identically.
        assert!(parse_all(&format!("[^cite:{}@page:44]", id())).is_empty());
    }

    #[test]
    fn an_unknown_locator_kind_is_left_as_prose() {
        // Not silently a page: citing a chapter as a page number is wrong output, and wrong
        // output is worse than visibly unrecognised text.
        assert!(parse_all(&format!("[^cite:{}@section:2]", id())).is_empty());
    }

    #[test]
    fn prose_that_looks_like_syntax_is_left_alone() {
        assert!(parse_all("A bare [^cite:not-an-id] and an unclosed [^cite:").is_empty());
        assert!(parse_all(&format!("[^cite:{}@]", id())).is_empty());
    }

    #[test]
    fn repeats_are_kept_because_each_one_is_a_place_in_the_text() {
        let parsed = parse_all(&format!("[^cite:{a}] and again [^cite:{a}@44]", a = id()));
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn a_document_can_cite_the_same_source_in_both_forms() {
        let parsed = parse_all(&format!(
            "{a} says so.[^cite:{a}!narrative] It is also true.[^cite:{a}]",
            a = id()
        ));
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].form, Form::Narrative);
        assert_eq!(parsed[1].form, Form::Parenthetical);
    }
}
