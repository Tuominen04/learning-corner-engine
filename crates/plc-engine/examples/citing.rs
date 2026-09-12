//! Citing, run end to end, so the syntax can be read on disk rather than taken on trust.
//!
//! `cargo run --example citing -- /path/to/somewhere`
//!
//! Builds a small library, cites its sources every way the syntax allows, then prints the
//! document file exactly as it sits on disk beside what each citation renders as. The two
//! together are the whole claim: plain text you can read in any editor, APA 7 generated from
//! the source record rather than typed.

use std::path::PathBuf;

use plc_engine::api::Engine;
use plc_engine::citation::reference::parse_all;
use plc_engine::citation::source::{Contributor, Date, Role, Source, SourceType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/plc-citing".into())
        .into();

    // Same refusal as the walkthrough: an example that empties a path handed to it on the
    // command line would contradict the third design principle immediately.
    if root.exists() && root.read_dir()?.next().is_some() {
        eprintln!(
            "{} is not empty. Remove it, or pass a path that does not exist:\n\n    \
             cargo run --example citing -- /tmp/plc-citing\n",
            root.display()
        );
        std::process::exit(1);
    }

    let mut engine = Engine::create(&root, "Learning")?;

    // Two articles by the same author in the same year. This is the disambiguation case: what
    // each cites as depends on the *other* existing, which is why the pass runs library-wide.
    let (amino, _) = engine.source_create(article(
        "Amino acids in adults",
        2021,
        "Nieminen",
        "Anna Maria",
    ))?;
    let (zinc, _) =
        engine.source_create(article("Zinc in adults", 2021, "Nieminen", "Anna Maria"))?;

    // A lecture, so a timestamp has something to point at.
    let mut talk = Source::new(SourceType::Video, "How muscle repairs", Date::Year(2020));
    talk.contributors = vec![Contributor::person(Role::Author, "Chen", "Wei")];
    talk.site = Some("Aalto University".into());
    let (lecture, _) = engine.source_create(talk)?;

    let topic = engine.topic_create(None, "Protein")?;
    let document = engine.document_create(topic, "What protein is")?;
    let revision = engine.document_read(document)?.revision;

    let body = format!(
        "Complete proteins contain all nine essential amino acids.[^cite:{amino}@44]\n\n\
         [^cite:{zinc}!narrative] argued the same for zinc, at greater length.[^cite:{zinc}@112-118]\n\n\
         The repair itself takes longer than most people expect.[^cite:{lecture}@time:1:04:22]\n\n\
         A point without a source, which is allowed and should look like what it is."
    );
    engine.document_write(document, body, revision)?;

    // 1. The file, exactly as it sits on disk.
    let path = root.join("topics/protein/what-protein-is.md");
    println!("--- {} ---\n", path.display());
    println!("{}", std::fs::read_to_string(&path)?);

    // 2. What each citation renders as. One call for all of them: every separate call would
    //    re-read the library and redo the disambiguation pass.
    let stored = engine.document_read(document)?.body;
    let references = parse_all(&stored);
    let requests: Vec<_> = references
        .iter()
        .map(|r| (r.source, r.form, r.locator.clone()))
        .collect();

    println!("--- what each citation reads as ---\n");
    for (reference, rendered) in references
        .iter()
        .zip(engine.citation_format_many(&requests)?)
    {
        println!(
            "{:<52}  {}",
            reference.render(),
            rendered.as_deref().unwrap_or("(source no longer exists)")
        );
    }

    // 3. The reference list, built from what this document actually cites.
    println!("\n--- the reference list ---\n");
    for entry in engine.citation_reference_list(document)? {
        println!("{entry}");
    }

    println!(
        "\nNote the year suffixes. Neither article is 2021a or 2021b on its own — it is the \
         other one existing that makes them so, which is why the pass runs over the whole \
         library and not over one document.\n\nLibrary left at {}",
        root.display()
    );
    Ok(())
}

fn article(title: &str, year: i32, family: &str, given: &str) -> Source {
    let mut source = Source::new(SourceType::JournalArticle, title, Date::Year(year));
    source.contributors = vec![Contributor::person(Role::Author, family, given)];
    source.container = Some("Journal of Nutrition".into());
    source.volume = Some("151".into());
    source
}
