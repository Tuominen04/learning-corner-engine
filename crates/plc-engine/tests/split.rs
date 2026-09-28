//! Taking part of a document out into a new one.
//!
//! The operation the README's example is built on: one topic becomes six documents, by
//! splitting as the writing grows rather than by planning a structure up front.
//!
//! It is also the one operation that rewrites somebody's writing rather than adding to it, so
//! most of what is asserted here is about what must *not* happen.

use std::fs;

use plc_engine::Error;

mod fixture;
use fixture::Fixture;

const CITE: &str = "01J8ZQ8D4Q6S8W0W2Y4A6C8E0G";

/// Put a known body in place and hand back its revision.
fn set(fixture: &mut Fixture, id: plc_engine::Id, body: &str) -> plc_engine::store::Revision {
    let view = fixture.engine().document_read(id).unwrap();
    fixture
        .engine()
        .document_write(id, body, view.revision)
        .unwrap()
        .revision
}

#[test]
fn the_taken_text_moves_and_a_link_is_left_behind() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let body = "What protein is.\n\nThere are many kinds.\n\nAnd that is all.";
    let revision = set(&mut fixture, ids.what_protein_is, body);

    let at = body.find("There").unwrap();
    let split = fixture
        .engine()
        .document_split(
            ids.what_protein_is,
            at..at + "There are many kinds.".len(),
            "Types of protein",
            revision,
        )
        .unwrap();

    assert_eq!(split.taken.body, "There are many kinds.");
    assert_eq!(split.taken.title, "Types of protein");
    assert_eq!(
        split.original.body,
        format!(
            "What protein is.\n\n[[{}]]\n\nAnd that is all.",
            split.taken.id
        ),
    );
}

#[test]
fn the_new_document_is_beside_the_one_it_came_out_of() {
    // Same topic, same folder. The folder is what makes the pictures keep working.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let revision = set(&mut fixture, ids.what_protein_is, "One.\n\nTwo.");

    fixture
        .engine()
        .document_split(ids.what_protein_is, 6..10, "The second part", revision)
        .unwrap();

    assert!(fixture
        .root()
        .join("topics/protein/the-second-part.md")
        .is_file());
}

#[test]
fn a_citation_in_the_taken_text_goes_with_it() {
    // Nothing moves a citation: it is *in* the text, and there is no second record of it. What
    // this asserts is that the derived index agrees afterwards — the new document is the one
    // citing the source, and the original is not.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let body = format!("Plain sentence.\n\nNine acids.[^cite:{CITE}]");
    let revision = set(&mut fixture, ids.what_protein_is, &body);

    let at = body.find("Nine").unwrap();
    let split = fixture
        .engine()
        .document_split(ids.what_protein_is, at..body.len(), "Amino acids", revision)
        .unwrap();

    assert!(split.taken.body.contains(&format!("[^cite:{CITE}]")));
    assert!(!split.original.body.contains("[^cite:"));

    let citing = fixture
        .engine()
        .source_citing_documents(CITE.parse().unwrap())
        .unwrap();
    assert_eq!(
        citing.len(),
        1,
        "the index still thinks the old document cites it"
    );
    assert_eq!(citing[0].id, split.taken.id);
}

#[test]
fn the_link_left_behind_is_a_real_backlink() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let revision = set(
        &mut fixture,
        ids.what_protein_is,
        "Keep this.\n\nTake this.",
    );

    let split = fixture
        .engine()
        .document_split(ids.what_protein_is, 12..22, "Taken", revision)
        .unwrap();

    let backlinks = fixture.engine().backlinks(split.taken.id).unwrap();
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].id, ids.what_protein_is);
}

