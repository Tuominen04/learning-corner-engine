//! The derived index.
//!
//! A SQLite database at `.plc/index.sqlite`. It is a **cache**: everything in it comes from
//! the library files, and deleting it loses nothing. Anything here that cannot be recomputed
//! from a scan is a bug — the rebuild-equivalence test exists to catch exactly that.
//!
//! Corruption is therefore never a data-loss event. It is detected, logged, and answered by
//! deleting the file and rebuilding, with nothing to salvage by construction.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, IoContext, Result};
use crate::id::Id;
use crate::library::Library;
use crate::store::{Problem, Scan, Store};

/// Bumped whenever the tables below change. A mismatch triggers a rebuild rather than a
/// migration — there is nothing here worth migrating.
const INDEX_VERSION: u32 = 1;

const FILE: &str = "index.sqlite";

const SCHEMA: &str = r#"
CREATE TABLE entity (
    id       TEXT PRIMARY KEY,
    kind     TEXT NOT NULL,
    path     TEXT NOT NULL,
    title    TEXT NOT NULL,
    modified TEXT NOT NULL,
    digest   TEXT NOT NULL,
    file     TEXT NOT NULL,
    revision TEXT NOT NULL
);
CREATE INDEX entity_path ON entity(path);
CREATE INDEX entity_kind ON entity(kind);

CREATE VIRTUAL TABLE entity_fts USING fts5(
    id UNINDEXED,
    title,
    body,
    tokenize = "unicode61 remove_diacritics 2"
);

CREATE TABLE link (
    from_id TEXT NOT NULL,
    to_id   TEXT NOT NULL,
    PRIMARY KEY (from_id, to_id)
);
CREATE INDEX link_to ON link(to_id);

CREATE TABLE citation (
    document_id TEXT NOT NULL,
    source_id   TEXT NOT NULL,
    PRIMARY KEY (document_id, source_id)
);
CREATE INDEX citation_source ON citation(source_id);

