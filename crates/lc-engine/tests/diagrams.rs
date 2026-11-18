//! Diagrams (ADR 0011): files of their own in `maps/`, with boxes that may point at pages, shown
//! in a page by a `![…](diagram:<id>)` line.
//!
//! What matters most is what must NOT happen. Deleting a page must never make a diagram
//! unsaveable; deleting a diagram must never touch a page's writing but for its own line; and
//! a page that shows a diagram must stay an ordinary page — written, moved, split, searched and
//! exported — to everything that only knows pages.

use std::fs;

use lc_engine::api::DiagramView;
use lc_engine::export::Format;
use lc_engine::model::diagram::{Diagram, Direction, Layout, Line, Node};
use lc_engine::{Engine, Error, Id};

mod fixture;
use fixture::Fixture;

fn write(engine: &mut Engine, id: Id, body: &str) {
    let revision = engine.document_read(id).unwrap().revision;
    engine.document_write(id, body, revision).unwrap();
}

/// The diagram as stored, for editing and handing back.
fn stored(view: &DiagramView) -> Diagram {
    Diagram {
        id: view.id,
        title: view.title.clone(),
        layout: view.layout,
        direction: view.direction,
        created: view.created,
        nodes: view
            .nodes
            .iter()
            .map(|n| Node {
                id: n.id,
                page: n.page.as_ref().map(|p| p.id),
                label: n.label.clone(),
                when: n.when.clone(),
                x: n.x,
                y: n.y,
            })
            .collect(),
        lines: view.lines.clone(),
    }
}

fn names(root: &std::path::Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(root.join("maps"))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

// ---- making, reading, writing ---------------------------------------------------------------

#[test]
fn a_diagram_is_a_toml_file_in_maps_and_is_listed_by_title() {
    let (_dir, mut engine) = Fixture::empty();
    engine
        .diagram_create("Timeline of ideas", Layout::Timeline)
        .unwrap();
    let tree = engine
        .diagram_create("Family tree", Layout::Layered)
        .unwrap();

    assert_eq!(
        names(engine.root()),
        ["family-tree.toml", "timeline-of-ideas.toml"]
    );
    let text = fs::read_to_string(engine.root().join("maps/family-tree.toml")).unwrap();
    assert!(
        text.contains("title = \"Family tree\"") && text.contains("layout = \"layered\""),
        "{text}"
    );

    let list = engine.diagram_list().unwrap();
    assert_eq!(
        list.iter()
            .map(|d| (d.title.as_str(), d.layout))
            .collect::<Vec<_>>(),
        [
            ("Family tree", Layout::Layered),
            ("Timeline of ideas", Layout::Timeline)
        ]
    );
    assert_eq!(engine.diagram_read(tree.id).unwrap(), tree);
}

#[test]
fn a_diagram_needs_a_title_and_two_with_one_name_get_two_files() {
    let (_dir, mut engine) = Fixture::empty();
    assert!(matches!(
        engine.diagram_create("  ", Layout::Free),
        Err(Error::Invalid { .. })
    ));
    engine.diagram_create("Network", Layout::Free).unwrap();
    engine.diagram_create("Network", Layout::Free).unwrap();
    assert_eq!(names(engine.root()), ["network-2.toml", "network.toml"]);
}

#[test]
fn what_is_written_is_what_is_read_back_whole() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let view = engine
        .diagram_create("Family tree", Layout::Layered)
        .unwrap();

    let mut diagram = stored(&view);
    diagram.direction = Direction::Right;
    let root = Node::page(ids.protein);
    let mut child = Node::label("Not written about yet");
    child.when = Some("c. 1200".into());
    child.x = Some(10.5);
    child.y = Some(-3.0);
    diagram.lines.push(Line {
        from: root.id,
        to: child.id,
        label: Some("parent of".into()),
    });
    diagram.nodes = vec![root.clone(), child.clone()];

    let written = engine
        .diagram_write(view.id, diagram, view.revision)
        .unwrap();
    let read = engine.diagram_read(view.id).unwrap();
    assert_eq!(written, read);
    assert_eq!(read.direction, Direction::Right);
    assert_eq!(
        read.nodes[0].page.as_ref().unwrap().title.as_deref(),
        Some("Protein")
    );
    assert_eq!(read.nodes[1].when.as_deref(), Some("c. 1200"));
    assert_eq!((read.nodes[1].x, read.nodes[1].y), (Some(10.5), Some(-3.0)));
    assert_eq!(read.lines[0].label.as_deref(), Some("parent of"));
}

