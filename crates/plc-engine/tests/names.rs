//! Filenames that say what is in them.
//!
//! A user is told their library is theirs to open in a file manager, and that promise is the
//! reason this application keeps Markdown in folders rather than rows in a database. A file
//! called `untitled-2.md` holding a document called "tuttujuttu" breaks it — not by losing
//! anything, but by making the folder useless to read, which is the whole point of the folder.
//!
//! Identity is the ULID inside the file, so a name is free to change: nothing here can break a
//! citation, a link, or a document somebody has open.

use std::fs;
use std::path::Path;

mod fixture;
use fixture::Fixture;

/// The names in a folder, sorted, so a test can say what a user would see.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn a_renamed_document_takes_its_filename_with_it() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let protein = fixture.root().join("topics/protein");

    assert!(protein.join("what-protein-is.md").is_file());

    fixture
        .engine()
        .document_rename(ids.what_protein_is, "What a protein really is")
        .unwrap();

    assert!(
        protein.join("what-a-protein-really-is.md").is_file(),
        "the folder still reads {:?}",
        listing(&protein)
    );
    assert!(!protein.join("what-protein-is.md").exists());
}

#[test]
fn the_document_is_still_the_same_document_afterwards() {
    // The file moved, so everything that finds a document by anything other than its path has
    // to still work: reading it by id, its body, its identity, and the links pointing at it.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let before = fixture.engine().document_read(ids.what_protein_is).unwrap();
    let backlinks_before = fixture.engine().backlinks(ids.types_of_protein).unwrap();

    let after = fixture
        .engine()
        .document_rename(ids.what_protein_is, "What a protein really is")
        .unwrap();

    assert_eq!(after.id, before.id);
    assert_eq!(after.body, before.body);
    assert_eq!(after.title, "What a protein really is");

    let read_again = fixture.engine().document_read(ids.what_protein_is).unwrap();
    assert_eq!(read_again.body, before.body);

    let backlinks_after = fixture.engine().backlinks(ids.types_of_protein).unwrap();
    assert_eq!(
        backlinks_after.len(),
        backlinks_before.len(),
        "a link broke when the file moved"
    );
}

#[test]
fn the_revision_a_rename_hands_back_is_the_one_the_file_has() {
    // The interface writes with the revision its last call returned. A rename rewrites the
    // file — the title lives in the front matter — so a revision taken before that write is
    // stale, and the next keystroke would be refused as a conflict against a change the user
    // had just made themselves. That is a defect this application has shipped once already.
    //
    // A revision is modification time and size, both of which survive a rename, so the move
    // itself cannot invalidate one. The write before it can.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let renamed = fixture
        .engine()
        .document_rename(ids.what_protein_is, "Something else entirely")
        .unwrap();

    fixture
        .engine()
        .document_write(
            ids.what_protein_is,
            "Written after the rename.",
            renamed.revision,
        )
        .expect("the revision from a rename was refused by the next write");
}

#[test]
fn renaming_a_document_to_the_name_it_has_does_not_number_it() {
    // Opening the rename box and pressing Enter without changing anything is an ordinary thing
    // to do, and `what-protein-is-2.md` would be an absurd result.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let protein = fixture.root().join("topics/protein");

    fixture
        .engine()
        .document_rename(ids.what_protein_is, "What protein is")
        .unwrap();

    assert!(protein.join("what-protein-is.md").is_file());
    assert!(!protein.join("what-protein-is-2.md").exists());
}

#[test]
fn a_rename_onto_a_name_already_in_use_does_not_land_on_it() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let protein = fixture.root().join("topics/protein");

    fixture
        .engine()
        .document_rename(ids.what_protein_is, "Types of protein")
        .unwrap();

    // Both are still there, and the one that was already called that is untouched.
    assert!(protein.join("types-of-protein.md").is_file());
    assert!(protein.join("types-of-protein-2.md").is_file());
    assert_eq!(
        fixture
            .engine()
            .document_read(ids.types_of_protein)
            .unwrap()
            .title,
        "Types of protein"
    );
    assert_eq!(
        fixture
            .engine()
            .document_read(ids.what_protein_is)
            .unwrap()
            .title,
        "Types of protein"
    );
}

#[test]
fn a_renamed_topic_takes_its_folder_with_it() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let topics = fixture.root().join("topics");

    assert!(topics.join("protein").is_dir());

    fixture
        .engine()
        .topic_rename(ids.protein, "Nutrition")
        .unwrap();

    assert!(
        topics.join("nutrition").is_dir(),
        "the folder still reads {:?}",
        listing(&topics)
    );
    assert!(!topics.join("protein").exists());
}

