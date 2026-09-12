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
| `[^cite:<id>]` | A citation of a source |

A citation can also say **where** in the source, and **how** the sentence uses it:

| Written | Renders as |
| --- | --- |
| `[^cite:<id>]` | `(Nieminen, 2021)` |
| `[^cite:<id>@44]` | `(Nieminen, 2021, p. 44)` |
| `[^cite:<id>@44-46]` | `(Nieminen, 2021, pp. 44–46)` |
| `[^cite:<id>@para:7]` | `(Nieminen, 2021, para. 7)` |
| `[^cite:<id>@chapter:3]` | `(Nieminen, 2021, Chapter 3)` |
| `[^cite:<id>@time:1:04:22]` | `(Nieminen, 2021, 1:04:22)` |
| `[^cite:<id>!narrative]` | `Nieminen (2021)` |
| `[^cite:<id>@44!narrative]` | `Nieminen (2021, p. 44)` |

A page is written bare, because it is what almost every citation carries and what the format
already meant. The other kinds name themselves. `!narrative` is the only form ever written —
the bracketed form is the default, and spelling it out would add five characters to nearly
every citation for nothing.

Each value has exactly one spelling. `@page:44` is deliberately **not** accepted: a second
spelling for the same value means a document cannot be opened and saved without its bytes
changing, and that guarantee is worth more than the convenience.

Both constructs are stored exactly as written and interpreted at read time. Malformed ones are
ordinary text — someone typing `[[` mid-sentence is writing prose, not a broken link, and a
locator kind nobody recognises stays visible rather than being guessed at as a page.

## Sources

| Call | Does |
| --- | --- |
| `source_create(source)` | Saves it. Returns its identifier **and** any required field it lacks |
| `source_read(id)` | Reads one back |
| `source_update(source)` | Replaces it. Returns what it still lacks |
| `source_list()` | Every source, already in reference-list order |
| `source_delete_plan(id)` | The documents that cite it — ask before deleting, never after |
| `source_delete(id)` | Removes it |
| `source_citing_documents(id)` | Who leans on this. The call you want when a study is retracted |
| `source_required_fields(type)` | What APA 7 needs for that kind of source |

A source short of a required field is **saved and reported**, never refused. Somebody who
cannot find a publisher must end up with an incomplete source rather than with none, and the
reference list shows the gap plainly rather than inventing something.

## Citations

| Call | Does |
| --- | --- |
| `citation_format(source, form, locator)` | One in-text citation |
| `citation_format_many(requests)` | Many at once. `None` where a source no longer exists |
| `citation_reference_list(document)` | The reference list for what a document actually cites |

Prefer `citation_format_many` for anything rendering a whole document. Each single call reads
the library and recomputes the disambiguation pass, so fifteen citations over a hundred sources
means fifteen hundred file reads; the batch reads once.

Disambiguation is computed over the **whole library**, never over one document's sources. Two
works by the same author in the same year are `2021a` and `2021b` everywhere they appear, which
is a fact about the library rather than about either citation — so formatting citations in
isolation would silently drop the suffixes.

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
| `backup_plan` | M2 — backup and restore |
| `work_create` | M3 — parallel texts |
| `map_read` | M4 — mind maps |

---

*Changing the engine rather than calling it? See the [technical documentation](../technical/).*