#[test]
fn a_write_that_could_not_be_drawn_is_refused_and_changes_nothing() {
    let (_dir, mut engine) = Fixture::empty();
    let view = engine.diagram_create("T", Layout::Free).unwrap();
    let before = fs::read_to_string(engine.root().join("maps/t.toml")).unwrap();

    let mut to_nowhere = stored(&view);
    to_nowhere.nodes.push(Node::label("a"));
    to_nowhere.lines.push(Line {
        from: to_nowhere.nodes[0].id,
        to: Id::new(),
        label: None,
    });
    assert!(matches!(
        engine.diagram_write(view.id, to_nowhere, view.revision),
        Err(Error::Invalid { .. })
    ));

    let mut someone_else = stored(&view);
    someone_else.id = Id::new();
    assert!(matches!(
        engine.diagram_write(view.id, someone_else, view.revision),
        Err(Error::Invalid { .. })
    ));

    assert_eq!(
        fs::read_to_string(engine.root().join("maps/t.toml")).unwrap(),
        before
    );
}

#[test]
fn a_box_cannot_point_at_a_source() {
    let (_dir, mut engine) = Fixture::empty();
    let (source, _) = engine
        .source_create(lc_engine::citation::source::Source::new(
            lc_engine::citation::source::SourceType::Book,
            "A book",
            lc_engine::citation::source::Date::Year(2020),
        ))
        .unwrap();
    let view = engine.diagram_create("T", Layout::Free).unwrap();
    let mut diagram = stored(&view);
    diagram.nodes.push(Node::page(source));

    assert!(matches!(
        engine.diagram_write(view.id, diagram, view.revision),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn a_diagram_changed_in_a_text_editor_meanwhile_is_a_conflict_and_is_not_overwritten() {
    let (_dir, mut engine) = Fixture::empty();
    let view = engine.diagram_create("T", Layout::Free).unwrap();
    let path = engine.root().join("maps/t.toml");

    let mut edited = fs::read_to_string(&path).unwrap();
    edited.push_str("\n# a note made by hand\n");
    fs::write(&path, &edited).unwrap();

    let mut diagram = stored(&view);
    diagram.nodes.push(Node::label("a"));
    assert!(matches!(
        engine.diagram_write(view.id, diagram, view.revision),
        Err(Error::Conflict { .. })
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
}

#[test]
fn a_new_title_renames_the_file_and_keeps_the_rest() {
    let (_dir, mut engine) = Fixture::empty();
    let other = engine.diagram_create("Taken", Layout::Free).unwrap();
    let view = engine.diagram_create("Draft", Layout::Layered).unwrap();

    let mut diagram = stored(&view);
    diagram.title = "  Taken ".into();
    diagram.created = lc_engine::Timestamp::now(); // not the caller's to change
    let written = engine
        .diagram_write(view.id, diagram, view.revision)
        .unwrap();

    assert_eq!(names(engine.root()), ["taken-2.toml", "taken.toml"]);
    assert_eq!(written.title, "Taken");
    assert_eq!(written.created, view.created);
    assert_eq!(engine.diagram_read(other.id).unwrap().title, "Taken");

    // The revision it came back with is the one to write with next.
    let mut again = stored(&written);
    again.title = "Settled".into();
    engine
        .diagram_write(view.id, again, written.revision)
        .unwrap();
    assert_eq!(names(engine.root()), ["settled.toml", "taken.toml"]);
}

// ---- boxes and pages -----------------------------------------------------------------------

#[test]
fn a_box_shows_its_page_s_title_as_it_is_now() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let view = engine.diagram_create("T", Layout::Layered).unwrap();
    let mut diagram = stored(&view);
    diagram.nodes.push(Node::page(ids.types_of_protein));
    engine
        .diagram_write(view.id, diagram, view.revision)
        .unwrap();

    engine
        .document_rename(ids.types_of_protein, "Kinds of protein")
        .unwrap();

    let read = engine.diagram_read(view.id).unwrap();
    assert_eq!(
        read.nodes[0].page.as_ref().unwrap().title.as_deref(),
        Some("Kinds of protein")
    );
}

#[test]
fn deleting_a_page_leaves_its_box_in_place_and_the_diagram_still_saveable() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let view = engine.diagram_create("Family", Layout::Layered).unwrap();
    let parent = Node::page(ids.how_used);
    let child = Node::label("Child");
    let mut diagram = stored(&view);
    diagram.lines.push(Line {
        from: parent.id,
        to: child.id,
        label: None,
    });
    diagram.nodes = vec![parent.clone(), child.clone()];
    engine
        .diagram_write(view.id, diagram, view.revision)
        .unwrap();

    engine
        .document_delete(ids.how_used, lc_engine::api::Contents::Delete)
        .unwrap();

    let read = engine.diagram_read(view.id).unwrap();
    assert_eq!(read.nodes.len(), 2, "the box is still there");
    let link = read.nodes[0].page.as_ref().unwrap();
    assert_eq!(
        (link.id, link.title.as_deref()),
        (ids.how_used, None),
        "saying its page is gone"
    );
    assert_eq!(read.lines.len(), 1, "so the shape holds");

    // And it can still be changed and saved: a deleted page never strands a diagram.
    let mut diagram = stored(&read);
    diagram.nodes.push(Node::label("Another"));
    let saved = engine
        .diagram_write(view.id, diagram, read.revision)
        .unwrap();
    assert_eq!(saved.nodes.len(), 3);
    assert_eq!(saved.nodes[0].page.as_ref().unwrap().title, None);

    // A box pointing at a page that is gone can be turned into a plain label.
    let mut unlinked = stored(&saved);
    unlinked.nodes[0].page = None;
    unlinked.nodes[0].label = Some("The one who left".into());
    engine
        .diagram_write(view.id, unlinked, saved.revision)
        .unwrap();
}

// ---- pages that show diagrams -----------------------------------------------------------------

#[test]
fn which_pages_show_a_diagram_and_which_diagrams_a_page_shows_or_is_in() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let tree = engine
        .diagram_create("Family tree", Layout::Layered)
        .unwrap();
    let map = engine.diagram_create("Network", Layout::Free).unwrap();

    write(
        engine,
        ids.what_protein_is,
        &format!("Intro.\n\n![Family tree](diagram:{})\n\nEnd.", tree.id),
    );
    write(
        engine,
        ids.protein,
        &format!(
            "![The same](diagram:{}) and ![gone](diagram:{})",
            tree.id,
            Id::new()
        ),
    );
    let mut diagram = stored(&map);
    diagram.nodes.push(Node::page(ids.what_protein_is));
    engine.diagram_write(map.id, diagram, map.revision).unwrap();

    let shown_in: Vec<String> = engine
        .diagram_shown_in(tree.id)
        .unwrap()
        .into_iter()
        .map(|p| p.title)
        .collect();
    assert_eq!(shown_in, ["Protein", "What protein is"]);
    assert!(engine.diagram_shown_in(map.id).unwrap().is_empty());

    let on = engine.document_diagrams(ids.what_protein_is).unwrap();
    assert_eq!(
        on.shown
            .iter()
            .map(|d| d.title.as_str())
            .collect::<Vec<_>>(),
        ["Family tree"]
    );
    assert_eq!(
        on.pointing_here
            .iter()
            .map(|d| d.title.as_str())
            .collect::<Vec<_>>(),
        ["Network"]
    );

    // A line to a diagram that does not exist is not listed, and is not an error.
    let protein = engine.document_diagrams(ids.protein).unwrap();
    assert_eq!(protein.shown.len(), 1);
    assert!(engine
        .document_diagrams(ids.how_used)
        .unwrap()
        .shown
        .is_empty());
}

#[test]
fn deleting_a_diagram_takes_its_lines_out_of_pages_and_nothing_else() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let tree = engine
        .diagram_create("Family tree", Layout::Layered)
        .unwrap();
    let kept = engine.diagram_create("Kept", Layout::Free).unwrap();

    write(
        engine,
        ids.what_protein_is,
        &format!(
            "Before.\n\n![Family tree](diagram:{})\n\nAfter, with [[{}]].\n\n![Kept](diagram:{})",
            tree.id, ids.types_of_protein, kept.id
        ),
    );
    write(
        engine,
        ids.types_of_protein,
        &format!("See ![tree](diagram:{}) for the whole family.", tree.id),
    );
    write(engine, ids.how_used, "Does not show it.");

    let plan = engine.diagram_delete_plan(tree.id).unwrap();
    assert_eq!(
        plan.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(),
        ["Types of protein", "What protein is"]
    );
    assert!(
        names(engine.root()).contains(&"family-tree.toml".to_string()),
        "a plan deletes nothing"
    );

    engine.diagram_delete(tree.id).unwrap();

    assert_eq!(names(engine.root()), ["kept.toml"]);
    assert!(matches!(
        engine.diagram_read(tree.id),
        Err(Error::NotFound(_))
    ));
    assert_eq!(
        engine.document_read(ids.what_protein_is).unwrap().body,
        format!(
            "Before.\n\nAfter, with [[{}]].\n\n![Kept](diagram:{})",
            ids.types_of_protein, kept.id
        )
    );
    assert_eq!(
        engine.document_read(ids.types_of_protein).unwrap().body,
        "See  for the whole family."
    );
    assert_eq!(
        engine.document_read(ids.how_used).unwrap().body,
        "Does not show it."
    );
    assert_eq!(
        engine.diagram_shown_in(kept.id).unwrap().len(),
        1,
        "another diagram's lines stay"
    );
    assert!(
        engine
            .document_diagrams(ids.what_protein_is)
            .unwrap()
            .shown
            .len()
            == 1
    );
}

// ---- a page that shows a diagram is still an ordinary page ----------------------------------------

#[test]
fn the_line_survives_writing_moving_exporting_and_searching_as_any_text_does() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let tree = engine
        .diagram_create("Family tree", Layout::Layered)
        .unwrap();
    let body = format!(
        "Intro.\n\n![Family tree](diagram:{})\n\nEnd, in Finnish: ääkköset.",
        tree.id
    );

    write(engine, ids.what_protein_is, &body);
    assert_eq!(
        engine.document_read(ids.what_protein_is).unwrap().body,
        body,
        "written back unchanged"
    );

    // Moving the page carries nothing and loses nothing: there is no file beside it to move.
    engine
        .document_move(ids.what_protein_is, Some(ids.in_the_body), None)
        .unwrap();
    assert_eq!(
        engine.document_read(ids.what_protein_is).unwrap().body,
        body
    );
    let beside: Vec<String> = fs::read_dir(engine.root().join("topics/protein/in-the-body"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(beside.iter().all(|n| !n.contains("diagram")), "{beside:?}");

    // Still shown, from its new place.
    assert_eq!(engine.diagram_shown_in(tree.id).unwrap().len(), 1);

    // Exported, it is a picture that is not there — named, and not a failure.
    let text = engine
        .export_document(ids.what_protein_is, Format::Text)
        .unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("[Picture: Family tree]"), "{text}");
    for format in [Format::Html, Format::Docx, Format::Odt, Format::Pdf] {
        assert!(!engine
            .export_document(ids.what_protein_is, format)
            .unwrap()
            .is_empty());
    }

    // What is read of the page for searching is the page's words: found by them, and a
    // diagram's address is not one of them.
    assert!(engine
        .search("ääkköset")
        .unwrap()
        .iter()
        .any(|h| h.title == "What protein is"));
    assert!(engine
        .search("diagram")
        .unwrap()
        .iter()
        .all(|h| h.title != "What protein is"));
}

#[test]
fn splitting_out_a_section_with_a_diagram_line_takes_the_diagram_with_it() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let tree = engine
        .diagram_create("Family tree", Layout::Layered)
        .unwrap();
    let line = format!("![Family tree](diagram:{})", tree.id);
    let body = format!("Keep this.\n\n## Part\n\n{line}\n\nPart text.");
    write(engine, ids.what_protein_is, &body);

    let revision = engine.document_read(ids.what_protein_is).unwrap().revision;
    let start = body.find("## Part").unwrap();
    let split = engine
        .document_split(ids.what_protein_is, start..body.len(), "Part", revision)
        .unwrap();

    let shown: Vec<Id> = engine
        .diagram_shown_in(tree.id)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(
        shown,
        vec![split.taken.id],
        "the new page shows it and the original no longer does"
    );
}

