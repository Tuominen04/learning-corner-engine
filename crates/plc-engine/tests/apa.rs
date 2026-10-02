//! APA 7, case by case, read from files.
//!
//! Each file in `tests/apa/` holds the cases for one kind of source: the source exactly as a
//! library stores it, and the parenthetical, narrative and reference-list forms APA prints for
//! it. Table-driven, as `citation-model.md` asks, so that adding an example from the manual is
//! adding a few lines of text rather than writing a test. The disambiguation rules, which cannot
//! be seen at all with only one source, are in `citations.rs`.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use plc_engine::citation::disambiguate::Disambiguation;
use plc_engine::citation::format::{in_text, reference_entry, Form};
use plc_engine::citation::source::{Source, SourceType};
use serde::Deserialize;

#[derive(Deserialize)]
struct Cases {
    case: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    parenthetical: String,
    narrative: String,
    reference: String,
    source: Source,
}

/// Every case in every file, each with the file it came from.
fn every_case() -> Vec<(String, Case)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/apa");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();

    files
        .into_iter()
        .flat_map(|path| {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = fs::read_to_string(&path).unwrap();
            let cases: Cases = toml::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"));
            cases.case.into_iter().map(move |case| (file.clone(), case))
        })
        .collect()
}

#[test]
fn every_example_is_printed_as_apa_prints_it() {
    let cases = every_case();
    let mut wrong = Vec::new();

    for (file, case) in &cases {
        // On its own, as a library holding nothing else would print it: the rules that depend
        // on the other sources are `citations.rs`'s.
        let alone = Disambiguation::compute(std::slice::from_ref(&case.source));
        let printed = [
            (
                "parenthetical",
                in_text(&case.source, &alone, Form::Parenthetical, None),
                &case.parenthetical,
            ),
            (
                "narrative",
                in_text(&case.source, &alone, Form::Narrative, None),
                &case.narrative,
            ),
            (
                "reference",
                reference_entry(&case.source, &alone),
                &case.reference,
            ),
        ];
        for (form, got, expected) in printed {
            if &got != expected {
                wrong.push(format!(
                    "{file} — {}\n  {form}\n  expected: {expected}\n  printed:  {got}",
                    case.name
                ));
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "{} of {} cases printed wrongly:\n\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n\n")
    );
}

#[test]
fn every_kind_of_source_has_examples() {
    let covered: HashSet<SourceType> = every_case()
        .into_iter()
        .map(|(_, case)| case.source.source_type)
        .collect();
    for kind in SourceType::all() {
        assert!(covered.contains(kind), "no APA examples for {kind:?}");
    }
}
