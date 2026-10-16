//! A small library used across the integration tests.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use lc_engine::api::Engine;
use lc_engine::Id;

pub struct Fixture {
    dir: tempfile::TempDir,
    engine: Engine,
    ids: Ids,
}

/// Copied out before borrowing the engine, so a test can pass an identifier into a call on
/// the engine without borrowing the fixture twice.
#[derive(Debug, Clone, Copy)]
pub struct Ids {
    pub protein: Id,
    pub in_the_body: Id,
    pub what_protein_is: Id,
    pub types_of_protein: Id,
    pub how_used: Id,
}

impl Fixture {
    /// An empty library.
    pub fn empty() -> (tempfile::TempDir, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::create(dir.path(), "Learning").unwrap();
        (dir, engine)
    }

    /// The README's protein example, plus text in Finnish and Russian so that searching,
    /// sorting, and filenames are exercised in the languages this application is for.
    pub fn populated() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        Self::populated_at(dir, &root)
    }

    /// The same library, in a folder of its own inside the temporary directory — for tests
    /// that rename the library, whose folder then moves. Moving the temporary directory itself
    /// would leave it behind when the test ends.
    pub fn populated_in(folder: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(folder);
        Self::populated_at(dir, &root)
    }

    fn populated_at(dir: tempfile::TempDir, root: &Path) -> Self {
        let mut engine = Engine::create(root, "Learning").unwrap();

        let protein = engine.topic_create(None, "Protein").unwrap();
        let in_the_body = engine.topic_create(Some(protein), "In the body").unwrap();

        let what_protein_is = engine.document_create(protein, "What protein is").unwrap();
        let types_of_protein = engine.document_create(protein, "Types of protein").unwrap();
        let how_used = engine
            .document_create(in_the_body, "How protein is used")
            .unwrap();

        // Cross-links, so backlinks have something to derive.
        write(
            &mut engine,
            what_protein_is,
            &format!(
                "Protein is built from amino acids.\n\nSee [[{types_of_protein}]] for the kinds."
            ),
        );
        write(
            &mut engine,
            types_of_protein,
            &format!(
                "Complete proteins contain all nine essential amino acids.[^cite:{}]\n\n\
             Ääkköset ja öljy. Я вас любил.\n\nBack to [[{what_protein_is}]].",
                Id::new()
            ),
        );
        write(&mut engine, how_used, "Protein is used for tissue repair.");

        let ids = Ids {
            protein,
            in_the_body,
            what_protein_is,
            types_of_protein,
            how_used,
        };
        Fixture { dir, engine, ids }
    }

    pub fn ids(&self) -> Ids {
        self.ids
    }

    pub fn engine(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Where the library is now — which renaming it can change.
    pub fn root(&self) -> &Path {
        self.engine.root()
    }

    /// The temporary directory the library was made in.
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    pub fn index_path(&self) -> PathBuf {
        self.root().join(".plc/index.sqlite")
    }

    pub fn delete_index(&mut self) {
        fs::remove_dir_all(self.root().join(".plc")).unwrap();
    }

    pub fn corrupt_index(&mut self) {
        fs::write(self.index_path(), b"this is not a database at all").unwrap();
    }

    /// Close and reopen, as restarting the application would.
    pub fn reopen(&mut self) {
        let root = self.root().to_path_buf();
        self.engine = Engine::open(root).unwrap();
    }
}

fn write(engine: &mut Engine, id: Id, body: &str) {
    let current = engine.document_read(id).unwrap();
    engine.document_write(id, body, current.revision).unwrap();
}