#[test]
fn a_picture_in_the_taken_text_still_resolves() {
    // The new document is beside the original, so a bare filename in the moved text points at
    // the same file it always did. This is why the split writes beside rather than elsewhere.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let downloads = tempfile::tempdir().unwrap();
    let original = downloads.path().join("diagram.png");
    fs::write(&original, b"\x89PNG\r\n\x1a\nbody").unwrap();

    let name = fixture
        .engine()
        .document_attach_image(ids.what_protein_is, &original, plc_engine::api::Keep::Copy)
        .unwrap();
    let body = format!("Intro.\n\nAs shown ![Diagram]({name}) it works.");
    let revision = set(&mut fixture, ids.what_protein_is, &body);

    let at = body.find("As shown").unwrap();
    let split = fixture
        .engine()
        .document_split(ids.what_protein_is, at..body.len(), "The diagram", revision)
        .unwrap();

    assert!(fixture
        .engine()
        .document_image(split.taken.id, &name)
        .unwrap()
        .is_some());
}

#[test]
fn a_stale_revision_is_refused_rather_than_cutting_the_wrong_place() {
    // Offsets into a file that has changed underneath address different text. Splitting on
    // them would take the wrong words out of somebody's document.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let stale = set(&mut fixture, ids.what_protein_is, "One.\n\nTwo.");
    set(
        &mut fixture,
        ids.what_protein_is,
        "Something else entirely.",
    );

    assert!(matches!(
        fixture
            .engine()
            .document_split(ids.what_protein_is, 6..10, "Taken", stale),
        Err(Error::Conflict { .. })
    ));
    assert_eq!(
        fixture
            .engine()
            .document_read(ids.what_protein_is)
            .unwrap()
            .body,
        "Something else entirely.",
        "a refused split rewrote the document anyway"
    );
}

