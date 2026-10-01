//! A document as somebody else will read it: exported, with its citations written out and its
//! reference list at the end.
//!
//! **One model, several writers.** The body is read once into an [`Export`] — headings, and
//! paragraphs made of plain and italic runs, line breaks and pictures — with every citation
//! already formatted against the whole library and every link already its document's title. A
//! writer only lays that out. None of them looks anything up, so all of them agree about what
//! the document says.
//!
//! **The body is read by the editor's rules, not Markdown's.** What somebody sees in the
//! application is what they get in the file: a `# ` line is a heading, and `*a*` is two
//! asterisks and a letter, exactly as the editor draws them (`storage-format.md`, "What the
//! editor renders"). A Markdown reading would turn `1990. That year…` into a numbered list
//! nobody wrote. The rules here mirror the interface's `body.ts` line for line, and its tests
//! are mirrored below.

pub mod html;
pub mod text;

use std::collections::HashSet;

use crate::citation::format::{Form, Locator};
use crate::citation::CitationRef;
use crate::id::Id;

/// The formats a document can be exported to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// Plain text, which survives being pasted anywhere.
    Text,
    /// One web page with its pictures inside it, needing nothing else to open.
    Html,
}

impl Format {
    /// The file extension a file in this format is saved with.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Text => "txt",
            Format::Html => "html",
        }
    }
}

/// A document ready to be written out. Nothing in it refers to anything else.
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub title: String,
    pub blocks: Vec<Block>,
    /// The reference list, one entry each, in APA order. Empty when nothing is cited.
    pub references: Vec<Vec<Run>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// `# ` to `###### `: level 1 to 6.
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(Run),
    /// A single newline inside a paragraph, which the editor shows as a new line.
    Break,
    Picture(Picture),
}

/// Text, all of it in italics or none of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub italic: bool,
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            italic: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    /// What the body says it shows: `![this](…)`.
    pub alt: String,
    /// Its filename, beside the document.
    pub name: String,
    /// The file itself, or `None` when there is no picture there to include.
    pub image: Option<Image>,
}

impl Picture {
    /// What stands in for the picture where it cannot be shown: its description, or failing
    /// that its name.
    pub fn label(&self) -> String {
        let what = if self.alt.trim().is_empty() {
            &self.name
        } else {
            &self.alt
        };
        format!("[Picture: {what}]")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub kind: ImageKind,
    pub bytes: Vec<u8>,
}

/// The kinds of picture a document can hold — the ones the engine accepts when one is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ImageKind {
    /// Read from the file's first bytes, never from its name: a renamed file is still what it
    /// was.
    pub fn of(bytes: &[u8]) -> Option<ImageKind> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(ImageKind::Png)
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            Some(ImageKind::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(ImageKind::Gif)
        } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            Some(ImageKind::Webp)
        } else {
            None
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ImageKind::Png => "image/png",
            ImageKind::Jpeg => "image/jpeg",
            ImageKind::Gif => "image/gif",
            ImageKind::Webp => "image/webp",
        }
    }
}

/// What a document's citations, links and pictures lead to. Answered by the engine, which
/// knows the library; the reading of the body knows only the body.
pub trait Resolve {
    /// Whether a source is still in the library.
    fn known(&self, source: Id) -> bool;
    /// The group a source's citation introduces a short form for, by its full name — "World
    /// Health Organization" for a report by the WHO — or `None` when there is none to
    /// introduce. A group's short form is introduced once in a document, whichever of its works
    /// is cited first.
    fn introduces(&self, source: Id) -> Option<String>;
    /// Parenthetical citations at one place, written as one — `(Kallio & Virtanen, 2019;
    /// Nieminen, 2021a, p. 44)` — with italics marked `*…*`. Every source they name is known,
    /// and each comes with whether its group's short form was introduced earlier.
    fn parenthetical(&self, together: &[(CitationRef, bool)]) -> String;
    /// A narrative citation — `Nieminen (2021)` — or `None` when its source has gone.
    fn narrative(&self, citation: &CitationRef, introduced: bool) -> Option<String>;
    /// A linked document's title, or `None` when the document is gone.
    fn link(&self, document: Id) -> Option<String>;
    /// A picture's bytes, or `None` when the file is not there.
    fn picture(&self, name: &str) -> Option<Vec<u8>>;
}

