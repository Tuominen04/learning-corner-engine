//! Removing a page, and what happens to the pages under it.
//!
//! The only operation in the engine that destroys somebody's writing on purpose. Everything
//! asserted here is either "the right thing went" or "the wrong thing stayed", and the second
//! kind matters more: a delete that takes one page too many is not a bug somebody notices and
//! reports, it is a bug they discover months later when they go looking for something.

use std::fs;

use lc_engine::api::{Contents, PageNode};

mod fixture;
use fixture::Fixture;

fn titles(nodes: &[PageNode]) -> Vec<&str> {
    nodes.iter().map(|n| n.title.as_str()).collect()
}

#[test]
fn the_plan_names_everything_underneath_not_only_the_first_level() {
    // A page with one empty-looking sub-page can hold a year of writing two levels down.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let plan = fixture.engine().document_delete_plan(ids.protein).unwrap();

    let titles: Vec<&str> = plan.pages.iter().map(|d| d.title.as_str()).collect();
    assert!(titles.contains(&"What protein is"));
    assert!(titles.contains(&"Types of protein"));
    assert!(titles.contains(&"In the body"));
    assert!(
        titles.contains(&"How protein is used"),
        "the page two levels down was missed: {titles:?}"
    );
    assert!(
        !titles.contains(&"Protein"),
        "the page itself is not under itself"
    );
}

#[test]
fn the_plan_of_a_page_with_nothing_under_it_is_empty() {
    let (_dir, mut engine) = Fixture::empty();
    let page = engine.document_create(None, "Nothing here").unwrap();

    let plan = engine.document_delete_plan(page).unwrap();
    assert!(plan.pages.is_empty());
}

#[test]
fn deleting_with_its_contents_takes_the_whole_subtree() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_delete(ids.protein, Contents::Delete)
        .unwrap();

    assert!(!fixture.root().join("topics/protein").exists());
    assert!(!fixture.root().join("topics/protein.md").exists());
    assert!(fixture.engine().document_read(ids.protein).is_err());
    assert!(fixture.engine().document_read(ids.what_protein_is).is_err());
    assert!(fixture.engine().document_read(ids.how_used).is_err());
    assert!(fixture.engine().page_tree().unwrap().is_empty());
}

#[test]
fn moving_the_contents_up_keeps_every_one_of_them_in_its_place() {
    // The choice that exists so that deleting a page is not the same thing as deleting the
    // writing under it.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let before = fixture.engine().document_read(ids.how_used).unwrap().body;

    fixture
        .engine()
        .document_delete(ids.in_the_body, Contents::MoveUp)
        .unwrap();

    assert!(!fixture.root().join("topics/protein/in-the-body").exists());
    assert!(!fixture
        .root()
        .join("topics/protein/in-the-body.md")
        .exists());
    let after = fixture.engine().document_read(ids.how_used).unwrap();
    assert_eq!(after.body, before, "the page changed on its way up");
    assert_eq!(
        after.parent,
        Some(ids.protein),
        "it did not arrive in the parent"
    );

    // Where "In the body" was: after the two pages made before it.
    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(
        titles(&tree[0].children),
        ["What protein is", "Types of protein", "How protein is used"]
    );
}

#[test]
fn a_sub_page_comes_up_with_its_own_pages_intact() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let deeper = fixture
        .engine()
        .document_create(Some(ids.in_the_body), "Deeper")
        .unwrap();
    let buried = fixture
        .engine()
        .document_create(Some(deeper), "Buried")
        .unwrap();

    fixture
        .engine()
        .document_delete(ids.in_the_body, Contents::MoveUp)
        .unwrap();

    // "Deeper" moved up one, and "Buried" is still under it rather than scattered.
    let tree = fixture.engine().page_tree().unwrap();
    let deeper_node = tree[0]
        .children
        .iter()
        .find(|n| n.title == "Deeper")
        .expect("Deeper moved up");
    assert_eq!(titles(&deeper_node.children), ["Buried"]);
    assert_eq!(
        fixture.engine().document_read(buried).unwrap().parent,
        Some(deeper)
    );
}

#[test]
fn a_picture_travels_with_the_page_that_moves_up() {
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
        .document_delete(ids.in_the_body, Contents::MoveUp)
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
fn a_top_page_s_sub_pages_move_up_to_the_top() {
    // In format 1 a document could not sit outside a topic, so this was refused. A page can
    // be anywhere, so the pages under a top page move up to the top like any others.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_delete(ids.protein, Contents::MoveUp)
        .unwrap();

    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(
        titles(&tree),
        ["What protein is", "Types of protein", "In the body"]
    );
    assert_eq!(
        fixture
            .engine()
            .document_read(ids.what_protein_is)
            .unwrap()
            .parent,
        None
    );
    assert!(!fixture.root().join("topics/protein").exists());
}

#[test]
fn a_page_with_nothing_under_it_goes_whichever_is_chosen() {
    let (_dir, mut engine) = Fixture::empty();
    let page = engine.document_create(None, "Nothing here").unwrap();

    engine.document_delete(page, Contents::MoveUp).unwrap();
    assert!(engine.page_tree().unwrap().is_empty());
}

#[test]
fn deleting_one_page_leaves_its_neighbours_alone() {
    // The failure nobody reports, because they find it months later.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let sleep = fixture.engine().document_create(None, "Sleep").unwrap();
    let notes = fixture
        .engine()
        .document_create(Some(sleep), "Notes")
        .unwrap();

    fixture
        .engine()
        .document_delete(ids.protein, Contents::Delete)
        .unwrap();

    assert_eq!(
        fixture.engine().document_read(notes).unwrap().title,
        "Notes"
    );
    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(titles(&tree), ["Sleep"]);
    assert_eq!(titles(&tree[0].children), ["Notes"]);
}

#[test]
fn a_page_whose_name_is_a_prefix_of_another_does_not_drag_it_in() {
    // `topics/protein` and `topics/protein-and-sleep` are siblings, and comparing paths as
    // strings rather than as paths would take the second one with the first.
    let (_dir, mut engine) = Fixture::empty();
    let protein = engine.document_create(None, "Protein").unwrap();
    let both = engine.document_create(None, "Protein and sleep").unwrap();
    let kept = engine.document_create(Some(both), "Kept").unwrap();

    let plan = engine.document_delete_plan(protein).unwrap();
    assert!(
        plan.pages.is_empty(),
        "a sibling was counted as being under it"
    );

    engine.document_delete(protein, Contents::Delete).unwrap();
    assert_eq!(engine.document_read(kept).unwrap().title, "Kept");
}

#[test]
fn a_deleted_page_stays_deleted_when_the_index_is_rebuilt() {
    // The index is derived. If a delete only removed rows from it, the page would come back.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_delete(ids.protein, Contents::Delete)
        .unwrap();
    fs::remove_dir_all(fixture.root().join(".plc")).unwrap();
    fixture.reopen();

    assert!(fixture.engine().page_tree().unwrap().is_empty());
}

#[test]
fn deleting_the_last_sub_page_takes_the_folder_with_it() {
    // A page with no sub-pages has no folder: the folder comes with the first and goes with
    // the last, `.order` and all.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_delete(ids.how_used, Contents::Delete)
        .unwrap();

    assert!(fixture
        .root()
        .join("topics/protein/in-the-body.md")
        .is_file());
    assert!(
        !fixture.root().join("topics/protein/in-the-body").exists(),
        "an empty folder of sub-pages was left behind"
    );
    assert!(fixture.engine().document_read(ids.types_of_protein).is_ok());
}
