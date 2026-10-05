//! Exporting a document, through a real library on disk.
//!
//! Plain text and the web page are compared **exactly** against files in `tests/export/`, which a
//! person has read: a change to what an export says must change one of those files, in a diff
//! somebody reviews. So is the PDF, read back out of the file line by line as a reader draws it
//! (`protein.pdf.txt`), so that a change to where its lines and pages break is in a diff too. To
//! write them afresh after a deliberate change, run with `PLC_BLESS=1` — then read what was
//! written before committing it, because blessing a wrong file makes it the standard.

use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use plc_engine::api::Keep;
use plc_engine::citation::{Contributor, Date, Role, Source, SourceType};
use plc_engine::export::Format;
use plc_engine::{Engine, Error, Id};

/// A real PNG — one amber pixel, with every checksum right — so the picture inside the web
/// page is one a browser draws.
const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xb8, 0xd9, 0x23, 0xff,
    0x1f, 0x00, 0x06, 0x4a, 0x02, 0x84, 0x6f, 0xf2, 0x1d, 0xb2, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// An identifier nothing in the library has.
const GONE: &str = "01J8ZQ8D4Q6S8W0W2Y4A6C8E09";

fn person(role: Role, family: &str, given: &str) -> Contributor {
    Contributor::person(role, family, given)
}

struct Library {
    _dir: tempfile::TempDir,
    downloads: tempfile::TempDir,
    engine: Engine,
    topic: Id,
}

impl Library {
    fn bytes(&self, document: Id, format: Format) -> Vec<u8> {
        self.engine.export_document(document, format).unwrap()
    }

    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::create(dir.path().join("learning"), "Learning").unwrap();
        let topic = engine.topic_create(None, "Protein").unwrap();
        Library {
            _dir: dir,
            downloads: tempfile::tempdir().unwrap(),
            engine,
            topic,
        }
    }

    fn source(&mut self, source: Source) -> Id {
        self.engine.source_create(source).unwrap().0
    }

    fn document(&mut self, title: &str, body: &str) -> Id {
        let id = self.engine.document_create(self.topic, title).unwrap();
        self.write(id, body);
        id
    }

    fn write(&mut self, id: Id, body: &str) {
        let revision = self.engine.document_read(id).unwrap().revision;
        self.engine.document_write(id, body, revision).unwrap();
    }

    fn picture(&mut self, document: Id, file: &str) -> String {
        let path: PathBuf = self.downloads.path().join(file);
        fs::write(&path, PIXEL).unwrap();
        self.engine
            .document_attach_image(document, &path, Keep::Copy)
            .unwrap()
    }

    fn export(&self, document: Id, format: Format) -> String {
        String::from_utf8(self.engine.export_document(document, format).unwrap()).unwrap()
    }
}

