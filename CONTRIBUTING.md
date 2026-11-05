# Contributing

Thanks for wanting to help. Optimum is deliberately small, so please read this first.

## Before you start

- Read the principles in [README.md](README.md) and the decisions in [docs/DECISIONS.md](docs/DECISIONS.md). Features outside them (tabs, splits, extra shortcuts, non-visual settings, network access) won't be merged.
- For anything bigger than a bug fix, open an issue first so we can agree on the approach.
- Security problems go through private reporting, not issues: see [SECURITY.md](SECURITY.md).

## Building and testing

Everything is in [docs/BUILDING.md](docs/BUILDING.md). Before building for Windows, run `scripts/fetch-conpty.sh` once.

## What a pull request needs

- `cargo fmt --all --check`, clippy with `-D warnings` and the tests pass. CI checks this on Windows and macOS.
- New behaviour comes with tests in the same change; bug fixes come with a test that fails without the fix where that's possible.
- `cargo deny check` passes. New dependencies must be permissively licensed, come from crates.io and make no network calls.
- No comments in code, except a one-line `// SAFETY:` on every `unsafe` block.
- One concern per commit, messages in [Conventional Commits](https://www.conventionalcommits.org) form.
- Design changes are recorded as a new entry in docs/DECISIONS.md.

## Licence

By contributing you agree that your work is released under the project's [0BSD licence](LICENSE).
