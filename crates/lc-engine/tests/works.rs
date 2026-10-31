//! Works and parallel texts, through the engine's own operations — `parallel-text-model.md`.

use std::fs;
use std::path::{Path, PathBuf};

use lc_engine::api::WorkView;
use lc_engine::citation::{Contributor, Date, Role, Source, SourceType};
use lc_engine::works::{EditionKind, NewEdition, Unit};
use lc_engine::{Engine, Error, Id};

const POEM: &str = "Я вас любил: любовь ещё, быть может,\n\
                    В душе моей угасла не совсем;\n\
                    Но пусть она вас больше не тревожит;\n";

fn library() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::create(dir.path().join("kielet"), "Kielet").unwrap();
    (dir, engine)
}

fn translation(language: &str) -> NewEdition {
    NewEdition {
        kind: EditionKind::Translation,
        language: language.into(),
        label: None,
        scheme: None,
        source: None,
    }
}

fn texts(view: &WorkView, edition: usize) -> Vec<String> {
    view.editions[edition]
        .cells
        .iter()
        .map(|c| c.text())
        .collect()
}

/// The work's folder: the one folder in `works/`, as these tests make one work each.
fn folder(engine: &Engine, view: &WorkView) -> PathBuf {
    let _ = view;
    let mut dirs: Vec<PathBuf> = fs::read_dir(engine.root().join("works"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    dirs.pop().unwrap()
}

fn poem(engine: &mut Engine) -> WorkView {
    engine
        .work_create("Я вас любил", Unit::Line, POEM, "ru", None)
        .unwrap()
}

#[test]
fn a_work_is_a_folder_of_plain_files() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    assert_eq!(view.editions.len(), 1);
    assert_eq!(view.editions[0].kind, EditionKind::Original);
    assert_eq!(texts(&view, 0).len(), 3);

    let dir = folder(&engine, &view);
    assert_eq!(fs::read_to_string(dir.join("original.txt")).unwrap(), POEM);
    let manifest = fs::read_to_string(dir.join("work.toml")).unwrap();
    assert!(manifest.contains("title = \"Я вас любил\""), "{manifest}");
    assert!(manifest.contains("unit = \"line\""), "{manifest}");

    let list = engine.work_list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].languages, ["ru"]);
    assert_eq!(engine.status().unwrap().works, 1);
}

#[test]
fn editions_are_added_lined_up_and_filled_row_by_row() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let view = engine
        .work_add_edition(view.id, translation("fi"), "", &view.revision)
        .unwrap();
    assert!(view.misalignments.is_empty());
    let fi = view.editions[1].id;
    let view = engine
        .work_write_row(
            view.id,
            fi,
            2,
            "ei sielussani\nole täysin sammunut;",
            &view.revision,
        )
        .unwrap();
    assert_eq!(
        texts(&view, 1),
        ["", "ei sielussani\nole täysin sammunut;", ""]
    );
    let file = fs::read_to_string(folder(&engine, &view).join("translation.fi.txt")).unwrap();
    assert_eq!(file, "~\nei sielussani\n+ ole täysin sammunut;\n~\n");

    let mut pronunciation = translation("ru");
    pronunciation.kind = EditionKind::Pronunciation;
    pronunciation.scheme = Some("bgn-pcgn".into());
    let view = engine
        .work_add_edition(
            view.id,
            pronunciation,
            "Ya vas lyubil\nV dushe\nNo pust'",
            &view.revision,
        )
        .unwrap();
    assert_eq!(view.editions[2].scheme.as_deref(), Some("bgn-pcgn"));

    let mut second = translation("fi");
    second.label = Some("Leino 1921".into());
    let view = engine
        .work_add_edition(view.id, second, "", &view.revision)
        .unwrap();
    assert!(folder(&engine, &view).join("translation.fi-2.txt").exists());
    assert_eq!(view.editions[3].label.as_deref(), Some("Leino 1921"));
}

