//! Back up, bring back and restore, on real folders — the cases `testing.md` lists.
//!
//! Edits are made straight to files, the way a person on another computer, or a file manager,
//! would make them: a backup compares files, and does not care which program wrote them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lc_engine::backup::{Action, Answer, Outcome, Plan};
use lc_engine::{Engine, Error};

// ---- helpers ---------------------------------------------------------------------------------

struct Setup {
    _dir: tempfile::TempDir,
    engine: Engine,
    drive: PathBuf,
    base: PathBuf,
}

/// A library with a few files of every shape, and a drive folder that does not exist yet.
fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_path_buf();
    let engine = Engine::create(base.join("library"), "Learning").unwrap();
    let root = engine.root().to_path_buf();
    write(
        &root,
        "topics/protein/what-protein-is.md",
        "Protein is made of amino acids.",
    );
    write(
        &root,
        "topics/protein/in-the-body/how-it-is-used.md",
        "Muscles, enzymes.",
    );
    write(&root, "topics/ruoka/mitä proteiini on.md", "Suomeksi.");
    write(&root, "topics/язык/текст.md", "По-русски.");
    fs::create_dir_all(root.join("sources/files")).unwrap();
    fs::write(
        root.join("sources/files/paper.pdf"),
        [0u8, 159, 146, 150, 255, 0, 1],
    )
    .unwrap();
    Setup {
        drive: base.join("drive/Learning"),
        _dir: dir,
        engine,
        base,
    }
}

impl Setup {
    fn root(&self) -> PathBuf {
        self.engine.root().to_path_buf()
    }

    fn backup(&mut self) -> lc_engine::backup::Applied {
        let plan = self.engine.backup_plan(&self.drive).unwrap();
        self.engine.backup_apply(&plan, false).unwrap()
    }
}

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// Every file under `root`, relative path to bytes, leaving out `.plc/` and, when asked,
/// `.conflicts/`.
fn tree(root: &Path, with_conflicts: bool) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    collect(root, root, with_conflicts, &mut out);
    out
}

fn collect(root: &Path, dir: &Path, with_conflicts: bool, out: &mut BTreeMap<String, Vec<u8>>) {
    if !dir.exists() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == ".plc" || (!with_conflicts && rel == ".conflicts") {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, with_conflicts, out);
        } else {
            out.insert(rel, fs::read(&path).unwrap());
        }
    }
}

fn outcomes(plan: &Plan) -> Vec<(&str, Outcome, Action)> {
    plan.entries
        .iter()
        .map(|e| (e.path.as_str(), e.outcome, e.action))
        .collect()
}

fn kept_contents(root: &Path) -> BTreeSet<Vec<u8>> {
    tree(&root.join(".conflicts"), true).into_values().collect()
}

// ---- back up ---------------------------------------------------------------------------------

#[test]
fn a_first_backup_mirrors_the_library_and_a_second_has_nothing_to_do() {
    let mut s = setup();
    let plan = s.engine.backup_plan(&s.drive).unwrap();
    assert!(plan.copy.is_none());
    assert_eq!(plan.drive_changes(), 0);
    assert!(plan
        .entries
        .iter()
        .all(|e| e.outcome == Outcome::NewInLibrary && e.action == Action::CopyToDrive));
    assert_eq!(
        plan.entries[0].path, "library.toml",
        "the manifest goes first"
    );

    let applied = s.engine.backup_apply(&plan, false).unwrap();
    assert!(applied.kept.is_empty());
    assert_eq!(tree(&s.root(), true), tree(&s.drive, true));
    assert!(s.drive.join(".plc/backup.toml").is_file());
    assert!(
        !s.drive.join(".plc/index.sqlite").exists(),
        ".plc/ is never copied"
    );

    let again = s.engine.backup_plan(&s.drive).unwrap();
    assert!(again.copy.is_some());
    assert!(again.entries.is_empty(), "{:?}", outcomes(&again));
    assert_eq!(again.unchanged.len(), tree(&s.root(), true).len());
}

