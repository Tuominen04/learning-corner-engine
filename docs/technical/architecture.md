# Architecture

> **Audience:** People changing the engine
> **Status:** In progress — M1 implemented
> **Last reviewed:** 2026-10-02

How this crate is put together, and the rules that keep it that way. What it is *required* to do
is specified in the main repository; this describes the implementation.

## Layers

Each depends only on those above it. Build and read them in this order — every layer is testable
before the next one exists.

```
error, id, timestamp    the vocabulary: failure causes, ULIDs, TOML datetimes
        ↓
atomic                  durable writes: temp → fsync → rename → fsync dir
        ↓
library                 library.toml, format versions, what is reserved
        ↓
model                   the entities: topics, documents, `+++` front matter
        ↓
store                   the folder tree: reading, writing, scanning, slugs
        ↓
index                   SQLite cache derived from a scan, and nothing else
        ↓
citation, export        APA 7, and documents as files: pure logic over values
        ↓
api                     Engine — what a client calls
```

| Module | Holds |
| --- | --- |
| `error` | The closed set of failure causes. Adding one is a contract change |
| `id` | ULIDs. Identity lives *in* files, never in their names |
| `timestamp` | Native TOML datetimes, so a hand-editing person sees a date, not a quoted string |
| `atomic` | The durability primitive everything else writes through |
| `library` | `library.toml`, the three format-version outcomes, `.plc/` and `.conflicts/` |
| `model` | `Topic`, `Document`, and the front-matter parser |
| `store` | Layout, entity read/write, the scan, revisions, slugs |
| `index` | Tables, full-text search, incremental update, rebuild |
| `citation` | Sources, the library-wide disambiguation pass, and the APA 7 formatters |
| `export` | A document as somebody else reads it: one model read from the body, a writer per format |
| `api` | `Engine`: the operations, and conversion into what a client sees |

## Rules

**Nothing above `store` touches the filesystem directly.** Every read and write goes through it,
so durability and the layout are decided in one place.

**Nothing writes a file except through `atomic`.** A single `fs::write` anywhere else silently
gives up the guarantee that a replaced file is whole.

**The index is derived and nothing else.** Anything it holds that a full scan cannot reproduce is
a bug. This is enforced by test, not by discipline — see [Testing](testing.md).

**`api` converts, it does not decide.** Logic there is logic in the wrong layer: it cannot be
tested without building an `Engine` over a real directory, and it will be duplicated the moment a
second entry point exists.

**Identity is the ULID inside a file.** Never the path. This is what lets `store` rename and move
freely while links and citations keep working.

## Two decisions that look odd until you hit them

**Revisions come from the filesystem, not from a document's `modified` field.** An external
editor changes a body without touching front matter, so a check against `modified` would miss
exactly the case the conflict check exists to catch.

**Structural changes rebuild the index; body writes update it in place.** Moving a topic changes
the paths of everything beneath it, and working out which descendants moved is precisely the
bookkeeping that drifts out of step with the files. Body writes are the hot path and are
incremental. A test asserts the two agree.

## Where a change belongs

| Changing… | Goes in |
| --- | --- |
| What is written to a file | `store`, or `model` for its shape |
| How durable a write is | `atomic` — nowhere else |
| What is searchable or linked | `index` |
| An operation a client calls | `api`, plus the contract in the main repository **first** |
| A new failure cause | `error`, plus the contract — it is part of the surface |

Changing an operation is a three-step sequence in a fixed order: the contract document in the
main repository, then the implementation here with tests, then the client. Any other order
produces two components each convinced the other is wrong.

## Related

- [Development](development.md) — building and running it
- [Testing](testing.md) — the suites and the properties they defend
- [The specification](https://github.com/Tuominen04/learning-corner/tree/main/docs/technical) — normative
