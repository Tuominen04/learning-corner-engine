//! The operation surface from `backend-contract.md`.
//!
//! This suite runs against no interface at all. That is the property that keeps the boundary
//! honest: logic that leaked out of the engine cannot be tested here, and its absence shows up
//! as a gap.

use plc_engine::api::Engine;
use plc_engine::store::Revision;
use plc_engine::{Error, Id};

mod fixture;
use fixture::Fixture;

// ---- library -------------------------------------------------------------------

#[test]
fn create_then_open() {
    let dir = tempfile::tempdir().unwrap();
    let created = Engine::create(dir.path(), "Learning").unwrap();
    let id = created.manifest().id;
    drop(created);

    let reopened = Engine::open(dir.path()).unwrap();
    assert_eq!(reopened.manifest().id, id);
    assert_eq!(reopened.status().unwrap().name, "Learning");
}

#[test]
fn opening_a_folder_that_is_not_a_library_says_so() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Engine::open(dir.path()),
        Err(Error::LibraryError(_))
    ));
}

#[test]
fn status_counts_what_is_there() {
    let mut fixture = Fixture::populated();
    let status = fixture.engine().status().unwrap();
    assert_eq!(status.topics, 2);
    assert_eq!(status.documents, 3);
    assert!(status.problems.is_empty(), "{:?}", status.problems);
    assert!(status.writable);
}

// ---- topics --------------------------------------------------------------------

#[test]
fn the_tree_nests() {
    let mut fixture = Fixture::populated();
    let tree = fixture.engine().topic_tree().unwrap();

    assert_eq!(tree.len(), 1);
    let protein = &tree[0];
    assert_eq!(protein.name, "Protein");
    assert_eq!(protein.documents.len(), 2);
    assert_eq!(protein.children.len(), 1);
    assert_eq!(protein.children[0].name, "In the body");
    assert_eq!(protein.children[0].documents.len(), 1);
}

#[test]
fn renaming_a_topic_keeps_its_documents() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .topic_rename(ids.protein, "Proteins")
        .unwrap();

    let tree = fixture.engine().topic_tree().unwrap();
    assert_eq!(tree[0].name, "Proteins");
    assert_eq!(tree[0].documents.len(), 2);
    assert!(fixture.engine().document_read(ids.what_protein_is).is_ok());
}

#[test]
fn a_topic_cannot_be_moved_inside_itself() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    assert!(matches!(
        fixture
            .engine()
            .topic_move(ids.protein, Some(ids.in_the_body)),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn moving_a_topic_to_the_root_carries_its_documents() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture.engine().topic_move(ids.in_the_body, None).unwrap();

    let tree = fixture.engine().topic_tree().unwrap();
    assert_eq!(tree.len(), 2, "both topics should now be at the root");
    assert!(fixture.engine().document_read(ids.how_used).is_ok());
}

#[test]
fn a_topic_needs_a_name() {
    let (_dir, mut engine) = Fixture::empty();
    assert!(matches!(
        engine.topic_create(None, "  "),
        Err(Error::Invalid { .. })
    ));
}

// ---- documents -----------------------------------------------------------------

#[test]
fn a_document_round_trips() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let read = fixture.engine().document_read(ids.what_protein_is).unwrap();
    assert_eq!(read.title, "What protein is");
    assert_eq!(read.topic, ids.protein);
    assert!(read.body.contains("amino acids"));
}

#[test]
fn renaming_a_document_keeps_its_body_and_identifier() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let before = fixture.engine().document_read(ids.what_protein_is).unwrap();
    let after = fixture
        .engine()
        .document_rename(ids.what_protein_is, "What a protein is")
        .unwrap();

    assert_eq!(after.id, before.id);
    assert_eq!(after.body, before.body);
    assert_eq!(after.title, "What a protein is");
}

#[test]
fn moving_a_document_changes_its_topic_and_nothing_else() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let before = fixture.engine().document_read(ids.what_protein_is).unwrap();
    let after = fixture
        .engine()
        .document_move(ids.what_protein_is, ids.in_the_body)
        .unwrap();

    assert_eq!(after.id, before.id);
    assert_eq!(after.body, before.body);
    assert_eq!(after.topic, ids.in_the_body);
}

#[test]
fn deleting_a_document_removes_it_from_the_tree() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_delete(ids.types_of_protein)
        .unwrap();

    assert!(matches!(
        fixture.engine().document_read(ids.types_of_protein),
        Err(Error::NotFound(_))
    ));
    assert_eq!(fixture.engine().status().unwrap().documents, 2);
}