#[test]
fn a_range_that_would_cut_a_character_in_half_is_refused() {
    // This application is for people writing Finnish and Russian. A byte offset landing inside
    // a character is an ordinary mistake, and slicing on it would take the process down.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let body = "Ääkköset ja öljy.";
    let revision = set(&mut fixture, ids.what_protein_is, body);

    // "Ä" is two bytes, so 1 is inside it.
    assert!(matches!(
        fixture
            .engine()
            .document_split(ids.what_protein_is, 1..body.len(), "Taken", revision),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn a_range_outside_the_document_is_refused() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let revision = set(&mut fixture, ids.what_protein_is, "Short.");

    // Built rather than written as `5..2`, which clippy refuses as a literal — but a caller
    // computing offsets can certainly hand one over, and it must be answered rather than
    // silently treated as empty.
    let backwards = std::ops::Range { start: 5, end: 2 };
    for range in [0..999usize, backwards] {
        assert!(
            matches!(
                fixture.engine().document_split(
                    ids.what_protein_is,
                    range.clone(),
                    "Taken",
                    revision
                ),
                Err(Error::Invalid { .. })
            ),
            "{range:?} was accepted"
        );
    }
}

#[test]
fn taking_nothing_is_refused() {
    // An empty selection, or one that is only whitespace. Both would make an empty document
    // and put a link to it in the middle of a sentence.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let revision = set(&mut fixture, ids.what_protein_is, "One.\n\nTwo.");

    for range in [4..4usize, 4..6] {
        assert!(
            matches!(
                fixture.engine().document_split(
                    ids.what_protein_is,
                    range.clone(),
                    "Taken",
                    revision
                ),
                Err(Error::Invalid { .. })
            ),
            "{range:?} made a document out of nothing"
        );
    }
}

#[test]
fn a_document_without_a_title_is_refused() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let revision = set(&mut fixture, ids.what_protein_is, "One.\n\nTwo.");

    assert!(matches!(
        fixture
            .engine()
            .document_split(ids.what_protein_is, 6..10, "   ", revision),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn splitting_twice_onto_one_title_does_not_land_on_itself() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let mut revision = set(&mut fixture, ids.what_protein_is, "One.\n\nTwo.\n\nThree.");

    let first = fixture
        .engine()
        .document_split(ids.what_protein_is, 6..10, "Notes", revision)
        .unwrap();
    revision = first.original.revision;

    let body = first.original.body.clone();
    let at = body.find("Three.").unwrap();
    let second = fixture
        .engine()
        .document_split(ids.what_protein_is, at..at + 6, "Notes", revision)
        .unwrap();

    assert_ne!(first.taken.id, second.taken.id);
    assert_eq!(first.taken.body, "Two.");
    assert_eq!(second.taken.body, "Three.");
    assert!(fixture.root().join("topics/protein/notes.md").is_file());
    assert!(fixture.root().join("topics/protein/notes-2.md").is_file());
}

#[test]
fn a_split_survives_the_index_being_deleted() {
    // The index is derived. Everything a split changed is in the files, or it did not happen.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let revision = set(
        &mut fixture,
        ids.what_protein_is,
        "Keep this.\n\nTake this.",
    );

    let split = fixture
        .engine()
        .document_split(ids.what_protein_is, 12..22, "Taken", revision)
        .unwrap();

    fs::remove_dir_all(fixture.root().join(".plc")).unwrap();
    fixture.reopen();

    assert_eq!(
        fixture.engine().document_read(split.taken.id).unwrap().body,
        "Take this."
    );
    assert_eq!(
        fixture.engine().backlinks(split.taken.id).unwrap()[0].id,
        ids.what_protein_is
    );
}

#[test]
fn a_range_that_would_cut_a_citation_in_half_is_refused() {
    // Reported from a real library: a split left `3R4QFEM]` sitting in somebody's document.
    // A citation the client does not render is ordinary text to the person reading it, so
    // they can put a selection boundary inside it — and half an identifier is a file carrying
    // text that means nothing, with nothing to say it happened.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let body = format!("Nine acids.[^cite:{CITE}] And more.");
    let revision = set(&mut fixture, ids.what_protein_is, &body);

    let inside = body.find("[^cite:").unwrap() + 20;
    assert!(matches!(
        fixture
            .engine()
            .document_split(ids.what_protein_is, 0..inside, "Taken", revision),
        Err(Error::Invalid { .. })
    ));
    assert!(matches!(
        fixture
            .engine()
            .document_split(ids.what_protein_is, inside..body.len(), "Taken", revision),
        Err(Error::Invalid { .. })
    ));

    assert_eq!(
        fixture
            .engine()
            .document_read(ids.what_protein_is)
            .unwrap()
            .body,
        body,
        "a refused split rewrote the document anyway"
    );
}

#[test]
fn the_markers_a_client_does_not_render_are_protected_too() {
    // These are the dangerous ones, and the reason the check reads the body rather than
    // trusting a client. `@page:44` is refused as a locator and stays literal text; a link to
    // a document that is gone stays literal text. Being text is what lets a selection land
    // inside them.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    for marker in [
        format!("[^cite:{CITE}@page:44]"),
        "[^cite:not-a-ulid]".to_string(),
        "[[gone-from-the-library]]".to_string(),
        "![Diagram](my diagram.png)".to_string(),
    ] {
        let body = format!("Before {marker} after.");
        let revision = set(&mut fixture, ids.what_protein_is, &body);
        let inside = body.find(&marker).unwrap() + 4;

        assert!(
            matches!(
                fixture
                    .engine()
                    .document_split(ids.what_protein_is, 0..inside, "Taken", revision),
                Err(Error::Invalid { .. })
            ),
            "{marker} was cut in half"
        );
    }
}

#[test]
fn a_range_that_only_touches_a_citation_is_allowed() {
    // Touching is not cutting. A range ending the moment a citation begins does not include
    // it, and refusing there would block an ordinary split for no reason.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let body = format!("Nine acids.[^cite:{CITE}] And more.");
    let revision = set(&mut fixture, ids.what_protein_is, &body);

    let opens = body.find("[^cite:").unwrap();
    let closes = opens + format!("[^cite:{CITE}]").len();

    assert!(fixture
        .engine()
        .document_split(ids.what_protein_is, 0..opens, "Taken", revision)
        .is_ok());

    let revision = fixture
        .engine()
        .document_read(ids.what_protein_is)
        .unwrap()
        .revision;
    let body = fixture
        .engine()
        .document_read(ids.what_protein_is)
        .unwrap()
        .body;
    let _ = closes;
    let from = body.find("[^cite:").unwrap();
    assert!(fixture
        .engine()
        .document_split(
            ids.what_protein_is,
            from..body.len(),
            "Also taken",
            revision
        )
        .is_ok());
}
