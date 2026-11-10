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
    assert_eq!(
        status.documents, 5,
        "two former topics and three documents, all pages now"
    );
    assert!(status.problems.is_empty(), "{:?}", status.problems);
    assert!(status.writable);
}

// ---- the tree of pages ----------------------------------------------------------

fn titles(nodes: &[lc_engine::api::PageNode]) -> Vec<&str> {
    nodes.iter().map(|n| n.title.as_str()).collect()
}

#[test]
fn the_tree_nests_in_the_order_pages_were_made() {
    let mut fixture = Fixture::populated();
    let tree = fixture.engine().page_tree().unwrap();

    assert_eq!(titles(&tree), ["Protein"]);
    let protein = &tree[0];
    assert_eq!(
        titles(&protein.children),
        ["What protein is", "Types of protein", "In the body"]
    );
    assert_eq!(
        titles(&protein.children[2].children),
        ["How protein is used"]
    );
}

#[test]
fn a_page_s_sub_pages_are_in_a_folder_named_like_it_beside_it() {
    let mut fixture = Fixture::populated();
    let root = fixture.root().to_path_buf();
    assert!(root.join("topics/protein.md").is_file());
    assert!(root.join("topics/protein/what-protein-is.md").is_file());
    assert!(root.join("topics/protein/in-the-body.md").is_file());
    assert!(root
        .join("topics/protein/in-the-body/how-protein-is-used.md")
        .is_file());
    // A page with no sub-pages has no folder.
    assert!(!root.join("topics/protein/what-protein-is").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("topics/protein/.order")).unwrap(),
        "what-protein-is\ntypes-of-protein\nin-the-body\n"
    );
    let _ = fixture.engine();
}

#[test]
fn renaming_a_page_takes_its_folder_and_keeps_its_place() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_rename(ids.in_the_body, "Inside us")
        .unwrap();

    let root = fixture.root().to_path_buf();
    assert!(root.join("topics/protein/inside-us.md").is_file());
    assert!(root
        .join("topics/protein/inside-us/how-protein-is-used.md")
        .is_file());
    assert!(!root.join("topics/protein/in-the-body").exists());
    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(
        titles(&tree[0].children),
        ["What protein is", "Types of protein", "Inside us"]
    );
    assert!(fixture.engine().document_read(ids.how_used).is_ok());
}

#[test]
fn a_page_cannot_be_moved_under_itself() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    for under in [ids.protein, ids.in_the_body] {
        assert!(matches!(
            fixture
                .engine()
                .document_move(ids.protein, Some(under), None),
            Err(Error::Invalid { .. })
        ));
    }
    assert!(fixture.root().join("topics/protein.md").is_file());
}

#[test]
fn moving_a_page_to_the_top_carries_its_pages() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_move(ids.in_the_body, None, None)
        .unwrap();

    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(titles(&tree), ["Protein", "In the body"]);
    assert_eq!(titles(&tree[1].children), ["How protein is used"]);
    assert_eq!(
        fixture.engine().document_read(ids.how_used).unwrap().parent,
        Some(ids.in_the_body)
    );
}

#[test]
fn moving_a_page_to_a_position_among_its_new_siblings() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_move(ids.how_used, Some(ids.protein), Some(1))
        .unwrap();

    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(
        titles(&tree[0].children),
        [
            "What protein is",
            "How protein is used",
            "Types of protein",
            "In the body"
        ]
    );
    assert!(
        !fixture.root().join("topics/protein/in-the-body").exists(),
        "the folder its last sub-page left was kept"
    );
}

#[test]
fn reordering_among_siblings_moves_no_file() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let before = fixture.engine().document_read(ids.in_the_body).unwrap();
    fixture
        .engine()
        .document_move(ids.in_the_body, Some(ids.protein), Some(0))
        .unwrap();

    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(
        titles(&tree[0].children),
        ["In the body", "What protein is", "Types of protein"]
    );
    let after = fixture.engine().document_read(ids.in_the_body).unwrap();
    assert_eq!(
        after.revision, before.revision,
        "the page itself was rewritten"
    );
}

#[test]
fn a_hand_made_page_is_listed_after_the_ordered_ones_and_a_missing_one_is_skipped() {
    // `.order` only ever orders what is there: a page added in a file manager still shows,
    // and a name left on the list after its page was deleted by hand breaks nothing.
    let mut fixture = Fixture::populated();
    let root = fixture.root().to_path_buf();
    std::fs::write(
        root.join("topics/protein/aardvarks.md"),
        "+++\nid = \"01J8ZQ9C3P5R7T9V1X3Z5B7D9F\"\ntitle = \"Aardvarks\"\ncreated = 2026-09-06T10:00:00Z\nmodified = 2026-09-06T10:00:00Z\n+++\n",
    )
    .unwrap();
    std::fs::write(
        root.join("topics/protein/.order"),
        "gone\ntypes-of-protein\nwhat-protein-is\n",
    )
    .unwrap();
    fixture.reopen();

    let tree = fixture.engine().page_tree().unwrap();
    assert_eq!(
        titles(&tree[0].children),
        [
            "Types of protein",
            "What protein is",
            "Aardvarks",
            "In the body"
        ]
    );
}

#[test]
fn a_page_needs_a_title() {
    let (_dir, mut engine) = Fixture::empty();
    assert!(matches!(
        engine.document_create(None, "  "),
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
    assert_eq!(read.parent, Some(ids.protein));
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
fn moving_a_page_changes_its_parent_and_nothing_else() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let before = fixture.engine().document_read(ids.what_protein_is).unwrap();
    let after = fixture
        .engine()
        .document_move(ids.what_protein_is, Some(ids.in_the_body), None)
        .unwrap();

    assert_eq!(after.id, before.id);
    assert_eq!(after.body, before.body);
    assert_eq!(after.parent, Some(ids.in_the_body));
}

#[test]
fn deleting_a_document_removes_it_from_the_tree() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    fixture
        .engine()
        .document_delete(ids.types_of_protein, lc_engine::api::Contents::Delete)
        .unwrap();

    assert!(matches!(
        fixture.engine().document_read(ids.types_of_protein),
        Err(Error::NotFound(_))
    ));
    assert_eq!(fixture.engine().status().unwrap().documents, 4);
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
fn a_page_with_sub_pages_is_a_document_like_any_other() {
    // What format 1 called a topic is a page with writing of its own: it reads, and takes a
    // body, like every other.
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let page = fixture.engine().document_read(ids.protein).unwrap();
    assert_eq!(page.title, "Protein");
    assert_eq!(page.parent, None);
    let written = fixture
        .engine()
        .document_write(ids.protein, "What protein is, in brief.", page.revision)
        .unwrap();
    assert_eq!(written.body, "What protein is, in brief.");
}

#[test]
fn a_document_needs_a_title() {
    let (_dir, mut engine) = Fixture::empty();
    let topic = engine.document_create(None, "Protein").unwrap();
    assert!(matches!(
        engine.document_create(Some(topic), ""),
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

    // A page that was a topic in format 1 is found like any page, by its title.
    let pages = fixture.engine().search("protein").unwrap();
    assert!(
        pages
            .iter()
            .any(|h| h.kind == Kind::Document && h.title == "Protein"),
        "expected the Protein page among {pages:?}"
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
    let topic = fixture.engine().document_create(None, "rename3").unwrap();
    fixture
        .engine()
        .document_create(Some(topic), "tuttujuttu")
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
    let topic = fixture.engine().document_create(None, "Probe").unwrap();
    let doc = fixture
        .engine()
        .document_create(Some(topic), "Suomi notes")
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