/// The protein example: a document with every construct an export has to handle.
fn protein() -> (Library, Id) {
    let mut library = Library::new();

    let mut book = Source::new(SourceType::Book, "Ravinto ja terveys", Date::Year(2021));
    book.contributors = vec![person(Role::Author, "Nieminen", "Anna Maria")];
    book.publisher = Some("Otava".into());
    book.edition = Some("2".into());
    let book = library.source(book);

    // A second Nieminen 2021 that this document never cites. It still makes the first one
    // "2021a", because letters are the library's, not the document's.
    let mut other = Source::new(SourceType::Book, "Vitamiinit", Date::Year(2021));
    other.contributors = vec![person(Role::Author, "Nieminen", "Anna Maria")];
    other.publisher = Some("Otava".into());
    library.source(other);

    let mut article = Source::new(
        SourceType::JournalArticle,
        "Protein intake in Finnish adults",
        Date::Year(2019),
    );
    article.contributors = vec![
        person(Role::Author, "Kallio", "Kari"),
        person(Role::Author, "Virtanen", "Vilja"),
    ];
    article.container = Some("Journal of Nutrition".into());
    article.volume = Some("149".into());
    article.issue = Some("4".into());
    article.pages = Some("600-612".into());
    article.doi = Some("10.1093/jn/nxz001".into());
    let article = library.source(article);

    let mut page = Source::new(SourceType::WebPage, "Protein requirements", Date::NoDate);
    page.site = Some("Terveyskirjasto".into());
    page.url = Some("https://www.terveyskirjasto.fi/protein".into());
    page.retrieved = Some(Date::Full(2026, 1, 12));
    let page = library.source(page);

    let mut report = Source::new(
        SourceType::Report,
        "Protein and amino acid requirements in human nutrition",
        Date::Year(2007),
    );
    report.contributors = vec![
        Contributor::group(Role::Author, "World Health Organization").with_abbreviation("WHO"),
    ];
    report.publisher = Some("World Health Organization".into());
    report.number = Some("935".into());
    let report = library.source(report);

    let mut russian = Source::new(SourceType::Book, "Белок в питании", Date::Year(2018));
    russian.contributors = vec![person(Role::Author, "Петров", "Алексей")];
    russian.publisher = Some("Наука".into());
    let russian = library.source(russian);

    let kinds = library.document("Kinds of protein", "Complete and incomplete.");
    let what = library.document("What protein is", "");
    let picture = library.picture(what, "Amino Acids.png");

    library.write(
        what,
        &[
            format!(
                "Protein is built from amino acids[^cite:{book}@44]. \
                 [^cite:{article}!narrative] measured how much of it Finnish adults eat."
            ),
            "# Kinds of protein".to_string(),
            // Two sources behind one claim, the group's short form introduced here; then the
            // same group again, and a citation put after the full stop rather than before it.
            format!(
                "Complete proteins contain all nine essential amino acids\
                 [^cite:{report}@para:3][^cite:{page}]. \
                 The requirement is set per kilogram of body weight.[^cite:{report}]\n\
                 A line of its own, inside the same paragraph."
            ),
            "## Plant protein".to_string(),
            format!("![Amino acids & their chains]({picture})"),
            format!(
                "See [[{kinds}]] for more, and [[{GONE}]] for nothing. \
                 [^cite:{report}!narrative] counts it per kilogram. \
                 An old claim[^cite:{GONE}]. Rated 5* online, which *is* not a source. \
                 Белок важен[^cite:{russian}@12-14]."
            ),
            "![A picture nobody kept](nowhere.png)".to_string(),
            "#Not a heading, as the editor shows it.".to_string(),
        ]
        .join("\n\n"),
    );

    (library, what)
}

/// The file in `tests/export/`, or — with `PLC_BLESS=1` — the file written from `actual`.
fn expect_file(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/export")
        .join(name);
    if std::env::var_os("PLC_BLESS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — run with PLC_BLESS=1 to write it", path.display()));
    if actual != expected {
        let written = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        fs::write(&written, actual).unwrap();
        let line = actual
            .lines()
            .zip(expected.lines())
            .position(|(a, e)| a != e)
            .unwrap_or_else(|| actual.lines().count().min(expected.lines().count()));
        panic!(
            "{name} differs from what is expected, first at line {}:\n  expected: {:?}\n  exported: {:?}\n\
             The whole export is in {}",
            line + 1,
            expected.lines().nth(line).unwrap_or("(end)"),
            actual.lines().nth(line).unwrap_or("(end)"),
            written.display()
        );
    }
}

#[test]
fn plain_text_reads_as_the_document_does() {
    let (library, what) = protein();
    expect_file("protein.txt", &library.export(what, Format::Text));
}

#[test]
fn a_web_page_reads_as_the_document_does() {
    let (library, what) = protein();
    expect_file("protein.html", &library.export(what, Format::Html));
}

#[test]
fn citations_are_disambiguated_against_the_whole_library() {
    let (library, what) = protein();
    let text = library.export(what, Format::Text);
    assert!(text.contains("(Nieminen, 2021a, p. 44)"), "{text}");
    assert!(text.contains("Nieminen, A. M. (2021a)."), "{text}");
    // The other one is in the library, not in this document's reference list.
    assert!(!text.contains("Vitamiinit"), "{text}");
}

