# Contributing

Thank you for looking. Learning Corner is a one-person project, and reports, ideas and fixes from
other people make it better.

## Where things go

| You want to… | Go to |
| --- | --- |
| report a problem you found **in the app** | [app issues](https://github.com/Tuominen04/learning-corner/issues/new/choose) — it is moved here if the cause is in the engine |
| suggest a feature, ask a question, give feedback | [app discussions](https://github.com/Tuominen04/learning-corner/discussions) |
| report a bug you can show **in the engine itself** | [engine issues](https://github.com/Tuominen04/learning-corner-engine/issues/new/choose) |
| report a security problem | privately — see [SECURITY.md](SECURITY.md) |

## Pull requests

Small fixes — a typo, a broken link, a clearer error message, a missing test — are welcome
straight away as a pull request.

For anything bigger, **open an issue first** and wait for a reply before writing the code. The
engine implements a specification that lives in the app repository, and a change to what the
engine does usually starts there. Agreeing on the change first means your time is never spent on
something that cannot be merged.

Before you push, run what CI runs:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

[Development](docs/technical/development.md) covers the layout, and
[Testing](docs/technical/testing.md) which suite a test belongs in.

## Who merges and releases

The maintainer reviews and merges every pull request, and is the only one who creates release
tags. `main` only changes through a pull request with passing checks, and a published tag never
moves.

## Licence of contributions

The project is dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE). Unless you
say otherwise, anything you contribute is offered under the same terms — there is nothing extra
to sign.
