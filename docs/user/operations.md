# Operations

> **Audience:** People calling the engine from a client
> **Status:** In progress — M3 and pages (format 2) implemented
> **Last reviewed:** 2026-11-10

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
| `Engine::open(path)` | Opens an existing library for reading and writing; `MigrationRequired` for a format 1 library |
| `Engine::migrate(path)` | Converts a format 1 library to format 2, after copying it whole beside itself; returns where the copy is |
| `Library::open_read_only(path)` | Opens any library, including a newer format, without permitting writes |
| `status()` | How many pages and works, whether the library is writable, and any problems found |
| `rebuild_index()` | Discards the search index and rebuilds it from the files |

`rebuild_index` is always safe. The index holds nothing that is not derivable from a scan, which
is why a corrupt one is simply deleted rather than repaired.

### Converting a format 1 library

Format 1 had topics: folders holding `topic.toml` and no writing. `Engine::open` answers a
format 1 library with `MigrationRequired`, and `Engine::migrate` converts it — only ever when the
person has agreed. It:

1. reads every `topic.toml` first, and refuses untouched, naming the file, if one cannot be read;
2. copies the whole library to a folder beside it, `<folder>-format-1`, numbered if taken, the
   index left out;
3. makes every topic a page beside its folder, deepest first, keeping the topic's id, name and
   date, with an empty body. A topic with nothing in it becomes a page with no folder; a document
   already holding the name the page needs keeps it, and the page and its folder take the next;
4. writes `.order` at every level in the order the tree showed before: a topic's documents
   first, then its nested topics, each by file name;
5. sets `format_version` to 2 — last, so a conversion that stopped part way leaves a library
   that still says format 1 and is simply converted again.

Identity is the id inside each file, so no link, citation or back-reference changes.

## Pages

Every entry in the tree is a **page**: a document, which can have pages under it, as deep as you
like. A page's sub-pages live in a folder named like its file, beside it — `protein.md` and
`protein/` — and that folder exists only while it has some. The order of the pages under one
parent is the person's, kept in that folder's `.order` (ADR 0009 in the application's
repository). Format 1's topics are gone; see [Converting a format 1 library](#converting-a-format-1-library).

| Call | Does |
| --- | --- |
| `page_tree()` | Every page, as a tree, each level in its order |
| `document_create(parent, title)` | An empty page under `parent`, or at the top for `None`, at the end of its level |
| `document_read(id)` | The page, with the page it is under (`parent`) and the revision needed to write it |
| `document_write(id, body, revision)` | Replaces the body, or `Conflict` if the file changed |
| `document_rename(id, title)` | Changes the title, the filename and its folder's name; the identifier, body and place are untouched |
| `document_split(id, range, title, revision)` | Takes a byte range out into a new page **under** this one, leaving `[[id]]` behind |
| `document_move(id, parent, position)` | Moves it, with every page under it, to under `parent` at `position` (from 0) or the end. The same parent only reorders. Refuses to move a page under itself |
| `document_delete_plan(id)` | Every page anywhere under it |
| `document_delete(id, contents)` | `Delete` takes every page under it too; `MoveUp` moves the pages directly under it into its place first |
| `backlinks(id)` | Pages linking here, derived from `[[id]]` in their text |

**Deleting a page never decides what happens to the pages under it.** `contents` has no
default. `document_delete_plan` names every page anywhere under it, not only the first level,
because a page with one empty-looking sub-page can hold a year of writing two levels down.
`MoveUp` moves only what is *directly* under it, in its order, into the place it had: a
sub-page carries its own pages with it. A page at the top moves its pages up to the top.

**`.order` only ever orders what is there.** It lists file names without `.md`, one per line.
A page not on the list — made in a file manager — comes after the listed ones, by title; a name
with no page is skipped. So a stale list can misplace a page but never lose one, and a person
may edit it by hand. Absent, a level is in title order. Moving, creating, renaming and deleting
keep it up to date.

**A split is one operation because a crash in the middle of three would lose writing.** The
new document is written first, so the worst outcome of a failure afterwards is the text
existing in two places — visible, and fixable. The other order loses it.

`range` is a byte range into the body the caller last read, which is why `revision` is
required: offsets into a file that changed underneath address different text, and splitting on
them takes the wrong words out of somebody's document. A range landing inside a character is
refused rather than panicking — this is an application for people writing Finnish and Russian,
where multi-byte characters are the ordinary case.

A range landing inside a citation, a link or a picture is refused too. Cutting a character in
half panics; cutting one of these in half **succeeds**, and the file quietly keeps `3R4QFEM]`
where an identifier used to be. Every marker is checked, not only the ones a client renders —
a client renders what it understands, so the markers it does not understand are the ones its
user can put a selection boundary inside, which makes them the dangerous ones. A boundary
*touching* a marker is not cutting it and is allowed.