#[test]
fn the_picture_goes_inside_the_web_page() {
    let (library, what) = protein();
    let html = library.export(what, Format::Html);
    assert!(html.contains(
        "<img src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mO42SP/HwAGSgKEb/IdsgAAAABJRU5ErkJggg==\" alt=\"Amino acids &amp; their chains\">"
    ), "{html}");
    assert!(html.contains("<span class=\"missing\">[Picture: A picture nobody kept]</span>"));
}

#[test]
fn what_has_gone_reads_as_the_editor_shows_it() {
    let (library, what) = protein();
    let text = library.export(what, Format::Text);
    assert!(text.contains("An old claim [source missing]."), "{text}");
    assert!(
        text.contains("and [missing document] for nothing"),
        "{text}"
    );
}

#[test]
fn citations_side_by_side_are_one_pair_of_parentheses() {
    let (library, what) = protein();
    let text = library.export(what, Format::Text);
    assert!(
        text.contains(
            "amino acids (Protein requirements, n.d.; \
             World Health Organization [WHO], 2007, para. 3)."
        ),
        "{text}"
    );
}

#[test]
fn a_group_is_spelled_out_once_and_short_after() {
    let (library, what) = protein();
    let text = library.export(what, Format::Text);
    assert!(text.contains("body weight. (WHO, 2007)"), "{text}");
    assert!(text.contains("WHO (2007) counts it"), "{text}");
    // Never in the list, which spells every group out.
    assert!(
        text.contains("\nWorld Health Organization. (2007)."),
        "{text}"
    );
}

#[test]
fn a_document_citing_nothing_has_no_reference_list() {
    let mut library = Library::new();
    let mine = library.document("My thoughts", "Only my own writing.");
    for format in [Format::Text, Format::Html] {
        assert!(!library.export(mine, format).contains("References"));
    }
    assert!(!drawn(&library.bytes(mine, Format::Pdf)).contains("References"));
}

#[test]
fn exporting_a_document_that_is_not_there_says_so() {
    let library = Library::new();
    let nothing: Id = GONE.parse().unwrap();
    assert!(matches!(
        library.engine.export_document(nothing, Format::Text),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn exporting_writes_nothing_into_the_library() {
    let (library, what) = protein();
    let before = files(library.engine.root());
    for format in [
        Format::Text,
        Format::Html,
        Format::Docx,
        Format::Odt,
        Format::Pdf,
    ] {
        library.bytes(what, format);
    }
    assert_eq!(files(library.engine.root()), before);
}

/// Every file under a folder, the index aside: it is a cache, free to change on a read.
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == ".plc") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = fs::read(&path).unwrap();
                found.push((path, bytes));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn each_format_knows_its_extension() {
    assert_eq!(Format::Text.extension(), "txt");
    assert_eq!(Format::Html.extension(), "html");
    assert_eq!(Format::Docx.extension(), "docx");
    assert_eq!(Format::Odt.extension(), "odt");
    assert_eq!(Format::Pdf.extension(), "pdf");
}

/// With `PLC_EXPORT_TO=<folder>`, the protein example in every format, for opening by hand in
/// a word processor and a browser — what no test here can do for you.
#[test]
fn every_format_can_be_written_out_for_opening_by_hand() {
    let Some(folder) = std::env::var_os("PLC_EXPORT_TO") else {
        return;
    };
    let (library, what) = protein();
    fs::create_dir_all(&folder).unwrap();
    for format in [
        Format::Text,
        Format::Html,
        Format::Docx,
        Format::Odt,
        Format::Pdf,
    ] {
        let name = format!("protein.{}", format.extension());
        fs::write(Path::new(&folder).join(name), library.bytes(what, format)).unwrap();
    }
}

// ---- Word and OpenDocument: zips of XML ------------------------------------------

/// Every part of a package, by name, in the order it was written.
fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut part = zip.by_index(i).unwrap();
            let mut content = Vec::new();
            part.read_to_end(&mut content).unwrap();
            (part.name().to_string(), content)
        })
        .collect()
}

