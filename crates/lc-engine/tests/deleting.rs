//! Removing a topic, and what happens to what was inside it.
//!
//! The only operation in the engine that destroys somebody's writing on purpose. Everything
//! asserted here is either "the right thing went" or "the wrong thing stayed", and the second
//! kind matters more: a delete that takes one document too many is not a bug somebody notices
//! and reports, it is a bug they discover months later when they go looking for something.

use std::fs;

use lc_engine::api::Contents;
use lc_engine::Error;

mod fixture;
use fixture::Fixture;

#[test]
fn the_plan_names_everything_underneath_not_only_the_first_level() {
    // A topic with one empty-looking sub-topic can hold a year of writing two levels down.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let plan = fixture.engine().topic_delete_plan(ids.protein).unwrap();

    assert_eq!(plan.topics, 1, "the nested topic was not counted");
    let titles: Vec<&str> = plan.documents.iter().map(|d| d.title.as_str()).collect();
    assert!(titles.contains(&"What protein is"));
    assert!(titles.contains(&"Types of protein"));
    assert!(
        titles.contains(&"How protein is used"),
        "the document two levels down was missed: {titles:?}"
    );
}

#[test]
fn the_plan_of_an_empty_topic_is_empty() {
    let (_dir, mut engine) = Fixture::empty();
    let topic = engine.topic_create(None, "Nothing here").unwrap();

    let plan = engine.topic_delete_plan(topic).unwrap();
    assert_eq!(plan.topics, 0);
    assert!(plan.documents.is_empty());
}

#[test]
fn deleting_with_its_contents_takes_the_whole_subtree() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .topic_delete(ids.protein, Contents::Delete)
        .unwrap();

    assert!(!fixture.root().join("topics/protein").exists());
    assert!(fixture.engine().document_read(ids.what_protein_is).is_err());
    assert!(fixture.engine().document_read(ids.how_used).is_err());
    assert!(fixture.engine().topic_tree().unwrap().is_empty());
}

#[test]
fn moving_the_contents_up_keeps_every_one_of_them() {
    // The choice that exists so that deleting a topic is not the same thing as deleting the
    // writing in it.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let before = fixture.engine().document_read(ids.how_used).unwrap().body;

    fixture
        .engine()
        .topic_delete(ids.in_the_body, Contents::MoveUp)
        .unwrap();

    assert!(!fixture.root().join("topics/protein/in-the-body").exists());
    let after = fixture.engine().document_read(ids.how_used).unwrap();
    assert_eq!(after.body, before, "the document changed on its way up");
    assert_eq!(after.topic, ids.protein, "it did not arrive in the parent");

    let tree = fixture.engine().topic_tree().unwrap();
    assert!(tree[0].children.is_empty());
    assert_eq!(tree[0].documents.len(), 3);
}

#[test]
fn a_nested_topic_comes_up_with_its_own_contents_intact() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let deeper = fixture
        .engine()
        .topic_create(Some(ids.in_the_body), "Deeper")
        .unwrap();
    let buried = fixture.engine().document_create(deeper, "Buried").unwrap();

    fixture
        .engine()
        .topic_delete(ids.in_the_body, Contents::MoveUp)
        .unwrap();

    // "Deeper" moved up one, and "Buried" is still inside it rather than scattered.
    let tree = fixture.engine().topic_tree().unwrap();
    assert_eq!(tree[0].children.len(), 1);
    assert_eq!(tree[0].children[0].name, "Deeper");
    assert_eq!(tree[0].children[0].documents.len(), 1);
    assert_eq!(
        fixture.engine().document_read(buried).unwrap().title,
        "Buried"
    );
}

#[test]
fn a_picture_travels_with_the_document_that_moves_up() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let original = downloads.path().join("diagram.png");
    fs::write(&original, b"\x89PNG\r\n\x1a\nbody").unwrap();

    let name = fixture
        .engine()
        .document_attach_image(ids.how_used, &original, lc_engine::api::Keep::Copy)
        .unwrap();
    let view = fixture.engine().document_read(ids.how_used).unwrap();
    fixture
        .engine()
        .document_write(
            ids.how_used,
            format!("{}\n\n![Diagram]({name})", view.body),
            view.revision,
        )
        .unwrap();

    fixture
        .engine()
        .topic_delete(ids.in_the_body, Contents::MoveUp)
        .unwrap();

    assert!(
        fixture
            .engine()
            .document_image(ids.how_used, &name)
            .unwrap()
            .is_some(),
        "the picture was left in the folder that was deleted"
    );
}

#[test]
fn a_root_topic_with_documents_cannot_move_them_up() {
    // There is nowhere above a root topic that a document may live, and both other answers —
    // inventing a topic, or deleting them because there is nowhere to put them — are worse
    // than saying so.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    assert!(matches!(
        fixture.engine().topic_delete(ids.protein, Contents::MoveUp),
        Err(Error::Invalid { .. })
    ));
    assert!(
        fixture.root().join("topics/protein").is_dir(),
        "a refused delete removed the topic anyway"
    );
    assert!(fixture.engine().document_read(ids.what_protein_is).is_ok());
}

#[test]
fn an_empty_root_topic_moves_up_to_nothing_and_goes() {
    let (_dir, mut engine) = Fixture::empty();
    let topic = engine.topic_create(None, "Nothing here").unwrap();

    engine.topic_delete(topic, Contents::MoveUp).unwrap();
    assert!(engine.topic_tree().unwrap().is_empty());
}

#[test]
fn deleting_one_topic_leaves_its_neighbours_alone() {
    // The failure nobody reports, because they find it months later.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let sleep = fixture.engine().topic_create(None, "Sleep").unwrap();
    let notes = fixture.engine().document_create(sleep, "Notes").unwrap();

    fixture
        .engine()
        .topic_delete(ids.protein, Contents::Delete)
        .unwrap();

    assert_eq!(
        fixture.engine().document_read(notes).unwrap().title,
        "Notes"
    );
    let tree = fixture.engine().topic_tree().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Sleep");
}

#[test]
fn a_topic_whose_name_is_a_prefix_of_another_is_not_dragged_in() {
    // `topics/protein` and `topics/protein-and-sleep` are siblings, and comparing paths as
    // strings rather than as paths would take the second one with the first.
    let (_dir, mut engine) = Fixture::empty();
    let protein = engine.topic_create(None, "Protein").unwrap();
    let both = engine.topic_create(None, "Protein and sleep").unwrap();
    let kept = engine.document_create(both, "Kept").unwrap();

    let plan = engine.topic_delete_plan(protein).unwrap();
    assert_eq!(plan.topics, 0, "a sibling was counted as being inside");
    assert!(plan.documents.is_empty());

    engine.topic_delete(protein, Contents::Delete).unwrap();
    assert_eq!(engine.document_read(kept).unwrap().title, "Kept");
}

#[test]
fn a_deleted_topic_stays_deleted_when_the_index_is_rebuilt() {
    // The index is derived. If a delete only removed rows from it, the topic would come back.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .topic_delete(ids.protein, Contents::Delete)
        .unwrap();
    fs::remove_dir_all(fixture.root().join(".plc")).unwrap();
    fixture.reopen();

    assert!(fixture.engine().topic_tree().unwrap().is_empty());
}

#[test]
fn deleting_a_document_leaves_the_others_and_the_topic() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_delete(ids.what_protein_is)
        .unwrap();

    assert!(fixture.engine().document_read(ids.what_protein_is).is_err());
    assert!(fixture.engine().document_read(ids.types_of_protein).is_ok());
    assert!(fixture.root().join("topics/protein").is_dir());
}