#[test]
fn a_destination_must_be_empty_or_this_library() {
    let s = setup();

    let foreign = s.base.join("photos");
    write(&foreign, "holiday.jpg", "not a library");
    assert!(matches!(
        s.engine.backup_plan(&foreign),
        Err(Error::Invalid { .. })
    ));

    let other = Engine::create(s.base.join("other"), "Another").unwrap();
    assert!(matches!(
        s.engine.backup_plan(other.root()),
        Err(Error::Invalid { .. })
    ));

    let inside = s.root().join("topics/backup");
    assert!(matches!(
        s.engine.backup_plan(&inside),
        Err(Error::Invalid { .. })
    ));
    assert!(matches!(
        s.engine.backup_plan(&s.root()),
        Err(Error::Invalid { .. })
    ));

    let only_notes = s.base.join("fresh");
    write(&only_notes, ".directory", "[Dolphin]");
    assert!(s.engine.backup_plan(&only_notes).is_ok());
}

#[test]
fn a_stale_drive_is_simply_brought_up_to_date_keeping_nothing() {
    let mut s = setup();
    s.backup();
    let root = s.root();
    write(
        &root,
        "topics/protein/what-protein-is.md",
        "Rewritten, months later.",
    );
    write(&root, "topics/new/a.md", "New.");
    fs::remove_dir_all(root.join("topics/protein/in-the-body")).unwrap();

    let plan = s.engine.backup_plan(&s.drive).unwrap();
    assert_eq!(plan.drive_changes(), 0);
    let mut seen: Vec<_> = outcomes(&plan)
        .into_iter()
        .map(|(_, o, a)| (o, a))
        .collect();
    seen.sort_by_key(|(o, _)| format!("{o:?}"));
    assert_eq!(
        seen,
        [
            (Outcome::ChangedInLibrary, Action::CopyToDrive),
            (Outcome::NewInLibrary, Action::CopyToDrive),
            (Outcome::RemovedInLibrary, Action::RemoveFromDrive),
        ]
    );

    let applied = s.engine.backup_apply(&plan, false).unwrap();
    assert!(
        applied.kept.is_empty(),
        "an old version is not a change: {:?}",
        applied.kept
    );
    assert_eq!(tree(&root, true), tree(&s.drive, true));
    assert!(
        !s.drive.join("topics/protein/in-the-body").exists(),
        "a folder emptied by removals goes too"
    );
}

#[test]
fn a_backup_over_changes_on_the_drive_warns_and_keeps_them_when_overwritten() {
    let mut s = setup();
    s.backup();
    // Elsewhere, from the drive: one document changed, one added, one removed.
    write(
        &s.drive,
        "topics/protein/what-protein-is.md",
        "Fixed on the other computer.",
    );
    write(
        &s.drive,
        "topics/protein/added-there.md",
        "Added on the other computer.",
    );
    fs::remove_file(s.drive.join("topics/ruoka/mitä proteiini on.md")).unwrap();
    let drive_before = tree(&s.drive, true);

    let plan = s.engine.backup_plan(&s.drive).unwrap();
    assert_eq!(plan.drive_changes(), 3, "{:?}", outcomes(&plan));
    assert!(matches!(
        s.engine.backup_apply(&plan, false),
        Err(Error::Invalid { .. })
    ));
    assert_eq!(
        tree(&s.drive, true),
        drive_before,
        "refused means nothing written"
    );

    let applied = s.engine.backup_apply(&plan, true).unwrap();
    assert_eq!(tree(&s.root(), false), tree(&s.drive, false));
    let kept = kept_contents(&s.root());
    assert!(kept.contains(b"Fixed on the other computer.".as_slice()));
    assert!(kept.contains(b"Added on the other computer.".as_slice()));
    assert_eq!(
        applied.kept.len(),
        2,
        "the removal kept nothing: nothing was lost"
    );
}

// ---- bring back ------------------------------------------------------------------------------

