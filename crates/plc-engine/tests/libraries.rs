//! The fixture libraries in `tests/fixtures/`: real folders, checked in, opened the way a
//! person's library is opened.
//!
//! Each is copied to a temporary folder first, because opening a library writes its index
//! (`.plc/`), and a fixture is never written to. What each one is for is in
//! `tests/fixtures/README.md`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use plc_engine::api::TopicNode;
use plc_engine::{Engine, Id};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A fixture, copied somewhere of its own and opened there.
fn open(name: &str) -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&fixture(name), dir.path());
    let engine = Engine::open(dir.path()).unwrap();
    (dir, engine)
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Every file under a folder, by its path inside it written with `/`, the index aside: it is a
/// cache, and writing it is what opening a library does.
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == ".plc") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                let relative: Vec<String> = path
                    .strip_prefix(root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                found.insert(relative.join("/"), fs::read(&path).unwrap());
            }
        }
    }
    let mut found = BTreeMap::new();
    walk(root, root, &mut found);
    found
}

/// The file in `tests/fixtures/`, or — with `PLC_BLESS=1` — the file written from `actual`.
fn expect_file(name: &str, actual: &str) {
    let path = fixture(name);
    if std::env::var_os("PLC_BLESS").is_some() {
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
            "{name} differs from what is recorded, first at line {}:\n  recorded: {:?}\n  read:     {:?}\n\
             The whole reading is in {}",
            line + 1,
            expected.lines().nth(line).unwrap_or("(end)"),
            actual.lines().nth(line).unwrap_or("(end)"),
            written.display()
        );
    }
}

// ---- minimal ---------------------------------------------------------------------------

#[test]
fn the_minimal_library_opens_empty_and_clean() {
    let (dir, engine) = open("minimal");
    let status = engine.status().unwrap();
    assert_eq!(status.name, "Learning");
    assert_eq!((status.topics, status.documents), (0, 0));
    assert!(status.problems.is_empty(), "{:?}", status.problems);
    assert!(status.writable);
    assert!(engine.topic_tree().unwrap().is_empty());
    assert!(engine.source_list().unwrap().is_empty());
    assert!(engine.search("protein").unwrap().is_empty());
    // Opening it wrote its index and nothing else.
    assert_eq!(files(dir.path()), files(&fixture("minimal")));
}

#[test]
fn the_minimal_library_takes_its_first_topic_and_document() {
    let (dir, mut engine) = open("minimal");
    let protein = engine.topic_create(None, "Protein").unwrap();
    engine.document_create(protein, "What protein is").unwrap();
    drop(engine);
    let engine = Engine::open(dir.path()).unwrap();
    let status = engine.status().unwrap();
    assert_eq!((status.topics, status.documents), (1, 1));
}

// ---- damaged ---------------------------------------------------------------------------

#[test]
fn a_damaged_library_opens_and_names_every_broken_file() {
    let (_dir, engine) = open("damaged");
    let status = engine.status().unwrap();
    let problems: Vec<(String, &str)> = status
        .problems
        .iter()
        .map(|p| {
            (
                p.path.to_string_lossy().replace('\\', "/"),
                p.message.as_str(),
            )
        })
        .collect();

    let named = |path: &str| problems.iter().find(|(p, _)| p == path).map(|(_, m)| *m);
    // Front matter that is not TOML, a source of a type there is no such thing as, and a topic
    // whose own file is not TOML: each named, with what is wrong with it.
    for broken in [
        "topics/protein/broken-front-matter.md",
        "sources/protein-requirements-nd.toml",
        "topics/vitamins/topic.toml",
    ] {
        let message = named(broken).unwrap_or_else(|| panic!("{broken} not named: {problems:?}"));
        assert!(message.contains("TOML parse error"), "{broken}: {message}");
    }
    // A document copied in a file manager: two files, one identifier, both named.
    let duplicate = problems
        .iter()
        .find(|(_, m)| m.contains("duplicate identifier"))
        .unwrap_or_else(|| panic!("the copy is not reported: {problems:?}"));
    assert!(
        [duplicate.0.as_str(), duplicate.1]
            .concat()
            .contains("what-protein-is-copy.md")
            && [duplicate.0.as_str(), duplicate.1]
                .concat()
                .contains("what-protein-is.md"),
        "{duplicate:?}"
    );
    // Those four, and nothing that is not wrong.
    assert_eq!(problems.len(), 4, "{problems:?}");

    // Everything else is the library as it was: one topic, its readable documents, and the
    // one readable source.
    assert_eq!((status.topics, status.documents), (1, 3));
    let titles: Vec<String> = engine.topic_tree().unwrap()[0]
        .documents
        .iter()
        .map(|d| d.title.clone())
        .collect();
    assert_eq!(
        titles,
        [
            "An orphaned citation",
            "Kinds of protein",
            "What protein is"
        ]
    );
    let sources = engine.source_list().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].title, "Ravinto ja terveys");
    assert_eq!(engine.search("amino").unwrap().len(), 1);
}

