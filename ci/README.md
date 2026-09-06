# CI configuration

`github-workflow.yml` belongs at `.github/workflows/ci.yml`. It is parked here because the
credential this branch was pushed with has no `workflow` scope, and GitHub refuses pushes that
create or change anything under `.github/workflows/`.

To activate it:

```sh
git mv ci/github-workflow.yml .github/workflows/ci.yml
rmdir ci 2>/dev/null || true
git commit -m "Activate CI workflow"
```

It runs `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test`, plus a guard asserting the engine needs no interface tooling to build — the
property the two-repository split exists to protect.