#[test]
fn bringing_back_takes_the_drive_s_changes_and_leaves_the_library_s() {
    let mut s = setup();
    s.backup();
    let root = s.root();
    write(
        &s.drive,
        "topics/protein/what-protein-is.md",
        "Changed on the drive.",
    );
    write(&s.drive, "topics/protein/new-there.md", "New on the drive.");
    fs::remove_file(s.drive.join("topics/язык/текст.md")).unwrap();
    write(
        &root,
        "topics/ruoka/mitä proteiini on.md",
        "Changed in the library.",
    );

    let plan = Engine::restore_plan(&s.drive, &root).unwrap();
    assert_eq!(plan.asks(), 0);
    let actions: BTreeMap<_, _> = plan
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.action))
        .collect();
    assert_eq!(
        actions["topics/protein/what-protein-is.md"],
        Action::CopyToLibrary
    );
    assert_eq!(
        actions["topics/protein/new-there.md"],
        Action::CopyToLibrary
    );
    assert_eq!(actions["topics/язык/текст.md"], Action::RemoveFromLibrary);
    assert_eq!(actions["topics/ruoka/mitä proteiini on.md"], Action::Leave);

    let applied = Engine::restore_apply(&plan, &BTreeMap::new()).unwrap();
    let after = tree(&root, false);
    assert_eq!(
        after["topics/protein/what-protein-is.md"],
        b"Changed on the drive."
    );
    assert_eq!(after["topics/protein/new-there.md"], b"New on the drive.");
    assert!(!after.contains_key("topics/язык/текст.md"));
    assert_eq!(
        after["topics/ruoka/mitä proteiini on.md"],
        b"Changed in the library."
    );
    let kept = kept_contents(&root);
    assert!(kept.contains(b"Protein is made of amino acids.".as_slice()));
    assert!(kept.contains("По-русски.".as_bytes()));
    assert_eq!(applied.kept.len(), 2);

    // The next backup sees the drive as up to date: only the library's own change goes out.
    let next = s.engine.backup_plan(&s.drive).unwrap();
    assert_eq!(next.drive_changes(), 0, "{:?}", outcomes(&next));
}

#[test]
fn a_file_changed_in_both_is_asked_about_and_every_answer_holds() {
    for answer in [Answer::KeepLibrary, Answer::TakeDrive, Answer::KeepBoth] {
        let mut s = setup();
        s.backup();
        let root = s.root();
        let path = "topics/protein/what-protein-is.md";
        write(&s.drive, path, "The drive's version.");
        write(&root, path, "The library's version.");

        let plan = Engine::restore_plan(&s.drive, &root).unwrap();
        assert_eq!(
            outcomes(&plan),
            [(path, Outcome::ChangedInBoth, Action::Ask)]
        );
        assert!(matches!(
            Engine::restore_apply(&plan, &BTreeMap::new()),
            Err(Error::Invalid { .. })
        ));

        let answers = BTreeMap::from([(path.to_string(), answer)]);
        Engine::restore_apply(&plan, &answers).unwrap();
        let after = tree(&root, false);
        match answer {
            Answer::KeepLibrary => {
                assert_eq!(after[path], b"The library's version.");
                let next = s.engine.backup_plan(&s.drive).unwrap();
                assert_eq!(
                    next.drive_changes(),
                    0,
                    "kept mine: the next backup just copies it"
                );
            }
            Answer::TakeDrive => {
                assert_eq!(after[path], b"The drive's version.");
                assert!(kept_contents(&root).contains(b"The library's version.".as_slice()));
            }
            Answer::KeepBoth => {
                assert_eq!(after[path], b"The library's version.");
                let beside: Vec<_> = after
                    .iter()
                    .filter(|(p, _)| p.starts_with("topics/protein/what-protein-is (from backup "))
                    .collect();
                assert_eq!(beside.len(), 1, "{:?}", after.keys());
                assert_eq!(beside[0].1, b"The drive's version.");
            }
        }
    }
}

#[test]
fn keep_both_needs_a_file_on_both_sides() {
    let mut s = setup();
    s.backup();
    let root = s.root();
    let path = "topics/protein/what-protein-is.md";
    fs::remove_file(s.drive.join(path)).unwrap();
    write(&root, path, "Changed here, removed there.");

    let plan = Engine::restore_plan(&s.drive, &root).unwrap();
    assert_eq!(
        outcomes(&plan),
        [(path, Outcome::RemovedOnDriveChangedInLibrary, Action::Ask)]
    );
    let answers = BTreeMap::from([(path.to_string(), Answer::KeepBoth)]);
    assert!(matches!(
        Engine::restore_apply(&plan, &answers),
        Err(Error::Invalid { .. })
    ));
}

// ---- restore ---------------------------------------------------------------------------------

