//! Where everything goes: lines broken to fit across the page, and pages broken to fit down it.
//!
//! Pages are A4 with an inch of margin all round, as the Word and OpenDocument files have. Text
//! is set ragged right, as APA asks, never stretched out to both margins. A line breaks where
//! there is a space. A word too long for any line — a web address, nearly always — breaks where
//! APA says one may: after a double slash or an ampersand, or before most other punctuation,
//! and never with a hyphen added.
//!
//! The settings mirror the Word file's styles: the title centred and bold, headings bold and
//! smaller at each level, the fourth to sixth told apart by italics, and the reference list
//! with its half-inch hanging indent. Coordinates here are in points from the top left of the
//! page, the way a page is read; the writer turns them the way up PDF counts.

use std::ops::Range;

use super::font::Font;
use super::picture::{self, Prepared};
use crate::export::{Block, Export, Inline, Run};

/// A4, in points.
pub(super) const PAGE_WIDTH: f32 = 595.276;
pub(super) const PAGE_HEIGHT: f32 = 841.89;
/// An inch, all round.
pub(super) const MARGIN: f32 = 72.0;
/// How wide a line of text may be.
const MEASURE: f32 = PAGE_WIDTH - 2.0 * MARGIN;
const BOTTOM: f32 = PAGE_HEIGHT - MARGIN;
/// Tab stops: every half inch from the margin, as in a word processor.
const TAB: f32 = 36.0;
/// APA's hanging indent for the reference list: half an inch.
const HANGING: f32 = 36.0;

/// How a piece of text is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Look {
    pub bold: bool,
    pub italic: bool,
    /// A web address: drawn blue and underlined, and a link.
    pub link: bool,
    /// Standing in for something that is not there: drawn grey.
    pub faint: bool,
}

const PLAIN: Look = Look {
    bold: false,
    italic: false,
    link: false,
    faint: false,
};

/// How one kind of block is set.
#[derive(Debug, Clone, Copy)]
struct Setting {
    size: f32,
    /// Baseline to baseline, as a multiple of the size.
    leading: f32,
    look: Look,
    centred: bool,
    /// Space above, left out at the top of a page.
    before: f32,
    after: f32,
    /// How far in every line after the first starts.
    hanging: f32,
}

/// Eleven points where the Word file has twelve: DejaVu Sans is a large face for its size, and
/// eleven of it reads like twelve of Times New Roman. The lines are spaced as on the web page.
const BODY: Setting = Setting {
    size: 11.0,
    leading: 1.6,
    look: PLAIN,
    centred: false,
    before: 0.0,
    after: 8.0,
    hanging: 0.0,
};

const TITLE: Setting = Setting {
    size: 18.0,
    leading: 1.3,
    look: Look {
        bold: true,
        ..PLAIN
    },
    centred: true,
    before: 0.0,
    after: 18.0,
    hanging: 0.0,
};

const ENTRY: Setting = Setting {
    hanging: HANGING,
    ..BODY
};

fn heading(level: u8) -> Setting {
    let bold = Look {
        bold: true,
        ..PLAIN
    };
    let (size, look) = match level {
        1 => (16.0, bold),
        2 => (14.0, bold),
        3 => (13.0, bold),
        4 => (
            BODY.size,
            Look {
                italic: true,
                ..bold
            },
        ),
        5 => (BODY.size, bold),
        _ => (
            BODY.size,
            Look {
                italic: true,
                ..PLAIN
            },
        ),
    };
    Setting {
        size,
        leading: 1.3,
        look,
        centred: false,
        before: if level == 1 { 18.0 } else { 12.0 },
        after: 6.0,
        hanging: 0.0,
    }
}

/// Everything on the pages, ready to be drawn.
pub(super) struct Layout<'a> {
    pub pages: Vec<Vec<Item>>,
    pub pictures: Vec<Prepared<'a>>,
    pub links: Vec<Link>,
    /// Every heading, for the PDF's outline.
    pub headings: Vec<Heading>,
}

