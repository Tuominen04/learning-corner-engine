//! Exporting a document, through a real library on disk.
//!
//! The two formats are compared **exactly** against files in `tests/export/`, which a person has
//! read: a change to what an export says must change one of those files, in a diff somebody
//! reviews. To write them afresh after a deliberate change, run with `PLC_BLESS=1` — then read
//! what was written before committing it, because blessing a wrong file makes it the standard.

use std::fs;
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
    library.export(what, Format::Html);
    library.export(what, Format::Text);
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
}