#[test]
fn everything_under_a_renamed_topic_comes_with_it() {
    // A folder rename moves the subtree in one operation. What has to hold afterwards is that
    // the documents, the nested topics and the pictures are all still found — the index is the
    // only thing that knew where any of them were.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let before = fixture.engine().document_read(ids.how_used).unwrap();

    fixture
        .engine()
        .topic_rename(ids.protein, "Nutrition")
        .unwrap();

    let nested = fixture.root().join("topics/nutrition/in-the-body");
    assert!(nested.is_dir(), "the nested topic did not come along");

    let after = fixture.engine().document_read(ids.how_used).unwrap();
    assert_eq!(after.body, before.body);
    assert_eq!(after.id, before.id);

    // And the tree still describes the library, rather than where it used to be.
    let tree = fixture.engine().topic_tree().unwrap();
    assert_eq!(tree[0].name, "Nutrition");
    assert_eq!(tree[0].documents.len(), 2);
    assert_eq!(tree[0].children[0].name, "In the body");
}

#[test]
fn a_renamed_nested_topic_stays_under_its_parent() {
    // The new folder goes beside the old one, not at the top. Getting this wrong would take a
    // sub-topic out of the topic it belongs to every time somebody corrected its name.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .topic_rename(ids.in_the_body, "Around the body")
        .unwrap();

    assert!(fixture
        .root()
        .join("topics/protein/around-the-body")
        .is_dir());
    assert!(!fixture.root().join("topics/around-the-body").exists());

    let tree = fixture.engine().topic_tree().unwrap();
    assert_eq!(tree[0].name, "Protein");
    assert_eq!(tree[0].children[0].name, "Around the body");
    assert_eq!(tree[0].children[0].documents.len(), 1);
}

#[test]
fn a_picture_still_sits_beside_its_document_after_both_are_renamed() {
    // Pictures are referenced by bare filename, so what matters is that they stay in the same
    // folder as the document naming them — through a document rename, which moves the file
    // inside the folder, and a topic rename, which moves the folder.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let original = downloads.path().join("diagram.png");
    fs::write(&original, b"\x89PNG\r\n\x1a\nbody").unwrap();

    let name = fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &original, plc_engine::api::Keep::Copy)
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
        .document_rename(ids.what_protein_is, "What a protein really is")
        .unwrap();
    fixture
        .engine()
        .topic_rename(ids.protein, "Nutrition")
        .unwrap();

    let found = fixture
        .engine()
        .document_image(ids.what_protein_is, &name)
        .unwrap()
        .expect("the picture was lost between the two renames");
    assert!(found.starts_with(fixture.root().join("topics/nutrition")));
    assert!(fixture
        .root()
        .join("topics/nutrition/what-a-protein-really-is.md")
        .is_file());
}

#[test]
fn a_renamed_topic_keeps_its_name_even_if_the_folder_cannot_be_moved() {
    // The name is written before the folder moves, deliberately: a failure to move must not
    // lose what the user just typed. This is the ordinary case, and it asserts the ordering by
    // asserting what the topic says rather than only where it lives.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .topic_rename(ids.protein, "Nutrition")
        .unwrap();

    let toml = fs::read_to_string(fixture.root().join("topics/nutrition/topic.toml")).unwrap();
    assert!(
        toml.contains("Nutrition"),
        "the folder moved but the topic did not say its new name: {toml}"
    );
}

#[test]
fn names_survive_the_index_being_deleted() {
    // The index is derived and disposable. If a rename only moved a row in it, deleting the
    // index would bring the old names back.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_rename(ids.what_protein_is, "What a protein really is")
        .unwrap();
    fixture
        .engine()
        .topic_rename(ids.protein, "Nutrition")
        .unwrap();

    fs::remove_dir_all(fixture.root().join(".plc")).unwrap();
    fixture.reopen();

    assert_eq!(
        listing(&fixture.root().join("topics/nutrition"))
            .into_iter()
            .filter(|n| n.ends_with(".md"))
            .collect::<Vec<_>>(),
        vec!["types-of-protein.md", "what-a-protein-really-is.md"]
    );
    let tree = fixture.engine().topic_tree().unwrap();
    assert_eq!(tree[0].name, "Nutrition");
}
