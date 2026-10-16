# Getting started

> **Audience:** People calling the engine from a client
> **Status:** Implemented
> **Last reviewed:** 2026-09-07

From nothing to a library with a document in it.

## Add the dependency

The crate is not published to a registry, so it is pinned by immutable git tag:

```toml
[dependencies]
lc-engine = { git = "https://github.com/Tuominen04/learning-corner-engine", tag = "v0.19.0" }
```

Never a `path` dependency in anything committed: it builds on its author's machine and nowhere
else.

Nothing to install alongside it. SQLite is compiled from source by `rusqlite`'s bundled feature,
so there are no system libraries to find.

## Open a library

A **library** is one folder that a person chose. Everything they write lives inside it.

```rust
use lc_engine::api::Engine;

// A new library, in a folder that does not yet hold one.
let mut engine = Engine::create("/home/someone/Learning", "Learning")?;

// Or an existing one.
let mut engine = Engine::open("/home/someone/Learning")?;
```

`create` refuses if the folder already holds a library, rather than merging into it. `open`
refuses a library written by a **newer** storage format rather than risking damage to it — use
`Library::open_read_only` to show such a library without writing to it.

## Write something

```rust
let protein = engine.topic_create(None, "Protein")?;
let in_the_body = engine.topic_create(Some(protein), "In the body")?;

let document = engine.document_create(protein, "What protein is")?;

let current = engine.document_read(document)?;
engine.document_write(document, "Protein is built from amino acids.", current.revision)?;
```

Two things worth noticing straight away.

**Identifiers, never paths.** Every call takes an identifier. Where an entity lives on disk is
the engine's business, which is what lets files be renamed and moved without anything else
needing to know.

**A write requires the revision from the last read.** If the file changed underneath — because
someone edited it in a text editor, which they are explicitly allowed to do — the write is
refused with `Conflict` rather than discarding their edit. Carry the revision from `read` to
`write`; never construct or compare one.

## Read it back

```rust
let document = engine.document_read(document)?;
println!("{}: {}", document.title, document.body);

// Backlinks are derived from `[[id]]` in the text, so they cannot go stale.
for source in engine.backlinks(document.id)? {
    println!("linked from {}", source.title);
}

// Search covers titles and bodies, and tolerates whatever a person types.
for hit in engine.search("amino acids")? {
    println!("{} — {}", hit.title, hit.excerpt);
}
```

## Check the library's health

```rust
let status = engine.status()?;
println!("{} topics, {} documents", status.topics, status.documents);

for problem in &status.problems {
    // A file that will not parse, or two files claiming the same identifier.
    // The library still opens; the person needs telling which files clash.
    println!("{}: {}", problem.path.display(), problem.message);
}
```

## See it on disk

The fastest way to understand the storage format is to look at one:

```sh
cargo run --example walkthrough
find /tmp/lc-walkthrough -not -path '*/.plc/*'
cat /tmp/lc-walkthrough/topics/protein/what-protein-is.md
```

Plain folders mirroring the topic tree, and documents that open in any text editor. That is the
whole point.

## Next

- [Operations](operations.md) — the full surface
- [Errors](errors.md) — what can fail and what to do about it

---

*Changing the engine rather than calling it? See the [technical documentation](../technical/).*