#[derive(Debug, PartialEq)]
pub(super) enum Item {
    /// Text on one line, all in one look, from the left end of its baseline.
    Text {
        x: f32,
        baseline: f32,
        width: f32,
        size: f32,
        look: Look,
        text: String,
    },
    Picture {
        x: f32,
        top: f32,
        width: f32,
        height: f32,
        index: usize,
    },
}

/// Part of a web address on a page: where to click, and where it leads.
#[derive(Debug, PartialEq)]
pub(super) struct Link {
    pub page: usize,
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub url: String,
}

#[derive(Debug, PartialEq)]
pub(super) struct Heading {
    pub level: u8,
    pub title: String,
    pub page: usize,
    pub top: f32,
}

pub(super) fn lay_out<'a>(export: &'a Export, font: &Font) -> Layout<'a> {
    let mut pages = Pages {
        font,
        layout: Layout {
            pages: vec![Vec::new()],
            pictures: Vec::new(),
            links: Vec::new(),
            headings: Vec::new(),
        },
        y: MARGIN,
    };

    let mut title = Paragraph::default();
    title.text(&export.title, PLAIN, None);
    pages.block(TITLE, &title, None);

    for (i, block) in export.blocks.iter().enumerate() {
        match block {
            Block::Heading { level, content } => {
                // Headings straight after this one go with it, and with what follows them.
                let following: f32 = export.blocks[i + 1..]
                    .iter()
                    .map_while(|next| match next {
                        Block::Heading { level, content } => {
                            Some(pages.measure(heading(*level), content))
                        }
                        Block::Paragraph(_) => None,
                    })
                    .sum();
                let paragraph = pages.paragraph(content);
                pages.block(
                    heading(*level),
                    &paragraph,
                    Some(Outline {
                        level: *level,
                        following,
                    }),
                );
            }
            Block::Paragraph(content) => {
                let paragraph = pages.paragraph(content);
                pages.block(BODY, &paragraph, None);
            }
        }
    }

    if !export.references.is_empty() {
        let mut references = Paragraph::default();
        references.text("References", PLAIN, None);
        let setting = Setting {
            centred: true,
            ..heading(1)
        };
        pages.block(
            setting,
            &references,
            Some(Outline {
                level: 1,
                following: 0.0,
            }),
        );
        for entry in &export.references {
            pages.block(ENTRY, &reference(entry), None);
        }
    }
    pages.layout
}

/// A reference entry, its web addresses links — as the Word file and the web page have them.
fn reference(entry: &[Run]) -> Paragraph {
    let mut paragraph = Paragraph::default();
    for run in entry {
        if run.italic {
            paragraph.text(
                &run.text,
                Look {
                    italic: true,
                    ..PLAIN
                },
                None,
            );
            continue;
        }
        for (i, word) in run.text.split(' ').enumerate() {
            if i > 0 {
                paragraph.text(" ", PLAIN, None);
            }
            if word.starts_with("https://") || word.starts_with("http://") {
                paragraph.text(
                    word,
                    Look {
                        link: true,
                        ..PLAIN
                    },
                    Some(word),
                );
            } else {
                paragraph.text(word, PLAIN, None);
            }
        }
    }
    paragraph
}

/// A block's content as the line breaker reads it: character by character, each in a look.
#[derive(Debug, Default)]
struct Paragraph {
    units: Vec<Unit>,
    /// The looks the characters are in, each with the web address it links to, if any.
    looks: Vec<(Look, Option<String>)>,
}

impl Paragraph {
    fn text(&mut self, text: &str, look: Look, url: Option<&str>) {
        let wanted = (look, url.map(str::to_string));
        let index = match self.looks.iter().position(|l| *l == wanted) {
            Some(index) => index,
            None => {
                self.looks.push(wanted);
                self.looks.len() - 1
            }
        };
        self.units
            .extend(text.chars().map(|c| Unit::Char(c, index)));
    }
}