fn part<'a>(parts: &'a [(String, Vec<u8>)], name: &str) -> &'a str {
    let (_, bytes) = parts
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name} in {:?}", names(parts)));
    std::str::from_utf8(bytes).unwrap()
}

fn names(parts: &[(String, Vec<u8>)]) -> Vec<&str> {
    parts.iter().map(|(n, _)| n.as_str()).collect()
}

/// Every XML part parses: a file with one malformed part is one an office suite refuses whole.
fn all_xml_parses(parts: &[(String, Vec<u8>)]) {
    for (name, bytes) in parts {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let text = std::str::from_utf8(bytes).unwrap();
            roxmltree::Document::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}

/// The text of every element named `name` in a part, one string per element.
fn texts(xml: &str, paragraph: &str, text: &str) -> Vec<String> {
    let doc = roxmltree::Document::parse(xml).unwrap();
    doc.descendants()
        .filter(|n| n.tag_name().name() == paragraph)
        .map(|p| {
            p.descendants()
                .filter(|n| n.tag_name().name() == text)
                .filter_map(|n| n.text())
                .collect()
        })
        .collect()
}

#[test]
fn a_word_file_holds_the_document_its_styles_and_its_picture() {
    let (library, what) = protein();
    let parts = unzip(&library.bytes(what, Format::Docx));
    assert_eq!(
        names(&parts),
        [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/styles.xml",
            "word/_rels/document.xml.rels",
            "docProps/core.xml",
            "word/media/image1.png",
        ]
    );
    all_xml_parses(&parts);

    let document = part(&parts, "word/document.xml");
    let paragraphs = texts(document, "p", "t");
    assert_eq!(paragraphs[0], "What protein is");
    assert!(paragraphs.contains(&"Kinds of protein".to_string()));
    assert!(
        paragraphs.iter().any(|p| {
            p.starts_with(
            "Protein is built from amino acids (Nieminen, 2021a, p. 44). Kallio and Virtanen (2019)"
        )
        }),
        "{paragraphs:#?}"
    );
    assert!(paragraphs
        .iter()
        .any(|p| p.contains("An old claim [source missing].")));
    assert!(paragraphs.contains(&"References".to_string()));
    assert!(paragraphs.contains(&"Петров, А. (2018). Белок в питании. Наука.".to_string()));

    // Styles, never formatting by hand: the headings are Word's, the list hangs.
    assert!(document.contains("<w:pStyle w:val=\"Title\"/>"));
    assert!(document.contains("<w:pStyle w:val=\"Heading1\"/>"));
    assert!(document.contains("<w:pStyle w:val=\"Heading2\"/>"));
    assert_eq!(
        document
            .matches("<w:pStyle w:val=\"Bibliography\"/>")
            .count(),
        5
    );
    let styles = part(&parts, "word/styles.xml");
    assert!(styles.contains("<w:name w:val=\"heading 1\"/>"));
    assert!(styles.contains("<w:ind w:left=\"720\" w:hanging=\"720\"/>"));

    // Italics where APA puts them: the no-author page's title, in the text and in the list.
    assert!(document
        .contains("<w:i/><w:iCs/></w:rPr><w:t xml:space=\"preserve\">Protein requirements</w:t>"));

    // The picture travels inside the file, one pixel at 96 to the inch: 9525 EMU square.
    assert_eq!(part_bytes(&parts, "word/media/image1.png"), PIXEL);
    assert!(document.contains("<wp:extent cx=\"9525\" cy=\"9525\"/>"));
    assert!(document.contains("descr=\"Amino acids &amp; their chains\""));
    let relationships = part(&parts, "word/_rels/document.xml.rels");
    assert!(relationships.contains("Target=\"media/image1.png\""));
    // A picture that is not there is its description.
    assert!(paragraphs.contains(&"[Picture: A picture nobody kept]".to_string()));

    // The list's web addresses are links.
    assert!(relationships
        .contains("Target=\"https://doi.org/10.1093/jn/nxz001\" TargetMode=\"External\""));
    assert!(part(&parts, "docProps/core.xml").contains("<dc:title>What protein is</dc:title>"));
}

fn part_bytes<'a>(parts: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
    &parts.iter().find(|(n, _)| n == name).unwrap().1
}

