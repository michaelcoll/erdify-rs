# Architecture

Key points and invariants of `erdify`. Details live in the code — this document exists so you know where to look and what must not be broken.

## Shape

A single binary crate, flat `src/`, no workspace. `main.rs` is a thin entry point: it parses args, calls `erdify_rs::run`, and maps the result to a process exit code. Everything else lives in `lib.rs` and its modules, so it is reachable from tests.

Async throughout (`tokio`), driven by `tokio-postgres`. There is no connection pool and no concurrency: one client, queries issued in sequence.

## Pipeline

`run()` in `lib.rs` is the only orchestrator. One linear flow, no branching strategies:

```
Args ──► ConnectionInfo ──► introspection ──► Vec<Table> ──┬─► Mermaid rendering ──► stdout | file
       (config.rs)          (db.rs)          (schema.rs)   ├─► Canonical Form ──► hash ──► erdify.lock   (--lock)
                                                           └─► Canonical Form ──► hash ──► compare       (--check)
```

`--lock` and `--check` are mutually exclusive; `--check` returns before any rendering happens.

## Modules

| Module       | Responsibility                                                                        |
| ------------ | ------------------------------------------------------------------------------------- |
| `config.rs`  | `clap` argument surface, `DATABASE_URL` fallback, PostgreSQL URL parsing, output mode |
| `db.rs`      | Connection and `pg_catalog` introspection; raw catalog rows assembled into the model  |
| `schema.rs`  | The domain model (`Table`, `Column`, keys, constraints, indexes) and filtering        |
| `mermaid.rs` | Rendering the model as a Markdown document containing a `mermaid` ER diagram          |
| `lock.rs`    | Canonical Form, Schema Hash, `erdify.lock` read/write, `--check` comparison           |
| `outcome.rs` | Successful-run outcomes and their exit codes                                          |
| `errors.rs`  | `ErdifyError`, the single error type crossing module boundaries                       |

`schema.rs` is the hinge: `db.rs` produces the model, `mermaid.rs` and `lock.rs` each consume it independently and never talk to each other.

## Invariants

**Determinism.** The same schema must always produce the same output and the same hash. Introspection returns tables ordered by `(schema, name)`; column and constraint order comes from the catalog and carries meaning. Nothing derived from a `HashSet` iteration may reach the output or the hash.

**The Canonical Form is a separate serialization.** It is built solely to be hashed, and is deliberately decoupled from both the Mermaid rendering and the Rust struct field names. Changing how the diagram looks, or renaming a field, must never change a Schema Hash. Any change to the Canonical Form itself requires bumping `LOCK_FORMAT_VERSION`. See [ADR-0005](docs/adr/0005-erdify-lock-format-and-semantics.md).

**`--check` never guesses.** Comparability is verified before equality: an unknown format version or filters differing from the current run yield _Incomparable_, never _changed_. A hash computed under different filters describes a different subset of the schema.

**Exit codes are a public contract.** `0` success, `1` error, `2` schema changed, `3` lock file missing, `4` incomparable. They are consumed by CI, so treat them as API.

**`--lock` is idempotent.** If the computed hash already matches what is on disk, the file is left untouched — mtime included — so build tools keying off it see no spurious change.

**Catalog queries are batched by OID.** The table list is fetched first; columns, constraints and indexes are then fetched in one query each for the whole OID set. Do not introduce per-table queries.

## Testing

Unit tests sit in `#[cfg(test)]` modules next to the code. Tests in `db.rs` and `lib.rs` are integration tests hitting a real PostgreSQL instance — `mise run test` starts the `erdify-test-db` container for them. Each such test creates and drops its own schema, so test schema names must stay unique.

See [mise.instructions.md](.agents/mise.instructions.md) for the commands.

## Vocabulary

Entity, Entity Kind, Table, Introspection Filter, Output Mode, Schema Hash, Canonical Form, Lock File, Incomparable are defined in [CONTEXT.md](CONTEXT.md). Use those terms; avoid the synonyms it lists.

Note the known drift: the model type is named `Table` but covers views and materialized views too — the glossary calls that an Entity.
