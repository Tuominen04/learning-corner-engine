# Errors

> **Audience:** People calling the engine from a client
> **Status:** In progress — M1 implemented
> **Last reviewed:** 2026-10-02

Every operation returns `Result<T, Error>`, and every failure names one of a small closed set.
The set is deliberately short so a client can handle each rather than showing a string and hoping.

| Cause | Means | A client should |
| --- | --- | --- |
| `NotFound` | No entity with that identifier | Refresh — it may have been deleted outside the application |
| `LibraryNotOpen` | No library is open | Send the person to the library picker |
| `FormatTooNew` | Written by a newer version of the storage format | Offer it read-only, and suggest updating |
| `MigrationRequired` | Written by an older version | Offer to migrate |
| `Conflict` | The file changed since it was read | Show both, ask. **Never retry blindly** |
| `Incomplete` | Saved, but required fields are missing | Save it, and show what is missing |
| `Invalid` | Rejected. Names the field and the reason | Show it against that field |
| `LibraryError` | The library on disk is inconsistent | Show the diagnostic. Retrying will not help |
| `Io` | Disk full, permissions, a removed drive | Show it plainly. Never claim success |
| `Unimplemented` | A later milestone's operation | Hide or disable the control, not an error message |

## The three worth understanding properly

**A revision is a string, and must stay one on the wire.** It looks like a detail and is not.
A revision carries nanoseconds since the epoch — around 1.8e18 — and any client reaching the
engine across a JSON boundary parses numbers as IEEE-754 doubles, which cannot hold anything
above 2^53. Serialised as a number, a revision arrives back *different* from the one issued,
every conflict check fails, and no document can ever be saved. It is serialised as an opaque
string for exactly this reason; treat it as one and never as something to parse.

**`Conflict` is not a retry.** It means someone's work is on disk and yours is in memory, and the
engine refused to choose between them. Re-reading and writing again silently discards theirs,
which is exactly the outcome the check exists to prevent. Show both versions and let a person
decide.

**`Incomplete` is not a rejection.** The entity was saved. Someone who cannot find a book's
publisher should end up with an incomplete source, never with no source at all — so the engine
stores it, cites it, and reports the gap. Treating this as a failure loses the very thing it was
designed to keep.

**`Unimplemented` is not a bug.** The whole operation surface exists before all of it is built,
so clients can be written against the full contract. A control that would call one should be
hidden or disabled, not left to produce an error someone has to read.

## Diagnostics that are not errors

Some problems belong to the library rather than to a call, and appear in `status().problems`
instead of failing anything:

- A file whose front matter will not parse. The rest of the library still opens.
- Two files claiming the same identifier — usually a document copied in a file manager. The first
  wins and both are named.

These are reported rather than raised because the library is usable and refusing to open it over
one bad file would turn a copied document into a lockout. Show them somewhere a person will see
them.

---

*Changing the engine rather than calling it? See the [technical documentation](../technical/).*