/// What stands in for a citation whose source has gone: the words the editor shows.
pub const MISSING_SOURCE: &str = "[source missing]";
/// What stands in for a link to a document that has gone: the words the editor shows.
pub const MISSING_DOCUMENT: &str = "[missing document]";

/// Read a document into the model every writer lays out.
///
/// `references` is the document's reference list as the engine formats it, italics marked
/// `*…*`.
pub fn build(title: &str, body: &str, references: &[String], resolve: &dyn Resolve) -> Export {
    // The groups whose short forms have been introduced so far, read in the document's order.
    let mut introduced = HashSet::new();
    let mut reader = Reader {
        resolve,
        introduced: &mut introduced,
    };
    Export {
        title: title.to_string(),
        blocks: body
            .split("\n\n")
            .filter_map(|block| reader.block(block))
            .collect(),
        references: references
            .iter()
            .map(|entry| read_emphasis(entry))
            .collect(),
    }
}

/// Reads a body block by block, in order, remembering what its citations have introduced.
struct Reader<'r> {
    resolve: &'r dyn Resolve,
    introduced: &'r mut HashSet<String>,
}

impl Reader<'_> {
    /// One block: a heading, a paragraph, or nothing at all when it holds nothing to read.
    ///
    /// The editor keeps an empty paragraph as space on the page. A reader of the export has no
    /// use for one, so it is left out rather than written as an empty line in every format.
    fn block(&mut self, block: &str) -> Option<Block> {
        if let Some((level, text)) = heading(block) {
            let content = self.inline(text);
            return (!is_blank(&content)).then_some(Block::Heading { level, content });
        }
        let content = self.inline(block.trim_matches('\n'));
        (!is_blank(&content)).then_some(Block::Paragraph(content))
    }
}

fn is_blank(content: &[Inline]) -> bool {
    content.iter().all(|inline| match inline {
        Inline::Text(run) => run.text.trim().is_empty(),
        Inline::Break => true,
        Inline::Picture(_) => false,
    })
}

/// `# ` to `###### `, on one line, and nothing else — the editor's `HEADING_PATTERN`.
///
/// "One line" is the regular expression's: no `\n`, `\r`, U+2028 or U+2029 anywhere in the
/// block, because its `.` matches none of them.
fn heading(block: &str) -> Option<(u8, &str)> {
    if block.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    let hashes = block.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let level = u8::try_from(hashes).ok()?;
    block[hashes..].strip_prefix(' ').map(|text| (level, text))
}

/// The three constructs a paragraph can hold, before they are looked up.
enum Construct {
    Citation(CitationRef),
    Link(Id),
    Image { alt: String, name: String },
}

/// What a paragraph is made of, in order, before anything is looked up.
enum Piece<'a> {
    Text(&'a str),
    Construct(Construct),
}

/// A paragraph's text as text and constructs, in order — the editor's `parseInline`.
///
/// Each construct is found separately and the earlier of two overlapping ones wins, exactly as
/// the editor decides, so the export never reads a citation the editor shows as text, or the
/// other way round.
fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut found: Vec<(usize, usize, Construct)> = Vec::new();
    found.extend(scan(text, "[^cite:", |s| {
        citation_at(s).map(|(len, c)| (len, Construct::Citation(c)))
    }));
    found.extend(scan(text, "[[", |s| {
        link_at(s).map(|(len, id)| (len, Construct::Link(id)))
    }));
    found.extend(scan(text, "![", |s| {
        image_at(s).map(|(len, alt, name)| (len, Construct::Image { alt, name }))
    }));
    found.sort_by_key(|(start, _, _)| *start);

    let mut pieces = Vec::new();
    let mut cursor = 0;
    for (start, end, construct) in found {
        if start < cursor {
            continue;
        }
        if start > cursor {
            pieces.push(Piece::Text(&text[cursor..start]));
        }
        pieces.push(Piece::Construct(construct));
        cursor = end;
    }
    if cursor < text.len() {
        pieces.push(Piece::Text(&text[cursor..]));
    }
    pieces
}