/// A heading's place in the outline, and the height of any headings straight after it, which
/// must go on the same page.
#[derive(Debug, Clone, Copy)]
struct Outline {
    level: u8,
    following: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Unit {
    /// A character, in the look its paragraph numbers so.
    Char(char, usize),
    /// A new line the writer asked for.
    Break,
    /// A picture, on a line of its own.
    Picture(usize),
}

#[derive(Debug, PartialEq)]
enum Line {
    /// Characters, the spaces a line broke at left out.
    Text(Range<usize>),
    Picture(usize),
}

/// Where the next line goes, and everything placed so far.
struct Pages<'f, 'a> {
    font: &'f Font,
    layout: Layout<'a>,
    /// How far down the current page the next line starts.
    y: f32,
}

impl<'a> Pages<'_, 'a> {
    fn page(&self) -> usize {
        self.layout.pages.len() - 1
    }

    fn at_top(&self) -> bool {
        self.y <= MARGIN
    }

    /// Room for `height` more: on this page if it fits, and otherwise on a new one.
    fn room(&mut self, height: f32) {
        if self.y + height > BOTTOM && !self.at_top() {
            self.layout.pages.push(Vec::new());
            self.y = MARGIN;
        }
    }

    /// A paragraph's content, its pictures read and kept, and those that cannot be shown
    /// written as their descriptions, as every other format does.
    fn paragraph(&mut self, content: &'a [Inline]) -> Paragraph {
        let mut paragraph = Paragraph::default();
        for inline in content {
            match inline {
                Inline::Text(run) => paragraph.text(
                    &run.text,
                    Look {
                        italic: run.italic,
                        ..PLAIN
                    },
                    None,
                ),
                Inline::Break => paragraph.units.push(Unit::Break),
                Inline::Picture(picture) => match picture::prepare(picture) {
                    Some(prepared) => {
                        self.layout.pictures.push(prepared);
                        paragraph
                            .units
                            .push(Unit::Picture(self.layout.pictures.len() - 1));
                    }
                    None => paragraph.text(
                        &picture.label(),
                        Look {
                            faint: true,
                            ..PLAIN
                        },
                        None,
                    ),
                },
            }
        }
        paragraph
    }

    /// How much of the page a heading will take, read as text alone — a picture in a heading
    /// counted as its description. What keeps a run of headings together.
    fn measure(&self, setting: Setting, content: &[Inline]) -> f32 {
        let mut paragraph = Paragraph::default();
        for inline in content {
            match inline {
                Inline::Text(run) => paragraph.text(&run.text, PLAIN, None),
                Inline::Break => paragraph.units.push(Unit::Break),
                Inline::Picture(picture) => paragraph.text(&picture.label(), PLAIN, None),
            }
        }
        let font = self.font;
        let advance = |c: char| font.advance(c) * setting.size;
        let lines = lines(&paragraph.units, &advance, MEASURE, setting.hanging);
        setting.before + lines.len() as f32 * setting.leading * setting.size + setting.after
    }

