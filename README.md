# personal-learning-corner-engine

The engine behind [Personal Learning Corner](https://github.com/Tuominen04/personal-learning-corner):
everything that touches a user's library. Reading and writing files, the derived index,
citation formatting, parallel-text alignment, and backup comparison.

It is usable with no interface at all. That is deliberate — it is what makes the rules about a
user's data testable, and what keeps the boundary in the engine contract honest.

## The specification lives in the other repository

This crate implements documents kept in `personal-learning-corner` under `docs/technical/`:

| Document | What it specifies |
| -------- | ----------------- |
| `backend-contract.md` | Every operation, its result, and its errors |
| `storage-format.md` | The on-disk library — layout, file shapes, identifiers |
| `search-index.md` | The derived index and why deleting it is safe |
| `citation-model.md` | APA 7: source types, in-text forms, reference ordering |
| `parallel-text-model.md` | Works, editions, segments, alignment |
| `backup-and-sync.md` | Comparison, conflict rules, the never-lose-data promise |

**Where this code and those documents disagree, the documents are normative and this is a
bug.**

## Building

Nothing to install beyond the Rust toolchain — SQLite is built from source by `rusqlite`'s
bundled feature, so there are no system libraries to hunt down.

`clippy` and `rustfmt` do ship separately from `cargo`, though, and CI runs both:

```sh
rustup component add clippy rustfmt   # or, on Fedora without rustup: sudo dnf install clippy rustfmt
```

Then:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

`cargo test` is the one that proves anything. The other two are style and lint gates that CI
will catch anyway, so a missing component is an annoyance rather than a blocker.

To see a library rather than read about one:

```sh
cargo run --example walkthrough
```

It builds the roadmap's M0 walkthrough in `/tmp/plc-walkthrough` and then does the thing worth
watching — deletes the index and rebuilds it, proving nothing observable changed. Afterwards,
open the folder in a text editor: that is the whole point of the storage format.

## Status

**M0 — foundations.** Libraries, topics, documents, the index, and the two properties the rest
of the design rests on:

- **Rebuild equivalence.** Deleting the index and rebuilding it changes nothing observable.
  Asserted in `tests/rebuild_equivalence.rs`.
- **Atomic writes.** A crash leaves the previous whole file or the new whole file, never a
  truncated one. Asserted in `src/atomic.rs`.

Sources and citations are M1, backup is M2, parallel texts M3, maps M4. Those operations exist
on the surface and return `Unimplemented`, so the interface can be written against the whole
contract now.

## Versioning

Released as immutable git tags, consumed by the interface as
`plc-engine = { git = "…", tag = "v0.1.0" }`. Both repositories are private, so no registry is
involved. See ADR 0006 in the other repository.

## Licence

Dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