impl Reader<'_> {
    /// A paragraph's text into runs, breaks and pictures.
    ///
    /// The citations are set the way APA sets them, which the editor's chips do not need to:
    /// parenthetical citations side by side become one pair of parentheses, a citation is never
    /// run into the word before it, and a group's short form is introduced once.
    fn inline(&mut self, text: &str) -> Vec<Inline> {
        let pieces = pieces(text);
        let mut out = Vec::new();
        let mut i = 0;
        while i < pieces.len() {
            match &pieces[i] {
                Piece::Text(text) => {
                    plain(text, &mut out);
                    i += 1;
                }
                Piece::Construct(Construct::Citation(first))
                    if first.form == Form::Parenthetical =>
                {
                    let mut together = vec![first.clone()];
                    i += 1;
                    while let Some((next, used)) = side_by_side(&pieces[i..]) {
                        together.push(next.clone());
                        i += used;
                    }
                    self.cited(&together, &mut out);
                }
                Piece::Construct(construct) => {
                    self.resolved(construct, &mut out);
                    i += 1;
                }
            }
        }
        out
    }

    /// Whether a source's group has had its short form introduced — and from now on, it has.
    fn introduce(&mut self, source: Id) -> bool {
        match self.resolve.introduces(source) {
            Some(group) => !self.introduced.insert(group),
            None => false,
        }
    }

    /// Parenthetical citations at one place: one pair of parentheses for those whose sources
    /// are still in the library, then a marker for each that has gone.
    fn cited(&mut self, together: &[CitationRef], out: &mut Vec<Inline>) {
        let (known, gone): (Vec<CitationRef>, Vec<CitationRef>) = together
            .iter()
            .cloned()
            .partition(|c| self.resolve.known(c.source));
        if !known.is_empty() {
            // Decided for all of them before any is marked: two works by one group cited
            // together both introduce it, and print it once.
            let before: Vec<bool> = known
                .iter()
                .map(|c| {
                    self.resolve
                        .introduces(c.source)
                        .is_some_and(|group| self.introduced.contains(&group))
                })
                .collect();
            for c in &known {
                self.introduce(c.source);
            }
            let marked: Vec<(CitationRef, bool)> = known.into_iter().zip(before).collect();
            space_before(out);
            let text = self.resolve.parenthetical(&marked);
            out.extend(read_emphasis(&text).into_iter().map(Inline::Text));
        }
        for _ in gone {
            space_before(out);
            out.push(Inline::Text(Run::plain(MISSING_SOURCE)));
        }
    }

    /// A narrative citation, a link or a picture. A narrative citation is part of the sentence
    /// — `Nieminen (2021) found` — so it is written exactly where it stands.
    fn resolved(&mut self, construct: &Construct, out: &mut Vec<Inline>) {
        match construct {
            Construct::Citation(citation) => {
                let rendered = if self.resolve.known(citation.source) {
                    let introduced = self.introduce(citation.source);
                    self.resolve.narrative(citation, introduced)
                } else {
                    None
                };
                match rendered {
                    Some(text) => out.extend(read_emphasis(&text).into_iter().map(Inline::Text)),
                    None => out.push(Inline::Text(Run::plain(MISSING_SOURCE))),
                }
            }
            Construct::Link(document) => {
                let title = self
                    .resolve
                    .link(*document)
                    .unwrap_or_else(|| MISSING_DOCUMENT.to_string());
                out.push(Inline::Text(Run::plain(title)));
            }
            Construct::Image { alt, name } => {
                let image = self
                    .resolve
                    .picture(name)
                    .and_then(|bytes| ImageKind::of(&bytes).map(|kind| Image { kind, bytes }));
                out.push(Inline::Picture(Picture {
                    alt: alt.clone(),
                    name: name.clone(),
                    image,
                }));
            }
        }
    }
}

/// The next parenthetical citation, if it is right beside the last one — nothing between
/// them, or only spaces — and how many pieces it takes.
fn side_by_side<'p>(pieces: &'p [Piece]) -> Option<(&'p CitationRef, usize)> {
    let parenthetical = |piece: Option<&'p Piece>| match piece {
        Some(Piece::Construct(Construct::Citation(c))) if c.form == Form::Parenthetical => Some(c),
        _ => None,
    };
    match pieces.first() {
        Some(Piece::Text(text)) if text.chars().all(|c| c == ' ') => {
            parenthetical(pieces.get(1)).map(|c| (c, 2))
        }
        first => parenthetical(first).map(|c| (c, 1)),
    }
}

