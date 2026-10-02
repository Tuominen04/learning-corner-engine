# Testing

> **Audience:** People changing the engine
> **Status:** In progress — M0 implemented
> **Last reviewed:** 2026-10-01

There is no server, no network, and one user. That removes whole categories of testing and
concentrates the risk in one place: **the library on disk.**

## What the risk actually is

Ranked by how bad it is when it happens. Effort follows this order, not the order things are easy
to test.

1. **Someone's work is lost or corrupted** — a bad write, a wrong conflict resolution, a
   migration that eats a field. Unrecoverable, and it ends their trust permanently.
2. **A citation is silently wrong** — the product's whole claim is that it gets this right.
3. **A parallel text is silently misaligned** — a learner cannot detect it, which is what makes
   it serious.
4. **The engine will not open a library.** Bad, but the files are still there and still readable.

## The two properties that must never break

Both are asserted from the first commit rather than added later. Retrofitting either means
discovering everything that already violates it, at the point where all of it is load-bearing.

### Rebuild equivalence — `tests/rebuild_equivalence.rs`

Record the result of every read, delete `.plc/`, rebuild, assert every result is identical.

This is what makes files authoritative and the index disposable. The way that claim stops being
true is gradually — one cached field that is not derivable, one write that updates the index but
not the file — and this catches it the moment it happens.

Three companions live with it: **incremental writes agree with a full rebuild** (the incremental
path runs constantly, so a divergence there is the one that bites), an **explicit rebuild changes
nothing**, and a **corrupt index is discarded and rebuilt** rather than surfaced as an error.

### Atomic writes — `src/atomic.rs`

A replaced file is wholly old or wholly new. No temporary files left behind.

Assert also that **the temporary file shares the target's directory.** Move it to a temp
directory and `rename` silently stops being atomic across filesystems, with no test failing to
say so — a guarantee can disappear without a single red build.

## The suites

| File | Covers |
| --- | --- |
| Unit tests, beside the code | Pure logic: identifiers, timestamps, front matter, slugs, atomic writes |
| `tests/contract.rs` | Every operation, its results, and the error each failure produces |
| `tests/filesystem.rs` | What actually lands on disk, and what happens when someone edits it |
| `tests/rebuild_equivalence.rs` | The property above |
| `tests/citations.rs` | APA 7 rule by rule, and the disambiguation rules that need more than one source to see |
| `tests/apa.rs`, `tests/apa/` | APA 7 example by example: one file per kind of source, each case a source as stored and the three forms APA prints for it |
| `tests/library.rs` | A library's name, and the folder that follows it |
| `tests/names.rs` | Filenames that follow titles |
| `tests/split.rs` | Splitting a document without cutting anything in half |
| `tests/deleting.rs` | Deleting topics and documents, and what is kept |
| `tests/images.rs`, `tests/attachments.rs` | Pictures and papers copied into the library |
| `tests/fixture/` | A small shared library — the protein example, with Finnish and Cyrillic text |

`contract.rs` runs against no client at all. That is the property keeping the two-repository
boundary honest: logic that leaked out of the engine cannot be tested here, and its absence shows
up as a gap.

## Cases that earn their place

The ones that have already caught real bugs:

- **A document edited outside the application** while it was closed — the index must notice.
- **A stale write** — refused with `Conflict`, and their edit still on disk afterwards.
- **Broken front matter** — diagnosed, named, and the rest of the library still opens.
- **A duplicated file** — reported, first occurrence wins, the library still opens. This one made
  the engine refuse to open entirely until it was tested.
- **Punctuation in a search** — `protein (2021)` is a search, not a syntax error.
- **Finnish and Cyrillic** in titles, filenames, and search. Diacritic folding is not optional in
  an application for people learning Russian.
- **A library copied to another location** and opened there.

## Adding a test

Put it beside the code if it is pure logic, in `tests/` if it spans layers or touches a real
directory. Use `tempfile::tempdir()` — never a fixed path, and never anything outside a temporary
directory.

An APA example from the manual is not a test to write but a case to add: a few lines in the
right `tests/apa/<kind>.toml`, holding the source exactly as a library stores it and what APA
prints for it in parentheses, in a sentence, and in the reference list. Every case runs, and a
failure lists every form that printed wrongly rather than stopping at the first.

If you are fixing a bug, write the test that fails first. Every case in the list above exists
because something was wrong and nothing said so.

## Related

- [Architecture](architecture.md) — the layers being tested
- [Development](development.md) — running them