#[test]
fn an_opendocument_file_starts_with_its_mimetype_and_holds_everything_else() {
    let (library, what) = protein();
    let bytes = library.bytes(what, Format::Odt);

    // The standard's one rule about order: `mimetype` first, uncompressed, so the file is
    // recognisable from its first bytes.
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(&bytes[38..77], b"application/vnd.oasis.opendocument.text");

    let parts = unzip(&bytes);
    assert_eq!(
        names(&parts),
        [
            "mimetype",
            "META-INF/manifest.xml",
            "content.xml",
            "styles.xml",
            "meta.xml",
            "Pictures/1.png",
        ]
    );
    all_xml_parses(&parts);

    let manifest = part(&parts, "META-INF/manifest.xml");
    for listed in ["content.xml", "styles.xml", "meta.xml", "Pictures/1.png"] {
        assert!(
            manifest.contains(&format!("manifest:full-path=\"{listed}\"")),
            "{listed}"
        );
    }

    let content = part(&parts, "content.xml");
    assert!(content.contains("<text:p text:style-name=\"Title\">What protein is</text:p>"));
    assert!(content.contains(
        "<text:h text:style-name=\"Heading_20_1\" text:outline-level=\"1\">Kinds of protein</text:h>"
    ));
    assert!(content.contains("<text:span text:style-name=\"T1\">Protein requirements</text:span>"));
    assert!(content.contains("An old claim [source missing]."));
    assert!(
        content.contains("A line of its own, inside the same paragraph.")
            && content.contains("<text:line-break/>A line of its own")
    );
    assert_eq!(
        content.matches("text:style-name=\"Bibliography\"").count(),
        5
    );
    assert!(content.contains(
        "<text:a xlink:type=\"simple\" xlink:href=\"https://doi.org/10.1093/jn/nxz001\">"
    ));
    assert!(content.contains("<svg:title>Amino acids &amp; their chains</svg:title>"));
    assert!(content.contains("svg:width=\"0.010in\" svg:height=\"0.010in\""));

    let styles = part(&parts, "styles.xml");
    assert!(styles.contains("fo:margin-left=\"0.5in\" fo:text-indent=\"-0.5in\""));
    assert_eq!(part_bytes(&parts, "Pictures/1.png"), PIXEL);
    assert!(part(&parts, "meta.xml").contains("<dc:title>What protein is</dc:title>"));
}

#[test]
fn an_unchanged_document_exports_the_same_bytes_every_time() {
    let (library, what) = protein();
    for format in [
        Format::Text,
        Format::Html,
        Format::Docx,
        Format::Odt,
        Format::Pdf,
    ] {
        assert_eq!(
            library.bytes(what, format),
            library.bytes(what, format),
            "{format:?}"
        );
    }
}

// ---- PDF: pages as a reader draws them --------------------------------------------