#[test]
fn a_second_original_and_a_scheme_on_a_translation_are_refused() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let mut original = translation("fi");
    original.kind = EditionKind::Original;
    assert!(matches!(
        engine.work_add_edition(view.id, original, "", &view.revision),
        Err(Error::Invalid { .. })
    ));
    let mut schemed = translation("fi");
    schemed.scheme = Some("iso-9".into());
    assert!(matches!(
        engine.work_add_edition(view.id, schemed, "", &view.revision),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn a_change_from_an_older_read_is_refused_after_a_hand_edit() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let original = view.editions[0].id;
    let file = folder(&engine, &view).join("original.txt");
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&file, format!("{POEM}Я не хочу печалить вас ничем.\n")).unwrap();

    assert!(matches!(
        engine.work_write_row(view.id, original, 1, "changed", &view.revision),
        Err(Error::Conflict { .. })
    ));
    let fresh = engine.work_read(view.id, &[]).unwrap();
    assert_eq!(texts(&fresh, 0).len(), 4, "the hand edit is what is read");
    assert_eq!(
        fs::read_to_string(&file).unwrap().lines().count(),
        4,
        "and it was not overwritten"
    );
}

#[test]
fn rows_go_in_and_out_of_every_edition_together() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let view = engine
        .work_add_edition(
            view.id,
            translation("fi"),
            "yksi\nkaksi\nkolme",
            &view.revision,
        )
        .unwrap();
    let (original, fi) = (view.editions[0].id, view.editions[1].id);
    let view = engine.work_insert_row(view.id, 2, &view.revision).unwrap();
    assert_eq!(texts(&view, 1), ["yksi", "", "kaksi", "kolme"]);
    assert_eq!(texts(&view, 0)[1], "");

    assert!(matches!(
        engine.work_remove_row(view.id, original, 3, &view.revision),
        Err(Error::Invalid { .. })
    ));
    let view = engine
        .work_remove_row(view.id, fi, 2, &view.revision)
        .unwrap();
    assert_eq!(texts(&view, 1), ["yksi", "kaksi", "kolme"]);
}

#[test]
fn a_hand_edit_that_breaks_alignment_is_shown_and_fixed_with_join() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let view = engine
        .work_add_edition(
            view.id,
            translation("fi"),
            "yksi\nkaksi\nkolme",
            &view.revision,
        )
        .unwrap();
    let fi = view.editions[1].id;
    let file = folder(&engine, &view).join("translation.fi.txt");
    fs::write(&file, "yksi\nkaksi, joka jatkuu\ntällä rivillä\nkolme\n").unwrap();

    let view = engine.work_read(view.id, &[]).unwrap();
    assert_eq!(view.misalignments.len(), 1);
    assert_eq!(view.misalignments[0].rows, 4);
    assert_eq!(view.misalignments[0].original_rows, 3);
    assert!(engine.work_list().unwrap()[0].misaligned);
    // The index notices a hand edit the next time the library opens, as for a document.
    engine = Engine::open(engine.root()).unwrap();
    let problems = engine.status().unwrap().problems;
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("do not line up")),
        "{problems:?}"
    );

    let view = engine.work_join(view.id, fi, 3, &view.revision).unwrap();
    assert!(view.misalignments.is_empty());
    assert_eq!(texts(&view, 1)[1], "kaksi, joka jatkuu\ntällä rivillä");
    assert!(engine.status().unwrap().problems.is_empty());
}

#[test]
fn a_chorus_is_translated_once() {
    let (_dir, mut engine) = library();
    let view = engine
        .work_create(
            "Калинка",
            Unit::Line,
            "Калинка!\nВ саду\nКалинка!",
            "ru",
            None,
        )
        .unwrap();
    let view = engine
        .work_add_edition(view.id, translation("fi"), "", &view.revision)
        .unwrap();
    let fi = view.editions[1].id;
    let (view, shared) = engine
        .work_set_repeat(view.id, 3, Some(1), &view.revision)
        .unwrap();
    assert_eq!(shared, 2);
    let view = engine
        .work_write_row(view.id, fi, 3, "Kalinka!", &view.revision)
        .unwrap();
    assert_eq!(texts(&view, 1), ["Kalinka!", "", "Kalinka!"]);
    let file = fs::read_to_string(folder(&engine, &view).join("translation.fi.txt")).unwrap();
    assert_eq!(
        file, "Kalinka!\n~\nKalinka!\n",
        "every repetition is in the file"
    );
}

