//! Keeping a copy of a source's paper.
//!
//! The promise is narrow and worth stating exactly: the file you attach is **copied into the
//! library**, so deleting the original — emptying a downloads folder, unplugging the stick it
//! came from — leaves the source with its paper. These tests are that promise, asserted.

use std::fs;
use std::path::{Path, PathBuf};

use lc_engine::api::{Engine, Keep};
use lc_engine::citation::source::{Contributor, Date, Role, Source, SourceType};
use lc_engine::Error;

mod fixture;
use fixture::Fixture;

/// A file that is a real enough PDF to be accepted, written wherever a test wants it.
fn write_pdf(at: &Path, marker: &str) -> PathBuf {
    fs::create_dir_all(at.parent().unwrap()).unwrap();
    fs::write(at, format!("%PDF-1.7\n% {marker}\n")).unwrap();
    at.to_path_buf()
}

fn a_source(engine: &mut Engine) -> lc_engine::Id {
    let mut source = Source::new(
        SourceType::JournalArticle,
        "Amino acids in adults",
        Date::Year(2021),
    );
    source.contributors = vec![Contributor::person(Role::Author, "Nieminen", "Anna Maria")];
    source.container = Some("Journal of Nutrition".into());
    source.volume = Some("151".into());
    engine.source_create(source).unwrap().0
}

#[test]
fn the_copy_survives_the_original_being_deleted() {
    // The whole feature, in one test: a copy, and then the original deleted by hand.
    let (_dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("some-paper.pdf"), "the paper");

    let id = a_source(&mut engine);
    engine.source_attach(id, &original, Keep::Copy).unwrap();

    fs::remove_file(&original).unwrap();
    assert!(!original.exists(), "the original is gone");

    let kept = engine.source_file(id).unwrap().expect("the library's copy");
    assert!(kept.is_file());
    assert!(fs::read_to_string(&kept).unwrap().contains("the paper"));
}

#[test]
fn moving_takes_the_original_away() {
    // What a downloads folder wants: the file has been filed, so it should not still be
    // sitting there to be found again next week and wondered about.
    let (_dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("some-paper.pdf"), "the paper");

    let id = a_source(&mut engine);
    engine.source_attach(id, &original, Keep::Move).unwrap();

    assert!(!original.exists(), "the original was left behind");
    let kept = engine.source_file(id).unwrap().expect("the library's copy");
    assert!(fs::read_to_string(&kept).unwrap().contains("the paper"));
}

#[test]
fn copying_leaves_the_original_alone() {
    // The other half of the choice, and the safe default. A file that lives somewhere
    // deliberate — a shared drive, a folder of papers — must not be moved out of it.
    let (_dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("some-paper.pdf"), "the paper");

    let id = a_source(&mut engine);
    engine.source_attach(id, &original, Keep::Copy).unwrap();

    assert!(original.is_file(), "copying removed the user's own file");
    assert!(engine.source_file(id).unwrap().is_some());
}

