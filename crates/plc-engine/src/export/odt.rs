//! An OpenDocument text: LibreOffice's own format, and an open standard any office suite can
//! read without guessing.
//!
//! The same document as the Word export, in the same way: named styles do all the work — Title,
//! Heading 1 to 6, Text body, and a Bibliography style with APA's half-inch hanging indent — so
//! it edits like a document written in LibreOffice, and changing a style changes everything in
//! it. A4 pages with inch margins, Times New Roman at twelve points.

use super::package::{pack, Part};
use super::{xml_text, Block, Export, Inline, Run};

pub const MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

pub fn write(export: &Export) -> zip::result::ZipResult<Vec<u8>> {
    let mut pictures: Vec<(String, &'static str, Vec<u8>)> = Vec::new();
    let mut body = String::new();

    body.push_str("<text:p text:style-name=\"Title\">");
    spaced(&mut body, &export.title, &mut true);
    body.push_str("</text:p>");

    for block in &export.blocks {
        match block {
            Block::Heading { level, content } => {
                body.push_str(&format!(
                    "<text:h text:style-name=\"Heading_20_{level}\" text:outline-level=\"{level}\">"
                ));
                inline(&mut body, content, &mut pictures);
                body.push_str("</text:h>");
            }
            Block::Paragraph(content) => {
                body.push_str("<text:p text:style-name=\"Text_20_body\">");
                inline(&mut body, content, &mut pictures);
                body.push_str("</text:p>");
            }
        }
    }

    if !export.references.is_empty() {
        body.push_str(
            "<text:h text:style-name=\"References_20_Heading\" text:outline-level=\"1\">References</text:h>",
        );
        for entry in &export.references {
            body.push_str("<text:p text:style-name=\"Bibliography\">");
            let mut start = true;
            for run in entry {
                linked(&mut body, run, &mut start);
            }
            body.push_str("</text:p>");
        }
    }

    let content = format!(
        "{HEADER}<office:document-content {NAMESPACES} office:version=\"1.3\">\
         <office:automatic-styles>\
         <style:style style:name=\"T1\" style:family=\"text\"><style:text-properties \
         fo:font-style=\"italic\" style:font-style-asian=\"italic\" style:font-style-complex=\"italic\"/></style:style>\
         </office:automatic-styles>\
         <office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );

    let mut manifest = format!(
        "{HEADER}<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\">\
         <manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.3\" manifest:media-type=\"{MIMETYPE}\"/>\
         <manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
         <manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>\
         <manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>"
    );
    for (name, mime, _) in &pictures {
        manifest.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"{mime}\"/>"
        ));
    }
    manifest.push_str("</manifest:manifest>");

    // `mimetype` first and uncompressed: that is how a reader recognises the file from its
    // first bytes, and the standard requires it.
    let mut parts = vec![
        Part::stored("mimetype", MIMETYPE.as_bytes().to_vec()),
        Part::xml("META-INF/manifest.xml", manifest),
        Part::xml("content.xml", content),
        Part::xml("styles.xml", STYLES.to_string()),
        Part::xml("meta.xml", meta(&export.title)),
    ];
    for (name, _, bytes) in pictures {
        parts.push(Part::stored(&name, bytes));
    }
    pack(&parts)
}

const HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

const NAMESPACES: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
     xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" \
     xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
     xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" \
     xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" \
     xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
     xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\"";

