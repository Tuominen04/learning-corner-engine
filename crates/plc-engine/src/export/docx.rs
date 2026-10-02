//! A Word document, for whoever goes on editing it — in Word, or in anything that opens Word's
//! files.
//!
//! The document's own styles do the work, never formatting applied by hand, so it edits like a
//! document written in Word: the headings are Word's Heading 1 to 6 and appear in its
//! navigation pane, the title is its Title, and the reference list is its Bibliography style,
//! which carries APA's half-inch hanging indent. Change a style and everything in it follows.
//!
//! Pages are A4 with inch margins, set in Times New Roman at twelve points.

use super::package::{pack, Part};
use super::{xml_text, Block, Export, Inline, Run};

/// Everything the body points at outside itself: pictures and web addresses, each with the
/// relationship that names it.
#[derive(Default)]
struct Outside {
    pictures: Vec<(String, Vec<u8>)>,
    links: Vec<String>,
}

impl Outside {
    fn picture(&mut self, extension: &str, bytes: &[u8]) -> (usize, String) {
        let n = self.pictures.len() + 1;
        self.pictures
            .push((format!("media/image{n}.{extension}"), bytes.to_vec()));
        (n, format!("rIdPicture{n}"))
    }

    fn link(&mut self, url: &str) -> String {
        self.links.push(url.to_string());
        format!("rIdLink{}", self.links.len())
    }
}

pub fn write(export: &Export) -> zip::result::ZipResult<Vec<u8>> {
    let mut outside = Outside::default();
    let mut body = String::new();

    paragraph(
        &mut body,
        Some("Title"),
        &[Inline::Text(Run::plain(export.title.clone()))],
        &mut outside,
    );
    for block in &export.blocks {
        match block {
            Block::Heading { level, content } => paragraph(
                &mut body,
                Some(&format!("Heading{level}")),
                content,
                &mut outside,
            ),
            Block::Paragraph(content) => paragraph(&mut body, None, content, &mut outside),
        }
    }
    if !export.references.is_empty() {
        paragraph(
            &mut body,
            Some("ReferencesHeading"),
            &[Inline::Text(Run::plain("References"))],
            &mut outside,
        );
        for entry in &export.references {
            body.push_str("<w:p><w:pPr><w:pStyle w:val=\"Bibliography\"/></w:pPr>");
            for run in entry {
                linked_runs(&mut body, run, &mut outside);
            }
            body.push_str("</w:p>");
        }
    }

    let document = format!(
        "{HEADER}<w:document {NAMESPACES}><w:body>{body}\
         <w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" \
         w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr>\
         </w:body></w:document>"
    );

    let mut parts = vec![
        Part::xml("[Content_Types].xml", content_types()),
        Part::xml("_rels/.rels", ROOT_RELATIONSHIPS.to_string()),
        Part::xml("word/document.xml", document),
        Part::xml("word/styles.xml", STYLES.to_string()),
        Part::xml("word/_rels/document.xml.rels", relationships(&outside)),
        Part::xml("docProps/core.xml", core(&export.title)),
    ];
    for (name, bytes) in outside.pictures {
        parts.push(Part::stored(&format!("word/{name}"), bytes));
    }
    pack(&parts)
}

const HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

const NAMESPACES: &str =
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
     xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
     xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
     xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
     xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"";

fn paragraph(out: &mut String, style: Option<&str>, content: &[Inline], outside: &mut Outside) {
    out.push_str("<w:p>");
    if let Some(style) = style {
        out.push_str(&format!("<w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr>"));
    }
    for inline in content {
        match inline {
            Inline::Text(run) => text_run(out, &run.text, run.italic, None),
            Inline::Break => out.push_str("<w:r><w:br/></w:r>"),
            Inline::Picture(picture) => match picture.placed() {
                Some(placed) => {
                    let (n, id) =
                        outside.picture(placed.image.kind.extension(), &placed.image.bytes);
                    out.push_str(&drawing(n, &id, &picture.alt, placed.size));
                }
                None => text_run(out, &picture.label(), false, None),
            },
        }
    }
    out.push_str("</w:p>");
}

/// A run of text. Tabs are elements of their own in Word, not characters in the text.
fn text_run(out: &mut String, text: &str, italic: bool, style: Option<&str>) {
    if text.is_empty() {
        return;
    }
    out.push_str("<w:r>");
    if italic || style.is_some() {
        out.push_str("<w:rPr>");
        if let Some(style) = style {
            out.push_str(&format!("<w:rStyle w:val=\"{style}\"/>"));
        }
        if italic {
            out.push_str("<w:i/><w:iCs/>");
        }
        out.push_str("</w:rPr>");
    }
    for (i, piece) in text.split('\t').enumerate() {
        if i > 0 {
            out.push_str("<w:tab/>");
        }
        if !piece.is_empty() {
            out.push_str(&format!(
                "<w:t xml:space=\"preserve\">{}</w:t>",
                xml_text(piece)
            ));
        }
    }
    out.push_str("</w:r>");
}

/// A reference entry's run, with each web address in it a link — as APA 7 allows, and as
/// the web page export has them.
fn linked_runs(out: &mut String, run: &Run, outside: &mut Outside) {
    if run.italic {
        text_run(out, &run.text, true, None);
        return;
    }
    let mut plain = String::new();
    for (i, word) in run.text.split(' ').enumerate() {
        if i > 0 {
            plain.push(' ');
        }
        if word.starts_with("https://") || word.starts_with("http://") {
            text_run(out, &std::mem::take(&mut plain), false, None);
            let id = outside.link(word);
            out.push_str(&format!("<w:hyperlink r:id=\"{id}\" w:history=\"1\">"));
            text_run(out, word, false, Some("Hyperlink"));
            out.push_str("</w:hyperlink>");
        } else {
            plain.push_str(word);
        }
    }
    text_run(out, &plain, false, None);
}