#[test]
fn notes_follow_their_rows() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let view = engine
        .work_set_note(view.id, 2, "угасла — to die out", &view.revision)
        .unwrap();
    let view = engine.work_insert_row(view.id, 1, &view.revision).unwrap();
    assert_eq!(view.notes[0].row, 3);
    let view = engine
        .work_set_note(view.id, 3, "", &view.revision)
        .unwrap();
    assert!(view.notes.is_empty());
}

fn source(engine: &mut Engine) -> Id {
    let mut s = Source::new(SourceType::Book, "Стихотворения", Date::Year(1829));
    s.publisher = Some("Гослитиздат".into());
    s.contributors = vec![Contributor::person(Role::Author, "Пушкин", "Александр")];
    engine.source_create(s).unwrap().0
}

#[test]
fn a_work_cites_sources_and_lets_go_of_a_deleted_one() {
    let (_dir, mut engine) = library();
    let pushkin = source(&mut engine);
    let view = engine
        .work_create("Я вас любил", Unit::Line, POEM, "ru", Some(pushkin))
        .unwrap();
    assert_eq!(view.editions[0].source, Some(pushkin));
    let other = source(&mut engine);
    let view = engine
        .work_update(view.id, "Я вас любил", vec![other], &view.revision)
        .unwrap();
    assert_eq!(view.sources, [other]);

    let citing = engine.source_citing_works(pushkin).unwrap();
    assert_eq!(citing.len(), 1);
    assert_eq!(citing[0].id, view.id);
    assert!(
        engine.source_citing_documents(pushkin).unwrap().is_empty(),
        "a work is not a document"
    );

    engine.source_delete(pushkin).unwrap();
    let view = engine.work_read(view.id, &[]).unwrap();
    assert_eq!(view.editions[0].source, None);
    assert_eq!(view.sources, [other], "the other source stays");
}

#[test]
fn works_are_found_by_search() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    engine
        .work_add_edition(
            view.id,
            translation("fi"),
            "sammunut\nkaksi\nkolme",
            &view.revision,
        )
        .unwrap();
    let hits = engine.search("sammun").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, lc_engine::index::Kind::Work);
    assert_eq!(engine.search("тревожит").unwrap().len(), 1);
}

#[test]
fn dividing_again_is_shown_first_and_keeps_notes_it_cannot_place() {
    let (_dir, mut engine) = library();
    let view = engine
        .work_create(
            "Letter",
            Unit::Line,
            "Dear friend. I write\nto you today.\nYours.",
            "en",
            None,
        )
        .unwrap();
    let view = engine
        .work_set_note(view.id, 1, "the opening", &view.revision)
        .unwrap();
    let plan = engine.work_resegment_plan(view.id, Unit::Sentence).unwrap();
    assert_eq!(plan.editions[0].rows_before, 3);
    assert_eq!(plan.editions[0].rows_after, 3);
    assert_eq!(plan.notes_moved.len(), 1);
    assert_eq!(
        engine.work_read(view.id, &[]).unwrap().unit,
        Unit::Line,
        "a plan writes nothing"
    );

    let done = engine.work_resegment(&plan).unwrap();
    assert_eq!(done.work.unit, Unit::Sentence);
    assert_eq!(
        texts(&done.work, 0),
        ["Dear friend.", "I write\nto you today.", "Yours."]
    );
    let kept = engine.root().join(done.notes_kept_at.unwrap());
    assert!(fs::read_to_string(kept).unwrap().contains("the opening"));
}

#[test]
fn a_stale_re_division_plan_is_refused() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let plan = engine.work_resegment_plan(view.id, Unit::Sentence).unwrap();
    engine
        .work_set_note(view.id, 1, "changed since", &view.revision)
        .unwrap();
    assert!(matches!(
        engine.work_resegment(&plan),
        Err(Error::Conflict { .. })
    ));
}