#[test]
fn an_orphaned_citation_is_kept_and_shows_as_missing() {
    let (_dir, engine) = open("damaged");
    let orphaned = find(&engine.topic_tree().unwrap(), "An orphaned citation");
    let document = engine.document_read(orphaned).unwrap();
    // The citation stays exactly as written, so the source can be saved again under it.
    assert!(
        document
            .body
            .contains("one whose source has gone[^cite:01J8ZQ8D4Q6S8W0W2Y4A6C8E09@12]"),
        "{}",
        document.body
    );
    let requests: Vec<_> = plc_engine::citation::reference::parse_all(&document.body)
        .into_iter()
        .map(|c| (c.source, c.form, c.locator))
        .collect();
    let formatted = engine.citation_format_many(&requests).unwrap();
    // The source that is there formats; the one that has gone is a hole, not a failure.
    assert!(
        formatted[0]
            .as_deref()
            .is_some_and(|f| f.contains("Nieminen")),
        "{formatted:?}"
    );
    assert_eq!(formatted[1], None);
    // The reference list holds what is there.
    let list = engine.citation_reference_list(orphaned).unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    assert!(list[0].starts_with("Nieminen, A. M. (2021)"), "{list:?}");
}

#[test]
fn opening_a_damaged_library_changes_none_of_its_files() {
    // Nothing broken is "repaired" by rewriting it, and nothing of the person's own — the
    // notes beside the documents — is touched.
    let (dir, engine) = open("damaged");
    engine.status().unwrap();
    engine.topic_tree().unwrap();
    engine.source_list().unwrap();
    assert_eq!(files(dir.path()), files(&fixture("damaged")));
    assert!(dir.path().join("topics/protein/notes.txt").exists());
}

fn find(tree: &[TopicNode], title: &str) -> Id {
    fn search(tree: &[TopicNode], title: &str) -> Option<Id> {
        tree.iter().find_map(|topic| {
            topic
                .documents
                .iter()
                .find(|d| d.title == title)
                .map(|d| d.id)
                .or_else(|| search(&topic.children, title))
        })
    }
    search(tree, title).unwrap_or_else(|| panic!("no document {title:?}"))
}

// ---- v1: frozen --------------------------------------------------------------------------

/// What `tests/fixtures/v1` held when it was frozen: its files, hashed in order of their paths.
/// It never changes. A format change is a new fixture, `v2`, made by the engine that writes it,
/// with `v1` left exactly as it is to prove the old format still opens.
const V1_FROZEN: &str = "a09b1e40a8d4ee00616d6e78bc366f41b6b23eba4984658506bd45d520765f2a";

