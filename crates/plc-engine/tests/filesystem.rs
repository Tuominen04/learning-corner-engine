//! What actually happens on disk.
//!
//! The user is promised their library is plain files they can read and edit in any text
//! editor. These tests are that promise, asserted.

use std::fs;

use plc_engine::api::Engine;
use plc_engine::Error;

mod fixture;
use fixture::Fixture;

#[test]
fn a_library_survives_being_closed_and_reopened() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let before = fixture.engine().document_read(ids.what_protein_is).unwrap();

    fixture.reopen();

    let after = fixture.engine().document_read(ids.what_protein_is).unwrap();
    assert_eq!(before, after);
}

#[test]
fn every_file_is_readable_text() {
    let fixture = Fixture::populated();

    let manifest = fs::read_to_string(fixture.root().join("library.toml")).unwrap();
    assert!(manifest.contains("name = \"Learning\""), "{manifest}");

    let document = find_document(fixture.root(), "what-protein-is.md");
    let text = fs::read_to_string(&document).unwrap();
    assert!(
        text.starts_with("+++\n"),
        "front matter must lead the file: {text}"
    );
    assert!(text.contains("title = \"What protein is\""), "{text}");
    assert!(
        text.contains("Protein is built from amino acids."),
        "the body must be plain Markdown: {text}"
    );
}

#[test]
fn folders_mirror_the_topic_tree() {
    let fixture = Fixture::populated();
    assert!(fixture.root().join("topics/protein/topic.toml").is_file());
    assert!(fixture
        .root()
        .join("topics/protein/in-the-body/topic.toml")
        .is_file());
    assert!(fixture
        .root()
        .join("topics/protein/what-protein-is.md")
        .is_file());
    assert!(fixture
        .root()
        .join("topics/protein/in-the-body/how-protein-is-used.md")
        .is_file());
}

#[test]
fn an_edit_made_outside_the_application_is_picked_up() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let path = fixture.root().join("topics/protein/what-protein-is.md");

    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replace("amino acids", "amino acids (edited by hand)"),
    )
    .unwrap();

    fixture.reopen();

    let document = fixture.engine().document_read(ids.what_protein_is).unwrap();
    assert!(
        document.body.contains("edited by hand"),
        "an external edit must be picked up: {}",
        document.body
    );
}

#[test]
fn a_stale_write_is_refused_rather_than_overwriting_an_external_edit() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let stale = fixture.engine().document_read(ids.what_protein_is).unwrap();

    // Someone edits the file in a text editor while the document is open.
    let path = fixture.root().join("topics/protein/what-protein-is.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        text.replace("amino acids", "something they typed themselves"),
    )
    .unwrap();

    let result = fixture
        .engine()
        .document_write(ids.what_protein_is, "my version", stale.revision);

    assert!(
        matches!(result, Err(Error::Conflict { .. })),
        "expected a conflict, got {result:?}"
    );
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("something they typed themselves"),
        "their edit must still be on disk"
    );
}

#[test]
fn a_write_succeeds_once_the_document_is_re_read() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let current = fixture.engine().document_read(ids.what_protein_is).unwrap();
    let written = fixture
        .engine()
        .document_write(ids.what_protein_is, "New body.", current.revision)
        .unwrap();
    assert_eq!(written.body, "New body.");
    assert_ne!(written.revision, current.revision);
}

#[test]
fn broken_front_matter_is_diagnosed_and_the_rest_of_the_library_survives() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let path = fixture.root().join("topics/protein/types-of-protein.md");
    fs::write(&path, "+++\nthis is not valid toml =\n+++\n\nbody").unwrap();

    fixture.reopen();
    let status = fixture.engine().status().unwrap();

    assert!(
        status
            .problems
            .iter()
            .any(|p| p.path.ends_with("types-of-protein.md")),
        "the broken file must be reported: {:?}",
        status.problems
    );
    assert!(
        fixture.engine().document_read(ids.what_protein_is).is_ok(),
        "one broken file must not take the library down with it"
    );
}

#[test]
fn a_duplicated_file_is_reported_rather_than_silently_accepted() {
    // Copying a document to duplicate it is a thing people do. Two files claiming one
    // identifier is a library error, and the user needs told which files.
    let mut fixture = Fixture::populated();
    let original = fixture.root().join("topics/protein/what-protein-is.md");
    let copy = fixture
        .root()
        .join("topics/protein/what-protein-is-copy.md");
    fs::copy(&original, &copy).unwrap();

    fixture.reopen();
    let status = fixture.engine().status().unwrap();

    assert!(
        status
            .problems
            .iter()
            .any(|p| p.message.contains("duplicate identifier")),
        "a duplicated identifier must be reported: {:?}",
        status.problems
    );
}

#[test]
fn the_working_directory_is_the_only_thing_the_engine_owns() {
    let fixture = Fixture::populated();
    let entries: Vec<String> = fs::read_dir(fixture.root())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    for name in &entries {
        assert!(
            matches!(name.as_str(), "library.toml" | "topics" | ".plc"),
            "unexpected entry in the library root: {name}"
        );
    }
}

#[test]
fn finnish_and_cyrillic_titles_become_usable_filenames() {
    let (dir, mut engine) = Fixture::empty();
    let topic = engine.topic_create(None, "Venäjän kieli").unwrap();
    engine.document_create(topic, "Я вас любил").unwrap();

    assert!(dir.path().join("topics/venäjän-kieli/topic.toml").is_file());
    assert!(dir
        .path()
        .join("topics/venäjän-kieli/я-вас-любил.md")
        .is_file());
}

#[test]
fn two_documents_with_the_same_title_get_distinct_files() {
    let (dir, mut engine) = Fixture::empty();
    let topic = engine.topic_create(None, "Protein").unwrap();
    let first = engine.document_create(topic, "Notes").unwrap();
    let second = engine.document_create(topic, "Notes").unwrap();

    assert_ne!(first, second);
    assert!(dir.path().join("topics/protein/notes.md").is_file());
    assert!(dir.path().join("topics/protein/notes-2.md").is_file());
}

#[test]
fn a_library_can_be_copied_to_another_location_and_opened() {
    // "Your library outlives the application" is only true if moving the folder works.
    let fixture = Fixture::populated();
    let destination = tempfile::tempdir().unwrap();
    copy_tree(fixture.root(), destination.path());

    let engine = Engine::open(destination.path()).unwrap();
    let status = engine.status().unwrap();
    assert_eq!(status.documents, 3);
    assert_eq!(status.topics, 2);
    assert!(status.problems.is_empty(), "{:?}", status.problems);
}

fn find_document(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    fn walk(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
        for entry in fs::read_dir(dir).ok()? {
            let path = entry.ok()?.path();
            if path.is_dir() {
                if let Some(found) = walk(&path, name) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(path);
            }
        }
        None
    }
    walk(root, name).unwrap_or_else(|| panic!("no {name} under {}", root.display()))
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
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
