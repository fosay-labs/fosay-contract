## What and why

Closes #

## How it was tested

```text
paste the real output of:
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

## Checklist

- [ ] One logical change; no unrelated reformatting
- [ ] Tests added or updated; none skipped or weakened
- [ ] Touches invariants I1–I7? If yes, which, and which test covers it:
- [ ] Docs and `CHANGELOG.md` (Unreleased) updated if behaviour changed
