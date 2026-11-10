//! Converting a format 1 library to format 2, on libraries written by hand in format 1 — the
//! cases the frozen `v1` fixture does not hold. ADR 0009.

use std::fs;
use std::path::Path;

use lc_engine::{Engine, Error};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn topic(root: &Path, dir: &str, id: &str, name: &str) {
    write(
        root,
        &format!("{dir}/topic.toml"),
        &format!("id = \"{id}\"\nname = \"{name}\"\ncreated = 2026-09-01T10:00:00Z\n"),
    );
}

fn document(root: &Path, path: &str, id: &str, title: &str) {
    write(
        root,
        path,
        &format!(
            "+++\nid = \"{id}\"\ntitle = \"{title}\"\ncreated = 2026-09-01T10:00:00Z\nmodified = 2026-09-01T10:00:00Z\n+++\n\nSome writing.\n"
        ),
    );
}

/// A format 1 library in `parent/learning`.
fn library(parent: &Path) -> std::path::PathBuf {
    let root = parent.join("learning");
    write(
        &root,
        "library.toml",
        "id = \"01J8ZQ5X9K3M7P2R4T6V8W0Y1Z\"\nname = \"Learning\"\nformat_version = 1\ncreated = 2026-09-01T10:00:00Z\n",
    );
    root
}

#[test]
fn a_document_named_like_its_topic_keeps_its_name_and_the_topic_takes_the_next() {
    // A topic "Protein" beside a document "Protein": the page the topic becomes cannot take
    // `protein.md`, so it and its folder take `protein-2` together, and nothing is overwritten.
    let dir = tempfile::tempdir().unwrap();
    let root = library(dir.path());
    topic(&root, "topics/food", "01J8ZQ6A1M3P5R7T9V1X3Z5B70", "Food");
    document(
        &root,
        "topics/food/protein.md",
        "01J8ZQ7B2N4Q6S8W0W2Y4A6C81",
        "Protein",
    );
    topic(
        &root,
        "topics/food/protein",
        "01J8ZQ6A1M3P5R7T9V1X3Z5B71",
        "Protein",
    );
    document(
        &root,
        "topics/food/protein/amino.md",
        "01J8ZQ7B2N4Q6S8W0W2Y4A6C82",
        "Amino",
    );

    Engine::migrate(&root).unwrap();
    let engine = Engine::open(&root).unwrap();

    let food = root.join("topics/food");
    assert!(fs::read_to_string(food.join("protein.md"))
        .unwrap()
        .contains("Some writing."));
    assert!(food.join("protein-2.md").is_file());
    assert!(food.join("protein-2/amino.md").is_file());
    assert!(!food.join("protein").exists());
    assert_eq!(
        fs::read_to_string(food.join(".order")).unwrap(),
        "protein\nprotein-2\n"
    );
    assert!(engine.status().unwrap().problems.is_empty());
}

#[test]
fn a_topic_with_nothing_in_it_becomes_a_page_with_no_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = library(dir.path());
    topic(&root, "topics/empty", "01J8ZQ6A1M3P5R7T9V1X3Z5B70", "Empty");

    Engine::migrate(&root).unwrap();

    assert!(root.join("topics/empty.md").is_file());
    assert!(!root.join("topics/empty").exists());
    let engine = Engine::open(&root).unwrap();
    let tree = engine.page_tree().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].title, "Empty");
    assert!(tree[0].children.is_empty());
}

#[test]
fn a_topic_file_that_cannot_be_read_stops_the_conversion_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let root = library(dir.path());
    topic(&root, "topics/food", "01J8ZQ6A1M3P5R7T9V1X3Z5B70", "Food");
    write(&root, "topics/food/broken/topic.toml", "name = \"Broken\n");

    let refused = Engine::migrate(&root);
    assert!(
        matches!(&refused, Err(Error::LibraryError(m)) if m.contains("broken")),
        "{refused:?}"
    );
    // Untouched, and no copy made of a library that was not converted.
    assert!(root.join("topics/food/topic.toml").is_file());
    assert!(!root.join("topics/food.md").exists());
    assert!(!dir.path().join("learning-format-1").exists());
    assert!(matches!(
        Engine::open(&root),
        Err(Error::MigrationRequired { .. })
    ));
}

#[test]
fn a_conversion_that_stopped_part_way_can_be_run_again() {
    // The page of one topic was written and its `topic.toml` was not yet removed when the
    // conversion stopped; the library still says format 1. Running it again finishes the job
    // without making a second page.
    let dir = tempfile::tempdir().unwrap();
    let root = library(dir.path());
    topic(&root, "topics/food", "01J8ZQ6A1M3P5R7T9V1X3Z5B70", "Food");
    document(
        &root,
        "topics/food/apples.md",
        "01J8ZQ7B2N4Q6S8W0W2Y4A6C81",
        "Apples",
    );
    write(
        &root,
        "topics/food.md",
        "+++\nid = \"01J8ZQ6A1M3P5R7T9V1X3Z5B70\"\ntitle = \"Food\"\ncreated = 2026-09-01T10:00:00Z\nmodified = 2026-09-01T10:00:00Z\n+++\n",
    );

    Engine::migrate(&root).unwrap();
    let engine = Engine::open(&root).unwrap();

    assert!(
        !root.join("topics/food-2.md").exists(),
        "a second page was made"
    );
    assert!(!root.join("topics/food/topic.toml").exists());
    let tree = engine.page_tree().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].title, "Food");
    assert_eq!(tree[0].children[0].title, "Apples");
    assert!(engine.status().unwrap().problems.is_empty());
}

#[test]
fn links_to_a_document_still_lead_to_it_after_converting() {
    // Identity is the id inside a file, so a link survives every file moving.
    let dir = tempfile::tempdir().unwrap();
    let root = library(dir.path());
    topic(&root, "topics/food", "01J8ZQ6A1M3P5R7T9V1X3Z5B70", "Food");
    document(
        &root,
        "topics/food/apples.md",
        "01J8ZQ7B2N4Q6S8W0W2Y4A6C81",
        "Apples",
    );
    write(
        &root,
        "topics/food/pears.md",
        "+++\nid = \"01J8ZQ7B2N4Q6S8W0W2Y4A6C82\"\ntitle = \"Pears\"\ncreated = 2026-09-01T10:00:00Z\nmodified = 2026-09-01T10:00:00Z\n+++\n\nUnlike [[01J8ZQ7B2N4Q6S8W0W2Y4A6C81]].\n",
    );

    Engine::migrate(&root).unwrap();
    let engine = Engine::open(&root).unwrap();
    let apples: lc_engine::Id = "01J8ZQ7B2N4Q6S8W0W2Y4A6C81".parse().unwrap();
    let backlinks = engine.backlinks(apples).unwrap();
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].title, "Pears");
}