/// A picture inline in the text, at its size on the page.
fn drawing(n: usize, id: &str, alt: &str, (width, height): (f64, f64)) -> String {
    // English Metric Units: 914 400 to the inch.
    let cx = (width * 914_400.0).round() as u64;
    let cy = (height * 914_400.0).round() as u64;
    let alt = xml_text(alt);
    format!(
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
         <wp:docPr id=\"{n}\" name=\"Picture {n}\" descr=\"{alt}\" title=\"{alt}\"/>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"Picture {n}\" descr=\"{alt}\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
         </a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )
}

fn content_types() -> String {
    format!(
        "{HEADER}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Default Extension=\"png\" ContentType=\"image/png\"/>\
         <Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>\
         <Default Extension=\"gif\" ContentType=\"image/gif\"/>\
         <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
         <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
         <Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
         </Types>"
    )
}

const ROOT_RELATIONSHIPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
     <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
     <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
     <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>\
     </Relationships>";

fn relationships(outside: &Outside) -> String {
    let mut out = format!(
        "{HEADER}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rIdStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>"
    );
    for (i, (name, _)) in outside.pictures.iter().enumerate() {
        out.push_str(&format!(
            "<Relationship Id=\"rIdPicture{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{name}\"/>",
            i + 1
        ));
    }
    for (i, url) in outside.links.iter().enumerate() {
        out.push_str(&format!(
            "<Relationship Id=\"rIdLink{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
            i + 1,
            xml_text(url)
        ));
    }
    out.push_str("</Relationships>");
    out
}

fn core(title: &str) -> String {
    format!(
        "{HEADER}<cp:coreProperties \
         xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:title>{}</dc:title></cp:coreProperties>",
        xml_text(title)
    )
}

/// The styles everything in the document is set in. Heading names are Word's own, written in
/// lower case as Word writes them, so Word treats them as its built-in headings.
const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults>\
<w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\" w:eastAsia=\"Times New Roman\" w:cs=\"Times New Roman\"/>\
<w:sz w:val=\"24\"/><w:szCs w:val=\"24\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"360\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault>\
</w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"><w:name w:val=\"Default Paragraph Font\"/><w:uiPriority w:val=\"1\"/><w:semiHidden/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:jc w:val=\"center\"/><w:spacing w:after=\"360\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val=\"36\"/><w:szCs w:val=\"36\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"360\" w:after=\"120\"/><w:outlineLvl w:val=\"0\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val=\"32\"/><w:szCs w:val=\"32\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val=\"28\"/><w:szCs w:val=\"28\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading3\"><w:name w:val=\"heading 3\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"2\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val=\"26\"/><w:szCs w:val=\"26\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading4\"><w:name w:val=\"heading 4\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"3\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:i/><w:iCs/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading5\"><w:name w:val=\"heading 5\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"4\"/></w:pPr><w:rPr><w:b/><w:bCs/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading6\"><w:name w:val=\"heading 6\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"5\"/></w:pPr><w:rPr><w:i/><w:iCs/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"ReferencesHeading\"><w:name w:val=\"References Heading\"/><w:basedOn w:val=\"Heading1\"/><w:next w:val=\"Bibliography\"/>\
<w:pPr><w:jc w:val=\"center\"/></w:pPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Bibliography\"><w:name w:val=\"Bibliography\"/><w:basedOn w:val=\"Normal\"/>\
<w:pPr><w:ind w:left=\"720\" w:hanging=\"720\"/></w:pPr></w:style>\
<w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/><w:basedOn w:val=\"DefaultParagraphFont\"/>\
<w:rPr><w:color w:val=\"0563C1\"/><w:u w:val=\"single\"/></w:rPr></w:style>\
</w:styles>";

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, italic: bool) -> String {
        let mut out = String::new();
        text_run(&mut out, text, italic, None);
        out
    }

    #[test]
    fn a_run_keeps_its_spaces_and_its_tabs() {
        assert_eq!(
            run("two  spaces", false),
            "<w:r><w:t xml:space=\"preserve\">two  spaces</w:t></w:r>"
        );
        assert_eq!(
            run("a\tb", false),
            "<w:r><w:t xml:space=\"preserve\">a</w:t><w:tab/><w:t xml:space=\"preserve\">b</w:t></w:r>"
        );
        assert_eq!(
            run("Kalevala", true),
            "<w:r><w:rPr><w:i/><w:iCs/></w:rPr><w:t xml:space=\"preserve\">Kalevala</w:t></w:r>"
        );
        assert_eq!(run("", false), "");
    }

    #[test]
    fn a_web_address_in_a_reference_is_a_link_and_the_rest_is_not() {
        let mut out = String::new();
        let mut outside = Outside::default();
        linked_runs(
            &mut out,
            &Run::plain("Otava. https://doi.org/10.1/a then more"),
            &mut outside,
        );
        assert_eq!(outside.links, ["https://doi.org/10.1/a"]);
        assert_eq!(
            out,
            "<w:r><w:t xml:space=\"preserve\">Otava. </w:t></w:r>\
             <w:hyperlink r:id=\"rIdLink1\" w:history=\"1\"><w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr>\
             <w:t xml:space=\"preserve\">https://doi.org/10.1/a</w:t></w:r></w:hyperlink>\
             <w:r><w:t xml:space=\"preserve\"> then more</w:t></w:r>"
        );
    }
}
