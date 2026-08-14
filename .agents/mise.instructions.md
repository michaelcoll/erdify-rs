# Mise & Development Workflow

## Quick Reference

All local commands are run through **mise** (task runner):

```
mise install           # One-time: install tools
mise run setup         # One-time: fetch cargo dependencies
```

## Command Summary

| Action              | Command            | Alias         |
| ------------------- | ------------------ | ------------- |
| **All checks**      | `mise run checks`  | —             |
| **Tests**           | `mise run test`    | —             |
| **Lint**            | `mise run lint`    | —             |
| **Format code**     | `mise run format`  | `mise run f`  |
| **Build**           | `mise run build`   | `mise run bb` |
| **Run CLI**         | `mise run run`     | —             |
| **Clean artifacts** | `mise run clean`   | —             |
| **Upgrade deps**    | `mise run upgrade` | —             |

## Detailed Commands

### Build

- **All**: `mise run build` (= `mise run bb`), i.e. `cargo build`

### Test

- `mise run test`, i.e. `cargo nextest run --status-level slow`

### Lint

- `mise run lint` → `cargo clippy --locked --workspace --all-features --all-targets -- -A dead_code -D clippy::all`

### Format

- `mise run format` (= `mise run f`) → `cargo fmt`
- Always use `mise run format`, never call `cargo fmt` directly

### Clean & Setup

```
mise run clean          # cargo clean
mise run setup          # cargo fetch
```

### Upgrade

```
mise run upgrade        # cargo update
```