// ---- finding them, and the library's own accounts ------------------------------------------------

#[test]
fn a_diagram_is_found_by_its_title_and_by_what_its_boxes_say() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let engine = fixture.engine();
    let view = engine.diagram_create("Trade routes", Layout::Free).unwrap();
    let mut diagram = stored(&view);
    diagram.nodes = vec![Node::label("Alexandria"), Node::page(ids.protein)];
    engine
        .diagram_write(view.id, diagram, view.revision)
        .unwrap();

    for query in ["trade", "alexandria"] {
        let hits = engine.search(query).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.id == view.id && h.kind == lc_engine::index::Kind::Diagram),
            "{query}: {hits:?}"
        );
    }
    // A page is found as the page it is: a box pointing at it does not make the diagram a hit.
    assert!(engine
        .search("protein")
        .unwrap()
        .iter()
        .all(|h| h.id != view.id));
}

#[test]
fn a_diagram_gone_wrong_by_hand_is_a_problem_with_the_library_and_nothing_more() {
    let (_dir, mut engine) = Fixture::empty();
    let good = engine.diagram_create("Good", Layout::Free).unwrap();
    let root = engine.root().to_path_buf();
    fs::write(root.join("maps/broken.toml"), "title = [unfinished").unwrap();
    let copy = fs::read_to_string(root.join("maps/good.toml")).unwrap();
    fs::write(root.join("maps/copy.toml"), copy).unwrap();

    let engine = Engine::open(&root).unwrap();
    let status = engine.status().unwrap();
    let said: Vec<String> = status
        .problems
        .iter()
        .map(|p| format!("{}: {}", p.path.display(), p.message))
        .collect();
    assert!(said.iter().any(|p| p.contains("broken.toml")), "{said:?}");
    assert!(
        said.iter().any(|p| p.contains("duplicate identifier")),
        "{said:?}"
    );

    // The library opens and the good diagram is still there.
    assert_eq!(engine.diagram_read(good.id).unwrap().title, "Good");
}

