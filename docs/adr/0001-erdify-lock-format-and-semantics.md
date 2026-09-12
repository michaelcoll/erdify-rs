# erdify.lock format and semantics

**Status**: accepted

## Context

Other tooling in a project needs a cheap way to know whether the database schema has drifted, without connecting to the database or re-running `erdify` in full. `--lock` writes a committed `erdify.lock` file carrying a Schema Hash; `--check` reads it back to answer "has the schema moved?".

## Decision

The Schema Hash is computed from a dedicated **Canonical Form** — a deterministic text serialization of the introspected schema model — not from the rendered Mermaid output and not from a `serde` dump of the internal Rust structs. Hashing the rendered diagram would couple the hash to presentation (`--minimal`/`--full`/`--title` would all change it); hashing a `serde` serialization would couple it to internal field names, breaking every lock on an unrelated refactor.

The hash is **conservative**, not semantic: column order, constraint and index names, `DEFAULT` expressions, and raw `CHECK` definitions all contribute, even though none of them changes the schema's meaning on their own. A false positive (hash changes, meaning didn't) only costs a consumer an unnecessary rebuild. A false negative (hash unchanged, meaning did change) serves a stale cache silently — for schema-drift detection, the false negative is the failure mode to avoid, so the hash errs conservative.

The introspection filters (`schemas`, `tables`, `ignore_tables`) used to produce the hash are stored in the lock file alongside it. Two hashes computed under different filters describe different subsets of the schema and are not meaningfully comparable; storing the filters lets `--check` detect this case (**incomparable**) instead of reporting a false "moved" or "unchanged".

The file carries no volatile field: no timestamp, no `erdify` version, no database name, no host. It is meant to be committed, and a field that changes on every run regardless of schema content would make every commit noisy. A `version` field tracks the format and the canonicalization algorithm instead; bumping it when either changes lets old locks be recognized as incomparable rather than silently misread.

## Considered alternatives

- **Hash the rendered Mermaid output.** Rejected: couples the hash to presentation options that have nothing to do with the schema's shape.
- **Hash a `serde` serialization of the schema model.** Rejected: couples the hash to internal Rust field names, breaking every lock on an unrelated refactor.
- **Semantic hash (ignore column order, constraint/index names).** Rejected: optimizes for a smaller diff at the cost of risking a false negative, which is the failure mode this file exists to avoid.
- **Store the `erdify` binary version in the lock file.** Rejected: violates the zero-volatile-field goal — every release would dirty every committed lock file with no schema change behind it.
