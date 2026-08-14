# Contributing to erdify-rs

Thanks for considering a contribution! This document covers how to set up the project, the expected workflow, and the conventions used in this repository.

## Prerequisites

This project uses [mise](https://mise.jdx.dev/) to manage tools and tasks. Install it first, then:

```bash
mise install       # installs Rust, cargo-nextest, cargo-llvm-cov, lefthook, dprint
mise run setup      # fetches cargo dependencies
```

`mise install` also registers [lefthook](https://github.com/evilmartians/lefthook) git hooks (`pre-commit` runs formatting, `pre-push` runs the full check suite).

Some tests require a running PostgreSQL instance, reachable via the `DATABASE_URL` environment variable (defaulted in `mise.toml` to `postgresql://postgres:password@localhost:5432/postgres`).

## Development workflow

All commands go through mise tasks — don't call the underlying tools directly, so behavior stays consistent with CI.

| Action          | Command            | Alias        |
| --------------- | ------------------ | ------------ |
| All checks      | `mise run checks`  | —            |
| Tests           | `mise run test`    | —            |
| Lint            | `mise run lint`    | —            |
| Format code     | `mise run format`  | `mise run f` |
| Build           | `mise run build`   | `mise run b` |
| Run the CLI     | `mise run run`     | —            |
| Clean artifacts | `mise run clean`   | —            |
| Upgrade deps    | `mise run upgrade` | —            |

Before opening a pull request, run:

```bash
mise run checks
```

This runs tests, clippy, and formatting checks — the same gate as CI (`.github/workflows/lint-test.yml`).

## Code style

- Formatting is enforced by `cargo fmt` and `dprint fmt` (Markdown/TOML/YAML/JSON) — run `mise run format`, never format manually.
- Linting is `cargo clippy --workspace --all-features --all-targets -D clippy::all`; keep the build clippy-clean.
- This project follows the Rust guidelines documented in `.agents/skills/rust-skills/` (ownership, error handling, API design, testing, etc.). When in doubt, favor `Result` over panics, explicit error types (`thiserror`), and small, focused modules.
- Comments should explain _why_, not _what_ — avoid restating what the code already makes obvious.
- Source code and comments are written in English. Feature specifications under `doc/specs/` are written in French.

## Tests

- Unit tests live next to the code they cover (`#[cfg(test)] mod tests`).
- Run them with `mise run test` (`cargo nextest`).
- Add tests for new behavior and for bug fixes (a regression test that reproduces the bug before fixing it is ideal).
- Coverage is tracked via Codecov; the CI test job uploads `lcov.info`.

## Commit messages

This repository follows [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>
```

Common types: `feat`, `fix`, `chore`, `refactor`, `test`, `docs`, `ci`. Examples from the history: `feat(erdify): add all features`, `chore(deps): bump softprops/action-gh-release`.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Make your changes, keeping commits focused and following the conventions above.
3. Ensure `mise run checks` passes locally.
4. Open a pull request describing the change and its motivation. Link any related issue.
5. CI (lint, tests, coverage) must pass before merge.

## Reporting issues

Use the [GitHub issue tracker](https://github.com/michaelcoll/erdify-rs/issues). Include the erdify version, the command used, and the expected vs. actual behavior. Never paste real database credentials or production data into an issue.

## License

By contributing, you agree that your contributions will be licensed under the project's [MIT license](LICENSE).
