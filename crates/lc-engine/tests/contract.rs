//! The operation surface from `backend-contract.md`.
//!
//! This suite runs against no interface at all. That is the property that keeps the boundary
//! honest: logic that leaked out of the engine cannot be tested here, and its absence shows up
//! as a gap.

use lc_engine::api::Engine;
use lc_engine::citation::source::{Contributor, Date, Role, Source, SourceType};
use lc_engine::index::Kind;
use lc_engine::store::Revision;
use lc_engine::{Error, Id};

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
fn a_hit_says_what_it_is() {
    // One index holds topics, documents and sources, so a hit that does not name its kind
    // cannot be acted on: opening a source is not opening a document, and there is no other
    // way to tell one row from another.
    let mut fixture = Fixture::populated();

    let mut source = Source::new(
        SourceType::JournalArticle,
        "Amino acids in adults",
        Date::Year(2021),
    );
    source.contributors = vec![Contributor::person(Role::Author, "Nieminen", "Anna Maria")];
    source.container = Some("Journal of Nutrition".into());
    fixture.engine().source_create(source).unwrap();

    let hits = fixture.engine().search("amino").unwrap();

    // The same word is in a document body and in a source title, so this one query reaches
    // both — which is exactly the case a caller has to be able to tell apart.
    assert!(
        hits.iter().any(|h| h.kind == Kind::Document),
        "expected a document among {hits:?}"
    );
    assert!(
        hits.iter().any(|h| h.kind == Kind::Source),
        "expected a source among {hits:?}"
    );

    let topics = fixture.engine().search("protein").unwrap();
    assert!(
        topics.iter().any(|h| h.kind == Kind::Topic),
        "expected a topic among {topics:?}"
    );

    // And the kind is the entity's own, not guessed from where the word was found.
    let article = hits
        .iter()
        .find(|h| h.title == "Amino acids in adults")
        .expect("the source");
    assert_eq!(article.kind, Kind::Source);
}

#[test]
fn a_partial_word_finds_the_whole_one() {
    // The search box answers while you type, so every query is a partial word until the
    // moment it is not. Matching only whole tokens meant a document called "tuttujuttu" was
    // reported as not existing while it was open on screen — "tuttu" is simply a different
    // token from "tuttujuttu", and nothing in the query said "or anything starting with it".
    let mut fixture = Fixture::populated();
    let topic = fixture.engine().topic_create(None, "rename3").unwrap();
    fixture
        .engine()
        .document_create(topic, "tuttujuttu")
        .unwrap();

    for (typed, expected) in [("tuttu", "tuttujuttu"), ("rename", "rename3")] {
        let hits = fixture.engine().search(typed).unwrap();
        assert!(
            hits.iter().any(|h| h.title == expected),
            "typing {typed:?} should find {expected:?}, got {:?}",
            hits.iter().map(|h| &h.title).collect::<Vec<_>>()
        );
    }

    // Each letter along the way finds it too, which is the point of a box that answers as you
    // type: no keystroke in the middle of a word may report nothing.
    for upto in 1..="tuttujuttu".len() {
        let typed = &"tuttujuttu"[..upto];
        assert!(
            fixture
                .engine()
                .search(typed)
                .unwrap()
                .iter()
                .any(|h| h.title == "tuttujuttu"),
            "typing {typed:?} lost the document"
        );
    }

    // A prefix and not a substring: this is the documented limit, so it is pinned rather than
    // left to be discovered as a surprise later.
    let hits = fixture.engine().search("juttu").unwrap();
    assert!(
        !hits.iter().any(|h| h.title == "tuttujuttu"),
        "matching inside a word needs a different tokeniser — see fts_query"
    );
}

