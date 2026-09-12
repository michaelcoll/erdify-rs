# Views and materialized views are Entities, with no flag to opt in or out

**Status**: accepted

## Context

The first implementation introspected `relkind IN ('r', 'p')` — ordinary and partitioned tables only. Views and materialized views were absent from the diagram even though they are part of the application schema the user reads and writes through. Most ERD generators either skip views or hide them behind a `--include-views` flag.

## Decision

Introspection covers `relkind IN ('r', 'p', 'v', 'm')`, and each Entity carries its Entity Kind. Views and materialized views are Entities like any other: they appear with no flag to ask for them, and no flag exists to remove them as a class. `--schema`, `--table` and `--ignore-tables` apply to them exactly as to tables, so `--ignore-tables` is the only way to exclude one — by name.

A view is part of the schema's contract; a diagram that omits it is describing a database the application does not see. Defaulting to "on" and dropping the opt-out keeps the Introspection Filters a single, name-based mechanism rather than two competing ones (an Entity Kind filter and a name filter) whose interaction would have to be defined, recorded in the Lock File, and compared.

Because the Mermaid entity block cannot express the distinction, a dedicated Markdown section after the diagram lists which Entities are views and which are materialized views. It appears in every Output Mode — the distinction is structural, not a detail level — and only when at least one such Entity survives filtering.

## Consequences

`--table` and `--ignore-tables` are misnomers: both accept view names. Renaming them would break the CLI contract for no functional gain, so the names stay and the glossary carries the correction (see `CONTEXT.md`).

Everything else falls out of the catalog with no special-casing: PostgreSQL forbids PK/FK/UNIQUE/CHECK on views and materialized views, so no relationship is ever drawn to or from one; materialized views can carry indexes, so the `UK` marker keeps working for them; and a column default is never populated for them, since only tables have one.