#[test]
fn a_restore_into_an_empty_folder_is_the_whole_library_and_already_in_step() {
    let mut s = setup();
    s.backup();
    let elsewhere = s.base.join("new-computer/Learning");

    let plan = Engine::restore_plan(&s.drive, &elsewhere).unwrap();
    assert!(plan
        .entries
        .iter()
        .all(|e| e.action == Action::CopyToLibrary));
    Engine::restore_apply(&plan, &BTreeMap::new()).unwrap();
    assert_eq!(tree(&elsewhere, true), tree(&s.drive, true));

    let restored = Engine::open(&elsewhere).unwrap();
    assert_eq!(restored.manifest().name, "Learning");
    let back = restored.backup_plan(&s.drive).unwrap();
    assert!(back.entries.is_empty(), "{:?}", outcomes(&back));
}

#[test]
fn a_restore_refuses_a_newer_format_before_writing_anything() {
    let mut s = setup();
    s.backup();
    let manifest = s.drive.join("library.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        text.replace("format_version = 1", "format_version = 99"),
    )
    .unwrap();

    let elsewhere = s.base.join("new-computer");
    assert!(matches!(
        Engine::restore_plan(&s.drive, &elsewhere),
        Err(Error::FormatTooNew { found: 99, .. })
    ));
    assert!(!elsewhere.exists());
}

#[test]
fn the_frozen_v1_library_backs_up_and_restores_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("v1");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1"),
        &library,
    );
    let engine = Engine::open(&library).unwrap();
    let drive = dir.path().join("drive");
    let plan = engine.backup_plan(&drive).unwrap();
    let mut engine = engine;
    engine.backup_apply(&plan, false).unwrap();

    let restored = dir.path().join("restored");
    let plan = Engine::restore_plan(&drive, &restored).unwrap();
    Engine::restore_apply(&plan, &BTreeMap::new()).unwrap();
    assert_eq!(tree(&restored, true), tree(&library, true));
    let reopened = Engine::open(&restored).unwrap();
    assert_eq!(
        reopened.status().unwrap().problems,
        engine.status().unwrap().problems
    );
}

// ---- interruption and surprises --------------------------------------------------------------

#[test]
fn a_file_changed_after_planning_stops_the_apply_and_a_fresh_plan_shows_what_is_left() {
    let mut s = setup();
    s.backup();
    let root = s.root();
    write(&root, "topics/a/1.md", "one");
    write(&root, "topics/b/2.md", "two");
    write(&root, "topics/c/3.md", "three");
    let plan = s.engine.backup_plan(&s.drive).unwrap();
    assert_eq!(plan.entries.len(), 3);

    write(
        &root,
        "topics/b/2.md",
        "two, edited after the plan was shown",
    );
    match s.engine.backup_apply(&plan, false) {
        Err(Error::Conflict { path }) => assert!(path.ends_with("2.md"), "{}", path.display()),
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(
        s.drive.join("topics/a/1.md").is_file(),
        "what was done stays done"
    );
    assert!(
        !s.drive.join("topics/c/3.md").exists(),
        "nothing after the surprise"
    );

    let fresh = s.engine.backup_plan(&s.drive).unwrap();
    let left: Vec<_> = fresh.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(left, ["topics/b/2.md", "topics/c/3.md"]);
    assert_eq!(fresh.drive_changes(), 0);
}

#[test]
fn a_swapped_drive_is_a_surprise() {
    let mut s = setup();
    s.backup();
    write(&s.root(), "topics/a/1.md", "one");
    let plan = s.engine.backup_plan(&s.drive).unwrap();
    fs::remove_dir_all(s.drive.join(".plc")).unwrap();
    assert!(matches!(
        s.engine.backup_apply(&plan, false),
        Err(Error::Conflict { .. })
    ));
}

#[test]
fn two_drives_each_keep_their_own_record() {
    let mut s = setup();
    let first = s.drive.clone();
    let second = s.base.join("second-drive");
    s.backup();
    let plan = s.engine.backup_plan(&second).unwrap();
    s.engine.backup_apply(&plan, false).unwrap();

    write(
        &s.root(),
        "topics/protein/what-protein-is.md",
        "Changed after both backups.",
    );
    let plan = s.engine.backup_plan(&first).unwrap();
    s.engine.backup_apply(&plan, false).unwrap();

    let plan = s.engine.backup_plan(&second).unwrap();
    assert_eq!(
        outcomes(&plan),
        [(
            "topics/protein/what-protein-is.md",
            Outcome::ChangedInLibrary,
            Action::CopyToDrive
        )]
    );
}

// ---- the round-trip property -----------------------------------------------------------------

/// A small deterministic generator, so a failure names the seed that produced it.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn name(&mut self) -> String {
        const PARTS: &[&str] = &[
            "proteiini",
            "äänne",
            "язык",
            "notes",
            "a b",
            "Ö",
            "x",
            "data",
        ];
        PARTS[self.below(PARTS.len() as u64) as usize].to_string()
    }
    fn contents(&mut self) -> Vec<u8> {
        (0..self.below(40)).map(|_| self.below(256) as u8).collect()
    }
}