#[test]
fn the_index_is_the_same_after_a_rebuild() {
    let mut fixture = Fixture::populated();
    let ids = fixture.ids();
    let tree = {
        let engine = fixture.engine();
        let view = engine
            .diagram_create("Family tree", Layout::Layered)
            .unwrap();
        let mut diagram = stored(&view);
        diagram.nodes = vec![Node::page(ids.protein), Node::label("Alexandria")];
        engine
            .diagram_write(view.id, diagram, view.revision)
            .unwrap();
        write(
            engine,
            ids.what_protein_is,
            &format!("![Family tree](diagram:{})", view.id),
        );
        view.id
    };
    let snapshot = |engine: &Engine| {
        (
            engine.diagram_list().unwrap(),
            engine.diagram_shown_in(tree).unwrap(),
            engine.document_diagrams(ids.protein).unwrap(),
            engine.document_diagrams(ids.what_protein_is).unwrap(),
            engine.search("alexandria").unwrap().len(),
        )
    };
    let before = snapshot(fixture.engine());

    fixture.delete_index();
    fixture.reopen();

    assert_eq!(snapshot(fixture.engine()), before);
}

#[test]
fn diagrams_do_not_stop_a_library_having_its_folder_to_itself() {
    let (_dir, mut engine) = Fixture::empty();
    engine.diagram_create("T", Layout::Free).unwrap();
    assert!(lc_engine::Library::open(engine.root())
        .unwrap()
        .has_its_folder_to_itself()
        .unwrap());
}

#[test]
fn a_backup_copies_diagrams_like_any_other_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::create(dir.path().join("library"), "Learning").unwrap();
    engine
        .diagram_create("Family tree", Layout::Layered)
        .unwrap();
    let drive = dir.path().join("drive");

    let plan = engine.backup_plan(&drive).unwrap();
    engine.backup_apply(&plan, false).unwrap();

    assert!(drive.join("maps/family-tree.toml").is_file());
}
