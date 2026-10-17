//! A web page: one file that opens in any browser, with its pictures inside it.
//!
//! Nothing is fetched from anywhere — no font, no stylesheet, no picture by address — so the
//! page reads the same with no network, years from now, attached to an email. Pictures are
//! carried as `data:` addresses; the style is a few lines in the page itself.
//!
//! The reference list has APA's hanging indent of half an inch, and its web addresses are
//! links. Web addresses in the writing are left as the person wrote them, as the editor shows
//! them.

use super::{base64, Block, Export, Inline, Run};

const STYLE: &str = "<style>
body { max-width: 42em; margin: 2em auto; padding: 0 1em; font-family: \"Times New Roman\", Times, serif; font-size: 12pt; line-height: 1.6; color: #111; background: #fff; }
h1, h2, h3, h4, h5, h6 { line-height: 1.3; }
h1.title { text-align: center; }
img { max-width: 100%; height: auto; }
.missing { color: #666; }
.references h1 { text-align: center; }
.references p { padding-left: 0.5in; text-indent: -0.5in; overflow-wrap: anywhere; }
</style>
";

pub fn write(export: &Export) -> String {
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(&export.title)));
    out.push_str(STYLE);
    out.push_str("</head>\n<body>\n");
    out.push_str(&format!(
        "<h1 class=\"title\">{}</h1>\n",
        escape(&export.title)
    ));

    for block in &export.blocks {
        match block {
            Block::Heading { level, content } => {
                out.push_str(&format!("<h{level}>{}</h{level}>\n", inline(content)));
            }
            Block::Paragraph(content) => {
                out.push_str(&format!("<p>{}</p>\n", inline(content)));
            }
        }
    }

    if !export.references.is_empty() {
        out.push_str("<section class=\"references\">\n<h1>References</h1>\n");
        for entry in &export.references {
            out.push_str(&format!("<p>{}</p>\n", reference(entry)));
        }
        out.push_str("</section>\n");
    }

    out.push_str("</body>\n</html>\n");
    out
}

fn inline(content: &[Inline]) -> String {
    content
        .iter()
        .map(|inline| match inline {
            Inline::Text(run) => run_html(run, escape(&run.text)),
            Inline::Break => "<br>\n".to_string(),
            Inline::Picture(picture) => match &picture.image {
                Some(image) => format!(
                    "<img src=\"data:{};base64,{}\" alt=\"{}\">",
                    image.kind.mime(),
                    base64(&image.bytes),
                    escape(&picture.alt)
                ),
                None => format!(
                    "<span class=\"missing\">{}</span>",
                    escape(&picture.label())
                ),
            },
        })
        .collect()
}

/// A reference entry, with its web addresses made into links.
fn reference(entry: &[Run]) -> String {
    entry
        .iter()
        .map(|run| run_html(run, linked(&run.text)))
        .collect()
}

fn run_html(run: &Run, html: String) -> String {
    if run.italic {
        format!("<em>{html}</em>")
    } else {
        html
    }
}

/// Text with every `http://` and `https://` address in it a link to itself.
fn linked(text: &str) -> String {
    let mut out = String::new();
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if word.starts_with("https://") || word.starts_with("http://") {
            let word = escape(word);
            out.push_str(&format!("<a href=\"{word}\">{word}</a>"));
        } else {
            out.push_str(&escape(word));
        }
    }
    out
}

/// Text made safe to put between tags or inside a quoted attribute.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{Image, ImageKind, Picture};

    fn page(blocks: Vec<Block>, references: Vec<Vec<Run>>) -> String {
        write(&Export {
            title: "Protein & <health>".into(),
            blocks,
            references,
        })
    }

    fn body(html: &str) -> &str {
        let start = html.find("</h1>\n").unwrap() + "</h1>\n".len();
        &html[start..html.find("</body>").unwrap()]
    }

    #[test]
    fn the_page_is_whole_and_its_title_is_escaped() {
        let html = page(vec![], vec![]);
        assert!(html.starts_with("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">"));
        assert!(html.contains("<title>Protein &amp; &lt;health&gt;</title>"));
        assert!(html.contains("<h1 class=\"title\">Protein &amp; &lt;health&gt;</h1>"));
        assert!(html.ends_with("</body>\n</html>\n"));
    }

    #[test]
    fn nothing_is_fetched_from_anywhere() {
        let html = page(vec![], vec![vec![Run::plain("https://example.org/a")]]);
        for outside in ["<link", "<script", "src=\"http", "@import", "url("] {
            assert!(!html.contains(outside), "{outside}");
        }
    }

    #[test]
    fn writing_is_escaped_and_italics_are_emphasis() {
        let html = page(
            vec![Block::Paragraph(vec![
                Inline::Text(Run::plain("5 < 6 & ")),
                Inline::Text(Run {
                    text: "Kalevala".into(),
                    italic: true,
                }),
                Inline::Break,
                Inline::Text(Run::plain("https://example.org")),
            ])],
            vec![],
        );
        assert_eq!(
            body(&html),
            "<p>5 &lt; 6 &amp; <em>Kalevala</em><br>\nhttps://example.org</p>\n"
        );
    }

    #[test]
    fn headings_keep_their_level() {
        let html = page(
            vec![Block::Heading {
                level: 4,
                content: vec![Inline::Text(Run::plain("Soy"))],
            }],
            vec![],
        );
        assert_eq!(body(&html), "<h4>Soy</h4>\n");
    }

    #[test]
    fn a_picture_travels_inside_the_page() {
        let html = page(
            vec![Block::Paragraph(vec![
                Inline::Picture(Picture {
                    alt: "Amino \"acids\"".into(),
                    name: "a.png".into(),
                    image: Some(Image {
                        kind: ImageKind::Png,
                        bytes: b"foo".to_vec(),
                    }),
                }),
                Inline::Picture(Picture {
                    alt: "Gone".into(),
                    name: "b.png".into(),
                    image: None,
                }),
            ])],
            vec![],
        );
        assert_eq!(
            body(&html),
            "<p><img src=\"data:image/png;base64,Zm9v\" alt=\"Amino &quot;acids&quot;\">\
             <span class=\"missing\">[Picture: Gone]</span></p>\n"
        );
    }

    #[test]
    fn references_hang_and_their_addresses_are_links() {
        let html = page(
            vec![],
            vec![vec![
                Run::plain("Nieminen, A. (2021). "),
                Run {
                    text: "Ravinto & terveys".into(),
                    italic: true,
                },
                Run::plain(". Otava. https://doi.org/10.1000/a<b"),
            ]],
        );
        assert!(html.contains(".references p { padding-left: 0.5in; text-indent: -0.5in;"));
        assert!(html.contains(
            "<section class=\"references\">\n<h1>References</h1>\n<p>Nieminen, A. (2021). \
             <em>Ravinto &amp; terveys</em>. Otava. \
             <a href=\"https://doi.org/10.1000/a&lt;b\">https://doi.org/10.1000/a&lt;b</a></p>\n\
             </section>\n"
        ));
    }

    #[test]
    fn nothing_cited_means_no_reference_list() {
        assert!(!page(vec![], vec![]).contains("references\""));
    }
}
