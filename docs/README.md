# Documentation

> **Audience:** Everyone
> **Status:** In progress
> **Last reviewed:** 2026-09-07

Split in two, the same way the main repository is: someone *using* this crate should never have
to read about how it is built, and someone *changing* it should not have to wade through a
tutorial to find the layering rules.

| Tree | Read it if you want to… |
| --- | --- |
| **[user/](user/)** | call the engine from a client |
| **[technical/](technical/)** | build, change, or test the engine itself |

## Where the specification lives

Neither tree is normative. **What the engine is required to do** is specified in the main
repository, under
[`docs/technical/`](https://github.com/Tuominen04/personal-learning-corner/tree/main/docs/technical):

| Document | Specifies |
| --- | --- |
| `backend-contract.md` | Every operation, its result, and its errors |
| `storage-format.md` | The on-disk library — layout, file shapes, identifiers |
| `search-index.md` | The derived index, and why deleting it is safe |
| `citation-model.md` | APA 7: source types, in-text forms, reference ordering |
| `parallel-text-model.md` | Works, editions, segments, alignment |
| `backup-and-sync.md` | Comparison, conflict rules, the never-lose-data promise |

**Where this crate and those documents disagree, the documents are right and this is a bug.**

The division is deliberate. The specification belongs with the product it describes, so a second
client would implement the same contract rather than copying this one's behaviour. What lives
here is everything true of *this implementation in particular* — its module layout, how to build
and test it, how to use its API — which would be noise in a product specification.

## User guide

| Page | Covers |
| --- | --- |
| [Getting started](user/getting-started.md) | Adding the dependency, opening a library, a first document |
| [Operations](user/operations.md) | Every call the engine offers, grouped by what it acts on |
| [Errors](user/errors.md) | What can fail, and what a caller should do about each |

## Technical documentation

| Page | Covers |
| --- | --- |
| [Architecture](technical/architecture.md) | The module layout and the layering rules that keep it honest |
| [Development](technical/development.md) | Toolchain, building, testing, linting, seeing a library on disk |
| [Testing](technical/testing.md) | The suites, the two properties that must never break, the fixtures |

## Conventions

Every page opens with a three-line header — `Audience`, `Status`, `Last reviewed` — where
`Status` is `Planned`, `In progress`, or `Implemented`. `docs/check-docs.sh` fails if a page is
missing one or if a relative link is dead.

When you change behaviour, update the `Status` line in the same commit. A pull request that
changes what the engine does without touching a `Status` line is incomplete.
