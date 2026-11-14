//! `library_graph`: the whole library as nodes and lines (ADR 0010).
//!
//! The graph is derived, so what matters is that it says exactly what the library says — every
//! page once, every line joining two things that are there — and that asking for it changes
//! nothing on disk.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use lc_engine::api::{GraphLine, LibraryGraph, LineKind, NodeKind};
use lc_engine::citation::source::{Contributor, Date, Role, Source, SourceType};
use lc_engine::works::Unit;
use lc_engine::{Engine, Id};

mod fixture;
use fixture::Fixture;

fn has(graph: &LibraryGraph, from: Id, to: Id, kind: LineKind) -> bool {
    graph.lines.contains(&GraphLine { from, to, kind })
}

fn count(graph: &LibraryGraph, kind: LineKind) -> usize {
    graph.lines.iter().filter(|l| l.kind == kind).count()
}

fn write(engine: &mut Engine, id: Id, body: &str) {
    let revision = engine.document_read(id).unwrap().revision;
    engine.document_write(id, body, revision).unwrap();
}

fn source(engine: &mut Engine, title: &str) -> Id {
    let mut s = Source::new(SourceType::Book, title, Date::Year(2021));
    s.contributors = vec![Contributor::person(Role::Author, "Nieminen", "Anna")];
    s.publisher = Some("Gaudeamus".into());
    engine.source_create(s).unwrap().0
}

/// Every file in the library outside `.plc/`, with its bytes.
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            if name == ".plc" {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn every_page_is_a_node_once_in_tree_order_coloured_by_its_top_page() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let elsewhere = fixture.engine().document_create(None, "Elsewhere").unwrap();

    let graph = fixture.engine().library_graph(false).unwrap();

    let order: Vec<Id> = graph.nodes.iter().map(|n| n.id).collect();
    assert_eq!(
        order,
        vec![
            ids.protein,
            ids.what_protein_is,
            ids.types_of_protein,
            ids.in_the_body,
            ids.how_used,
            elsewhere,
        ]
    );
    assert!(graph.nodes.iter().all(|n| n.kind == NodeKind::Page));
    for node in &graph.nodes[..5] {
        assert_eq!(node.top, Some(ids.protein), "{}", node.title);
    }
    assert_eq!(
        graph.nodes[5].top,
        Some(elsewhere),
        "a top page is its own top"
    );
    assert_eq!(graph.nodes[4].title, "How protein is used");
}

#[test]
fn the_tree_is_lines_from_each_page_to_the_page_it_is_under() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();

    let graph = fixture.engine().library_graph(false).unwrap();

    assert_eq!(count(&graph, LineKind::Under), 4);
    assert!(has(
        &graph,
        ids.what_protein_is,
        ids.protein,
        LineKind::Under
    ));
    assert!(has(&graph, ids.in_the_body, ids.protein, LineKind::Under));
    assert!(has(&graph, ids.how_used, ids.in_the_body, LineKind::Under));
}

#[test]
fn links_are_lines_and_a_link_to_nothing_or_to_itself_is_not() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    write(
        engine,
        ids.how_used,
        &format!(
            "See [[{}]], [[{}]] and [[{}]].",
            ids.what_protein_is,
            Id::new(),
            ids.how_used
        ),
    );

    let graph = engine.library_graph(false).unwrap();

    // The fixture's two cross-links, and the one written here to a page that exists.
    assert_eq!(count(&graph, LineKind::LinksTo), 3, "{:?}", graph.lines);
    assert!(has(
        &graph,
        ids.what_protein_is,
        ids.types_of_protein,
        LineKind::LinksTo
    ));
    assert!(has(
        &graph,
        ids.types_of_protein,
        ids.what_protein_is,
        LineKind::LinksTo
    ));
    assert!(has(
        &graph,
        ids.how_used,
        ids.what_protein_is,
        LineKind::LinksTo
    ));

    let on: Vec<Id> = graph.nodes.iter().map(|n| n.id).collect();
    assert!(
        graph
            .lines
            .iter()
            .all(|l| on.contains(&l.from) && on.contains(&l.to)),
        "every line joins two nodes that are on the graph"
    );
}

#[test]
fn sources_and_citations_are_there_only_when_asked_for() {
    let (_dir, mut engine) = Fixture::empty();
    let zinc = source(&mut engine, "Zinc");
    let amino = source(&mut engine, "amino acids");
    let page = engine.document_create(None, "Notes").unwrap();
    write(
        &mut engine,
        page,
        &format!("Claimed.[^cite:{zinc}] Twice.[^cite:{zinc}]"),
    );

    let without = engine.library_graph(false).unwrap();
    assert_eq!(without.nodes.len(), 1);
    assert_eq!(count(&without, LineKind::Cites), 0);

    let with = engine.library_graph(true).unwrap();
    let sources: Vec<(&str, Option<Id>)> = with
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Source)
        .map(|n| (n.title.as_str(), n.top))
        .collect();
    assert_eq!(
        sources,
        vec![("amino acids", None), ("Zinc", None)],
        "by title, whatever the case, and under nothing"
    );
    assert_eq!(
        count(&with, LineKind::Cites),
        1,
        "one line however often it is cited"
    );
    assert!(has(&with, page, zinc, LineKind::Cites));
    assert!(
        !with.lines.iter().any(|l| l.to == amino),
        "a source nobody cites stands alone"
    );
}

#[test]
fn a_work_s_citation_is_not_a_line_while_works_are_not_on_the_graph() {
    let (_dir, mut engine) = Fixture::empty();
    let poet = source(&mut engine, "Collected poems");
    engine
        .work_create(
            "Я вас любил",
            Unit::Line,
            "Я вас любил.\n",
            "ru",
            Some(poet),
        )
        .unwrap();
    assert!(
        !engine.source_citing_works(poet).unwrap().is_empty(),
        "the work does cite it"
    );

    let graph = engine.library_graph(true).unwrap();
    assert_eq!(graph.nodes.len(), 1, "only the source");
    assert!(graph.lines.is_empty(), "{:?}", graph.lines);
}

#[test]
fn asking_for_the_graph_writes_nothing() {
    let mut fixture = Fixture::populated();
    let before = files(fixture.root());

    fixture.engine().library_graph(true).unwrap();

    assert_eq!(files(fixture.root()), before);
}

#[test]
fn the_graph_from_a_rebuilt_index_is_the_same_graph() {
    let mut fixture = Fixture::populated();
    let before = fixture.engine().library_graph(true).unwrap();

    fixture.delete_index();
    fixture.reopen();

    assert_eq!(fixture.engine().library_graph(true).unwrap(), before);
}

#[test]
fn an_empty_library_is_an_empty_graph() {
    let (_dir, engine) = Fixture::empty();
    let graph = engine.library_graph(true).unwrap();
    assert!(graph.nodes.is_empty() && graph.lines.is_empty());
}
