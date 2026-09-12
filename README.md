# erdify-rs

[![codecov](https://codecov.io/gh/michaelcoll/erdify-rs/graph/badge.svg?token=oHYBQAJWwU)](https://codecov.io/gh/michaelcoll/erdify-rs)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

CLI tool to generate Mermaid ER (Entity-Relationship) diagrams from PostgreSQL databases.

## Install

Download a prebuilt binary from the [releases page](https://github.com/michaelcoll/erdify-rs/releases) (Windows x86_64, macOS arm64, Linux x86_64), or build from source:

```bash
mise install
mise run setup
mise run build
```

## Usage

```bash
erdify --url postgresql://user:pass@host:5432/dbname
```

If `--url` is omitted, erdify falls back to the `DATABASE_URL` environment variable. Output goes to stdout unless `--output` is set.

### Options

| Flag              | Description                                                               |
| ----------------- | ------------------------------------------------------------------------- |
| `-u, --url`       | PostgreSQL connection URL (or set `DATABASE_URL`)                         |
| `--schema`        | Only include these schemas (comma-separated)                              |
| `--table`         | Only include these tables (comma-separated)                               |
| `--ignore-tables` | Exclude these tables (comma-separated), mutually exclusive with `--table` |
| `--minimal`       | Columns only, no PK/FK                                                    |
| `--full`          | Columns + PK/FK/NOT NULL + relations + constraints + indexes              |
| `-o, --output`    | Write to file instead of stdout                                           |
| `--title`         | Diagram title (default: database name)                                    |
| `--lock [path]`   | Write the schema hash to a lock file (default: `./erdify.lock`)           |
| `--check [path]`  | Verify the schema against a lock file, without writing anything           |

### Examples

```bash
# Only the public schema, full detail
erdify --url postgresql://user:pass@host/db --schema public --full

# Everything except audit tables
erdify --url postgresql://user:pass@host/db --ignore-tables logs,audit_trail

# Write to a file
erdify --url postgresql://user:pass@host/db --output erdify.md

# Write the schema hash to ./erdify.lock
erdify --url postgresql://user:pass@host/db --lock

# Fail if the schema no longer matches erdify.lock (e.g. in CI)
erdify --url postgresql://user:pass@host/db --check
```

## Schema lock file

`--lock` introspects the database and writes a `sha256` hash of the schema to a lock file (`erdify.lock` by default, committed to the repository). Other tooling can read this file to detect schema drift without connecting to the database. The hash covers the introspected schema itself — tables, views, columns, keys, constraints, indexes — never the rendered diagram: `--minimal`, `--full`, `--title` and `--output` don't affect it.

The lock file also records the introspection filters (`--schema`, `--table`, `--ignore-tables`) used to produce the hash, since a hash computed under different filters describes a different subset of the schema and isn't comparable to one computed without them.

```toml
version = 1
hash = "sha256:9f2b…"
schemas = ["public"]
tables = []
ignore_tables = ["audit_trail"]
```

Writing is idempotent: if the computed hash matches the one already on disk, the file is left untouched, so build tools that key off its mtime don't see spurious changes. No lock file is written when no relation matches the filters.

`--check` reads the lock file back and compares it against a fresh introspection, without writing anything or printing a diagram. It's mutually exclusive with `--lock`. The exit code reports the outcome:

| Code | Meaning                                                                           |
| ---- | --------------------------------------------------------------------------------- |
| `0`  | Schema unchanged                                                                  |
| `1`  | Technical error (invalid URL, connection failure, no tables found)                |
| `2`  | Schema has changed                                                                |
| `3`  | No lock file found at the given path                                              |
| `4`  | Incomparable: unknown lock format version, or filters differ from the current run |
