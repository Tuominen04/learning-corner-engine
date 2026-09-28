//! Renaming the library itself.
//!
//! A library's name is what the application calls it: the heading in the sidebar, and the
//! entry on the start screen. It is not the folder's name. The folder is the one the user
//! chose, and renaming the library leaves it alone — unlike a topic or a document, whose
//! files follow their names.

use std::fs;
use std::path::Path;

use plc_engine::api::Engine;
use plc_engine::library::MANIFEST;
use plc_engine::Error;

mod fixture;
use fixture::Fixture;

fn manifest(root: &Path) -> String {
    fs::read_to_string(root.join(MANIFEST)).unwrap()
}

#[test]
fn a_renamed_library_is_called_by_its_new_name() {
    let (_dir, mut engine) = Fixture::empty();
    engine.library_rename("Nutrition").unwrap();
    assert_eq!(engine.status().unwrap().name, "Nutrition");
}

#[test]
fn the_new_name_is_the_one_it_opens_with_next_time() {
    let mut fixture = Fixture::populated();
    fixture.engine().library_rename("Nutrition").unwrap();
    fixture.reopen();

    assert_eq!(fixture.engine().status().unwrap().name, "Nutrition");
    assert!(
        manifest(fixture.root()).contains(r#"name = "Nutrition""#),
        "{}",
        manifest(fixture.root())
    );
}

#[test]
fn the_folder_keeps_its_own_name() {
    // The user chose this folder, and it may hold more than a library's worth of meaning to
    // them. Renaming the library must not move it, rename it, or make another beside it.
    let mut fixture = Fixture::populated();
    let root = fixture.root().to_path_buf();
    let parent = root.parent().unwrap().to_path_buf();
    let neighbours = |dir: &Path| {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let before = neighbours(&parent);
    let inside = neighbours(&root);

    fixture.engine().library_rename("Nutrition").unwrap();

    assert!(
        root.join(MANIFEST).is_file(),
        "the library is not where it was"
    );
    assert_eq!(fixture.engine().root(), root);
    assert_eq!(neighbours(&parent), before);
    assert_eq!(
        neighbours(&root),
        inside,
        "nothing inside was moved or added"
    );
}

#[test]
fn nothing_else_about_the_library_changes() {
    let mut fixture = Fixture::populated();
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
    assert_eq!(
        reread.revision, document.revision,
        "renaming the library rewrote a document"
    );
}

#[test]
fn a_library_needs_a_name() {
    let (dir, mut engine) = Fixture::empty();
    let before = manifest(dir.path());

    for empty in ["", "   ", "\n\t"] {
        assert!(
            matches!(engine.library_rename(empty), Err(Error::Invalid { .. })),
            "{empty:?} was accepted as a name"
        );
    }
    assert_eq!(engine.status().unwrap().name, "Learning");
    assert_eq!(
        manifest(dir.path()),
        before,
        "a refused name was written anyway"
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
    Engine::create(dir.path(), "Learning").unwrap();

    for name in names {
        let mut engine = Engine::open(dir.path()).unwrap();
        engine.library_rename(name).unwrap();
        drop(engine);
        let reopened = Engine::open(dir.path()).unwrap();
        assert_eq!(reopened.status().unwrap().name, name);
    }
}
