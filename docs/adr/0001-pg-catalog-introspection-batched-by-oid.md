# Introspect `pg_catalog` directly, batched by OID

**Status**: accepted

## Context

The originating spec called for reading the schema from `information_schema` (`tables`, `columns`) with `pg_catalog` only where `information_schema` has no equivalent. `information_schema` is the standard, portable, documented surface — the obvious choice.

## Decision

Every Entity, Column, key, constraint and index is read from `pg_catalog` (`pg_class`, `pg_namespace`, `pg_attribute`, `pg_attrdef`, `pg_constraint`, `pg_index`). `information_schema` is not queried at all; it survives only as the name of a schema to exclude.

Two reasons decided it. First, the model needs things `information_schema` cannot express: `relkind` (which separates ordinary tables, partitioned tables, views and materialized views — see [ADR-0004](0004-entities-include-views-without-an-opt-in-flag.md)), `relispartition` (to drop partition children), index definitions, and `format_type`, which renders user-defined types and domains exactly as PostgreSQL displays them. That last one removed the spec's `"unknown"` type fallback entirely: there is no type `erdify` cannot name. Second, `information_schema` filters by the current user's privileges, so an under-privileged role silently yields a smaller diagram rather than an error — invisible truncation is the worst failure mode for a tool whose output is a contract.

The queries are **batched by OID**, and this is load-bearing. The Entity list is fetched first; columns, constraints and indexes are then each fetched in a single query over the whole OID set (`WHERE ... = ANY($1)`) and assembled in memory. Introspecting a 200-table schema costs four queries, not six hundred.

## Consequences

`erdify` is tied to PostgreSQL and to `pg_catalog`'s shape — it can never be a generic SQL tool without replacing this layer wholesale. That is accepted: the tool is PostgreSQL-specific by definition.

Adding a new piece of metadata means extending an existing batched query, never adding a per-Entity one. A per-Entity query would reintroduce the N+1 this decision exists to prevent.
