# Development

> **Audience:** People changing the engine
> **Status:** Implemented
> **Last reviewed:** 2026-09-07

## Toolchain

Rust 1.82 or newer. **`clippy` and `rustfmt` ship separately from `cargo`** and CI runs both, so
install them before your first push:

```sh
rustup component add clippy rustfmt
```

On Fedora without rustup:

```sh
sudo dnf install clippy rustfmt
```

Nothing else. SQLite is compiled from source by `rusqlite`'s bundled feature, so there are no
system libraries to hunt down and no version skew between machines.

## The loop

```sh
cargo test                                   # the one that proves anything
cargo clippy --all-targets -- -D warnings    # CI treats every warning as an error
cargo fmt --all --check                      # or `cargo fmt --all` to fix
```

A missing component is an annoyance rather than a blocker: `cargo test` needs neither, and CI
catches the other two on every push.

## Seeing a library rather than reading about one

```sh
cargo run --example walkthrough
```

Builds the roadmap's M0 walkthrough in `/tmp/plc-walkthrough`, then does the part worth watching:
deletes the index, rebuilds it, and proves nothing observable changed.

Then look at what it wrote, which is the entire point of the storage format:

```sh
find /tmp/plc-walkthrough -not -path '*/.plc/*'
cat /tmp/plc-walkthrough/topics/protein/what-protein-is.md
```

It refuses to run against a directory that already holds something, and takes a path if you want
one elsewhere:

```sh
cargo run --example walkthrough -- /tmp/somewhere-else
```

## Layout

```
crates/plc-engine/
├── src/
│   ├── error.rs        failure causes
│   ├── id.rs           ULIDs
│   ├── timestamp.rs    TOML datetimes
│   ├── atomic.rs       durable writes
│   ├── library.rs      library.toml and format versions
│   ├── model/          topics, documents, front matter
│   ├── store/          the folder tree, scanning, slugs
│   ├── index/          the SQLite cache
│   └── api/            Engine
├── tests/              contract, filesystem, rebuild equivalence
└── examples/           walkthrough
```

Unit tests sit beside the code they test; anything spanning layers goes in `tests/`. See
[Architecture](architecture.md) for what belongs where and [Testing](testing.md) for which suite.

## Versioning

Released as immutable git tags — both repositories are private, so no registry is involved. Tags
are never moved, deleted, or reused; a consumer bumping a version is a one-line diff naming the
old tag and the new one.

Semantic versioning is against the contract: a breaking contract change is a major bump, a new
operation a minor one, a fix that changes no documented behaviour a patch.

## Before you push

CI runs formatting, clippy with `-D warnings`, and the test suite, plus a guard asserting the
engine builds with no interface tooling present — the property the two-repository split exists to
protect. Run the three commands above first; one validated push beats three speculative ones.

## Related

- [Architecture](architecture.md)
- [Testing](testing.md)