fn inline(
    out: &mut String,
    content: &[Inline],
    pictures: &mut Vec<(String, &'static str, Vec<u8>)>,
) {
    // Whether the next space would be one OpenDocument drops: at the start of the paragraph,
    // or straight after another space.
    let mut start = true;
    for inline in content {
        match inline {
            Inline::Text(run) => styled(out, run, &mut start),
            Inline::Break => {
                out.push_str("<text:line-break/>");
                start = true;
            }
            Inline::Picture(picture) => match picture.placed() {
                Some(placed) => {
                    let n = pictures.len() + 1;
                    let kind = placed.image.kind;
                    let name = format!("Pictures/{n}.{}", kind.extension());
                    let (w, h) = placed.size;
                    out.push_str(&format!(
                        "<draw:frame draw:name=\"Picture {n}\" text:anchor-type=\"as-char\" \
                         svg:width=\"{w:.3}in\" svg:height=\"{h:.3}in\" draw:z-index=\"{z}\">\
                         <draw:image xlink:href=\"{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>\
                         <svg:title>{alt}</svg:title></draw:frame>",
                        z = n - 1,
                        alt = xml_text(&picture.alt),
                    ));
                    pictures.push((name, kind.mime(), placed.image.bytes.clone()));
                    start = false;
                }
                None => styled(out, &Run::plain(picture.label()), &mut start),
            },
        }
    }
}

fn styled(out: &mut String, run: &Run, start: &mut bool) {
    if run.italic {
        out.push_str("<text:span text:style-name=\"T1\">");
        spaced(out, &run.text, start);
        out.push_str("</text:span>");
    } else {
        spaced(out, &run.text, start);
    }
}

/// Text with its spacing kept. OpenDocument collapses a run of spaces to one and drops a
/// paragraph's leading space, as HTML does, unless they are written as `<text:s/>`; a tab is
/// an element of its own.
fn spaced(out: &mut String, text: &str, start: &mut bool) {
    let mut pending = 0;
    let flush = |out: &mut String, pending: &mut usize| {
        if *pending > 0 {
            out.push_str(&if *pending == 1 {
                "<text:s/>".to_string()
            } else {
                format!("<text:s text:c=\"{pending}\"/>")
            });
            *pending = 0;
        }
    };
    for c in text.chars() {
        match c {
            ' ' if *start => pending += 1,
            ' ' => {
                out.push(' ');
                *start = true;
            }
            '\t' => {
                flush(out, &mut pending);
                out.push_str("<text:tab/>");
                *start = false;
            }
            c => {
                flush(out, &mut pending);
                out.push_str(&xml_text(c.encode_utf8(&mut [0; 4])));
                *start = false;
            }
        }
    }
    flush(out, &mut pending);
}

/// A reference entry's run, with each web address in it a link.
fn linked(out: &mut String, run: &Run, start: &mut bool) {
    if run.italic {
        styled(out, run, start);
        return;
    }
    for (i, word) in run.text.split(' ').enumerate() {
        if i > 0 {
            spaced(out, " ", start);
        }
        if word.starts_with("https://") || word.starts_with("http://") {
            out.push_str(&format!(
                "<text:a xlink:type=\"simple\" xlink:href=\"{href}\">",
                href = xml_text(word)
            ));
            spaced(out, word, start);
            out.push_str("</text:a>");
        } else {
            spaced(out, word, start);
        }
    }
}

fn meta(title: &str) -> String {
    format!(
        "{HEADER}<office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" office:version=\"1.3\">\
         <office:meta><meta:generator>Personal Learning Corner</meta:generator>\
         <dc:title>{}</dc:title></office:meta></office:document-meta>",
        xml_text(title)
    )
}

const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<office:document-styles xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" \
xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" \
xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" office:version=\"1.3\">\
<office:font-face-decls><style:font-face style:name=\"Times New Roman\" svg:font-family=\"&apos;Times New Roman&apos;\" \
style:font-family-generic=\"roman\" style:font-pitch=\"variable\"/></office:font-face-decls>\
<office:styles>\
<style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:line-height=\"150%\"/>\
<style:text-properties style:font-name=\"Times New Roman\" fo:font-size=\"12pt\" \
style:font-name-asian=\"Times New Roman\" style:font-size-asian=\"12pt\" \
style:font-name-complex=\"Times New Roman\" style:font-size-complex=\"12pt\"/></style:default-style>\
<style:style style:name=\"Standard\" style:family=\"paragraph\" style:class=\"text\"/>\
<style:style style:name=\"Text_20_body\" style:display-name=\"Text body\" style:family=\"paragraph\" \
style:parent-style-name=\"Standard\" style:class=\"text\"><style:paragraph-properties fo:margin-top=\"0in\" fo:margin-bottom=\"0.111in\"/></style:style>\
<style:style style:name=\"Title\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" \
style:next-style-name=\"Text_20_body\" style:class=\"chapter\"><style:paragraph-properties fo:text-align=\"center\" fo:margin-bottom=\"0.25in\"/>\
<style:text-properties fo:font-size=\"18pt\" fo:font-weight=\"bold\" style:font-weight-asian=\"bold\" style:font-weight-complex=\"bold\"/></style:style>\
<style:style style:name=\"Heading\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" \
style:next-style-name=\"Text_20_body\" style:class=\"text\"><style:paragraph-properties fo:margin-top=\"0.25in\" \
fo:margin-bottom=\"0.083in\" fo:keep-with-next=\"always\"/><style:text-properties fo:font-weight=\"bold\" \
style:font-weight-asian=\"bold\" style:font-weight-complex=\"bold\"/></style:style>\
<style:style style:name=\"Heading_20_1\" style:display-name=\"Heading 1\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" \
style:next-style-name=\"Text_20_body\" style:default-outline-level=\"1\" style:class=\"text\"><style:text-properties fo:font-size=\"16pt\"/></style:style>\
<style:style style:name=\"Heading_20_2\" style:display-name=\"Heading 2\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" \
style:next-style-name=\"Text_20_body\" style:default-outline-level=\"2\" style:class=\"text\"><style:text-properties fo:font-size=\"14pt\"/></style:style>\
<style:style style:name=\"Heading_20_3\" style:display-name=\"Heading 3\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" \
style:next-style-name=\"Text_20_body\" style:default-outline-level=\"3\" style:class=\"text\"><style:text-properties fo:font-size=\"13pt\"/></style:style>\
<style:style style:name=\"Heading_20_4\" style:display-name=\"Heading 4\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" \
style:next-style-name=\"Text_20_body\" style:default-outline-level=\"4\" style:class=\"text\"><style:text-properties fo:font-style=\"italic\" \
style:font-style-asian=\"italic\" style:font-style-complex=\"italic\"/></style:style>\
<style:style style:name=\"Heading_20_5\" style:display-name=\"Heading 5\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" \
style:next-style-name=\"Text_20_body\" style:default-outline-level=\"5\" style:class=\"text\"/>\
<style:style style:name=\"Heading_20_6\" style:display-name=\"Heading 6\" style:family=\"paragraph\" style:parent-style-name=\"Heading\" \
style:next-style-name=\"Text_20_body\" style:default-outline-level=\"6\" style:class=\"text\"><style:text-properties fo:font-weight=\"normal\" \
style:font-weight-asian=\"normal\" style:font-weight-complex=\"normal\" fo:font-style=\"italic\" style:font-style-asian=\"italic\" style:font-style-complex=\"italic\"/></style:style>\
<style:style style:name=\"References_20_Heading\" style:display-name=\"References Heading\" style:family=\"paragraph\" \
style:parent-style-name=\"Heading_20_1\" style:next-style-name=\"Bibliography\" style:class=\"text\"><style:paragraph-properties fo:text-align=\"center\"/></style:style>\
<style:style style:name=\"Bibliography\" style:family=\"paragraph\" style:parent-style-name=\"Text_20_body\" style:class=\"text\">\
<style:paragraph-properties fo:margin-left=\"0.5in\" fo:text-indent=\"-0.5in\"/></style:style>\
</office:styles>\
<office:automatic-styles><style:page-layout style:name=\"Page\"><style:page-layout-properties fo:page-width=\"8.268in\" \
fo:page-height=\"11.693in\" style:print-orientation=\"portrait\" fo:margin-top=\"1in\" fo:margin-bottom=\"1in\" \
fo:margin-left=\"1in\" fo:margin-right=\"1in\"/></style:page-layout></office:automatic-styles>\
<office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"Page\"/></office:master-styles>\
</office:document-styles>";

#[cfg(test)]
mod tests {
    use super::*;

    fn spaced_text(text: &str) -> String {
        let mut out = String::new();
        spaced(&mut out, text, &mut true);
        out
    }

    #[test]
    fn spaces_survive_as_written() {
        // OpenDocument collapses a run of spaces and drops a leading one, unless they are
        // written as elements.
        assert_eq!(spaced_text("one two"), "one two");
        assert_eq!(spaced_text("two  spaces"), "two <text:s/>spaces");
        assert_eq!(
            spaced_text("four    spaces"),
            "four <text:s text:c=\"3\"/>spaces"
        );
        assert_eq!(spaced_text(" leading"), "<text:s/>leading");
        assert_eq!(spaced_text("a\tb"), "a<text:tab/>b");
        assert_eq!(spaced_text("5 < 6 & ä"), "5 &lt; 6 &amp; ä");
    }

    #[test]
    fn a_space_after_another_run_is_still_counted() {
        let mut out = String::new();
        let mut start = true;
        spaced(&mut out, "end ", &mut start);
        spaced(&mut out, " next", &mut start);
        assert_eq!(out, "end <text:s/>next");
    }
}