    /// A block, line by line. A heading — `outline` says at which level — is kept on the same
    /// page as the headings straight after it and the first lines after those, and noted for
    /// the outline.
    fn block(&mut self, setting: Setting, paragraph: &Paragraph, outline: Option<Outline>) {
        let size = setting.size;
        let leading = setting.leading * size;
        let font = self.font;
        let advance = |c: char| font.advance(c) * size;
        let lines = lines(&paragraph.units, &advance, MEASURE, setting.hanging);

        if !self.at_top() {
            self.y += setting.before;
        }
        if let Some(Outline { level, following }) = outline {
            let height: f32 = lines
                .iter()
                .map(|line| match line {
                    Line::Text(_) => leading,
                    Line::Picture(index) => self.layout.pictures[*index].size.1,
                })
                .sum();
            // Two lines of what follows, as a paragraph never starts with one line alone.
            self.room(height + setting.after + following + 2.0 * BODY.leading * BODY.size);
            self.layout.headings.push(Heading {
                level,
                title: paragraph
                    .units
                    .iter()
                    .filter_map(|unit| match unit {
                        Unit::Char(c, _) => Some(*c),
                        _ => None,
                    })
                    .collect::<String>()
                    .trim()
                    .to_string(),
                page: self.page(),
                top: self.y,
            });
        }

        // No line alone at the foot of a page or at the head of the next, as a word processor
        // keeps them by default: a paragraph of three lines or fewer moves whole, and a longer
        // one leaves at least two on each page. Pictures break a paragraph wherever they fall.
        let text_only = lines.iter().all(|line| matches!(line, Line::Text(_)));
        let count = lines.len();
        for (n, line) in lines.iter().enumerate() {
            let indent = if n == 0 { 0.0 } else { setting.hanging };
            match line {
                Line::Text(range) => {
                    let together = match (text_only, n) {
                        (false, _) => 1,
                        (true, 0) if count <= 3 => count,
                        (true, 0) => 2,
                        (true, n) if count >= 4 && n + 2 == count => 2,
                        _ => 1,
                    };
                    self.room(together as f32 * leading);
                    let content = (self.font.ascent() - self.font.descent()) * size;
                    let baseline = self.y + (leading - content) / 2.0 + self.font.ascent() * size;
                    self.text(
                        &paragraph.units[range.clone()],
                        paragraph,
                        setting,
                        indent,
                        baseline,
                    );
                    self.y += leading;
                }
                Line::Picture(index) => {
                    let (width, height) = self.layout.pictures[*index].size;
                    self.room(height);
                    let page = self.page();
                    self.layout.pages[page].push(Item::Picture {
                        x: MARGIN + indent,
                        top: self.y,
                        width,
                        height,
                        index: *index,
                    });
                    self.y += height;
                }
            }
        }
        self.y += setting.after;
    }

    /// One line of text, as runs of characters in one look each.
    fn text(
        &mut self,
        units: &[Unit],
        paragraph: &Paragraph,
        setting: Setting,
        indent: f32,
        baseline: f32,
    ) {
        let size = setting.size;
        // Laid out from the margin first, then moved over as a whole if it is centred.
        let mut runs: Vec<(f32, f32, usize, String)> = Vec::new();
        let mut x = indent;
        for unit in units {
            let Unit::Char(c, look) = *unit else { continue };
            if c == '\t' {
                x = next_tab(x);
                runs.push((x, 0.0, look, String::new()));
                continue;
            }
            let width = self.font.advance(c) * size;
            match runs.last_mut() {
                Some((_, run_width, run_look, text)) if *run_look == look => {
                    text.push(c);
                    *run_width += width;
                }
                _ => runs.push((x, width, look, c.to_string())),
            }
            x += width;
        }
        let shift = if setting.centred {
            MARGIN + (MEASURE - x) / 2.0
        } else {
            MARGIN
        };

        let page = self.page();
        for (x, width, look, text) in runs {
            if text.is_empty() {
                continue;
            }
            let (run_look, url) = &paragraph.looks[look];
            let look = Look {
                bold: setting.look.bold || run_look.bold,
                italic: setting.look.italic || run_look.italic,
                ..*run_look
            };
            if let Some(url) = url {
                self.layout.links.push(Link {
                    page,
                    left: shift + x,
                    top: baseline - self.font.ascent() * size,
                    right: shift + x + width,
                    bottom: baseline - self.font.descent() * size,
                    url: url.clone(),
                });
            }
            self.layout.pages[page].push(Item::Text {
                x: shift + x,
                baseline,
                width,
                size,
                look,
                text,
            });
        }
    }
}

/// Where a tab at `x` points from the margin moves the text to.
fn next_tab(x: f32) -> f32 {
    ((x / TAB).floor() + 1.0) * TAB
}

