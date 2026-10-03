# Fixture libraries

Real libraries, as folders, opened by `tests/libraries.rs` the way a person's library is
opened. A test always copies one to a temporary folder first, because opening a library writes
its index (`.plc/`) — a fixture itself is never written to, and holds no `.plc/`.

| Library | What it is | What it proves |
| --- | --- | --- |
| `minimal` | `library.toml` and nothing else | An empty library opens clean, and takes its first topic and document |
| `damaged` | A small library broken the ways people break them | Every broken file is named, the rest opens, and nothing is rewritten |
| `v1` | A small library in the format as M1 ships it — **frozen** | That format still opens, and reads exactly as `v1-reading.txt` records |

## `damaged`

Each fault is one a person makes, by hand or by file manager:

- `topics/protein/broken-front-matter.md`: front matter that is not TOML, the title's closing
  quote gone. Its writing must survive.
- `topics/protein/what-protein-is-copy.md`: a document copied beside itself, so two files
  claim one identifier. Both are named; the one read is whichever sorts first.
- `sources/protein-requirements-nd.toml`: a source edited into a type there is no such thing as.
- `topics/vitamins/topic.toml`: a topic whose own file is not TOML.
- `topics/protein/an-orphaned-citation.md`: a citation of a source that has gone. Not a broken
  file — the citation is kept as written and shows as a missing source, as in the editor.
- `topics/protein/notes.txt`: something of the person's own beside the documents, which is
  theirs and is left alone.

## `v1` is frozen

`v1` was made by the engine itself (v0.17.1), through its own operations, so every file is
exactly what that engine writes. It holds every kind of source, every role, every shape of date,
an attached paper, a picture beside its document, every form of citation, links between
documents, and Finnish and Cyrillic in titles, folders and filenames.

**It never changes.** `v1_is_exactly_as_it_was_frozen` holds a hash of its bytes, and fails if
anything in it is edited, added or removed. When the format changes, the engine that writes the
new format makes `v2` beside it, and `v1` stays, proving that libraries made before the change
still open.

`v1-reading.txt` is not frozen: it is what this engine reads from `v1`. A deliberate change to
how a library is read changes it, and is rewritten with `PLC_BLESS=1` like every expected file
— read what was written before committing it.