/// A space, unless what comes before already parts it from the word before: a space, the start
/// of a line, or an opening bracket or quotation mark. `tissue(Nieminen, 2021)` and
/// `tissue.(Nieminen, 2021)` are what the text holds when a citation was put straight after
/// the words it supports, and neither is how a citation is printed.
fn space_before(out: &mut Vec<Inline>) {
    const OPENING: &[char] = &[
        '(', '[', '{', '\u{201c}', '\u{2018}', '\u{201e}', '\u{201a}', '\u{ab}',
    ];
    let needs = match out.last() {
        None | Some(Inline::Break) => false,
        Some(Inline::Picture(_)) => true,
        Some(Inline::Text(run)) => run
            .text
            .chars()
            .last()
            .is_some_and(|c| !c.is_whitespace() && !OPENING.contains(&c)),
    };
    if needs {
        out.push(Inline::Text(Run::plain(" ")));
    }
}

/// Every non-overlapping match of one construct, left to right, as a global regular
/// expression finds them: a failed attempt moves on one character, a match moves past itself.
fn scan(
    text: &str,
    prefix: &str,
    parse: impl Fn(&str) -> Option<(usize, Construct)>,
) -> Vec<(usize, usize, Construct)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(prefix) {
        let start = from + offset;
        match parse(&text[start..]) {
            Some((len, construct)) => {
                found.push((start, start + len, construct));
                from = start + len;
            }
            // Every prefix starts with an ASCII character, so one byte on is a character on.
            None => from = start + 1,
        }
    }
    found
}

/// Plain text, with each newline a line break.
fn plain(text: &str, out: &mut Vec<Inline>) {
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push(Inline::Break);
        }
        if !line.is_empty() {
            out.push(Inline::Text(Run::plain(line)));
        }
    }
}

/// An identifier as the editor recognises one: 26 characters of upper-case Crockford base 32.
fn identifier(text: &str) -> Option<Id> {
    let candidate = text.get(..26)?;
    let crockford = |b: u8| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"ILOU".contains(&b));
    if !candidate.bytes().all(crockford) {
        return None;
    }
    candidate.parse().ok()
}

/// `[^cite:<id>]`, optionally `@<locator>`, optionally `!narrative` — the editor's
/// `CITATION_PATTERN`. Returns the length matched.
fn citation_at(s: &str) -> Option<(usize, CitationRef)> {
    let after_prefix = s.strip_prefix("[^cite:")?;
    let source = identifier(after_prefix)?;
    let mut rest = &after_prefix[26..];

    let mut locator = None;
    if let Some(after) = rest.strip_prefix('@') {
        let (found, len) = locator_at(after)?;
        locator = Some(found);
        rest = &after[len..];
    }
    let mut form = Form::Parenthetical;
    if let Some(after) = rest.strip_prefix("!narrative") {
        form = Form::Narrative;
        rest = after;
    }
    let rest = rest.strip_prefix(']')?;

    Some((
        s.len() - rest.len(),
        CitationRef {
            source,
            locator,
            form,
        },
    ))
}

/// A locator: a kind that names itself and a value up to `]` or `!`, or a bare page up to `]`,
/// `!` or `:`. Never empty.
fn locator_at(s: &str) -> Option<(Locator, usize)> {
    type Kind = fn(String) -> Locator;
    let kinds: [(&str, Kind); 3] = [
        ("para:", Locator::Paragraph),
        ("chapter:", Locator::Chapter),
        ("time:", Locator::Timestamp),
    ];
    for (kind, make) in kinds {
        if let Some(value) = s.strip_prefix(kind) {
            let len = value.find([']', '!']).unwrap_or(value.len());
            if len > 0 {
                return Some((make(value[..len].to_string()), kind.len() + len));
            }
            // An empty value falls through to the bare page, which stops at the colon and so
            // never completes a citation — the same dead end the regular expression reaches.
        }
    }
    let len = s.find([']', '!', ':']).unwrap_or(s.len());
    (len > 0).then(|| (Locator::Page(s[..len].to_string()), len))
}

/// `[[<id>]]` — the editor's `LINK_PATTERN`.
fn link_at(s: &str) -> Option<(usize, Id)> {
    let after_prefix = s.strip_prefix("[[")?;
    let document = identifier(after_prefix)?;
    after_prefix[26..]
        .starts_with("]]")
        .then_some((2 + 26 + 2, document))
}

