//! Edition files, and dividing pasted text into rows — `parallel-text-model.md`, as pure
//! functions.
//!
//! Kept apart from everything that touches a disk, so every marker, every escape and every
//! unit is tested on strings alone.

use serde::{Deserialize, Serialize};

/// How pasted text is divided into rows. A property of the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    /// Every line break starts a row: songs and poems.
    Line,
    /// Every sentence end starts a row: prose.
    Sentence,
    /// Every blank line starts a row; the lines within a paragraph stay in it.
    Paragraph,
}

/// One edition's part of one row.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Cell {
    /// The row's lines. None means the row is deliberately empty in this edition (`~`).
    pub lines: Vec<String>,
}

impl Cell {
    pub fn empty() -> Self {
        Cell { lines: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Made of more than one line — written with `+ ` lines, and shown as joined.
    pub fn is_joined(&self) -> bool {
        self.lines.len() > 1
    }

    /// The row's text, its lines joined by line breaks.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// A row from text somebody typed: its lines, with blank lines at either end dropped. Text
    /// with nothing in it is an empty row.
    pub fn from_text(text: &str) -> Self {
        let lines: Vec<String> = text
            .replace("\r\n", "\n")
            .split('\n')
            .map(|l| l.trim_end().to_string())
            .collect();
        let start = lines.iter().position(|l| !l.trim().is_empty());
        let end = lines.iter().rposition(|l| !l.trim().is_empty());
        match (start, end) {
            (Some(s), Some(e)) => Cell {
                lines: lines[s..=e].to_vec(),
            },
            _ => Cell::empty(),
        }
    }
}

/// The rows an edition file holds.
///
/// A line starts a row; `+ ` (or a bare `+`) continues the one above; a lone `~` is an empty
/// row; blank lines are ignored; a backslash in front of something that would be a marker
/// makes it text. A `+ ` line with no row above it starts one, so nothing in a hand-edited
/// file is ever dropped.
pub fn parse(file: &str) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for raw in file.replace("\r\n", "\n").split('\n') {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        if line == "~" {
            cells.push(Cell::empty());
        } else if let Some(rest) = continuation(line) {
            match cells.last_mut() {
                Some(cell) => cell.lines.push(rest.to_string()),
                None => cells.push(Cell {
                    lines: vec![rest.to_string()],
                }),
            }
        } else {
            cells.push(Cell {
                lines: vec![unescape(line).to_string()],
            });
        }
    }
    cells
}

/// An edition file for these rows: the inverse of [`parse`].
pub fn render(cells: &[Cell]) -> String {
    let mut out = String::new();
    for cell in cells {
        match cell.lines.split_first() {
            None => out.push_str("~\n"),
            Some((first, rest)) => {
                out.push_str(&escape(first));
                out.push('\n');
                for line in rest {
                    if line.is_empty() {
                        out.push_str("+\n");
                    } else {
                        out.push_str("+ ");
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        }
    }
    out
}

/// The rest of a continuation line, or `None` if it is not one.
fn continuation(line: &str) -> Option<&str> {
    if line == "+" {
        Some("")
    } else {
        line.strip_prefix("+ ")
    }
}

/// Whether a line, read as the first of a row, would be taken for a marker — counting any
/// backslashes already in front of it, so escaping always round-trips.
fn looks_like_marker(line: &str) -> bool {
    let bare = line.trim_start_matches('\\');
    bare == "~" || bare == "+" || bare.starts_with("+ ")
}

fn escape(line: &str) -> String {
    if looks_like_marker(line) {
        format!("\\{line}")
    } else {
        line.to_string()
    }
}

fn unescape(line: &str) -> &str {
    if line.starts_with('\\') && looks_like_marker(line) {
        &line[1..]
    } else {
        line
    }
}

/// Pasted text, divided into rows by `unit`.
///
/// `language` is BCP 47; only its primary subtag matters, and only for sentences.
pub fn divide(text: &str, unit: Unit, language: &str) -> Vec<Cell> {
    let text = text.replace("\r\n", "\n");
    match unit {
        Unit::Line => text
            .split('\n')
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty())
            .map(|l| Cell {
                lines: vec![l.to_string()],
            })
            .collect(),
        Unit::Paragraph => paragraphs(&text)
            .into_iter()
            .map(|lines| Cell {
                lines: lines.into_iter().map(str::to_string).collect(),
            })
            .collect(),
        Unit::Sentence => {
            let abbreviations = abbreviations(language);
            paragraphs(&text)
                .into_iter()
                .flat_map(|lines| sentences(&lines.join("\n"), abbreviations))
                .collect()
        }
    }
}

/// Blocks of non-blank lines, separated by blank ones.
fn paragraphs(text: &str) -> Vec<Vec<&str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end();
        if line.trim().is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// One paragraph's sentences, each a row whose lines are the paragraph's line breaks inside it.
fn sentences(paragraph: &str, abbreviations: &[&str]) -> Vec<Cell> {
    let chars: Vec<char> = paragraph.chars().collect();
    let mut cells = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if !is_terminal(chars[i]) {
            i += 1;
            continue;
        }
        let word_start = chars[start..i]
            .iter()
            .rposition(|c| c.is_whitespace())
            .map(|p| start + p + 1)
            .unwrap_or(start);
        let mut end = i;
        while end < chars.len() && is_terminal(chars[end]) {
            end += 1;
        }
        while end < chars.len() && is_closing(chars[end]) {
            end += 1;
        }
        let mut next = end;
        while next < chars.len() && chars[next].is_whitespace() {
            next += 1;
        }
        let boundary = next > end
            && next < chars.len()
            && starts_sentence(chars[next])
            && !(chars[i] == '.' && is_exception(&chars[word_start..=i], abbreviations));
        if boundary {
            cells.push(lines_of(&chars[start..end]));
            start = next;
        }
        i = end.max(i + 1);
    }
    if start < chars.len() {
        let cell = lines_of(&chars[start..]);
        if !cell.is_empty() {
            cells.push(cell);
        }
    }
    cells
}

fn lines_of(chars: &[char]) -> Cell {
    let text: String = chars.iter().collect();
    Cell::from_text(
        &text
            .split('\n')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn is_terminal(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

fn is_closing(c: char) -> bool {
    matches!(c, '"' | '\'' | '”' | '’' | '»' | ')' | ']')
}

fn starts_sentence(c: char) -> bool {
    c.is_uppercase()
        || c.is_ascii_digit()
        || matches!(
            c,
            '"' | '\'' | '“' | '‘' | '«' | '„' | '(' | '—' | '–' | '-'
        )
}

/// A word ending in `.` that does not end a sentence: an abbreviation on the language's list,
/// or an initial.
fn is_exception(word: &[char], abbreviations: &[&str]) -> bool {
    let word: String = word.iter().collect();
    let word = word.trim_start_matches(['(', '"', '“', '«', '\'']);
    let letters: Vec<char> = word.trim_end_matches('.').chars().collect();
    if letters.len() == 1 && letters[0].is_uppercase() {
        return true;
    }
    let lower = word.to_lowercase();
    abbreviations.contains(&lower.as_str())
}

/// The abbreviations after which a `.` does not end a sentence, for a BCP 47 language.
fn abbreviations(language: &str) -> &'static [&'static str] {
    let primary = language
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match primary.as_str() {
        "fi" => &[
            "esim.", "mm.", "ns.", "ts.", "jne.", "yms.", "ym.", "vrt.", "ks.", "s.", "n.",
            "prof.", "tri.", "toim.", "huom.", "kpl.", "nro.", "puh.", "ap.", "ip.", "eaa.",
            "jaa.", "ekr.", "jkr.",
        ],
        "en" => &[
            "e.g.", "i.e.", "mr.", "mrs.", "ms.", "dr.", "prof.", "st.", "vs.", "no.", "vol.",
            "p.", "pp.", "ch.", "fig.", "approx.", "jr.", "sr.", "cf.",
        ],
        "ru" => &[
            "т.е.",
            "т.к.",
            "т.д.",
            "т.п.",
            "г.",
            "гг.",
            "в.",
            "вв.",
            "см.",
            "др.",
            "проф.",
            "им.",
            "ул.",
            "стр.",
            "с.",
            "т.",
            "и.о.",
            "напр.",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(lines: &[&str]) -> Cell {
        Cell {
            lines: lines.iter().map(|l| l.to_string()).collect(),
        }
    }

    #[test]
    fn markers_read_as_the_specification_says() {
        let file = "Я вас любил:\n~\n\nВ душе моей\n+ угасла не совсем;\n+\n+ end\n";
        assert_eq!(
            parse(file),
            vec![
                cell(&["Я вас любил:"]),
                Cell::empty(),
                cell(&["В душе моей", "угасла не совсем;", "", "end"]),
            ]
        );
    }

    #[test]
    fn a_continuation_with_nothing_above_starts_a_row() {
        assert_eq!(
            parse("+ orphan\nnext"),
            vec![cell(&["orphan"]), cell(&["next"])]
        );
    }

    #[test]
    fn text_that_looks_like_a_marker_round_trips() {
        let cells = vec![
            cell(&["+ not a continuation"]),
            cell(&["~"]),
            cell(&["+"]),
            cell(&["\\+ already one backslash"]),
            cell(&["\\~"]),
            cell(&["\\plain backslash"]),
            cell(&["first", "+ inside a row is just text", "~"]),
        ];
        let file = render(&cells);
        assert!(
            file.starts_with("\\+ not a continuation\n\\~\n\\+\n\\\\+ "),
            "{file}"
        );
        assert_eq!(parse(&file), cells);
    }

    #[test]
    fn windows_line_endings_are_read() {
        assert_eq!(
            parse("a\r\n+ b\r\n~\r\n"),
            vec![cell(&["a", "b"]), Cell::empty()]
        );
    }

    #[test]
    fn by_line_every_line_is_a_row() {
        let rows = divide("Я вас любил:\n\nлюбовь ещё,\n", Unit::Line, "ru");
        assert_eq!(rows, vec![cell(&["Я вас любил:"]), cell(&["любовь ещё,"])]);
    }

    #[test]
    fn by_paragraph_the_lines_stay_together() {
        let rows = divide("one\ntwo\n\n\nthree\n", Unit::Paragraph, "en");
        assert_eq!(rows, vec![cell(&["one", "two"]), cell(&["three"])]);
    }

    #[test]
    fn by_sentence_in_finnish_english_and_russian() {
        assert_eq!(
            divide(
                "Proteiini on tärkeää, esim. lihaksille. Se koostuu aminohapoista!",
                Unit::Sentence,
                "fi"
            ),
            vec![
                cell(&["Proteiini on tärkeää, esim. lihaksille."]),
                cell(&["Se koostuu aminohapoista!"])
            ]
        );
        assert_eq!(
            divide(
                "Dr. Smith wrote it, i.e. the book. J. R. R. Tolkien read it? \"Yes.\" Fine…",
                Unit::Sentence,
                "en-GB"
            ),
            vec![
                cell(&["Dr. Smith wrote it, i.e. the book."]),
                cell(&["J. R. R. Tolkien read it?"]),
                cell(&["\"Yes.\""]),
                cell(&["Fine…"]),
            ]
        );
        assert_eq!(
            divide(
                "Это было в 1825 г. в Москве. Т.е. давно. Он ушёл.",
                Unit::Sentence,
                "ru"
            ),
            vec![
                cell(&["Это было в 1825 г. в Москве."]),
                cell(&["Т.е. давно."]),
                cell(&["Он ушёл."]),
            ]
        );
    }

    #[test]
    fn a_sentence_over_two_lines_keeps_the_line_break() {
        assert_eq!(
            divide(
                "A long\nsentence. Next\none.\n\nNew paragraph",
                Unit::Sentence,
                "en"
            ),
            vec![
                cell(&["A long", "sentence."]),
                cell(&["Next", "one."]),
                cell(&["New paragraph"]),
            ]
        );
    }

    #[test]
    fn a_language_without_a_list_uses_punctuation_alone() {
        assert_eq!(
            divide("Hola. Adiós.", Unit::Sentence, "es"),
            vec![cell(&["Hola."]), cell(&["Adiós."])]
        );
        // Lowercase after a full stop is not a new sentence, in any language.
        assert_eq!(
            divide("etc. and so", Unit::Sentence, "es"),
            vec![cell(&["etc. and so"])]
        );
    }

    #[test]
    fn typed_text_drops_blank_lines_at_the_ends() {
        assert_eq!(
            Cell::from_text("\n  \nTwo\n\nlines\n\n"),
            cell(&["Two", "", "lines"])
        );
        assert!(Cell::from_text(" \n ").is_empty());
    }
}
