//! The `+++`-delimited TOML block at the top of every document.
//!
//! This is the part of a document a hand-editing user is told to leave alone, so the parser's
//! job is to fail loudly and specifically when they don't. A document that silently loses its
//! identity because the delimiter was mistyped is exactly the data-loss class the storage
//! format exists to avoid.

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Error, Result};

pub const DELIMITER: &str = "+++";

/// Split a file into its front matter block and the body beneath it.
pub fn split(text: &str, path_for_errors: &str) -> Result<(String, String)> {
    let mut lines = text.lines();

    match lines.next() {
        Some(first) if first.trim_end() == DELIMITER => {}
        Some(_) | None => {
            return Err(Error::library(format!(
                "{path_for_errors}: expected the file to begin with a `{DELIMITER}` line"
            )))
        }
    }

    let mut front = String::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == DELIMITER {
            closed = true;
            break;
        }
        front.push_str(line);
        front.push('\n');
    }

    if !closed {
        return Err(Error::library(format!(
            "{path_for_errors}: the `{DELIMITER}` block was opened but never closed"
        )));
    }

    let body = lines.collect::<Vec<_>>().join("\n");
    Ok((front, body.trim_start_matches('\n').to_string()))
}

/// Parse a file into its typed front matter and body.
pub fn parse<T: DeserializeOwned>(text: &str, path_for_errors: &str) -> Result<(T, String)> {
    let (front, body) = split(text, path_for_errors)?;
    let meta =
        toml::from_str(&front).map_err(|e| Error::library(format!("{path_for_errors}: {e}")))?;
    Ok((meta, body))
}

/// Render typed front matter and a body back into file contents.
pub fn render<T: Serialize>(meta: &T, body: &str) -> Result<String> {
    let front = toml::to_string_pretty(meta)
        .map_err(|e| Error::library(format!("cannot serialise front matter: {e}")))?;
    let body = body.trim_start_matches('\n').trim_end();
    Ok(format!("{DELIMITER}\n{front}{DELIMITER}\n\n{body}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Meta {
        title: String,
    }

    #[test]
    fn splits_front_matter_from_body() {
        let text = "+++\ntitle = \"Protein\"\n+++\n\nComplete proteins.\n";
        let (meta, body): (Meta, _) = parse(text, "note.md").unwrap();
        assert_eq!(meta.title, "Protein");
        assert_eq!(body, "Complete proteins.");
    }

    #[test]
    fn round_trips() {
        let meta = Meta {
            title: "Types of protein".into(),
        };
        let rendered = render(&meta, "Body text.").unwrap();
        let (back, body): (Meta, _) = parse(&rendered, "note.md").unwrap();
        assert_eq!(meta, back);
        assert_eq!(body, "Body text.");
    }

    #[test]
    fn keeps_blank_lines_and_markdown_inside_the_body() {
        let body = "# Heading\n\nOne.\n\n- a\n- b";
        let rendered = render(&Meta { title: "T".into() }, body).unwrap();
        let (_, back): (Meta, _) = parse(&rendered, "note.md").unwrap();
        assert_eq!(back, body);
    }

    #[test]
    fn a_body_containing_the_delimiter_survives() {
        // `+++` inside the body must not be mistaken for the closing delimiter, because the
        // closing one is found before the body begins.
        let body = "Some maths:\n\n+++\n\nand more.";
        let rendered = render(&Meta { title: "T".into() }, body).unwrap();
        let (_, back): (Meta, _) = parse(&rendered, "note.md").unwrap();
        assert_eq!(back, body);
    }

    #[test]
    fn a_missing_opening_delimiter_names_the_file() {
        let err = split("title = \"x\"\n", "topics/protein/note.md").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("topics/protein/note.md"), "{message}");
        assert!(message.contains(DELIMITER), "{message}");
    }

    #[test]
    fn an_unclosed_block_names_the_file() {
        let err = split("+++\ntitle = \"x\"\n", "note.md").unwrap_err();
        assert!(err.to_string().contains("never closed"), "{err}");
    }

    #[test]
    fn broken_toml_names_the_file_and_the_reason() {
        let err = parse::<Meta>("+++\ntitle = \n+++\n\nbody", "note.md").unwrap_err();
        assert!(
            err.to_string().starts_with("library error: note.md:"),
            "{err}"
        );
    }

    #[test]
    fn an_empty_file_is_diagnosed_rather_than_panicking() {
        assert!(split("", "note.md").is_err());
    }
}