Citations and links inside the range move because they are *in* the text; there is no separate
record of them to update. The pictures the taken text shows go with it into the new page's
folder — copied rather than moved where the text left behind shows them too — so the bare
filenames both bodies carry still resolve.

**A document's file is named after its title, and follows it.** Identity is the ULID inside
the file, so the `.md` can be moved freely: no citation, link, or open document breaks. The
title is written first, at the name the file already has, so a failed move still leaves the
document saying what it was just called. Its folder of sub-pages is renamed with it, so the two
keep one name. Pictures stay where they are — they sit beside the page in the same folder, which
a rename does not change.

A name already in use gets the usual `-2` suffix, and a document renamed to the title it
already has keeps its filename rather than acquiring one.

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
| `source_delete(id)` | Removes it, and takes it off every work that cited it |
| `source_citing_documents(id)` | The documents that lean on this. The call you want when a study is retracted |
| `source_citing_works(id)` | The works that cite it, as the work's source or an edition's |
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

## Export

| Call | Does |
| --- | --- |
| `export_document(document, format)` | The document as a file's bytes: `Format::Text`, `Html`, `Docx`, `Odt` or `Pdf` |

Nothing is written anywhere — where the bytes go is the caller's choice. `Format::extension`
names the extension to save them with.

The writing reads as it does in the editor: a `# ` line is a heading, a single newline is a line
break, and `*a*` stays two asterisks and a letter, because the editor shows it that way. Links
read as the titles of the documents they lead to. A web page carries its pictures inside it;
plain text names them.

Word (`Docx`) and OpenDocument (`Odt`) are for somebody who goes on editing, so they are set in
the document's own named styles rather than formatting applied by hand: Title, Heading 1 to 6,
and a Bibliography style carrying APA's half-inch hanging indent. Change a style and everything
in it follows. Pages are A4 with inch margins, in Times New Roman at twelve points.

A PDF (`Pdf`) is for somebody who only reads, so it is laid out here, line by line and page by
page, and looks the same on every computer and on paper:

- Pages are A4 with inch margins, numbered at the top right as APA numbers them. The title, the
  headings and the reference list are set as in the Word file.
- It is set in DejaVu Sans, which goes inside the file, cut down to the letters the document
  uses. It covers Finnish and Swedish, the rest of the Latin alphabets, Greek and Cyrillic. A
  character it has no letter for — Chinese, an emoji — is drawn as an empty box, but copies out
  of the PDF as itself.
- There is one typeface and no italic or bold one, so italics are the letters slanted and bold
  is the letters drawn heavier, as a word processor does for such a font.
- Text is ragged right, as APA asks. A line breaks at a space. A web address too long for a
  line breaks where APA allows — after `//` or `&`, or before other punctuation — with no hyphen
  added.
- No paragraph leaves one line alone at the foot of a page or the head of the next, and a
  heading is always on the same page as the start of what follows it.
- The headings are the PDF's outline, which a PDF reader lists in its sidebar.

In Word, OpenDocument and PDF alike:

- PNG, JPEG and GIF pictures go inside the file, at 96 pixels to the inch and never wider than
  six inches or taller than eight. A WebP picture, which Word cannot show, and a picture that
  is not there are written as their description.
- The reference list's web addresses are links.
- The same document always makes the same bytes: every part of a package carries one fixed
  date, and a PDF carries no date at all.

Citations are written as APA 7 sets them, which the editor's separate chips do not need to:

- Parenthetical citations side by side become one pair of parentheses, in reference-list
  order: `(Kallio & Virtanen, 2019; Nieminen, 2021a)`.
- A citation is never run into the word before it or the word after it, whatever the text
  holds between them. Punctuation stays against it: `(Nieminen, 2021).`
- A group's short form is introduced the first time the document cites the group, and used
  alone after that: `(World Health Organization [WHO], 2007)`, then `(WHO, 2007)`.
- A source that has gone reads `[source missing]`, and a link to a document that has gone
  `[missing document]`, as in the editor.

The reference list follows the writing, disambiguated against the whole library.

## Search

| Call | Does |
| --- | --- |
| `search(query)` | Ranked hits across titles and bodies, with excerpts |

A person's words are quoted before they reach the index, so `protein (2021)` is a search rather
than a syntax error. Diacritics are folded: `aakkoset` finds `Ääkköset`.

A document is found by its title and its writing. A source is found by its title, by everyone
who made it — whole names, and the short name a group goes by, so `WHO` finds the World Health
Organization's report — and by where it appeared: the journal, book or podcast, the publisher,
the site.

## Backup

