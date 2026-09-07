# Operations

> **Audience:** People calling the engine from a client
> **Status:** In progress — M0 implemented
> **Last reviewed:** 2026-09-07

Everything `Engine` offers. The shape of this surface is fixed: operations belonging to later
milestones already exist and return `Unimplemented`, so a client can be written against the whole
contract now and filling one in later changes a body rather than a signature.

Four rules hold everywhere:

1. **Identifiers, never paths.** Where an entity lives is the engine's business.
2. **No hidden session state.** There is no "current document" the engine remembers. The only
   session state is the open library.
3. **A write needs the revision from the last read.** A mismatch is a `Conflict`, never a
   silent overwrite.
4. **Every call can fail, and names why.** See [Errors](errors.md).

## Library

| Call | Does |
| --- | --- |
| `Engine::create(path, name)` | Creates a library in a folder that does not hold one |
| `Engine::open(path)` | Opens an existing library for reading and writing |
| `Library::open_read_only(path)` | Opens any library, including a newer format, without permitting writes |
| `status()` | Counts, whether the library is writable, and any problems found |
| `rebuild_index()` | Discards the search index and rebuilds it from the files |

`rebuild_index` is always safe. The index holds nothing that is not derivable from a scan, which
is why a corrupt one is simply deleted rather than repaired.

## Topics

A topic is a subject. It holds documents, and it can hold other topics, as deep as you like.

| Call | Does |
| --- | --- |
| `topic_create(parent, name)` | `None` as the parent makes a root topic |
| `topic_rename(id, name)` | Renames it. Links and documents are unaffected |
| `topic_move(id, new_parent)` | `None` moves it to the root. Refuses to move a topic inside itself |
| `topic_tree()` | The whole tree, with each topic's documents |

**Renaming a topic does not rename its folder.** Moving a whole subtree to follow a rename risks
far more than a slightly stale folder name costs.

## Documents

| Call | Does |
| --- | --- |
| `document_create(topic, title)` | An empty document in that topic |
| `document_read(id)` | The document, with the revision needed to write it |
| `document_write(id, body, revision)` | Replaces the body, or `Conflict` if the file changed |
| `document_rename(id, title)` | Changes the title; the identifier and body are untouched |
| `document_move(id, topic)` | Moves it to another topic |
| `document_delete(id)` | Removes it |
| `backlinks(id)` | Documents linking here, derived from `[[id]]` in their text |

Two constructs in a document's text mean something to the engine:

| Written | Means |
| --- | --- |
| `[[<id>]]` | A link to another document. Produces a backlink |
| `[^cite:<id>]` or `[^cite:<id>@44-46]` | A citation of a source, optionally with a locator |

Both are stored exactly as written and interpreted at read time. Malformed ones are ordinary
text — someone typing `[[` mid-sentence is writing prose, not a broken link.

## Search

| Call | Does |
| --- | --- |
| `search(query)` | Ranked hits across titles and bodies, with excerpts |

A person's words are quoted before they reach the index, so `protein (2021)` is a search rather
than a syntax error. Diacritics are folded: `aakkoset` finds `Ääkköset`.

## Later milestones

These exist and return `Unimplemented`:

| Call | Arrives in |
| --- | --- |
| `source_create`, `citation_format` | M1 — sources and APA 7 citations |
| `backup_plan` | M2 — backup and restore |
| `work_create` | M3 — parallel texts |
| `map_read` | M4 — mind maps |

---

*Changing the engine rather than calling it? See the [technical documentation](../technical/).*