/// The text a PDF string holds: UTF-16 after a byte-order mark, and otherwise a byte to a
/// character, as PDF writes text.
fn pdf_text(bytes: &[u8]) -> String {
    match bytes.strip_prefix(&[0xfe, 0xff]) {
        Some(utf16) => {
            let units: Vec<u16> = utf16
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16(&units).unwrap()
        }
        None => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// What each glyph a font draws stands for, from its `/ToUnicode` map — what a reader goes by
/// when text is copied out of the PDF, or searched.
fn to_unicode(pdf: &lopdf::Document, font: &lopdf::Dictionary) -> HashMap<u16, String> {
    let cmap = font
        .get_deref(b"ToUnicode", pdf)
        .unwrap()
        .as_stream()
        .unwrap()
        .decompressed_content()
        .unwrap();
    let cmap = String::from_utf8(cmap).unwrap();
    let hex = |digits: &str| u16::from_str_radix(digits, 16).unwrap();
    let mut glyphs = HashMap::new();
    let mut mapping = false;
    for line in cmap.lines() {
        if line.ends_with("beginbfchar") {
            mapping = true;
        } else if line == "endbfchar" {
            mapping = false;
        } else if mapping {
            // <0001> <0050>: a glyph, then the UTF-16 it stands for.
            let mut parts = line.split(['<', '>', ' ']).filter(|p| !p.is_empty());
            let glyph = hex(parts.next().unwrap());
            let units: Vec<u16> = parts
                .next()
                .unwrap()
                .as_bytes()
                .chunks(4)
                .map(|digits| hex(std::str::from_utf8(digits).unwrap()))
                .collect();
            glyphs.insert(glyph, String::from_utf16(&units).unwrap());
        }
    }
    glyphs
}

/// A PDF's pages as a reader draws them, read from the file itself: each page from the top,
/// line by line, each line the text drawn at one height from left to right, with `<i>italics</i>`,
/// `<b>bold</b>` and `<link>links</link>` marked as they are drawn — tags rather than asterisks,
/// which a body may hold as itself — and a picture as its size in points.
fn drawn(bytes: &[u8]) -> String {
    let pdf = lopdf::Document::load_mem(bytes).unwrap();
    let mut out = String::new();
    for (number, page) in pdf.get_pages() {
        let fonts = pdf.get_page_fonts(page).unwrap();
        let glyphs = to_unicode(&pdf, fonts[b"F".as_slice()]);
        let content = lopdf::content::Content::decode(&pdf.get_page_content(page)).unwrap();

        // Everything drawn: how high, how far across, and what.
        let mut pieces: Vec<(f32, f32, String)> = Vec::new();
        let (mut bold, mut link, mut italic) = (false, false, false);
        let mut actual: Option<String> = None;
        let mut text = String::new();
        let mut at = (0.0, 0.0);
        let mut placed = [0.0f32; 6];
        for operation in &content.operations {
            let number = |i: usize| operation.operands[i].as_float().unwrap();
            match operation.operator.as_str() {
                "Tr" => bold = operation.operands[0].as_i64().unwrap() == 2,
                // Links are blue; everything else is black or grey.
                "rg" => link = number(2) > 0.5 && number(0) < 0.1,
                "Tm" => {
                    italic = number(2) != 0.0;
                    at = (number(5), number(4));
                }
                "BDC" => {
                    let properties = operation.operands[1].as_dict().unwrap();
                    actual = Some(pdf_text(
                        properties.get(b"ActualText").unwrap().as_str().unwrap(),
                    ));
                }
                "EMC" => actual = None,
                "Tj" => match &actual {
                    Some(stands_for) => text.push_str(stands_for),
                    None => {
                        for code in operation.operands[0].as_str().unwrap().chunks_exact(2) {
                            text.push_str(&glyphs[&u16::from_be_bytes([code[0], code[1]])]);
                        }
                    }
                },
                "ET" => {
                    let mut marked = std::mem::take(&mut text);
                    if italic {
                        marked = format!("<i>{marked}</i>");
                    }
                    if bold {
                        marked = format!("<b>{marked}</b>");
                    }
                    if link {
                        marked = format!("<link>{marked}</link>");
                    }
                    pieces.push((at.0, at.1, marked));
                }
                "cm" => placed = std::array::from_fn(number),
                "Do" => pieces.push((
                    placed[5] + placed[3],
                    placed[4],
                    format!("[picture {:.2} × {:.2}]", placed[0], placed[3]),
                )),
                _ => {}
            }
        }

        // Top to bottom, and across each line from the left.
        pieces.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.total_cmp(&b.1)));
        out.push_str(&format!("--- page {number} ---\n"));
        let mut line: Option<f32> = None;
        for (height, _, piece) in pieces {
            if line.is_some_and(|y| (y - height).abs() > 0.01) {
                out.push('\n');
            }
            line = Some(height);
            out.push_str(&piece);
        }
        out.push('\n');
    }
    out
}