The library leads and the drive follows. Every call is two-phase: a `Plan` is what the person is
shown, and nothing is written until it is applied with their answers. The rules are the
specification's, in
[Backup and synchronisation](https://github.com/Tuominen04/learning-corner/blob/main/docs/technical/backup-and-sync.md).

| Call | Does |
| --- | --- |
| `backup_plan(destination)` | What backing up the open library to `destination` would do. The destination must be empty, or a copy of this library |
| `backup_apply(plan, overwrite)` | Makes the drive exactly like the library. `overwrite` is required when `plan.drive_changes()` is not zero |
| `Engine::restore_plan(drive, destination)` | What bringing `drive` back into `destination` would do — the library's own folder (a bring-back), or an empty one (a restore). Needs no open library |
| `Engine::restore_apply(plan, answers)` | Brings the drive's changes in, with an `Answer` for every entry whose `action` is `Ask` |
| `Engine::backup_texts(plan, path)` | Both versions of a file the plan asks about because it changed in both places, as text, to compare line by line. `None` when either is not text or is over 1 MiB; any other path is refused |

A plan lists only the paths that differ. Each `Entry` names its `outcome` (a row of the
comparison table), both sides' `digest`, `size` and `modified`, the `action` applying it takes,
and whether it is a `drive_change` — a change made on the drive itself, which is what a backup
warns about. `drive_changes()` and `asks()` count them.

```rust
let plan = engine.backup_plan(drive)?;
if plan.drive_changes() > 0 {
    // Show the person the changes on the drive: bring them back first, or overwrite them.
}
let done = engine.backup_apply(&plan, overwrite)?;
println!("{} copied, {} removed, {} kept aside", done.copied.len(), done.removed.len(), done.kept.len());
```

An `Answer` is `KeepLibrary`, `TakeDrive`, or `KeepBoth` — the last only where both sides have
the file, and saved beside the library's as `<name> (from backup YYYY-MM-DD).<ext>`.

Every version an apply replaces or removes in the library, and every change on the drive a
backup overwrites, is kept first under `.conflicts/<time>/` in the library; `Applied::kept` lists
where. A file whose digest is no longer what the plan saw stops the apply with `Conflict`; what
was done stays done, and a fresh plan shows what is left. Bringing back into the library that is
open means closing it first.

## Works

A text read beside its other versions: the original, how it is said, what it means. The rules
are the specification's, in
[Parallel text model](https://github.com/Tuominen04/learning-corner/blob/main/docs/technical/parallel-text-model.md).
Rows count from 1, in the original; an edition is named by its id. Every change takes the
`WorkRevision` from the last read and is refused with `Conflict` if any of the work's files
changed since.

| Call | Does |
| --- | --- |
| `work_list()` | Every work: title, its editions' languages, and whether it is out of line |
| `work_create(title, unit, text, language, source)` | A new work; its original divided by `unit` (`Line`, `Sentence`, `Paragraph`) |
| `work_read(id, editions)` | The work with its rows, notes, repeats and misalignments — all editions when `editions` is empty |
| `work_update(id, title, sources, revision)` | A new title, or the sources the work cites |
| `work_add_edition(id, edition, text, revision)` | A pronunciation or translation. Empty text gives it an empty row for every row |
| `work_update_edition(id, edition, language, label, scheme, source, revision)` | Its language, who it is by, how it is written. A translation given another language has its file renamed to match; its rows stay as they are |
| `work_move_edition(id, edition, position, revision)` | An edition to another place among the panes, counting from 1 |
| `work_remove_edition_plan(id, edition)` / `work_remove_edition(…)` | What goes, then removing it. Never the original |
| `work_write_row(id, edition, row, text, revision)` | One row of one edition. Line breaks become lines within the row; a repeated row writes all its repetitions |
| `work_insert_row(id, row, revision)` | An empty row at that place in every edition |
| `work_remove_row(id, edition, row, revision)` | The row out of every edition — refused while another edition has text there, or the row has a note |
| `work_mark_empty(…)` / `work_join(…)` / `work_split(…)` | One edition only: an empty row, a row joined to the one above, a row split at a line |
| `work_set_repeat(id, row, of, revision)` | The row is the same text as an earlier one; answers with how many rows share it |
| `work_set_note(id, row, text, revision)` | A note on a row; empty text removes it |
| `work_resegment_plan(id, unit)` / `work_resegment(plan)` | Dividing again: what changes, then doing it. Notes it cannot place are kept under `.conflicts/` |
| `work_delete_plan(id)` / `work_delete(id)` | What goes, then the whole folder |

```rust
let work = engine.work_create("Я вас любил", Unit::Line, poem, "ru", None)?;
let work = engine.work_add_edition(work.id, finnish, "", &work.revision)?;
let fi = work.editions[1].id;
let work = engine.work_write_row(work.id, fi, 1, "Teitä rakastin", &work.revision)?;
assert!(work.misalignments.is_empty());
```

An edition whose row count differs from the original's — after a join or split, or a hand edit
— is listed in `misalignments` and under the library's problems. Where the texts stop
corresponding is not something the engine can know, so it never shifts a line to make the
counts agree.

## Later milestones

These exist and return `Unimplemented`:

| Call | Arrives in |
| --- | --- |
| `map_read` | M4 — mind maps |

---

*Changing the engine rather than calling it? See the [technical documentation](../technical/).*