fn random_files(rng: &mut Rng, root: &Path, count: usize) {
    for i in 0..count {
        let depth = rng.below(3);
        let mut path = PathBuf::from("topics");
        for _ in 0..depth {
            path.push(rng.name());
        }
        path.push(format!("{}-{i}.md", rng.name()));
        let full = root.join(&path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, rng.contents()).unwrap();
    }
}

/// Edit, add and remove a few files under `root`'s `topics/`.
fn mutate(rng: &mut Rng, root: &Path, tag: &str) {
    let files: Vec<_> = tree(root, false)
        .into_keys()
        .filter(|p| p.starts_with("topics/"))
        .collect();
    for path in &files {
        match rng.below(6) {
            0 => fs::write(root.join(path), format!("{tag} edited {}", rng.next())).unwrap(),
            1 => fs::remove_file(root.join(path)).unwrap(),
            _ => {}
        }
    }
    write(root, &format!("topics/{tag}-{}.md", rng.next()), tag);
}

/// Every content either side held before, that is not merely an older version of the library.
fn contents(root: &Path) -> BTreeSet<Vec<u8>> {
    tree(root, true).into_values().collect()
}

#[test]
fn backup_and_restore_round_trip_and_nothing_made_is_ever_lost() {
    for seed in 1..=24u64 {
        let mut rng = Rng(seed);
        let mut s = setup();
        random_files(&mut rng, &s.root(), 12);

        // Round trip: back up, restore elsewhere, byte-identical.
        s.backup();
        let elsewhere = s.base.join(format!("restored-{seed}"));
        let plan = Engine::restore_plan(&s.drive, &elsewhere).unwrap();
        Engine::restore_apply(&plan, &BTreeMap::new()).unwrap();
        assert_eq!(tree(&elsewhere, true), tree(&s.root(), true), "seed {seed}");

        // Both sides change; a bring-back, answered at random, loses nothing either side had.
        mutate(&mut rng, &s.root(), "library");
        mutate(&mut rng, &s.drive, "drive");
        let before: BTreeSet<_> = contents(&s.root())
            .union(&contents(&s.drive))
            .cloned()
            .collect();
        let plan = Engine::restore_plan(&s.drive, &s.root()).unwrap();
        let answers: BTreeMap<_, _> = plan
            .entries
            .iter()
            .filter(|e| e.action == Action::Ask)
            .map(|e| {
                let both = e.library.is_some() && e.drive.is_some();
                let pick = match rng.below(if both { 3 } else { 2 }) {
                    0 => Answer::KeepLibrary,
                    1 => Answer::TakeDrive,
                    _ => Answer::KeepBoth,
                };
                (e.path.clone(), pick)
            })
            .collect();
        Engine::restore_apply(&plan, &answers).unwrap();
        let after: BTreeSet<_> = contents(&s.root())
            .union(&contents(&s.drive))
            .cloned()
            .collect();
        let lost: Vec<_> = before.difference(&after).collect();
        assert!(lost.is_empty(), "seed {seed}: bring back lost {lost:?}");

        // More changes on both; a backup that overwrites the drive's keeps every one of them.
        mutate(&mut rng, &s.root(), "library-again");
        mutate(&mut rng, &s.drive, "drive-again");
        let plan = s.engine.backup_plan(&s.drive).unwrap();
        let drive_changes: BTreeSet<Vec<u8>> = plan
            .entries
            .iter()
            .filter(|e| e.drive_change && e.drive.is_some())
            .map(|e| fs::read(s.drive.join(&e.path)).unwrap())
            .collect();
        let library_before = contents(&s.root());
        s.engine.backup_apply(&plan, true).unwrap();
        let after = contents(&s.root());
        for content in library_before.iter().chain(&drive_changes) {
            assert!(
                after.contains(content),
                "seed {seed}: backup lost {content:?}"
            );
        }
        assert_eq!(tree(&s.root(), false), tree(&s.drive, false), "seed {seed}");
    }
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