/// A paragraph's lines: where each starts and ends, and where its pictures go.
///
/// The first line starts at the margin and the rest `hanging` points in; none may be wider
/// than `measure`. Each character is as wide as `advance` says.
fn lines(units: &[Unit], advance: &dyn Fn(char) -> f32, measure: f32, hanging: f32) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    // A picture ends its line, so a new line asked for straight after one is no new line.
    let mut after_picture = false;
    'line: loop {
        let mut x = if lines.is_empty() { 0.0 } else { hanging };
        // The last place this line could end, and where the next would then start.
        let mut space: Option<(usize, usize)> = None;
        let mut i = start;
        loop {
            let Some(unit) = units.get(i) else {
                if !(after_picture && start == i) {
                    lines.push(Line::Text(start..trimmed(units, start, i)));
                }
                return lines;
            };
            match *unit {
                Unit::Break => {
                    if !(after_picture && start == i) {
                        lines.push(Line::Text(start..trimmed(units, start, i)));
                    }
                    after_picture = false;
                    start = i + 1;
                    continue 'line;
                }
                Unit::Picture(index) => {
                    if trimmed(units, start, i) > start {
                        lines.push(Line::Text(start..trimmed(units, start, i)));
                    }
                    lines.push(Line::Picture(index));
                    after_picture = true;
                    // The space between a picture and the words after it is where its line
                    // ended, as a space a line breaks at is.
                    start = i + 1;
                    while let Some(Unit::Char(' ', _)) = units.get(start) {
                        start += 1;
                    }
                    continue 'line;
                }
                Unit::Char(' ', _) => {
                    // Spaces at the very start of a paragraph are the writer's, and stay.
                    let end = i;
                    while let Some(Unit::Char(' ', _)) = units.get(i) {
                        x += advance(' ');
                        i += 1;
                    }
                    if end > start {
                        space = Some((end, i));
                    }
                }
                Unit::Char('\t', _) => {
                    let stop = next_tab(x);
                    if stop > measure && i > start {
                        lines.push(Line::Text(start..trimmed(units, start, i)));
                        after_picture = false;
                        start = i + 1;
                        continue 'line;
                    }
                    x = stop;
                    space = Some((i, i + 1));
                    i += 1;
                }
                Unit::Char(c, _) => {
                    let width = advance(c);
                    if x + width > measure + 0.001 && i > start {
                        let (end, next) = space.unwrap_or_else(|| {
                            let at = word_break(units, start, i);
                            (at, at)
                        });
                        lines.push(Line::Text(start..end));
                        after_picture = false;
                        start = next;
                        continue 'line;
                    }
                    x += width;
                    i += 1;
                }
            }
        }
    }
}

/// The end of a line's text with the spaces after it left off.
fn trimmed(units: &[Unit], start: usize, mut end: usize) -> usize {
    while end > start && matches!(units[end - 1], Unit::Char(' ', _)) {
        end -= 1;
    }
    end
}

