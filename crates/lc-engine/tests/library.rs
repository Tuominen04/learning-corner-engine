//! A library's name, and the folder that holds it.
//!
//! A new library gets a folder of its own, named after it, inside wherever the person chose.
//! Renaming the library renames that folder too, the way a topic's follows its name — but
//! only while the folder is the library's alone. One that holds other things as well belongs to
//! whoever put them there, and keeps its name.
//!
//! Every library here is made one folder down inside a temporary directory. A test that
//! renamed a library made at the top of one would move the temporary directory itself, and
//! leave it behind when the test ended.

use std::fs;
use std::path::Path;

use lc_engine::api::{Engine, LibraryFolder};
use lc_engine::library::MANIFEST;
use lc_engine::Error;

mod fixture;
use fixture::Fixture;

fn manifest(root: &Path) -> String {
    fs::read_to_string(root.join(MANIFEST)).unwrap()
}

/// The names in a folder, sorted, so a test can say what a person would see there.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// A library called `name`, in `dir/folder`.
fn library_at(dir: &Path, folder: &str, name: &str) -> Engine {
    Engine::create(dir.join(folder), name).unwrap()
}

// ---- making one --------------------------------------------------------------------

#[test]
fn a_new_library_gets_a_folder_of_its_own_named_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::create_in(dir.path(), "I testing forders").unwrap();

    assert_eq!(engine.root(), dir.path().join("i-testing-forders"));
    assert!(engine.root().join(MANIFEST).is_file());
    assert_eq!(listing(dir.path()), ["i-testing-forders"]);
    assert_eq!(engine.status().unwrap().name, "I testing forders");
}

#[test]
fn its_folder_keeps_the_letters_of_its_name() {
    let dir = tempfile::tempdir().unwrap();
    Engine::create_in(dir.path(), "Ravitsemus ja terveys").unwrap();
    Engine::create_in(dir.path(), "Русский язык").unwrap();
    assert_eq!(
        listing(dir.path()),
        ["ravitsemus-ja-terveys", "русский-язык"]
    );
}

#[test]
fn a_name_already_taken_there_gets_a_number() {
    // Taken whatever its case: `learning` beside `Learning` is one folder on some systems, and
    // a mistake waiting to happen on the rest.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("Learning")).unwrap();
    fs::write(
        dir.path().join("Learning").join("mine.txt"),
        "not a library",
    )
    .unwrap();

    let engine = Engine::create_in(dir.path(), "Learning").unwrap();

    assert_eq!(engine.root(), dir.path().join("learning-2"));
    assert_eq!(listing(&dir.path().join("Learning")), ["mine.txt"]);
}

#[test]
fn a_library_cannot_be_made_inside_another() {
    let dir = tempfile::tempdir().unwrap();
    let outer = library_at(dir.path(), "outer", "Outer");
    let root = outer.root().to_path_buf();
    let before = listing(&root);

    let refused = Engine::create_in(&root, "Inner").err().unwrap();
    assert!(matches!(refused, Error::Invalid { .. }), "{refused}");
    assert!(refused.to_string().contains("Outer"), "{refused}");

    // Anywhere inside it, not only at the top — and the exact-path way in as well.
    assert!(matches!(
        Engine::create_in(root.join("topics"), "Inner"),
        Err(Error::Invalid { .. })
    ));
    assert!(matches!(
        Engine::create(root.join("inner"), "Inner"),
        Err(Error::Invalid { .. })
    ));

    assert_eq!(
        listing(&root),
        before,
        "something was made inside the library"
    );
}

#[test]
fn nothing_is_made_for_a_library_without_a_name() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Engine::create_in(dir.path(), "   "),
        Err(Error::Invalid { .. })
    ));
    assert!(listing(dir.path()).is_empty());
}

// ---- renaming it -------------------------------------------------------------------

