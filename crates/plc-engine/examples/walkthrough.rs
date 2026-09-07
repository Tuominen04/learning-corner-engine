//! The M0 walkthrough from the roadmap, run end to end.
//!
//! `cargo run --example walkthrough -- /path/to/somewhere`

use std::path::PathBuf;

use plc_engine::api::Engine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/plc-walkthrough".into())
        .into();

    // Refuse to touch a directory that already holds something. An example that quietly
    // deletes a path handed to it on the command line would contradict the third design
    // principle in the first thirty seconds anyone spends with this project.
    if root.exists() && root.read_dir()?.next().is_some() {
        eprintln!(
            "{} is not empty. Remove it, or pass a path that does not exist:\n\n    \
             cargo run --example walkthrough -- /tmp/plc-walkthrough\n",
            root.display()
        );
        std::process::exit(1);
    }

    // 1. Create a library with two nested topics and two documents.
    let mut engine = Engine::create(&root, "Learning")?;
    let protein = engine.topic_create(None, "Protein")?;
    let in_the_body = engine.topic_create(Some(protein), "In the body")?;

    let what = engine.document_create(protein, "What protein is")?;
    let used = engine.document_create(in_the_body, "How protein is used")?;

    let rev = engine.document_read(what)?.revision;
    engine.document_write(
        what,
        format!("Protein is built from amino acids.\n\nSee [[{used}]]."),
        rev,
    )?;
    let rev = engine.document_read(used)?.revision;
    engine.document_write(used, "Used for tissue repair.", rev)?;

    // 2. Close and reopen.
    drop(engine);
    let engine = Engine::open(&root)?;
    let status = engine.status()?;
    println!(
        "reopened: {} topics, {} documents",
        status.topics, status.documents
    );
    assert!(status.problems.is_empty());

    // 3. Backlinks are derived from the text, not stored.
    println!(
        "backlinks into 'How protein is used': {:?}",
        engine
            .backlinks(used)?
            .iter()
            .map(|d| &d.title)
            .collect::<Vec<_>>()
    );

    // 5. Delete the index; nothing observable changes.
    let before = engine.document_read(what)?;
    std::fs::remove_dir_all(root.join(".plc"))?;
    let engine = Engine::open(&root)?;
    assert_eq!(
        before,
        engine.document_read(what)?,
        "rebuild changed something"
    );
    println!("index deleted and rebuilt: no observable change");

    println!(
        "search 'amino' -> {:?}",
        engine
            .search("amino")?
            .iter()
            .map(|h| &h.title)
            .collect::<Vec<_>>()
    );

    Ok(())
}