/// `![<alt>](<name>)`, the alt on one line and the name without white space — the editor's
/// `IMAGE_PATTERN`.
fn image_at(s: &str) -> Option<(usize, String, String)> {
    let after_prefix = s.strip_prefix("![")?;
    let close = after_prefix.find(']')?;
    let alt = &after_prefix[..close];
    if alt.contains('\n') {
        return None;
    }
    let target = after_prefix[close + 1..].strip_prefix('(')?;
    let len = target
        .find(|c: char| c == ')' || is_js_space(c))
        .unwrap_or(target.len());
    if len == 0 || !target[len..].starts_with(')') {
        return None;
    }
    let consumed = 2 + close + 2 + len + 1;
    Some((consumed, alt.to_string(), target[..len].to_string()))
}

/// What a JavaScript regular expression's `\s` matches, which is not quite Unicode's white
/// space: it includes U+FEFF and leaves out U+0085.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Split formatted text into plain and italic runs at its `*…*` marks — the interface's
/// `readEmphasis`, character for character.
///
/// Not a Markdown parser. Its first duty is to lose nothing: an asterisk with no partner is an
/// asterisk, and `**` opens nothing.
pub fn read_emphasis(text: &str) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut plain = String::new();
    let mut rest = text;

    while let Some(c) = rest.chars().next() {
        if c != '*' {
            plain.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        // The closing asterisk is looked for from one character past the opening one.
        let close = rest[1..]
            .chars()
            .next()
            .map(|next| 1 + next.len_utf8())
            .and_then(|from| rest[from..].find('*').map(|i| from + i));
        match close {
            None => {
                plain.push('*');
                rest = &rest[1..];
            }
            Some(close) => {
                if !plain.is_empty() {
                    runs.push(Run::plain(std::mem::take(&mut plain)));
                }
                runs.push(Run {
                    text: rest[1..close].to_string(),
                    italic: true,
                });
                rest = &rest[close + 1..];
            }
        }
    }
    if !plain.is_empty() {
        runs.push(Run::plain(plain));
    }
    runs
}