#[test]
fn a_folder_holding_only_the_library_follows_its_name() {
    let mut fixture = Fixture::populated_in("testing");
    let dir = fixture.dir().to_path_buf();

    let folder = fixture.engine().library_rename("Nutrition").unwrap();

    assert_eq!(folder, LibraryFolder::Follows);
    assert_eq!(fixture.root(), dir.join("nutrition"));
    assert_eq!(
        listing(&dir),
        ["nutrition"],
        "the old folder is still there"
    );
    assert!(manifest(fixture.root()).contains(r#"name = "Nutrition""#));
}

#[test]
fn the_library_works_from_where_its_folder_went() {
    // Everything that finds a file by path has to find it at the new one: reading, writing —
    // the index's own database included, since it moved with the folder — searching, and
    // opening again the way the application does when it starts.
    let mut fixture = Fixture::populated_in("testing");
    let ids = fixture.ids();
    fixture.engine().library_rename("Nutrition").unwrap();

    let document = fixture.engine().document_read(ids.what_protein_is).unwrap();
    fixture
        .engine()
        .document_write(
            ids.what_protein_is,
            "Written after the move.",
            document.revision,
        )
        .unwrap();
    fixture
        .engine()
        .topic_create(None, "Made after the move")
        .unwrap();
    assert!(!fixture.engine().search("Written").unwrap().is_empty());

    fixture.reopen();
    let status = fixture.engine().status().unwrap();
    assert_eq!(status.name, "Nutrition");
    assert!(status.problems.is_empty(), "{:?}", status.problems);
    let reread = fixture.engine().document_read(ids.what_protein_is).unwrap();
    assert_eq!(reread.body, "Written after the move.");
}

#[test]
fn nothing_else_about_the_library_changes() {
    let mut fixture = Fixture::populated_in("testing");
    let ids = fixture.ids();
    let before = fixture.engine().manifest().clone();
    let status = fixture.engine().status().unwrap();
    let document = fixture.engine().document_read(ids.what_protein_is).unwrap();

    fixture.engine().library_rename("Nutrition").unwrap();
    fixture.reopen();

    let after = fixture.engine().manifest().clone();
    assert_eq!(after.id, before.id, "the library is still the same library");
    assert_eq!(after.format_version, before.format_version);
    assert_eq!(after.created, before.created);

    let now = fixture.engine().status().unwrap();
    assert_eq!(
        (now.topics, now.documents),
        (status.topics, status.documents)
    );
    let reread = fixture.engine().document_read(ids.what_protein_is).unwrap();
    assert_eq!(reread.body, document.body);
    // An editor holding this revision must still be able to save once the folder has moved.
    assert_eq!(
        reread.revision, document.revision,
        "moving the folder changed a document's revision"
    );
}

#[test]
fn a_folder_holding_other_things_keeps_its_name() {
    // The folder from the report: a library made straight into `learning-corner`, beside
    // folders that are not the library's. Renaming it must not move them.
    let dir = tempfile::tempdir().unwrap();
    let mut engine = library_at(dir.path(), "learning-corner", "I testing forders");
    let root = engine.root().to_path_buf();
    fs::create_dir(root.join("Testing")).unwrap();
    fs::write(root.join("Testing").join("notes.md"), "somebody else's").unwrap();

    let folder = engine.library_rename("Nutrition").unwrap();

    assert_eq!(folder, LibraryFolder::Shared);
    assert_eq!(engine.root(), root);
    assert!(root.join("Testing").join("notes.md").is_file());
    assert_eq!(
        engine.status().unwrap().name,
        "Nutrition",
        "the name changes all the same"
    );
}

#[test]
fn a_file_managers_notes_are_no_reason_to_keep_the_folder() {
    // Dolphin leaves `.directory` in any folder whose view was changed. It belongs to the
    // folder, and goes where the folder goes.
    let dir = tempfile::tempdir().unwrap();
    let mut engine = library_at(dir.path(), "testing", "Learning2");
    fs::write(engine.root().join(".directory"), "[Dolphin]\nViewMode=1\n").unwrap();

    assert_eq!(
        engine.library_rename("Nutrition").unwrap(),
        LibraryFolder::Follows
    );
    assert!(dir.path().join("nutrition").join(".directory").is_file());
}

#[test]
fn a_name_taken_beside_it_gets_a_number() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("nutrition")).unwrap();
    fs::write(
        dir.path().join("nutrition").join("mine.txt"),
        "not a library",
    )
    .unwrap();
    let mut engine = library_at(dir.path(), "testing", "Learning");

    engine.library_rename("Nutrition").unwrap();

    assert_eq!(engine.root(), dir.path().join("nutrition-2"));
    assert_eq!(listing(&dir.path().join("nutrition")), ["mine.txt"]);
}

#[test]
fn a_folder_already_named_for_it_stays_where_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = library_at(dir.path(), "nutrition", "Learning");

    assert_eq!(
        engine.library_rename("Nutrition").unwrap(),
        LibraryFolder::Follows
    );
    assert_eq!(engine.root(), dir.path().join("nutrition"));
    assert_eq!(listing(dir.path()), ["nutrition"]);
}

#[test]
fn a_library_needs_a_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = library_at(dir.path(), "testing", "Learning");
    let root = engine.root().to_path_buf();
    let before = manifest(&root);

    for empty in ["", "   ", "\n\t"] {
        assert!(
            matches!(engine.library_rename(empty), Err(Error::Invalid { .. })),
            "{empty:?} was accepted as a name"
        );
    }
    assert_eq!(engine.status().unwrap().name, "Learning");
    assert_eq!(manifest(&root), before, "a refused name was written anyway");
    assert_eq!(
        listing(dir.path()),
        ["testing"],
        "a refused name moved the folder"
    );
}

#[test]
fn any_name_a_person_would_give_it_survives_the_file() {
    // Written into TOML, so quotes and backslashes have to come back as they went in, and the
    // languages this application is for have to come back at all.
    let names = [
        "Ravinto ja terveys",
        "Русский язык",
        r#"Notes on "protein""#,
        r"C:\ is not a path here",
        "Karjalan runot — kerätty ja selitetty",
    ];
    let dir = tempfile::tempdir().unwrap();
    let mut root = library_at(dir.path(), "learning", "Learning")
        .root()
        .to_path_buf();

    for name in names {
        let mut engine = Engine::open(&root).unwrap();
        engine.library_rename(name).unwrap();
        root = engine.root().to_path_buf();
        drop(engine);
        let reopened = Engine::open(&root).unwrap();
        assert_eq!(reopened.status().unwrap().name, name);
    }
}
