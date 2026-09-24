//! Pictures kept beside the document that shows them.
//!
//! The promise: an image you insert is copied into your library and referenced by a plain
//! filename, so the `.md` renders the picture in any Markdown reader — not only in this app.

use std::fs;
use std::path::{Path, PathBuf};

use plc_engine::api::{Engine, Keep};
use plc_engine::Error;

mod fixture;
use fixture::Fixture;

/// A real enough PNG to be accepted.
fn write_png(at: &Path, marker: &str) -> PathBuf {
    fs::create_dir_all(at.parent().unwrap()).unwrap();
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(marker.as_bytes());
    fs::write(at, bytes).unwrap();
    at.to_path_buf()
}

#[test]
fn an_image_lands_beside_the_document_and_is_named_plainly() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_png(&downloads.path().join("Muscle Repair.png"), "a");

    let name = fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &original, Keep::Copy)
        .unwrap();

    // A plain filename, never a path: that is what a Markdown reader can follow.
    assert!(!name.contains('/'), "recorded as {name}, which is a path");
    assert_eq!(name, "muscle-repair.png");

    // And it is genuinely beside the document, in the same folder.
    let beside = fixture.root().join("topics/protein").join(&name);
    assert!(
        beside.is_file(),
        "not beside the document: {}",
        beside.display()
    );
    assert!(original.is_file(), "Copy removed the user's own file");
}

#[test]
fn moving_it_takes_the_original_away() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_png(&downloads.path().join("diagram.png"), "a");

    fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &original, Keep::Move)
        .unwrap();

    assert!(!original.exists(), "the original was left behind");
    assert!(fixture.root().join("topics/protein/diagram.png").is_file());
}

#[test]
fn two_pictures_with_one_name_do_not_land_on_each_other() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();

    let first = write_png(&downloads.path().join("one/diagram.png"), "the first");
    let second = write_png(&downloads.path().join("two/diagram.png"), "the second");

    let a = fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &first, Keep::Copy)
        .unwrap();
    let b = fixture
        .engine()
        .document_attach_image(ids.types_of_protein, &second, Keep::Copy)
        .unwrap();

    assert_ne!(a, b, "the second picture overwrote the first");
    let read = |name: &str| fs::read(fixture.root().join("topics/protein").join(name)).unwrap();
    assert!(read(&a).ends_with(b"the first"));
    assert!(read(&b).ends_with(b"the second"));
}

#[test]
fn a_document_that_moves_takes_its_pictures_with_it() {
    // Images are referenced by bare filename, so a document that moved without them would
    // point at names no longer beside it — broken pictures here and in every other reader.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_png(&downloads.path().join("diagram.png"), "the picture");

    let name = fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &original, Keep::Copy)
        .unwrap();
    let view = fixture.engine().document_read(ids.what_protein_is).unwrap();
    fixture
        .engine()
        .document_write(
            ids.what_protein_is,
            format!("{}\n\n![Diagram]({name})", view.body),
            view.revision,
        )
        .unwrap();

    fixture
        .engine()
        .document_move(ids.what_protein_is, ids.in_the_body)
        .unwrap();

    let moved = fixture
        .root()
        .join("topics/protein/in-the-body")
        .join(&name);
    assert!(
        moved.is_file(),
        "the picture stayed behind: {}",
        moved.display()
    );
    assert!(
        !fixture.root().join("topics/protein").join(&name).exists(),
        "the picture was left in both places"
    );
    // Still referenced by the same bare name, so the body needed no rewriting.
    assert!(fixture
        .engine()
        .document_read(ids.what_protein_is)
        .unwrap()
        .body
        .contains(&format!("![Diagram]({name})")));
}

#[test]
fn a_file_that_is_not_an_image_is_refused() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let not_an_image = downloads.path().join("notes.png");
    fs::write(&not_an_image, "just some notes").unwrap();

    assert!(matches!(
        fixture
            .engine()
            .document_attach_image(ids.what_protein_is, &not_an_image, Keep::Copy),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn a_refused_move_leaves_the_original_where_it_is() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let not_an_image = downloads.path().join("notes.png");
    fs::write(&not_an_image, "just some notes").unwrap();

    assert!(fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &not_an_image, Keep::Move)
        .is_err());
    assert!(
        not_an_image.is_file(),
        "a refused attach deleted the user's file"
    );
}

#[test]
fn a_body_naming_a_picture_outside_its_folder_is_refused() {
    // Bodies are plain text a user can edit, so a name in one is not a path to follow.
    let outside = tempfile::tempdir().unwrap();
    let root = outside.path().join("library");
    let mut engine = Engine::create(&root, "Learning").unwrap();
    let topic = engine.topic_create(None, "Protein").unwrap();
    let doc = engine.document_create(topic, "What protein is").unwrap();

    let elsewhere = write_png(&outside.path().join("elsewhere.png"), "not ours");
    let view = engine.document_read(doc).unwrap();
    engine
        .document_write(doc, "![Escape](../../../elsewhere.png)", view.revision)
        .unwrap();

    let nested = engine.topic_create(Some(topic), "In the body").unwrap();

    // The move succeeds. A reference that was already broken must not strand the document as
    // well — refusing here would leave somebody unable to reorganise their own writing
    // because of one bad line they typed.
    engine.document_move(doc, nested).unwrap();

    assert!(
        elsewhere.is_file(),
        "a hand-edited name moved a file outside the library"
    );
    assert!(
        root.join("topics/protein/in-the-body")
            .read_dir()
            .unwrap()
            .count()
            > 0,
        "the document did not arrive"
    );
    assert!(
        !root
            .join("topics/protein/in-the-body/elsewhere.png")
            .exists(),
        "a file from outside the library was dragged in"
    );
}

#[test]
fn an_image_survives_the_index_being_deleted() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_png(&downloads.path().join("diagram.png"), "the picture");

    let name = fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &original, Keep::Copy)
        .unwrap();
    let before = fs::read(fixture.root().join("topics/protein").join(&name)).unwrap();

    fs::remove_dir_all(fixture.root().join(".plc")).unwrap();
    fixture.reopen();

    let after = fs::read(fixture.root().join("topics/protein").join(&name)).unwrap();
    assert_eq!(before, after);
}