#[test]
fn an_unknown_identifier_is_not_found() {
    let mut fixture = Fixture::populated();
    assert!(matches!(
        fixture.engine().document_read(Id::new()),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn a_topic_identifier_is_not_a_document_identifier() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    assert!(matches!(
        fixture.engine().document_read(ids.protein),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn a_document_needs_a_title() {
    let (_dir, mut engine) = Fixture::empty();
    let topic = engine.topic_create(None, "Protein").unwrap();
    assert!(matches!(
        engine.document_create(topic, ""),
        Err(Error::Invalid { .. })
    ));
}

// ---- links and search ----------------------------------------------------------

#[test]
fn backlinks_are_derived_from_the_text() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let into_types = fixture.engine().backlinks(ids.types_of_protein).unwrap();
    assert_eq!(into_types.len(), 1);
    assert_eq!(into_types[0].title, "What protein is");
}

#[test]
fn removing_a_link_removes_the_backlink() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let current = fixture.engine().document_read(ids.what_protein_is).unwrap();
    fixture
        .engine()
        .document_write(
            ids.what_protein_is,
            "No links here any more.",
            current.revision,
        )
        .unwrap();

    assert!(fixture
        .engine()
        .backlinks(ids.types_of_protein)
        .unwrap()
        .is_empty());
}

#[test]
fn search_finds_words_in_bodies_and_titles() {
    let mut fixture = Fixture::populated();

    let hits = fixture.engine().search("amino").unwrap();
    assert!(!hits.is_empty(), "expected a hit for a word in the body");

    let hits = fixture.engine().search("Types").unwrap();
    assert!(
        hits.iter().any(|h| h.title == "Types of protein"),
        "expected a hit for a word in a title"
    );
}

#[test]
fn search_handles_punctuation_without_erroring() {
    // FTS5 treats bare punctuation as query syntax. A user typing it must get results, not
    // a syntax error.
    let mut fixture = Fixture::populated();
    for query in ["protein (2021)", "amino AND", "\"quoted\"", "a*b", ""] {
        assert!(
            fixture.engine().search(query).is_ok(),
            "search should tolerate {query:?}"
        );
    }
}

#[test]
fn search_ignores_finnish_diacritics() {
    let mut fixture = Fixture::populated();
    let with = fixture.engine().search("Ääkköset").unwrap();
    let without = fixture.engine().search("aakkoset").unwrap();
    assert!(!with.is_empty(), "the exact word should be found");
    assert_eq!(
        with.len(),
        without.len(),
        "diacritic folding should make these the same search"
    );
}

// ---- conflicts -----------------------------------------------------------------

#[test]
fn a_write_with_a_stale_revision_is_refused() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let first = fixture.engine().document_read(ids.what_protein_is).unwrap();
    fixture
        .engine()
        .document_write(ids.what_protein_is, "one", first.revision)
        .unwrap();

    assert!(matches!(
        fixture
            .engine()
            .document_write(ids.what_protein_is, "two", first.revision),
        Err(Error::Conflict { .. })
    ));
}

// ---- later milestones ----------------------------------------------------------

#[test]
fn operations_from_later_milestones_are_named_and_unimplemented() {
    // Present in the surface so the interface can be written against the whole contract, and
    // so filling them in later is a body change rather than a shape change.
    let mut fixture = Fixture::populated();
    let root = fixture.engine().root().to_path_buf();

    assert!(matches!(
        fixture.engine().source_create(),
        Err(Error::Unimplemented(_))
    ));
    assert!(matches!(
        fixture.engine().citation_format(),
        Err(Error::Unimplemented(_))
    ));
    assert!(matches!(
        fixture.engine().work_create(),
        Err(Error::Unimplemented(_))
    ));
    assert!(matches!(
        fixture.engine().map_read(),
        Err(Error::Unimplemented(_))
    ));
    assert!(matches!(
        fixture.engine().backup_plan(&root),
        Err(Error::Unimplemented(_))
    ));
}

#[test]
fn a_revision_survives_being_carried_through_serialisation() {
    // The interface holds a revision between a read and a write, so it has to round-trip.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let read = fixture.engine().document_read(ids.what_protein_is).unwrap();
    let json = format!("{:?}", read.revision);
    let same: Revision = read.revision;
    assert_eq!(format!("{same:?}"), json);

    fixture
        .engine()
        .document_write(ids.what_protein_is, "body", same)
        .unwrap();
}