#[test]
fn what_is_searched_is_the_writing_not_the_markers() {
    // The index holds a document as a reader sees it. Markers are addresses, not words, and
    // indexing them costs three things at once: excerpts made of identifiers instead of
    // somebody's own sentences, `cite` matching every document that cites anything, and ULIDs
    // being matchable — by prefix, now that terms are prefixes.
    let mut fixture = Fixture::populated();
    let topic = fixture.engine().topic_create(None, "Probe").unwrap();
    let doc = fixture
        .engine()
        .document_create(topic, "Suomi notes")
        .unwrap();
    let revision = fixture.engine().document_read(doc).unwrap().revision;
    fixture
        .engine()
        .document_write(
            doc,
            "Suomiiiiiii [^cite:01M2BE6DYWCEGGBJGSHYFN6PJJ@34] and more text",
            revision,
        )
        .unwrap();

    for typed in ["cite", "01M2BE6DYWCEGGBJGSHYFN6PJJ", "01M2BE"] {
        let hits = fixture.engine().search(typed).unwrap();
        assert!(
            hits.is_empty(),
            "searching {typed:?} matched {:?} — markers are in the index",
            hits.iter().map(|h| &h.title).collect::<Vec<_>>()
        );
    }

    // The writing itself is still found, and the excerpt is the writing.
    let hits = fixture.engine().search("suomi").unwrap();
    let hit = hits
        .iter()
        .find(|h| h.title == "Suomi notes")
        .expect("the document is still found by its own words");
    assert!(
        !hit.excerpt.contains("[^cite:") && !hit.excerpt.contains("01M2BE"),
        "the excerpt still shows markup: {:?}",
        hit.excerpt
    );
    assert!(
        hit.excerpt.contains("more text"),
        "lost the words around it: {:?}",
        hit.excerpt
    );

    // The file on disk is untouched — the index is derived, the document is not.
    assert!(fixture
        .engine()
        .document_read(doc)
        .unwrap()
        .body
        .contains("[^cite:01M2BE6DYWCEGGBJGSHYFN6PJJ@34]"));

    // And it holds across a reopen, which is the path an existing library takes: an index
    // built under the old rules fails its version check, is thrown away, and is rebuilt from
    // the files. Nobody has to know to delete anything.
    fixture.reopen();
    assert!(
        fixture.engine().search("cite").unwrap().is_empty(),
        "reopening brought the markers back"
    );
    assert!(
        !fixture.engine().search("suomi").unwrap().is_empty(),
        "reopening lost the document"
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

    // source_create and citation_format stood here until M1 filled them in, backup_plan until
    // M2 and work_create until M3, which is the point of the pattern: a body changed, and no
    // client broke.
    assert!(matches!(
        fixture.engine().map_read(),
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

// ---- the wire ------------------------------------------------------------------
//
// Every test above calls the engine directly in Rust. A real client does not: it reaches these
// types across a JSON boundary where numbers are IEEE-754 doubles. Anything above 2^53 is
// rounded in transit, so a value can leave correct and arrive wrong with nothing failing
// anywhere in Rust.
//
// That is not hypothetical. It shipped: revisions carried nanoseconds since the epoch as a
// number, came back rounded, and every single document write failed its conflict check. The
// engine was right, the client was right, and nothing could be saved.

/// JavaScript's largest exactly-representable integer.
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Necessary but **not** sufficient, and worth knowing which.
///
/// This test passes against the buggy version too: `serde_json` handles `u128` exactly, so a
/// Rust-to-Rust round trip never loses anything. The corruption only happens in a *JavaScript*
/// client, where the number becomes a double. The test below is the one that actually catches
/// it — do not delete it thinking this one covers the same ground.
#[test]
fn a_revision_survives_a_json_round_trip() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let issued = fixture
        .engine()
        .document_read(ids.what_protein_is)
        .unwrap()
        .revision;

    let json = serde_json::to_string(&issued).unwrap();
    let returned: Revision = serde_json::from_str(&json).unwrap();

    assert_eq!(
        issued, returned,
        "a revision must survive the journey to a client and back"
    );

    // And it must still be accepted, which is the thing the user actually experiences.
    fixture
        .engine()
        .document_write(
            ids.what_protein_is,
            "written with a round-tripped revision",
            returned,
        )
        .expect("a round-tripped revision must still be accepted");
}

/// **This is the guard.** Verified to fail against the version that shipped the bug.
#[test]
fn nothing_crossing_the_wire_is_a_number_javascript_cannot_hold() {
    // The general property, rather than one type's symptom. Any large integer sent as a JSON
    // number is silently corrupted by a JavaScript client, so none may be sent that way.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let document = fixture.engine().document_read(ids.what_protein_is).unwrap();
    let payload = serde_json::json!({
        "revision": document.revision,
        "status": {
            "topics": fixture.engine().status().unwrap().topics,
            "documents": fixture.engine().status().unwrap().documents,
        },
    });

    let mut unsafe_numbers = Vec::new();
    collect_unsafe_numbers(&payload, String::new(), &mut unsafe_numbers);
    assert!(
        unsafe_numbers.is_empty(),
        "these would lose precision in a JavaScript client: {unsafe_numbers:?}"
    );
}

fn collect_unsafe_numbers(value: &serde_json::Value, path: String, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Number(n) => {
            let too_big = n.as_u64().is_some_and(|v| v > MAX_SAFE_INTEGER)
                || n.as_i64()
                    .is_some_and(|v| v.unsigned_abs() > MAX_SAFE_INTEGER);
            if too_big {
                out.push(format!("{path} = {n}"));
            }
        }
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                collect_unsafe_numbers(child, format!("{path}.{key}"), out);
            }
        }
        serde_json::Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                collect_unsafe_numbers(child, format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}
