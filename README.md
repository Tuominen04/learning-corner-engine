# Personal Learning Corner Engine

The engine behind [Personal Learning Corner](https://github.com/Tuominen04/personal-learning-corner).
Everything that touches a person's library lives here: their files, the search index built from
those files, citation formatting, parallel-text alignment, and backup comparison.

## Why it exists separately

The application it serves is a desktop program, but the rules about someone's data are not a
desktop concern. They are the product.

Keeping them in their own crate means they can be tested with no window open, reasoned about
without a user interface in the way, and reused by anything that comes later — a command-line
tool, a second client, an importer. A rule that only runs inside a running application is a rule
nobody can check.

## What it guarantees

These are commitments, not implementation details. Everything in the crate is arranged to keep
them true.

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

**Nothing leaves the machine.** No network calls, no telemetry, no account. The engine works on
a computer that has never been online.

## Status

**M0 — foundations.** Libraries, topics, documents, and the index. Sources and APA 7 citations
are M1, backup M2, parallel texts M3, mind maps M4.

Operations belonging to later milestones already exist and report themselves as unimplemented,
so a client can be written against the whole surface now.

## Documentation

| Read this if you want to… | Go to |
| --- | --- |
| **use** the engine in a client | [`docs/user/`](docs/user/) |
| **change** the engine | [`docs/technical/`](docs/technical/) |
| know what the engine is *required* to do | [the specification](https://github.com/Tuominen04/personal-learning-corner/tree/main/docs/technical) in the main repository |

That last row matters. The specification is normative and lives with the product it describes;
where this crate and those documents disagree, the documents are right and this is a bug.

## Licence

Dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.

The one font the engine carries, DejaVu Sans, is not ours and keeps its own free licence: the
Bitstream Vera licence, with the DejaVu changes in the public domain. It is in
[`crates/plc-engine/fonts/LICENSE`](crates/plc-engine/fonts/LICENSE), and it lets the font
travel inside every PDF the engine writes.