/// Bytes as base 64, the standard alphabet with padding: how a web page carries its pictures.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "01J8ZQ8D4Q6S8W0W2Y4A6C8E0G";
    const LINKED: &str = "01J8ZQ9C3P5R7T9V1X3Z5B7D9F";

    const GONE: &str = "01J8ZQ8D4Q6S8W0W2Y4A6C8E09";

    /// A group author, which `GROUP` and `GROUP_TOO` are both by.
    const GROUP: &str = "01J8ZQ8D4Q6S8W0W2Y4A6C8E0A";
    const GROUP_TOO: &str = "01J8ZQ8D4Q6S8W0W2Y4A6C8E0B";

    /// Knows every source but `GONE`, and writes a citation as its form and locator: `(p)`,
    /// `(n Page("44"))`, or `(p; p Page("4"))` for two together, with `short` after a group
    /// whose short form was introduced earlier. Names every link "Linked", and has one
    /// picture, `diagram.png`.
    struct Everything;

    fn describe(form: &str, citation: &CitationRef, introduced: bool) -> String {
        let mut out = match &citation.locator {
            None => form.to_string(),
            Some(locator) => format!("{form} {locator:?}"),
        };
        if introduced {
            out.push_str(" short");
        }
        out
    }

    impl Resolve for Everything {
        fn known(&self, source: Id) -> bool {
            source.to_string() != GONE
        }
        fn introduces(&self, source: Id) -> Option<String> {
            [GROUP, GROUP_TOO]
                .contains(&source.to_string().as_str())
                .then(|| "The Group".to_string())
        }
        fn parenthetical(&self, together: &[(CitationRef, bool)]) -> String {
            let each: Vec<String> = together
                .iter()
                .map(|(c, introduced)| describe("p", c, *introduced))
                .collect();
            format!("({})", each.join("; "))
        }
        fn narrative(&self, citation: &CitationRef, introduced: bool) -> Option<String> {
            Some(format!("({})", describe("n", citation, introduced)))
        }
        fn link(&self, _: Id) -> Option<String> {
            Some("Linked".into())
        }
        fn picture(&self, name: &str) -> Option<Vec<u8>> {
            (name == "diagram.png").then(|| b"\x89PNG\r\n\x1a\nrest".to_vec())
        }
    }

    /// Knows nothing: every source, document and picture is gone.
    struct Nothing;

    impl Resolve for Nothing {
        fn known(&self, _: Id) -> bool {
            false
        }
        fn introduces(&self, _: Id) -> Option<String> {
            None
        }
        fn parenthetical(&self, _: &[(CitationRef, bool)]) -> String {
            unreachable!("asked to write citations whose sources are all gone")
        }
        fn narrative(&self, _: &CitationRef, _: bool) -> Option<String> {
            unreachable!("asked to write a citation whose source is gone")
        }
        fn link(&self, _: Id) -> Option<String> {
            None
        }
        fn picture(&self, _: &str) -> Option<Vec<u8>> {
            None
        }
    }

    fn blocks(body: &str) -> Vec<Block> {
        build("T", body, &[], &Everything).blocks
    }

    /// A paragraph's content as text, with italics in `_…_`, breaks as `|`, pictures as `[P]`.
    fn read(body: &str) -> String {
        read_with(body, &Everything)
    }

    fn read_with(body: &str, resolve: &dyn Resolve) -> String {
        match &build("T", body, &[], resolve).blocks[..] {
            [Block::Paragraph(content)] => content
                .iter()
                .map(|inline| match inline {
                    Inline::Text(run) if run.italic => format!("_{}_", run.text),
                    Inline::Text(run) => run.text.clone(),
                    Inline::Break => "|".into(),
                    Inline::Picture(p) => format!("[P {} {}]", p.alt, p.image.is_some()),
                })
                .collect(),
            other => panic!("not one paragraph: {other:?}"),
        }
    }

    // ---- blocks: the editor's parseBlock ------------------------------------------

    #[test]
    fn blocks_are_separated_by_a_blank_line() {
        assert_eq!(blocks("One.\n\nTwo.").len(), 2);
    }

    #[test]
    fn a_hash_and_a_space_is_a_heading_of_that_level() {
        assert_eq!(
            blocks("### Kinds of protein"),
            vec![Block::Heading {
                level: 3,
                content: vec![Inline::Text(Run::plain("Kinds of protein"))]
            }]
        );
    }

    #[test]
    fn what_is_not_a_heading_in_the_editor_is_not_one_here() {
        for body in [
            "#Protein",
            "####### Seven",
            "# Title\nand prose",
            "# Title\r",
            " # Indented",
        ] {
            assert!(
                matches!(&blocks(body)[..], [Block::Paragraph(_)]),
                "{body:?} became {:?}",
                blocks(body)
            );
        }
    }

    #[test]
    fn a_blank_block_is_left_out() {
        assert_eq!(blocks("One.\n\n\n\nTwo.\n\n# ").len(), 2);
    }

    #[test]
    fn a_newline_inside_a_paragraph_is_a_line_break() {
        assert_eq!(read("First line\nsecond line"), "First line|second line");
    }

    #[test]
    fn newlines_at_the_edges_of_a_paragraph_are_spacing() {
        // Three newlines in a row leave the second paragraph starting with one.
        let read = build("T", "One.\n\n\nTwo.", &[], &Everything).blocks;
        assert_eq!(
            read[1],
            Block::Paragraph(vec![Inline::Text(Run::plain("Two."))])
        );
    }

    // ---- inline: the editor's parseInline ------------------------------------------

    #[test]
    fn a_citation_is_written_out_where_it_sits() {
        assert_eq!(
            read(&format!("Tissue [^cite:{SOURCE}]. Next.")),
            "Tissue (p). Next."
        );
    }

    #[test]
    fn each_kind_of_locator_and_the_narrative_form_are_read() {
        assert_eq!(
            read(&format!("[^cite:{SOURCE}@44-46]")),
            "(p Page(\"44-46\"))"
        );
        assert_eq!(
            read(&format!("[^cite:{SOURCE}@time:1:04:22!narrative]")),
            "(n Timestamp(\"1:04:22\"))"
        );
        assert_eq!(
            read(&format!(
                "[^cite:{SOURCE}@para:7] and [^cite:{SOURCE}@chapter:3]"
            )),
            "(p Paragraph(\"7\")) and (p Chapter(\"3\"))"
        );
    }

    // ---- citations as APA sets them --------------------------------------------------

    #[test]
    fn citations_side_by_side_are_written_as_one() {
        // Two sources behind one claim: one pair of parentheses, as APA 7 §8.12 has it.
        assert_eq!(
            read(&format!("Claim[^cite:{SOURCE}][^cite:{LINKED}@4].")),
            "Claim (p; p Page(\"4\"))."
        );
        assert_eq!(
            read(&format!(
                "Claim [^cite:{SOURCE}] [^cite:{LINKED}] [^cite:{SOURCE}]."
            )),
            "Claim (p; p; p)."
        );
    }

    #[test]
    fn citations_with_words_between_them_stay_apart() {
        assert_eq!(
            read(&format!("[^cite:{SOURCE}], [^cite:{LINKED}]")),
            "(p), (p)"
        );
    }

    #[test]
    fn a_narrative_citation_is_part_of_the_sentence_and_joins_nothing() {
        assert_eq!(
            read(&format!("[^cite:{SOURCE}!narrative][^cite:{LINKED}] found")),
            "(n) (p) found"
        );
        assert_eq!(
            read(&format!("As[^cite:{SOURCE}!narrative] found")),
            "As(n) found"
        );
    }

    #[test]
    fn a_citation_is_not_run_into_the_word_before_it() {
        assert_eq!(read(&format!("tissue[^cite:{SOURCE}].")), "tissue (p).");
        assert_eq!(
            read(&format!("tissue.[^cite:{SOURCE}] Next")),
            "tissue. (p) Next"
        );
        assert_eq!(read(&format!("“Quoted”[^cite:{SOURCE}]")), "“Quoted” (p)");
        // Already parted from it: a space, a line, an opening bracket or quotation mark.
        assert_eq!(read(&format!("tissue [^cite:{SOURCE}]")), "tissue (p)");
        assert_eq!(read(&format!("line\n[^cite:{SOURCE}]")), "line|(p)");
        assert_eq!(read(&format!("(see [^cite:{SOURCE}])")), "(see (p))");
        assert_eq!(read(&format!("([^cite:{SOURCE}])")), "((p))");
        assert_eq!(read(&format!("«[^cite:{SOURCE}]")), "«(p)");
    }

    #[test]
    fn a_groups_short_form_is_introduced_once_whichever_work_comes_first() {
        // APA 7 §8.21: spelled out with the short form the first time, the short form after —
        // per group, not per work, and in either form.
        assert_eq!(
            read(&format!(
                "A[^cite:{GROUP}] B[^cite:{GROUP_TOO}] [^cite:{GROUP}!narrative] C[^cite:{SOURCE}]"
            )),
            "A (p) B (p short) (n short) C (p)"
        );
        assert_eq!(
            read(&format!("[^cite:{GROUP}!narrative] then[^cite:{GROUP}]")),
            "(n) then (p short)"
        );
    }

    #[test]
    fn two_works_by_one_group_cited_together_both_introduce_it() {
        assert_eq!(
            read(&format!(
                "A[^cite:{GROUP}][^cite:{GROUP_TOO}] B[^cite:{GROUP}]"
            )),
            "A (p; p) B (p short)"
        );
    }

    #[test]
    fn introducing_carries_on_from_one_paragraph_to_the_next() {
        let read = build(
            "T",
            &format!("# On [^cite:{GROUP}]\n\nThen[^cite:{GROUP}]"),
            &[],
            &Everything,
        );
        let Block::Paragraph(content) = &read.blocks[1] else {
            panic!()
        };
        assert_eq!(content[2], Inline::Text(Run::plain("(p short)")));
    }

    #[test]
    fn a_gone_source_among_others_is_marked_after_them() {
        assert_eq!(
            read(&format!("Claim[^cite:{GONE}][^cite:{SOURCE}].")),
            "Claim (p) [source missing]."
        );
    }

    #[test]
    fn what_the_editor_shows_as_text_stays_text() {
        for body in [
            format!("[^cite:{SOURCE}@page:44]"),
            format!("[^cite:{SOURCE}@]"),
            format!("[^cite:{SOURCE}@time:]"),
            format!("[^cite:{}]", SOURCE.to_lowercase()),
            format!("[^cite:{SOURCE}!parenthetical]"),
            format!("[[{SOURCE}]"),
            "![Diagram](my file.png)".to_string(),
            "![Two\nlines](diagram.png)".to_string(),
        ] {
            assert_eq!(read(&body), body.replace('\n', "|"), "{body:?}");
        }
    }

    #[test]
    fn a_link_reads_as_the_title_of_what_it_links_to() {
        assert_eq!(read(&format!("See [[{LINKED}]] too.")), "See Linked too.");
    }

    #[test]
    fn a_picture_is_carried_with_its_bytes() {
        assert_eq!(read("![Amino acids](diagram.png)"), "[P Amino acids true]");
        assert_eq!(read("![Gone](elsewhere.png)"), "[P Gone false]");
    }

    #[test]
    fn what_has_gone_reads_the_way_the_editor_shows_it() {
        assert_eq!(
            read_with(
                &format!(
                    "A[^cite:{SOURCE}] B[[{LINKED}]] [^cite:{SOURCE}!narrative] ![C](diagram.png)"
                ),
                &Nothing
            ),
            format!("A {MISSING_SOURCE} B{MISSING_DOCUMENT} {MISSING_SOURCE} [P C false]")
        );
    }

    #[test]
    fn of_two_overlapping_constructs_the_earlier_wins() {
        // A page "number" holding what would otherwise be a link: the citation starts first,
        // so the link inside it is not read, and the bracket left over is text.
        let body = format!("[^cite:{SOURCE}@[[{LINKED}]]");
        assert_eq!(read(&body), format!("(p Page(\"[[{LINKED}\"))]"));
    }

    #[test]
    fn a_picture_that_is_not_an_image_is_not_carried() {
        struct Text;
        impl Resolve for Text {
            fn known(&self, _: Id) -> bool {
                false
            }
            fn introduces(&self, _: Id) -> Option<String> {
                None
            }
            fn parenthetical(&self, _: &[(CitationRef, bool)]) -> String {
                unreachable!()
            }
            fn narrative(&self, _: &CitationRef, _: bool) -> Option<String> {
                None
            }
            fn link(&self, _: Id) -> Option<String> {
                None
            }
            fn picture(&self, _: &str) -> Option<Vec<u8>> {
                Some(b"not a picture".to_vec())
            }
        }
        let read = build("T", "![x](diagram.png)", &[], &Text);
        assert!(matches!(
            &read.blocks[0],
            Block::Paragraph(c) if matches!(&c[0], Inline::Picture(Picture { image: None, .. }))
        ));
    }

    // ---- emphasis: the interface's readEmphasis -------------------------------------

    #[test]
    fn italics_are_read_from_their_asterisks() {
        assert_eq!(
            read_emphasis("Nieminen, A. (2021). *Ravinto*. Otava."),
            vec![
                Run::plain("Nieminen, A. (2021). "),
                Run {
                    text: "Ravinto".into(),
                    italic: true
                },
                Run::plain(". Otava."),
            ]
        );
    }

    #[test]
    fn an_asterisk_with_no_partner_is_an_asterisk() {
        assert_eq!(
            read_emphasis("Rated 5* and nothing else."),
            vec![Run::plain("Rated 5* and nothing else.")]
        );
        assert_eq!(read_emphasis("ends *"), vec![Run::plain("ends *")]);
    }

    #[test]
    fn two_asterisks_together_open_nothing() {
        // As the interface reads it: the second asterisk is skipped over, not taken as closing.
        assert_eq!(
            read_emphasis("**a*"),
            vec![Run {
                text: "*a".into(),
                italic: true
            }]
        );
        assert_eq!(read_emphasis("a ** b"), vec![Run::plain("a ** b")]);
    }

    #[test]
    fn emphasis_is_read_through_letters_of_any_width() {
        assert_eq!(
            read_emphasis("*Основы питания*, *ä*"),
            vec![
                Run {
                    text: "Основы питания".into(),
                    italic: true
                },
                Run::plain(", "),
                Run {
                    text: "ä".into(),
                    italic: true
                },
            ]
        );
    }

    // ---- the rest ------------------------------------------------------------------

    #[test]
    fn base64_matches_the_standard_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected);
        }
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn a_picture_is_known_by_its_bytes() {
        assert_eq!(ImageKind::of(b"\x89PNG\r\n\x1a\n..."), Some(ImageKind::Png));
        assert_eq!(ImageKind::of(b"\xff\xd8\xff\xe0"), Some(ImageKind::Jpeg));
        assert_eq!(ImageKind::of(b"GIF89a"), Some(ImageKind::Gif));
        assert_eq!(
            ImageKind::of(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageKind::Webp)
        );
        assert_eq!(ImageKind::of(b"<svg"), None);
    }
}