#[test]
fn a_pdf_reads_as_the_document_does() {
    let (library, what) = protein();
    expect_file("protein.pdf.txt", &drawn(&library.bytes(what, Format::Pdf)));
}

#[test]
fn what_the_font_cannot_draw_still_copies_out_as_itself() {
    let mut library = Library::new();
    let mine = library.document("Words", "Protein is 蛋白质 in Chinese 🙂.");
    let drawn = drawn(&library.bytes(mine, Format::Pdf));
    // Drawn as empty boxes, but carried as the characters, for copying and searching.
    assert!(
        drawn.contains("Protein is 蛋白质 in Chinese 🙂."),
        "{drawn}"
    );
}

#[test]
fn a_long_document_runs_over_numbered_pages() {
    let mut library = Library::new();
    let paragraph = "Protein is built from amino acids. ".repeat(12);
    let mut body: Vec<String> = (0..12).map(|_| paragraph.clone()).collect();
    body.push("# The last section".to_string());
    body.push("It ends here.".to_string());
    let mine = library.document("A long one", &body.join("\n\n"));
    let bytes = library.bytes(mine, Format::Pdf);
    let drawn = drawn(&bytes);
    let pages: Vec<&str> = drawn.split("--- page ").skip(1).collect();
    assert!(pages.len() >= 2, "{drawn}");
    for (n, page) in pages.iter().enumerate() {
        // The number, then the page's own first line: the number is at the top.
        let mut lines = page.lines();
        assert_eq!(lines.next(), Some(format!("{} ---", n + 1).as_str()));
        assert_eq!(lines.next(), Some((n + 1).to_string().as_str()), "{page}");
    }

    // The outline leads to the page the heading is on.
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    let outlines = entry(&pdf, pdf.catalog().unwrap(), "Outlines")
        .as_dict()
        .unwrap();
    let item = entry(&pdf, outlines, "First").as_dict().unwrap();
    let destination = entry(&pdf, item, "Dest").as_array().unwrap();
    let page = destination[0].as_reference().unwrap();
    let number = pdf
        .get_pages()
        .into_iter()
        .find(|(_, id)| *id == page)
        .map(|(number, _)| number)
        .unwrap();
    let on_page = pages[usize::try_from(number).unwrap() - 1];
    assert!(on_page.contains("<b>The last section</b>"), "{on_page}");
}

/// A dictionary entry, followed if it is a reference.
fn entry<'a>(
    pdf: &'a lopdf::Document,
    dictionary: &'a lopdf::Dictionary,
    key: &str,
) -> &'a lopdf::Object {
    dictionary
        .get_deref(key.as_bytes(), pdf)
        .unwrap_or_else(|e| panic!("{key}: {e}"))
}

/// The outline from one item on: each heading's title, and the headings below it.
fn outline(pdf: &lopdf::Document, first: Option<&lopdf::Object>) -> Vec<(String, Vec<String>)> {
    let mut items = Vec::new();
    let mut at = first.map(|item| pdf.dereference(item).unwrap().1.as_dict().unwrap());
    while let Some(item) = at {
        let title = pdf_text(entry(pdf, item, "Title").as_str().unwrap());
        let below = outline(pdf, item.get(b"First").ok())
            .into_iter()
            .map(|(title, _)| title)
            .collect();
        items.push((title, below));
        at = item
            .get(b"Next")
            .ok()
            .map(|next| pdf.dereference(next).unwrap().1.as_dict().unwrap());
    }
    items
}

