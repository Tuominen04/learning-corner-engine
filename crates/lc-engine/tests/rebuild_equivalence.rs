//! The test that decides whether ADR 0003 is real.
//!
//! Files are the source of truth and the index is a disposable cache. The way that claim stops
//! being true is gradually: one field cached that is not derivable, one write that updates the
//! index but not the file. This test catches that the moment it happens.
//!
//! It belongs here from M0 rather than later. Retrofitting it means discovering everything
//! that already violates it, at the point when all of it is load-bearing.

use lc_engine::api::{DocumentView, Engine, TopicNode};
use lc_engine::Id;

mod fixture;
use fixture::Fixture;

/// Everything the interface can observe. If deleting the index changes any of it, the index
/// is holding something the files do not.
#[derive(Debug, PartialEq)]
struct Observations {
    topics: usize,
    documents: usize,
    tree: Vec<TopicNode>,
    documents_read: Vec<DocumentView>,
    backlinks: Vec<(Id, Vec<String>)>,
    searches: Vec<(String, Vec<String>)>,
}

fn observe(engine: &Engine) -> Observations {
    let status = engine.status().unwrap();
    let tree = engine.topic_tree().unwrap();

    let mut ids = Vec::new();
    collect_ids(&tree, &mut ids);
    ids.sort();

    let documents_read = ids
        .iter()
        .map(|id| engine.document_read(*id).unwrap())
        .collect();
    let backlinks = ids
        .iter()
        .map(|id| {
            let mut titles: Vec<String> = engine
                .backlinks(*id)
                .unwrap()
                .into_iter()
                .map(|d| d.title)
                .collect();
            titles.sort();
            (*id, titles)
        })
        .collect();

    let searches = [
        "protein",
        "amino",
        "Ääkköset",
        "любил",
        "nothing-matches-this",
    ]
    .iter()
    .map(|q| {
        let mut titles: Vec<String> = engine
            .search(q)
            .unwrap()
            .into_iter()
            .map(|h| h.title)
            .collect();
        titles.sort();
        (q.to_string(), titles)
    })
    .collect();

    Observations {
        topics: status.topics,
        documents: status.documents,
        tree,
        documents_read,
        backlinks,
        searches,
    }
}

fn collect_ids(nodes: &[TopicNode], out: &mut Vec<Id>) {
    for node in nodes {
        out.extend(node.documents.iter().map(|d| d.id));
        collect_ids(&node.children, out);
    }
}

#[test]
fn deleting_the_index_changes_nothing_observable() {
    let mut fixture = Fixture::populated();

    let before = observe(fixture.engine());

    fixture.delete_index();
    assert!(!fixture.index_path().exists(), "the index should be gone");

    fixture.reopen();
    assert!(
        fixture.index_path().exists(),
        "reopening should rebuild the index"
    );

    let after = observe(fixture.engine());

    assert_eq!(
        before, after,
        "the index is holding something that is not derivable from the files"
    );
}

#[test]
fn an_explicit_rebuild_changes_nothing_observable() {
    let mut fixture = Fixture::populated();
    let before = observe(fixture.engine());

    let summary = fixture.engine().rebuild_index().unwrap();
    assert_eq!(summary.documents, before.documents);
    assert_eq!(summary.topics, before.topics);

    assert_eq!(before, observe(fixture.engine()));
}

#[test]
fn incremental_writes_agree_with_a_full_rebuild() {
    // The index is updated in place when a document is written, and rebuilt wholesale on a
    // scan. If those two paths ever disagree, one of them is wrong — and the incremental one
    // is the one that runs constantly.
    let mut fixture = Fixture::populated();

    let ids: Vec<Id> = {
        let tree = fixture.engine().topic_tree().unwrap();
        let mut ids = Vec::new();
        collect_ids(&tree, &mut ids);
        ids.sort();
        ids
    };

    for (n, id) in ids.iter().enumerate() {
        let current = fixture.engine().document_read(*id).unwrap();
        let body = format!("{}\n\nRevised paragraph {n}.", current.body);
        fixture
            .engine()
            .document_write(*id, body, current.revision)
            .unwrap();
    }

    let incremental = observe(fixture.engine());
    fixture.engine().rebuild_index().unwrap();
    let rebuilt = observe(fixture.engine());

    assert_eq!(
        incremental, rebuilt,
        "incremental index updates diverged from a full rebuild"
    );
}

#[test]
fn a_corrupt_index_is_discarded_and_rebuilt() {
    // A bad sector, a process killed mid-write, a library folder copied while open. There is
    // nothing to salvage by construction, so the only correct response is to start again.
    let mut fixture = Fixture::populated();
    let before = observe(fixture.engine());

    fixture.corrupt_index();
    fixture.reopen();

    assert_eq!(
        before,
        observe(fixture.engine()),
        "a corrupt index must be rebuilt, not surfaced as an error"
    );
}