CREATE TABLE problem (
    path     TEXT NOT NULL,
    message  TEXT NOT NULL,
    revision TEXT NOT NULL
);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: Id,
    pub kind: Kind,
    pub path: PathBuf,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Topic,
    Document,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Topic => "topic",
            Kind::Document => "document",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "topic" => Ok(Kind::Topic),
            "document" => Ok(Kind::Document),
            other => Err(Error::library(format!("unknown entity kind {other:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: Id,
    pub title: String,
    pub excerpt: String,
}

pub struct Index {
    conn: Connection,
    path: PathBuf,
}

impl Index {
    /// Open the index, rebuilding it if it is missing, stale, or damaged.
    ///
    /// Every failure mode has the same answer, because the index holds nothing that is not
    /// also in the files.
    pub fn open(library: &Library, store: &Store) -> Result<Self> {
        let dir = library.work_dir();
        fs::create_dir_all(&dir).ctx(format!("creating {}", dir.display()))?;
        let path = dir.join(FILE);

        if path.exists() {
            match Self::open_existing(&path) {
                Ok(index) => {
                    // The library can change while the application is closed — the user is
                    // told they may edit their files. A stat-only pass catches that without
                    // reading every document.
                    if index.is_current(store)? {
                        return Ok(index);
                    }
                    let mut index = index;
                    index.rebuild(store)?;
                    return Ok(index);
                }
                Err(_) => {
                    // Damaged or from another version. Nothing to salvage, by design.
                    let _ = fs::remove_file(&path);
                }
            }
        }

        let mut index = Self::create(&path)?;
        index.rebuild(store)?;
        Ok(index)
    }

    fn open_existing(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(sql)?;

        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(sql)?;
        if integrity != "ok" {
            return Err(Error::library(format!(
                "index integrity check said {integrity:?}"
            )));
        }

        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'index_version'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql)?;
        if version.as_deref() != Some(&INDEX_VERSION.to_string()) {
            return Err(Error::library("index was built by another version"));
        }

        Ok(Index {
            conn,
            path: path.to_path_buf(),
        })
    }

    fn create(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(sql)?;
        conn.execute_batch(SCHEMA).map_err(sql)?;
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('index_version', ?1)",
            params![INDEX_VERSION.to_string()],
        )
        .map_err(sql)?;
        Ok(Index {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// Discard everything and rebuild from a full walk of the library.
    pub fn rebuild(&mut self, store: &Store) -> Result<RebuildSummary> {
        let scan = store.scan()?;
        self.replace_with(&scan)?;
        Ok(RebuildSummary {
            topics: scan.topics.len(),
            documents: scan.documents.len(),
            problems: scan.problems.len(),
        })
    }

    fn replace_with(&mut self, scan: &Scan) -> Result<()> {
        let tx = self.conn.transaction().map_err(sql)?;
        for table in ["entity", "entity_fts", "link", "citation", "problem"] {
            tx.execute(&format!("DELETE FROM {table}"), [])
                .map_err(sql)?;
        }

        // A duplicated identifier — usually a document copied in a file manager — is reported
        // as a problem, not treated as a failure. The first occurrence in the sorted scan
        // wins and the rest are skipped, so a library with a duplicate still opens and the
        // user is told which files clash.
        let mut inserted: std::collections::HashSet<Id> = std::collections::HashSet::new();

        for found in &scan.topics {
            let t = &found.entity;
            if !inserted.insert(t.id) {
                continue;
            }
            insert_entity(
                &tx,
                t.id,
                Kind::Topic,
                &found.path,
                &t.name,
                &t.created.to_string(),
                &digest(t.name.as_bytes()),
                "",
                &found.path.join(crate::model::topic::MANIFEST),
                &found.revision.token(),
            )?;
        }

        for found in &scan.documents {
            let d = &found.entity;
            if !inserted.insert(d.id()) {
                continue;
            }
            insert_entity(
                &tx,
                d.id(),
                Kind::Document,
                &found.path,
                d.title(),
                &d.meta.modified.to_string(),
                &digest(d.body.as_bytes()),
                &d.body,
                &found.path,
                &found.revision.token(),
            )?;
            for to in d.links() {
                tx.execute(
                    "INSERT OR IGNORE INTO link (from_id, to_id) VALUES (?1, ?2)",
                    params![d.id().to_string(), to.to_string()],
                )
                .map_err(sql)?;
            }
            for source in d.citations() {
                tx.execute(
                    "INSERT OR IGNORE INTO citation (document_id, source_id) VALUES (?1, ?2)",
                    params![d.id().to_string(), source.to_string()],
                )
                .map_err(sql)?;
            }
        }

        for problem in &scan.problems {
            tx.execute(
                "INSERT INTO problem (path, message, revision) VALUES (?1, ?2, ?3)",
                params![
                    problem.path.display().to_string(),
                    problem.message,
                    problem.revision.clone().unwrap_or_default()
                ],
            )
            .map_err(sql)?;
        }

        tx.commit().map_err(sql)
    }

    /// Refresh one document's rows in place.
    ///
    /// The hot path: writing a document must not cost a full walk of the library. Structural
    /// operations that change paths rebuild instead, because working out which descendants
    /// moved is exactly the kind of bookkeeping that drifts out of step with the files.
    pub fn upsert_document(
        &mut self,
        found: &crate::store::Found<crate::model::Document>,
    ) -> Result<()> {
        let d = &found.entity;
        let id = d.id().to_string();
        let tx = self.conn.transaction().map_err(sql)?;

        for (table, column) in [
            ("entity", "id"),
            ("entity_fts", "id"),
            ("link", "from_id"),
            ("citation", "document_id"),
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE {column} = ?1"),
                params![id],
            )
            .map_err(sql)?;
        }

        insert_entity(
            &tx,
            d.id(),
            Kind::Document,
            &found.path,
            d.title(),
            &d.meta.modified.to_string(),
            &digest(d.body.as_bytes()),
            &d.body,
            &found.path,
            &found.revision.token(),
        )?;
        for to in d.links() {
            tx.execute(
                "INSERT OR IGNORE INTO link (from_id, to_id) VALUES (?1, ?2)",
                params![id, to.to_string()],
            )
            .map_err(sql)?;
        }
        for source in d.citations() {
            tx.execute(
                "INSERT OR IGNORE INTO citation (document_id, source_id) VALUES (?1, ?2)",
                params![id, source.to_string()],
            )
            .map_err(sql)?;
        }

        tx.commit().map_err(sql)
    }

    /// Drop every row belonging to an entity that no longer exists.
    pub fn forget(&mut self, id: Id) -> Result<()> {
        let id = id.to_string();
        let tx = self.conn.transaction().map_err(sql)?;
        for (table, column) in [
            ("entity", "id"),
            ("entity_fts", "id"),
            ("link", "from_id"),
            ("citation", "document_id"),
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE {column} = ?1"),
                params![id],
            )
            .map_err(sql)?;
        }
        tx.commit().map_err(sql)
    }

    /// Does the index still match what is on disk?
    ///
    /// Compares recorded paths, modification times and sizes against a stat-only walk. Cheap
    /// enough to run on every open, and conservative: anything it cannot account for counts
    /// as a difference, so the failure direction is a needless rebuild rather than a stale
    /// index.
    pub fn is_current(&self, store: &Store) -> Result<bool> {
        let on_disk = store.signature()?;
        let recorded = self.signature()?;
        Ok(on_disk == recorded)
    }

    fn signature(&self) -> Result<std::collections::BTreeMap<PathBuf, String>> {
        let mut out = std::collections::BTreeMap::new();

        let mut stmt = self
            .conn
            .prepare("SELECT file, revision FROM entity")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?;
        for row in rows {
            let (file, revision) = row.map_err(sql)?;
            out.insert(PathBuf::from(file), revision);
        }

        // Files that failed to parse are still accounted for, or a library with one broken
        // document would rebuild its index on every single open.
        let mut stmt = self
            .conn
            .prepare("SELECT path, revision FROM problem WHERE revision != ''")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?;
        for row in rows {
            let (path, revision) = row.map_err(sql)?;
            out.insert(PathBuf::from(path), revision);
        }

        Ok(out)
    }

    // ---- reads -----------------------------------------------------------------

    pub fn resolve(&self, id: Id) -> Result<Entry> {
        self.conn
            .query_row(
                "SELECT id, kind, path, title FROM entity WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(sql)?
            .ok_or_else(|| Error::NotFound(id.to_string()))
            .and_then(|(id, kind, path, title)| {
                Ok(Entry {
                    id: id.parse()?,
                    kind: Kind::parse(&kind)?,
                    path: PathBuf::from(path),
                    title,
                })
            })
    }

    pub fn entries(&self, kind: Kind) -> Result<Vec<Entry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, path, title FROM entity WHERE kind = ?1 ORDER BY path")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![kind.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sql)?;

        let mut out = Vec::new();
        for row in rows {
            let (id, kind, path, title) = row.map_err(sql)?;
            out.push(Entry {
                id: id.parse()?,
                kind: Kind::parse(&kind)?,
                path: PathBuf::from(path),
                title,
            });
        }
        Ok(out)
    }

    /// Documents linking to `id`. Derived from `[[…]]`, so it cannot go stale.
    pub fn backlinks(&self, id: Id) -> Result<Vec<Id>> {
        let mut stmt = self
            .conn
            .prepare("SELECT from_id FROM link WHERE to_id = ?1 ORDER BY from_id")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![id.to_string()], |row| row.get::<_, String>(0))
            .map_err(sql)?;
        rows.map(|r| r.map_err(sql).and_then(|s| s.parse()))
            .collect()
    }

    pub fn search(&self, query: &str) -> Result<Vec<Hit>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, snippet(entity_fts, 2, '', '', '…', 12)
                 FROM entity_fts WHERE entity_fts MATCH ?1 ORDER BY rank LIMIT 100",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![fts_query(query)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sql)?;

        let mut hits = Vec::new();
        for row in rows {
            let (id, title, excerpt) = row.map_err(sql)?;
            hits.push(Hit {
                id: id.parse()?,
                title,
                excerpt,
            });
        }
        Ok(hits)
    }

    pub fn problems(&self) -> Result<Vec<Problem>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, message, revision FROM problem ORDER BY path, message")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sql)?;

        let mut out = Vec::new();
        for row in rows {
            let (path, message, revision) = row.map_err(sql)?;
            out.push(Problem {
                path: PathBuf::from(path),
                message,
                revision: Some(revision).filter(|r| !r.is_empty()),
            });
        }
        Ok(out)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RebuildSummary {
    pub topics: usize,
    pub documents: usize,
    pub problems: usize,
}

#[allow(clippy::too_many_arguments)]
fn insert_entity(
    tx: &rusqlite::Transaction<'_>,
    id: Id,
    kind: Kind,
    path: &Path,
    title: &str,
    modified: &str,
    digest: &str,
    body: &str,
    file: &Path,
    revision: &str,
) -> Result<()> {
    tx.execute(
        "INSERT INTO entity (id, kind, path, title, modified, digest, file, revision)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id.to_string(),
            kind.as_str(),
            path.display().to_string(),
            title,
            modified,
            digest,
            file.display().to_string(),
            revision
        ],
    )
    .map_err(sql)?;
    tx.execute(
        "INSERT INTO entity_fts (id, title, body) VALUES (?1, ?2, ?3)",
        params![id.to_string(), title, body],
    )
    .map_err(sql)?;
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Escape a user's words so FTS5 treats them as terms rather than as query syntax.
///
/// Without this, a search for `protein (2021)` is a syntax error rather than a search.
fn fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn sql(e: rusqlite::Error) -> Error {
    Error::library(format!("index: {e}"))
}