#[test]
fn a_refused_move_leaves_the_original_where_it_is() {
    // The ordering, stated as a test. The original is only removed once the library's copy is
    // written, recorded and indexed — so an attach that is refused must not have touched it.
    // Removing first and validating second would destroy a file to store nothing.
    let (_dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let not_a_pdf = downloads.path().join("notes.txt");
    fs::write(&not_a_pdf, "just some notes").unwrap();

    let id = a_source(&mut engine);
    assert!(matches!(
        engine.source_attach(id, &not_a_pdf, Keep::Move),
        Err(Error::Invalid { .. })
    ));
    assert!(
        not_a_pdf.is_file(),
        "a refused attach deleted the user's file anyway"
    );
    assert_eq!(engine.source_file(id).unwrap(), None);

    // And a refusal that comes *after* the file itself is judged fine: a real PDF, attached
    // to a source that is not there. This is the one that pins the removal to the very end
    // rather than merely to after the PDF check.
    let real = write_pdf(&downloads.path().join("paper.pdf"), "the paper");
    assert!(engine
        .source_attach(lc_engine::Id::new(), &real, Keep::Move)
        .is_err());
    assert!(
        real.is_file(),
        "an attach to an unknown source deleted the user's file"
    );
}

#[test]
fn what_is_recorded_is_a_name_and_not_a_path() {
    // A library has to survive being moved to another disk or another computer. Recording
    // where the PDF sat on one machine would break that on the first move.
    let (dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("paper.pdf"), "x");

    let id = a_source(&mut engine);
    let source = engine.source_attach(id, &original, Keep::Copy).unwrap();

    let name = source.file.expect("a file is recorded");
    assert!(!name.contains('/'), "recorded as {name}, which is a path");
    assert!(!name.contains(&*dir.path().to_string_lossy()));
    assert!(!name.contains(&*downloads.path().to_string_lossy()));
}

#[test]
fn the_copy_is_named_after_the_source_it_belongs_to() {
    // Opening the folder in a file manager should pair them by eye.
    let (dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("2103.15348v2.pdf"), "x");

    let id = a_source(&mut engine);
    let source = engine.source_attach(id, &original, Keep::Copy).unwrap();

    assert_eq!(source.file.as_deref(), Some("nieminen-2021.pdf"));
    assert!(dir.path().join("sources/nieminen-2021.toml").is_file());
    assert!(dir.path().join("sources/files/nieminen-2021.pdf").is_file());
}

#[test]
fn editing_a_source_does_not_detach_its_file() {
    // The trap this is here to catch: an interface sends the fields its form knows about, and
    // a form that had not heard of attachments would drop the paper on every typo fix.
    let (_dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("paper.pdf"), "x");

    let id = a_source(&mut engine);
    engine.source_attach(id, &original, Keep::Copy).unwrap();

    let mut edited = engine.source_read(id).unwrap();
    edited.file = None; // exactly what a form that does not carry the field would send
    edited.title = "Amino acids in adults, corrected".into();
    engine.source_update(edited).unwrap();

    assert!(
        engine.source_file(id).unwrap().is_some(),
        "correcting a title detached the paper"
    );
}

#[test]
fn attaching_again_replaces_the_first_copy() {
    let (dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let first = write_pdf(&downloads.path().join("first.pdf"), "the first one");
    let second = write_pdf(&downloads.path().join("second.pdf"), "the second one");

    let id = a_source(&mut engine);
    engine.source_attach(id, &first, Keep::Copy).unwrap();
    engine.source_attach(id, &second, Keep::Copy).unwrap();

    let kept = engine.source_file(id).unwrap().unwrap();
    assert!(fs::read_to_string(&kept)
        .unwrap()
        .contains("the second one"));

    let files: Vec<_> = fs::read_dir(dir.path().join("sources/files"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 1, "left an orphan behind: {files:?}");
}

#[test]
fn detaching_removes_the_copy() {
    let (dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("paper.pdf"), "x");

    let id = a_source(&mut engine);
    engine.source_attach(id, &original, Keep::Copy).unwrap();
    let source = engine.source_detach(id).unwrap();

    assert_eq!(source.file, None);
    assert_eq!(engine.source_file(id).unwrap(), None);
    assert!(!dir.path().join("sources/files/nieminen-2021.pdf").exists());
}

#[test]
fn deleting_a_source_takes_its_copy_with_it() {
    let (dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("paper.pdf"), "x");

    let id = a_source(&mut engine);
    engine.source_attach(id, &original, Keep::Copy).unwrap();
    engine.source_delete(id).unwrap();

    assert!(
        !dir.path().join("sources/files/nieminen-2021.pdf").exists(),
        "a paper nobody can reach from inside the app or identify from outside it"
    );
}

#[test]
fn a_file_that_is_not_a_pdf_is_refused() {
    let (_dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let not_a_pdf = downloads.path().join("notes.txt");
    fs::write(&not_a_pdf, "just some notes").unwrap();

    let id = a_source(&mut engine);
    assert!(matches!(
        engine.source_attach(id, &not_a_pdf, Keep::Copy),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn a_pdf_in_name_only_is_refused() {
    // The extension is a claim, not a fact. Catching it now beats a file that will not open
    // months later, when somebody goes back to check what they wrote.
    let (_dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let liar = downloads.path().join("paper.pdf");
    fs::write(&liar, "PK\u{3}\u{4} this is a zip wearing a hat").unwrap();

    let id = a_source(&mut engine);
    assert!(matches!(
        engine.source_attach(id, &liar, Keep::Copy),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn a_missing_file_is_refused_rather_than_copied_empty() {
    let (_dir, mut engine) = Fixture::empty();
    let id = a_source(&mut engine);
    let nowhere = PathBuf::from("/nonexistent/paper.pdf");
    assert!(matches!(
        engine.source_attach(id, &nowhere, Keep::Copy),
        Err(Error::Invalid { .. })
    ));
}

#[test]
fn a_record_naming_a_file_outside_the_library_resolves_to_nothing() {
    // Source files are plain text a user can edit, so the name in one is not to be trusted as
    // a path. Nothing the engine opens may be reached by climbing out of the library.
    //
    // The file outside is real, and really is where the escape would land. A test pointing at
    // somewhere nothing exists passes whether or not the guard is there, which is no test.
    let outside = tempfile::tempdir().unwrap();
    let root = outside.path().join("library");
    let mut engine = Engine::create(&root, "Learning").unwrap();
    let id = a_source(&mut engine);

    let elsewhere = outside.path().join("elsewhere.pdf");
    write_pdf(&elsewhere, "not the library's to open");
    assert!(elsewhere.is_file());

    // Attach something first, so `sources/files/` genuinely exists — otherwise the escape is
    // stopped by the missing directory rather than by the guard, and the test proves nothing.
    engine.source_attach(id, &elsewhere, Keep::Copy).unwrap();
    assert!(root.join("sources/files").is_dir());

    // `sources/files/` + `../../../elsewhere.pdf` lands exactly on it.
    let record = root.join("sources/nieminen-2021.toml");
    let text = fs::read_to_string(&record).unwrap();
    let hand_edited = text.replace(
        "file = \"nieminen-2021.pdf\"",
        "file = \"../../../elsewhere.pdf\"",
    );
    assert_ne!(
        hand_edited, text,
        "the record did not name its file as expected"
    );
    fs::write(&record, hand_edited).unwrap();

    assert_eq!(
        engine.source_file(id).unwrap(),
        None,
        "a hand-edited record reached a file outside the library"
    );
}

#[test]
fn a_copy_deleted_from_the_folder_is_reported_as_gone() {
    // A user can delete anything in their own folder. The honest answer to "where is it" is
    // then that there is nothing to open, rather than a path that does not exist.
    let (dir, mut engine) = Fixture::empty();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("paper.pdf"), "x");

    let id = a_source(&mut engine);
    engine.source_attach(id, &original, Keep::Copy).unwrap();
    fs::remove_file(dir.path().join("sources/files/nieminen-2021.pdf")).unwrap();

    assert_eq!(engine.source_file(id).unwrap(), None);
}

#[test]
fn an_attachment_survives_the_index_being_deleted() {
    // `.plc/` is derived and disposable (ADR 0003). An attachment is not derived, and must
    // come back exactly as it was.
    let mut fixture = Fixture::populated();
    let downloads = tempfile::tempdir().unwrap();
    let original = write_pdf(&downloads.path().join("paper.pdf"), "the paper");

    let id = a_source(fixture.engine());
    fixture
        .engine()
        .source_attach(id, &original, Keep::Copy)
        .unwrap();
    let before = fs::read(fixture.engine().source_file(id).unwrap().unwrap()).unwrap();

    fs::remove_dir_all(fixture.root().join(".plc")).unwrap();
    fixture.reopen();

    let after = fs::read(fixture.engine().source_file(id).unwrap().unwrap()).unwrap();
    assert_eq!(before, after);
}
