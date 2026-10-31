# Learning Corner Engine

The Rust library behind [Learning Corner](https://github.com/Tuominen04/learning-corner), a
local-first desktop app for notes, sources and parallel texts. Everything that touches a person's
library lives here: their files, the search index built from those files, citation formatting,
parallel-text alignment, and backup comparison.

It has no window and no interface code of its own. Learning Corner is its one client today; a
command-line tool or a second client could use it the same way.

> **Looking for the app?** Downloads, the user guide, questions and ideas all live in
> [Learning Corner](https://github.com/Tuominen04/learning-corner). This repository is only the
> engine.

## Start here

You need Rust (the version is pinned in [`rust-toolchain.toml`](rust-toolchain.toml); `rustup`
picks it up on its own). There are no system libraries to install — SQLite is compiled in.

```sh
git clone https://github.com/Tuominen04/learning-corner-engine
cd learning-corner-engine
cargo test
cargo run --example walkthrough -- /tmp/lc-walkthrough
```

The walkthrough builds a small library on disk — topics, linked documents, a search index it
deletes and rebuilds — so you can open the folder and see exactly what the engine writes.

To use the engine from your own code, depend on it by tag:

```toml
[dependencies]
lc-engine = { git = "https://github.com/Tuominen04/learning-corner-engine", tag = "v0.19.0" }
```

Then follow [Getting started](docs/user/getting-started.md).

## Privacy

The engine has no network code. It makes no requests, sends no telemetry, and needs no account;
it works on a computer that has never been online. It reads and writes one folder — the library
a person chose — and nothing else.

## What it guarantees

These are commitments, not implementation details. Everything in the crate is arranged to keep
them true, and each one has tests behind it.

**Files are the truth.** Everything a person writes is a readable file in a folder they chose.
The search index is a cache built from those files; delete it and nothing is lost. Anything the
index knows that cannot be recovered from a full scan is a bug, and a test fails the moment one
appears.

**Nothing is destroyed without an explicit decision.** No operation overwrites or discards
content on its own. Where intent cannot be determined the engine stops and asks, and the copy
that lost is kept. This holds for a document edited in two places at once as much as for a
backup restored over a library.

**A write survives a power cut.** Files are replaced whole or not at all — never truncated,
never half-written.

**The library outlives the software.** Plain text, ordinary folders, no proprietary container.
If this project stopped tomorrow, someone's notes would still open in any text editor.

## Status

**M1 — documents and sources.** On top of M0's libraries, topics, documents and index: sources
of eight kinds, the library-wide disambiguation pass, APA 7 citations and reference lists,
search across documents and sources, and export to PDF, Word, OpenDocument, a web page and
plain text.

**M2 — backup and restore**, in the engine: back up to a drive, bring the drive's changes back,
restore into an empty folder — two-phase, resumable, and keeping every version it replaces.

**M3 — parallel texts**, in the engine: works with an original, pronunciations and
translations lined up row by row, divided by line, sentence or paragraph; repeated rows, notes,
sources, and misalignment shown rather than hidden. Mind maps are M4.

Operations belonging to later milestones already exist and report themselves as unimplemented,
so a client can be written against the whole surface now.

## Documentation

| Read this if you want to… | Go to |
| --- | --- |
| **use** the engine in a client | [`docs/user/`](docs/user/) |
| **change** the engine | [`docs/technical/`](docs/technical/) |
| know what the engine is *required* to do | [the specification](https://github.com/Tuominen04/learning-corner/tree/main/docs/technical) in the app repository |

The specification is normative and lives with the product it describes; where this crate and
those documents disagree, the documents are right and this is a bug.

## Why a separate repository

The rules about someone's data are not a desktop concern — they are the product. Keeping them in
their own crate means they can be tested with no window open, reasoned about without a user
interface in the way, and reused by anything that comes later. A rule that only runs inside a
running application is a rule nobody can check.

## Contributing

Bug reports and improvements are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). If you found a
problem while using the app, report it in
[Learning Corner](https://github.com/Tuominen04/learning-corner/issues) instead; it will be moved
here if the cause turns out to be in the engine. Security problems go through
[SECURITY.md](SECURITY.md), not a public issue.

## Licence

Dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.

The one font the engine carries, DejaVu Sans, is not ours and keeps its own free licence: the
Bitstream Vera licence, with the DejaVu changes in the public domain. It is in
[`crates/lc-engine/fonts/LICENSE`](crates/lc-engine/fonts/LICENSE), and it lets the font
travel inside every PDF the engine writes.