/// Where to break a word too long for any line, when `overflow` is the first character that
/// does not fit: at the last place APA allows in a web address, or failing one, just before
/// the character that does not fit.
///
/// APA 7 (9.36): after a double slash or an ampersand, or before most other punctuation, and
/// with no hyphen added.
fn word_break(units: &[Unit], start: usize, overflow: usize) -> usize {
    let char_at = |i: usize| match units.get(i) {
        Some(Unit::Char(c, _)) => Some(*c),
        _ => None,
    };
    (start + 1..=overflow)
        .rev()
        .find(|&i| {
            let before = char_at(i - 1);
            let after_double_slash = before == Some('/') && i >= 2 && char_at(i - 2) == Some('/');
            let after_ampersand = before == Some('&');
            let before_punctuation = matches!(
                char_at(i),
                Some('.' | ',' | '-' | '?' | '_' | '/' | '#' | '=' | '~' | '%')
            ) && !matches!(before, Some('/' | ':'))
                && char_at(i + 1) != Some('/');
            after_double_slash || after_ampersand || before_punctuation
        })
        .unwrap_or(overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character one point wide, so the tests read in characters.
    fn one(_: char) -> f32 {
        1.0
    }

    fn units(text: &str) -> Vec<Unit> {
        text.chars()
            .map(|c| match c {
                '\n' => Unit::Break,
                '@' => Unit::Picture(0),
                c => Unit::Char(c, 0),
            })
            .collect()
    }

    /// The lines as text: a picture as `@`.
    fn broken(text: &str, measure: f32, hanging: f32) -> Vec<String> {
        let units = units(text);
        lines(&units, &one, measure, hanging)
            .into_iter()
            .map(|line| match line {
                Line::Text(range) => units[range]
                    .iter()
                    .map(|u| match u {
                        Unit::Char(c, _) => *c,
                        _ => '?',
                    })
                    .collect(),
                Line::Picture(_) => "@".to_string(),
            })
            .collect()
    }

    #[test]
    fn a_line_breaks_at_the_last_space_that_fits() {
        assert_eq!(broken("aaa bbb ccc", 7.0, 0.0), ["aaa bbb", "ccc"]);
        assert_eq!(broken("aaa bbb ccc", 11.0, 0.0), ["aaa bbb ccc"]);
        // The spaces a line breaks at go with the break, however many there are.
        assert_eq!(broken("aaa   bbb", 5.0, 0.0), ["aaa", "bbb"]);
        // Inside a line, they stay, as the editor shows them.
        assert_eq!(broken("a  b", 10.0, 0.0), ["a  b"]);
    }

    #[test]
    fn spaces_starting_a_paragraph_are_kept_and_spaces_ending_one_are_not() {
        assert_eq!(broken("  indented", 20.0, 0.0), ["  indented"]);
        assert_eq!(broken("end   \nnext  ", 20.0, 0.0), ["end", "next"]);
    }

    #[test]
    fn lines_after_the_first_hang() {
        // Ten wide, the rest four in: six left for them.
        assert_eq!(
            broken("aaaa bbbb cccc dddd", 10.0, 4.0),
            ["aaaa bbbb", "cccc", "dddd"]
        );
    }

    #[test]
    fn a_web_address_too_long_for_a_line_breaks_where_apa_allows() {
        assert_eq!(
            broken("https://doi.org/10.1093/jn/nxz001", 20.0, 0.0),
            ["https://doi.org/10", ".1093/jn/nxz001"]
        );
        // Never inside "://", but straight after it.
        assert_eq!(
            broken("https://example", 10.0, 0.0),
            ["https://", "example"]
        );
        // After an ampersand rather than before it.
        assert_eq!(broken("aaaa&bbbbbb", 8.0, 0.0), ["aaaa&", "bbbbbb"]);
    }

    #[test]
    fn a_word_with_nowhere_to_break_breaks_where_it_must() {
        assert_eq!(broken("abcdefghij", 4.0, 0.0), ["abcd", "efgh", "ij"]);
        // Even a line too narrow for one character holds one, so the text always moves on.
        assert_eq!(broken("ab", 0.5, 0.0), ["a", "b"]);
    }

    #[test]
    fn a_long_word_goes_to_the_next_line_before_it_is_broken() {
        assert_eq!(
            broken("see https://doi.org/10.1093/jn", 20.0, 0.0),
            ["see", "https://doi.org/10", ".1093/jn"]
        );
    }

    #[test]
    fn a_new_line_is_kept_and_a_picture_has_a_line_of_its_own() {
        assert_eq!(broken("one\ntwo", 20.0, 0.0), ["one", "two"]);
        assert_eq!(broken("one @ two", 20.0, 0.0), ["one", "@", "two"]);
        // A new line after a picture is the one the picture already made.
        assert_eq!(broken("one\n@\ntwo", 20.0, 0.0), ["one", "@", "two"]);
        assert_eq!(broken("@", 20.0, 0.0), ["@"]);
    }

    #[test]
    fn a_tab_moves_to_the_next_half_inch() {
        assert_eq!(next_tab(0.0), 36.0);
        assert_eq!(next_tab(35.0), 36.0);
        assert_eq!(next_tab(36.0), 72.0);
        // A tab past the end of the line starts a new one.
        assert_eq!(broken("aaaa\tb", 40.0, 0.0), ["aaaa\tb"]);
        assert_eq!(broken("aaaa\tb", 30.0, 0.0), ["aaaa", "b"]);
    }

    fn layout(export: &Export) -> Layout<'_> {
        lay_out(export, &Font::new().unwrap())
    }

    fn export(blocks: Vec<Block>) -> Export {
        Export {
            title: "Protein".into(),
            blocks,
            references: Vec::new(),
        }
    }

    fn paragraph(text: &str) -> Block {
        Block::Paragraph(vec![Inline::Text(Run::plain(text))])
    }

    fn texts(page: &[Item]) -> Vec<&str> {
        page.iter()
            .filter_map(|item| match item {
                Item::Text { text, .. } => Some(text.as_str()),
                Item::Picture { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_title_is_centred_and_bold() {
        let export = export(Vec::new());
        let layout = layout(&export);
        let Item::Text { x, width, look, .. } = &layout.pages[0][0] else {
            panic!("{:?}", layout.pages[0]);
        };
        assert!((x + width / 2.0 - PAGE_WIDTH / 2.0).abs() < 0.01);
        assert!(look.bold);
    }

    #[test]
    fn text_flows_on_to_a_new_page_when_one_is_full() {
        let words = "protein ".repeat(2000);
        let export = export(vec![paragraph(&words)]);
        let layout = layout(&export);
        assert!(layout.pages.len() > 1, "{}", layout.pages.len());
        for page in &layout.pages {
            for item in page {
                let Item::Text {
                    x, baseline, width, ..
                } = item
                else {
                    continue;
                };
                assert!(*x >= MARGIN - 0.001 && x + width <= PAGE_WIDTH - MARGIN + 0.001);
                assert!(*baseline > MARGIN && *baseline < BOTTOM);
            }
        }
    }

    #[test]
    fn a_heading_never_ends_a_page_alone() {
        // Wherever on the page a heading falls — alone, or with another straight after it —
        // what follows it is on the same page.
        for headings in [1, 2] {
            for before in 0..60 {
                let mut blocks: Vec<Block> = (0..before).map(|_| paragraph("Line.")).collect();
                blocks.push(Block::Heading {
                    level: 2,
                    content: vec![Inline::Text(Run::plain("Plant protein"))],
                });
                if headings == 2 {
                    blocks.push(Block::Heading {
                        level: 3,
                        content: vec![Inline::Text(Run::plain("Beans and peas"))],
                    });
                }
                blocks.push(paragraph("Beans."));
                let export = export(blocks);
                let layout = layout(&export);
                let heading = &layout.headings[0];
                let page = texts(&layout.pages[heading.page]);
                let at = page.iter().position(|t| *t == "Plant protein").unwrap();
                let expected: &[&str] = if headings == 2 {
                    &["Plant protein", "Beans and peas", "Beans."]
                } else {
                    &["Plant protein", "Beans."]
                };
                assert_eq!(
                    page.get(at..at + expected.len()),
                    Some(expected),
                    "{headings} heading(s) after {before} lines"
                );
                if heading.page > 0 && at == 0 {
                    assert_eq!(heading.top, MARGIN, "after {before} lines");
                }
            }
        }
    }

    #[test]
    fn a_paragraph_never_leaves_one_line_alone_on_a_page() {
        // A long paragraph after more and more short ones, so every line of it in turn is
        // where the page would break.
        let long = "protein ".repeat(60);
        let short = "protein ".repeat(25);
        for paragraph_text in [long.as_str(), short.as_str()] {
            for before in 0..40 {
                let mut blocks: Vec<Block> = (0..before).map(|_| paragraph("Line.")).collect();
                blocks.push(paragraph(paragraph_text));
                let export = export(blocks);
                let layout = layout(&export);
                let per_page: Vec<usize> = layout
                    .pages
                    .iter()
                    .map(|page| {
                        texts(page)
                            .iter()
                            .filter(|t| t.starts_with("protein"))
                            .count()
                    })
                    .filter(|&lines| lines > 0)
                    .collect();
                let total: usize = per_page.iter().sum();
                if total <= 3 {
                    assert_eq!(per_page.len(), 1, "after {before}: {per_page:?}");
                } else {
                    assert!(
                        per_page.iter().all(|&n| n >= 2),
                        "after {before}: {per_page:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn headings_are_kept_for_the_outline() {
        let export = Export {
            references: vec![vec![Run::plain("Kallio, K. (2019).")]],
            ..export(vec![
                Block::Heading {
                    level: 1,
                    content: vec![
                        Inline::Text(Run::plain("Kinds of ")),
                        Inline::Text(Run {
                            text: "protein".into(),
                            italic: true,
                        }),
                    ],
                },
                Block::Heading {
                    level: 2,
                    content: vec![Inline::Text(Run::plain("Plant protein"))],
                },
            ])
        };
        let layout = layout(&export);
        let headings: Vec<(u8, &str)> = layout
            .headings
            .iter()
            .map(|h| (h.level, h.title.as_str()))
            .collect();
        assert_eq!(
            headings,
            [
                (1, "Kinds of protein"),
                (2, "Plant protein"),
                (1, "References")
            ]
        );
    }

    #[test]
    fn a_reference_hangs_and_its_web_address_is_a_link() {
        let entry = vec![
            Run::plain("Kallio, K., & Virtanen, V. (2019). Protein intake in Finnish adults. "),
            Run {
                text: "Journal of Nutrition, 149".into(),
                italic: true,
            },
            Run::plain("(4), 600–612. https://doi.org/10.1093/jn/nxz001"),
        ];
        let export = Export {
            references: vec![entry],
            ..export(Vec::new())
        };
        let layout = layout(&export);
        let page = &layout.pages[0];
        let lefts: Vec<f32> = page
            .iter()
            .filter_map(|item| match item {
                Item::Text { x, baseline, .. } => Some((*x, *baseline)),
                _ => None,
            })
            .fold(Vec::new(), |mut lefts: Vec<(f32, f32)>, (x, baseline)| {
                // The leftmost run of each line.
                match lefts.last_mut() {
                    Some(last) if last.1 == baseline => last.0 = last.0.min(x),
                    _ => lefts.push((x, baseline)),
                }
                lefts
            })
            .into_iter()
            .map(|(x, _)| x)
            .collect();
        // Title, heading, then the entry's lines: the first at the margin, the rest in.
        assert_eq!(lefts[2], MARGIN);
        assert!(lefts.len() > 3, "{lefts:?}");
        assert!(
            lefts[3..].iter().all(|&x| x == MARGIN + HANGING),
            "{lefts:?}"
        );

        assert_eq!(layout.links.len(), 1);
        assert_eq!(layout.links[0].url, "https://doi.org/10.1093/jn/nxz001");
        let link = page.iter().find_map(|item| match item {
            Item::Text { look, text, .. } if look.link => Some(text.as_str()),
            _ => None,
        });
        assert_eq!(link, Some("https://doi.org/10.1093/jn/nxz001"));
        // The italics may well break across lines; together, they are the journal and volume.
        let italic: Vec<&str> = page
            .iter()
            .filter_map(|item| match item {
                Item::Text { look, text, .. } if look.italic => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(italic.join(" "), "Journal of Nutrition, 149");
    }
}