#[test]
fn v1_is_exactly_as_it_was_frozen() {
    let mut hasher = blake3::Hasher::new();
    for (path, bytes) in files(&fixture("v1")) {
        hasher.update(path.as_bytes());
        hasher.update(&[0]);
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    let hash = hasher.finalize().to_hex().to_string();
    assert_eq!(
        hash, V1_FROZEN,
        "tests/fixtures/v1 has changed. It is frozen: it is the library format as M1 shipped it, \
         and it stays exactly so for as long as that format must open. Put it back as it was, \
         and make a new fixture for a new format."
    );
}

#[test]
fn opening_v1_writes_nothing_but_its_index() {
    let (dir, engine) = open("v1");
    reading(&engine);
    assert_eq!(files(dir.path()), files(&fixture("v1")));
}

#[test]
fn v1_reads_exactly_as_recorded() {
    let (_dir, engine) = open("v1");
    expect_file("v1-reading.txt", &reading(&engine));
}

#[test]
fn every_v1_source_reads_back_as_the_file_it_came_from() {
    // Nothing in a source's file is lost on reading, nor added: written back out as the engine
    // writes sources, each is its own file again, byte for byte.
    let (dir, engine) = open("v1");
    for source in engine.source_list().unwrap() {
        let path = source_path(dir.path(), source.id);
        let written = toml::to_string_pretty(&source).unwrap();
        assert_eq!(
            written,
            fs::read_to_string(&path).unwrap(),
            "{}",
            path.display()
        );
    }
}

fn source_path(root: &Path, id: Id) -> PathBuf {
    fs::read_dir(root.join("sources"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.extension().is_some_and(|e| e == "toml")
                && fs::read_to_string(p)
                    .unwrap()
                    .starts_with(&format!("id = \"{id}\""))
        })
        .unwrap_or_else(|| panic!("no file for {id}"))
}

/// Everything the engine reads from a library, written out in order: its topics, every
/// document with its body and backlinks, every source as the engine would store it, the files
/// beside them, and what a few searches find.
fn reading(engine: &Engine) -> String {
    let mut out = String::new();
    let status = engine.status().unwrap();
    writeln!(
        out,
        "library {:?}: {} topics, {} documents, {} problems",
        status.name,
        status.topics,
        status.documents,
        status.problems.len()
    )
    .unwrap();

    fn topics(engine: &Engine, nodes: &[TopicNode], depth: usize, out: &mut String) {
        for topic in nodes {
            writeln!(
                out,
                "\n{}topic {:?} {}",
                "  ".repeat(depth),
                topic.name,
                topic.id
            )
            .unwrap();
            for summary in &topic.documents {
                let document = engine.document_read(summary.id).unwrap();
                let indent = "  ".repeat(depth + 1);
                writeln!(
                    out,
                    "{indent}document {:?} {}\n{indent}  created {}, modified {}",
                    document.title, document.id, document.created, document.modified
                )
                .unwrap();
                for line in document.body.lines() {
                    writeln!(out, "{indent}  | {line}").unwrap();
                }
                let backlinks: Vec<String> = engine
                    .backlinks(document.id)
                    .unwrap()
                    .into_iter()
                    .map(|b| format!("{:?}", b.title))
                    .collect();
                if !backlinks.is_empty() {
                    writeln!(out, "{indent}  linked from {}", backlinks.join(", ")).unwrap();
                }
                for name in pictures(&document.body) {
                    match engine.document_image(document.id, &name).unwrap() {
                        Some(path) => {
                            let size = fs::metadata(path).unwrap().len();
                            writeln!(out, "{indent}  picture {name:?}: {size} bytes").unwrap();
                        }
                        None => writeln!(out, "{indent}  picture {name:?}: missing").unwrap(),
                    }
                }
            }
            topics(engine, &topic.children, depth + 1, out);
        }
    }
    topics(engine, &engine.topic_tree().unwrap(), 0, &mut out);

    let mut sources = engine.source_list().unwrap();
    sources.sort_by_key(|s| s.id.to_string());
    for source in sources {
        writeln!(out, "\nsource {}", source.id).unwrap();
        for line in toml::to_string_pretty(&source).unwrap().lines() {
            writeln!(out, "  | {line}").unwrap();
        }
        if let Some(path) = engine.source_file(source.id).unwrap() {
            writeln!(out, "  paper: {} bytes", fs::metadata(path).unwrap().len()).unwrap();
        }
    }

    writeln!(out).unwrap();
    for query in [
        "amino",
        "auringonvalosta",
        "белок",
        "Nieminen",
        "Organization",
        "Sports",
    ] {
        let mut hits: Vec<String> = engine
            .search(query)
            .unwrap()
            .into_iter()
            .map(|hit| format!("{:?} {:?}", hit.kind, hit.title))
            .collect();
        // What is found is the reading's; the order is the ranking's, free to change with it.
        hits.sort();
        writeln!(out, "search {query:?}: {}", hits.join(", ")).unwrap();
    }
    out
}

/// The picture names a body shows: `![…](name)`.
fn pictures(body: &str) -> Vec<String> {
    body.match_indices("![")
        .filter_map(|(at, _)| {
            let rest = &body[at..];
            let open = rest.find("](")?;
            let close = rest[open..].find(')')?;
            Some(rest[open + 2..open + close].to_string())
        })
        .collect()
}