#[test]
fn removing_an_edition_shows_what_goes_and_the_original_stays() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let view = engine
        .work_add_edition(
            view.id,
            translation("fi"),
            "yksi\nkaksi\nkolme",
            &view.revision,
        )
        .unwrap();
    let (original, fi) = (view.editions[0].id, view.editions[1].id);
    let view = engine
        .work_write_row(view.id, fi, 2, "", &view.revision)
        .unwrap();
    assert!(engine.work_remove_edition_plan(view.id, original).is_err());
    let plan = engine.work_remove_edition_plan(view.id, fi).unwrap();
    assert_eq!(plan.rows_with_text, 2);
    let view = engine
        .work_remove_edition(view.id, fi, &view.revision)
        .unwrap();
    assert_eq!(view.editions.len(), 1);
    assert!(!folder(&engine, &view).join("translation.fi.txt").exists());
}

#[test]
fn deleting_a_work_shows_what_goes_first() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let view = engine
        .work_set_note(view.id, 1, "a note", &view.revision)
        .unwrap();
    let dir = folder(&engine, &view);
    let plan = engine.work_delete_plan(view.id).unwrap();
    assert_eq!(plan.notes, 1);
    assert_eq!(plan.editions[0].rows_with_text, 3);
    engine.work_delete(view.id).unwrap();
    assert!(!dir.exists());
    assert!(engine.work_list().unwrap().is_empty());
}

#[test]
fn incremental_work_writes_agree_with_a_full_rebuild() {
    let (_dir, mut engine) = library();
    let view = poem(&mut engine);
    let view = engine
        .work_add_edition(view.id, translation("fi"), "yksi\nkaksi", &view.revision)
        .unwrap();
    let observe = |engine: &Engine| {
        (
            engine.work_list().unwrap(),
            engine.status().unwrap().problems,
            engine.search("kaksi").unwrap(),
        )
    };
    let before = observe(&engine);
    assert!(!before.1.is_empty(), "the short translation is a problem");
    engine.rebuild_index().unwrap();
    assert_eq!(before, observe(&engine));
    let _ = view;
}

// ---- the parallel fixture ------------------------------------------------------------

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parallel")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn the_parallel_fixture_reads_as_written_by_hand() {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&fixture(), dir.path());
    let engine = Engine::open(dir.path()).unwrap();

    let list = engine.work_list().unwrap();
    assert_eq!(list.len(), 2);
    let kalinka = list.iter().find(|w| w.title == "Калинка").unwrap();
    let poem = list.iter().find(|w| w.title == "Я вас любил").unwrap();
    assert!(kalinka.misaligned, "a row added by hand to the Finnish");
    assert!(!poem.misaligned);
    assert_eq!(poem.languages, ["ru", "ru", "fi", "en"]);

    let view = engine.work_read(poem.id, &[]).unwrap();
    assert_eq!(texts(&view, 0).len(), 9);
    assert!(
        view.editions[2].cells[7].is_empty(),
        "a deliberately empty row"
    );
    assert_eq!(view.notes.len(), 1);
    assert_eq!(view.repeats.len(), 1);
    assert_eq!(texts(&view, 3)[8], texts(&view, 3)[0], "the repeat");
    assert_eq!(view.editions[2].label.as_deref(), Some("Oma käännös"));

    let source: Id = "01M4P0B7Q8R9S0T1V2W3X4Y5ZA".parse().unwrap();
    assert_eq!(engine.source_citing_works(source).unwrap().len(), 1);

    let kalinka = engine.work_read(kalinka.id, &[]).unwrap();
    assert_eq!(
        texts(&kalinka, 1)[1],
        "Puutarhassa marja vadelma,\nvadelmani!"
    );
    assert_eq!(kalinka.misalignments[0].rows, 4);
    let problems = engine.status().unwrap().problems;
    assert_eq!(problems.len(), 1, "{problems:?}");
}

#[test]
fn hand_aligned_rows_survive_a_round_trip_through_the_files() {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&fixture(), dir.path());
    let mut engine = Engine::open(dir.path()).unwrap();
    let poem = engine
        .work_list()
        .unwrap()
        .into_iter()
        .find(|w| w.title == "Я вас любил")
        .unwrap();
    let view = engine.work_read(poem.id, &[]).unwrap();
    let fi = view.editions[2].id;
    let before: Vec<Vec<String>> = (0..4).map(|e| texts(&view, e)).collect();

    // A write through the engine rewrites every file; what was lined up by hand stays.
    let view = engine
        .work_write_row(
            view.id,
            fi,
            8,
            "kuin suokoon Jumala teidän tulla rakastetuksi toisen",
            &view.revision,
        )
        .unwrap();
    for e in [0, 1, 3] {
        assert_eq!(texts(&view, e), before[e]);
    }
    let reopened = Engine::open(dir.path()).unwrap();
    assert_eq!(reopened.work_read(poem.id, &[]).unwrap(), view);
}

