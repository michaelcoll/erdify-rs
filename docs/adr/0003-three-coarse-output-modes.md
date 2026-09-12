# Three coarse Output Modes instead of granular display flags

**Status**: accepted

## Context

A generated ER diagram can carry very different amounts of detail: columns and types, primary and foreign keys, `NOT NULL`, column defaults, relationship cardinalities, UNIQUE and CHECK constraints, indexes. Someone has to choose what appears.

## Decision

Detail is selected by one of three mutually exclusive Output Modes — `--minimal`, the default (no flag), and `--full` — not by per-feature flags such as `--show-indexes` or `--with-constraints`.

The modes correspond to how the diagram is actually read: _minimal_ for a slide or a README overview, _default_ for a working ER diagram, _full_ for auditing the real shape of the schema. Granular flags would offer a combinatorial surface where almost every combination is nonsense (relationships without foreign keys, defaults without columns), each one a rendering path to test, and would make "the default diagram" undefinable.

## Consequences

The set of three is a public CLI contract, so a new piece of metadata is assigned to an existing mode rather than given its own flag — column defaults went into `--full` for exactly that reason. Adding a fourth mode later stays possible; adding one granular flag would break the model.

The Output Mode must never reach the Schema Hash: it is presentation, and [ADR-0005](0005-erdify-lock-format-and-semantics.md) keeps the Canonical Form independent of it.