#[test]
fn a_pdf_carries_its_font_its_picture_its_links_and_its_outline() {
    let (library, what) = protein();
    let bytes = library.bytes(what, Format::Pdf);
    assert!(bytes.starts_with(b"%PDF-1.7"));
    let pdf = lopdf::Document::load_mem(&bytes).unwrap();
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), 1);
    let page = pdf.get_dictionary(pages[&1]).unwrap();

    // A4, in points.
    let media: Vec<f32> = entry(&pdf, page, "MediaBox")
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_float().unwrap())
        .collect();
    assert_eq!(media, [0.0, 0.0, 595.276, 841.89]);

    // The font goes inside the file, cut down to the glyphs the document uses.
    let fonts = pdf.get_page_fonts(pages[&1]).unwrap();
    let font = fonts[b"F".as_slice()];
    let name =
        String::from_utf8(entry(&pdf, font, "BaseFont").as_name().unwrap().to_vec()).unwrap();
    let (tag, family) = name.split_once('+').unwrap();
    assert_eq!(family, "DejaVuSans");
    assert!(
        tag.len() == 6 && tag.chars().all(|c| c.is_ascii_uppercase()),
        "{name}"
    );
    let cid_font = pdf
        .dereference(&entry(&pdf, font, "DescendantFonts").as_array().unwrap()[0])
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    let descriptor = entry(&pdf, cid_font, "FontDescriptor").as_dict().unwrap();
    let file = entry(&pdf, descriptor, "FontFile2").as_stream().unwrap();
    let subset = file.decompressed_content().unwrap();
    assert_eq!(
        usize::try_from(entry(&pdf, &file.dict, "Length1").as_i64().unwrap()).unwrap(),
        subset.len()
    );
    assert!(subset.len() < 100_000, "{} bytes of font", subset.len());

    // The picture, one pixel square.
    let pictures = entry(
        &pdf,
        entry(&pdf, page, "Resources").as_dict().unwrap(),
        "XObject",
    )
    .as_dict()
    .unwrap();
    assert_eq!(pictures.len(), 1);
    let (_, picture) = pictures.iter().next().unwrap();
    let picture = &pdf
        .dereference(picture)
        .unwrap()
        .1
        .as_stream()
        .unwrap()
        .dict;
    assert_eq!(entry(&pdf, picture, "Subtype").as_name().unwrap(), b"Image");
    assert_eq!(entry(&pdf, picture, "Width").as_i64().unwrap(), 1);
    assert_eq!(entry(&pdf, picture, "Height").as_i64().unwrap(), 1);

    // The reference list's web addresses go where they say.
    let links: Vec<String> = pdf
        .get_page_annotations(pages[&1])
        .unwrap()
        .iter()
        .map(|annotation| {
            let action = entry(&pdf, annotation, "A").as_dict().unwrap();
            pdf_text(entry(&pdf, action, "URI").as_str().unwrap())
        })
        .collect();
    assert_eq!(
        links,
        [
            "https://doi.org/10.1093/jn/nxz001",
            "https://www.terveyskirjasto.fi/protein"
        ]
    );

    // The headings are the outline, nested as they are; the title is the document's.
    let catalog = pdf.catalog().unwrap();
    let outlines = entry(&pdf, catalog, "Outlines").as_dict().unwrap();
    assert_eq!(
        outline(&pdf, outlines.get(b"First").ok()),
        [
            (
                "Kinds of protein".to_string(),
                vec!["Plant protein".to_string()]
            ),
            ("References".to_string(), vec![]),
        ]
    );
    let preferences = entry(&pdf, catalog, "ViewerPreferences").as_dict().unwrap();
    assert!(entry(&pdf, preferences, "DisplayDocTitle")
        .as_bool()
        .unwrap());
    let info = entry(&pdf, &pdf.trailer, "Info").as_dict().unwrap();
    assert_eq!(
        pdf_text(entry(&pdf, info, "Title").as_str().unwrap()),
        "What protein is"
    );
}