// ---- the round-trip property -----------------------------------------------------------

/// A small deterministic generator: no new crate for a test.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> usize {
        (self.next() % n) as usize
    }
}

const WORDS: &[&str] = &[
    "любовь",
    "ещё",
    "sammunut",
    "äänettömästi",
    "love",
    "+ plus",
    "~",
    "\\~",
    "\\+ x",
    "rivi",
];

fn line(rng: &mut Lcg) -> String {
    let n = 1 + rng.below(3);
    (0..n)
        .map(|_| WORDS[rng.below(WORDS.len() as u64)])
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn random_works_round_trip_and_edits_keep_their_promises() {
    for seed in 1..=24u64 {
        let mut rng = Lcg(seed);
        let (_dir, mut engine) = library();
        let rows = 1 + rng.below(6);
        let text: Vec<String> = (0..rows).map(|_| line(&mut rng)).collect();
        let mut view = engine
            .work_create(
                format!("Work {seed}"),
                Unit::Line,
                &text.join("\n"),
                "ru",
                None,
            )
            .unwrap();
        view = engine
            .work_add_edition(view.id, translation("fi"), "", &view.revision)
            .unwrap();
        let editions = [view.editions[0].id, view.editions[1].id];

        for step in 0..30 {
            let e = editions[rng.below(2)];
            let rows = view.editions[0].cells.len().max(1);
            let row = 1 + rng.below(rows as u64);
            let aligned_before = view.misalignments.is_empty();
            let op = rng.below(6);
            let result = match op {
                0 => {
                    let text = if rng.below(3) == 0 {
                        format!("{}\n{}", line(&mut rng), line(&mut rng))
                    } else {
                        line(&mut rng)
                    };
                    engine.work_write_row(view.id, e, row, &text, &view.revision)
                }
                1 => engine.work_insert_row(view.id, row, &view.revision),
                2 => engine.work_remove_row(view.id, e, row, &view.revision),
                3 => engine.work_set_note(view.id, row, &line(&mut rng), &view.revision),
                4 => engine.work_join(view.id, e, row, &view.revision),
                _ => engine.work_split(view.id, e, row, 2, &view.revision),
            };
            match result {
                Ok(next) => {
                    let counts = |v: &WorkView| -> Vec<usize> {
                        v.editions.iter().map(|e| e.cells.len()).collect()
                    };
                    match op {
                        // Writing a row, a note, or a row across every edition never puts an
                        // aligned work out of line.
                        0..=3 if aligned_before => {
                            assert!(next.misalignments.is_empty(), "seed {seed} step {step}")
                        }
                        // Join and split change the edition they name, and no other.
                        4 | 5 => {
                            let (was, now) = (counts(&view), counts(&next));
                            for (i, ed) in next.editions.iter().enumerate() {
                                if ed.id != e {
                                    assert_eq!(was[i], now[i], "seed {seed} step {step}");
                                }
                            }
                        }
                        _ => {}
                    }
                    // No note's words are lost, except one a note write replaced on purpose.
                    if op != 3 {
                        let all: String = next
                            .notes
                            .iter()
                            .map(|n| n.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        for note in &view.notes {
                            assert!(
                                all.contains(&note.text),
                                "seed {seed} step {step}: note lost"
                            );
                        }
                    }
                    view = next;
                }
                Err(Error::Invalid { .. }) => {}
                Err(other) => panic!("seed {seed} step {step}: {other:?}"),
            }
            // Whatever was written reads back the same from the files.
            let reopened = Engine::open(engine.root()).unwrap();
            assert_eq!(
                reopened.work_read(view.id, &[]).unwrap(),
                view,
                "seed {seed} step {step}"
            );
        }
    }
}
