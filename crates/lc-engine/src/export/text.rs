//! Plain text: the document as it reads with nothing but letters to show it.
//!
//! The title and the top two levels of heading are underlined, the way plain text has always
//! marked them. Paragraphs are left on one line each, so the text pastes into anything without
//! carrying line breaks it does not mean. The reference list is the exception: it is wrapped at
//! 72 columns with a hanging indent of four spaces, because the indent is how a reader tells one
//! entry from the next, and an indent needs lines to hang from.
//!
//! Italics are dropped. Plain text has no way to show them that is not also a way of writing
//! asterisks.

use super::{Block, Export, Inline, Run};

/// The width the reference list is wrapped to.
pub const WIDTH: usize = 72;
/// How far the second and later lines of a reference are indented.
pub const INDENT: usize = 4;

pub fn write(export: &Export) -> String {
    let mut out = String::new();
    underlined(&mut out, &export.title, '=');

    for block in &export.blocks {
        out.push('\n');
        match block {
            Block::Heading { level, content } => {
                let text = flatten(content);
                match level {
                    1 => underlined(&mut out, &text, '='),
                    2 => underlined(&mut out, &text, '-'),
                    _ => {
                        out.push_str(&text);
                        out.push('\n');
                    }
                }
            }
            Block::Paragraph(content) => {
                out.push_str(&flatten(content));
                out.push('\n');
            }
        }
    }

    if !export.references.is_empty() {
        out.push('\n');
        underlined(&mut out, "References", '=');
        for entry in &export.references {
            out.push('\n');
            out.push_str(&hanging(&words(entry)));
        }
    }
    out
}

fn underlined(out: &mut String, text: &str, mark: char) {
    out.push_str(text);
    out.push('\n');
    out.extend(std::iter::repeat_n(mark, text.chars().count()));
    out.push('\n');
}

/// Runs and breaks as text, and a picture as what it shows.
fn flatten(content: &[Inline]) -> String {
    content
        .iter()
        .map(|inline| match inline {
            Inline::Text(run) => run.text.clone(),
            Inline::Break => "\n".to_string(),
            Inline::Picture(picture) => picture.label(),
        })
        .collect()
}

fn words(entry: &[Run]) -> String {
    entry.iter().map(|run| run.text.as_str()).collect()
}

/// Wrapped to [`WIDTH`], every line after the first indented by [`INDENT`].
///
/// Lines break only at an ordinary space, so a non-breaking one typed into a title keeps its
/// two words together. A word longer than a line — a web address, usually — gets a line of its
/// own and is never cut.
fn hanging(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut width = 0;

    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let len = word.chars().count();
        let room = if lines.is_empty() {
            WIDTH
        } else {
            WIDTH - INDENT
        };
        if width > 0 && width + 1 + len > room {
            lines.push(std::mem::take(&mut line));
            width = 0;
        }
        if width > 0 {
            line.push(' ');
            width += 1;
        }
        line.push_str(word);
        width += len;
    }
    if !line.is_empty() {
        lines.push(line);
    }

    let indent = " ".repeat(INDENT);
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push_str(&indent);
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{Picture, Run};

    fn export(blocks: Vec<Block>, references: Vec<Vec<Run>>) -> Export {
        Export {
            title: "Protein".into(),
            blocks,
            references,
        }
    }

    fn text(s: &str) -> Inline {
        Inline::Text(Run::plain(s))
    }

    #[test]
    fn the_title_and_the_top_two_heading_levels_are_underlined() {
        let written = write(&export(
            vec![
                Block::Heading {
                    level: 1,
                    content: vec![text("Kinds")],
                },
                Block::Heading {
                    level: 2,
                    content: vec![text("Plant")],
                },
                Block::Heading {
                    level: 3,
                    content: vec![text("Soy")],
                },
            ],
            vec![],
        ));
        assert_eq!(
            written,
            "Protein\n=======\n\nKinds\n=====\n\nPlant\n-----\n\nSoy\n"
        );
    }

    #[test]
    fn an_underline_is_as_long_as_the_letters_not_the_bytes() {
        let written = write(&Export {
            title: "Белок".into(),
            blocks: vec![],
            references: vec![],
        });
        assert_eq!(written, "Белок\n=====\n");
    }

    #[test]
    fn italics_are_dropped_and_a_line_break_is_kept() {
        let written = write(&export(
            vec![Block::Paragraph(vec![
                text("As "),
                Inline::Text(Run {
                    text: "Kalevala".into(),
                    italic: true,
                }),
                text(" says,"),
                Inline::Break,
                text("next line."),
            ])],
            vec![],
        ));
        assert!(
            written.ends_with("\nAs Kalevala says,\nnext line.\n"),
            "{written}"
        );
    }

    #[test]
    fn a_picture_is_its_description() {
        let picture = |alt: &str| {
            Inline::Picture(Picture {
                alt: alt.into(),
                name: "amino-acids.png".into(),
                image: None,
            })
        };
        let written = write(&export(
            vec![
                Block::Paragraph(vec![picture("Amino acids")]),
                Block::Paragraph(vec![picture(" ")]),
            ],
            vec![],
        ));
        assert!(
            written.ends_with("\n[Picture: Amino acids]\n\n[Picture: amino-acids.png]\n"),
            "{written}"
        );
    }

    #[test]
    fn references_hang_from_their_first_line_inside_72_columns() {
        let entry =
            "Grady, J. S., Her, M., Moreno, G., Perez, C., & Yelinek, J. (2019). Emotions in \
                     storybooks: A comparison of storybooks that represent ethnic and racial \
                     groups in the United States. Psychology of Popular Media Culture, 8(3), \
                     207–217. https://doi.org/10.1037/ppm0000185";
        let written = write(&export(vec![], vec![vec![Run::plain(entry)]]));
        let list = written.split("==========\n\n").nth(1).unwrap();
        assert_eq!(
            list,
            "Grady, J. S., Her, M., Moreno, G., Perez, C., & Yelinek, J. (2019).\n    \
             Emotions in storybooks: A comparison of storybooks that represent\n    \
             ethnic and racial groups in the United States. Psychology of Popular\n    \
             Media Culture, 8(3), 207–217. https://doi.org/10.1037/ppm0000185\n"
        );
        for line in list.lines() {
            assert!(line.chars().count() <= WIDTH, "{line:?}");
        }
    }

    #[test]
    fn a_word_longer_than_a_line_is_never_cut() {
        let url = format!("https://example.org/{}", "a".repeat(80));
        let written = hanging(&format!("Author, A. (2020). Title. {url}"));
        assert_eq!(written, format!("Author, A. (2020). Title.\n    {url}\n"));
    }

    #[test]
    fn a_non_breaking_space_keeps_its_words_together() {
        // Fourteen words fill 69 columns: "12" alone would just fit, "12 345" does not.
        let entry = format!("{} 12\u{a0}345", "word ".repeat(14));
        let written = hanging(&entry);
        assert!(written.contains("\n    12\u{a0}345\n"), "{written:?}");
    }

    #[test]
    fn nothing_cited_means_no_reference_list() {
        let written = write(&export(vec![Block::Paragraph(vec![text("Mine.")])], vec![]));
        assert!(!written.contains("References"));
    }
}
