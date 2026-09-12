# erdify

CLI tool that introspects a PostgreSQL database and generates Mermaid ER diagrams from it.

## Language

### Schema model

**Entity**:
Anything rendered as a block in the ER diagram: an ordinary table, a partitioned table, a view or a materialized view. The umbrella term — say Entity whenever the statement holds for all four.
_Avoid_: relation, object, table (when views are included)

**Entity Kind**:
Which of the four an Entity is. Carried on every Entity because the Mermaid block cannot express it, which is why a dedicated section lists views and materialized views alongside the diagram.
_Avoid_: relkind, type

**Table**:
The Entity Kind covering ordinary and partitioned tables — not views. Only tables carry keys, constraints and column defaults.
_Avoid_: base table

> The code still names the umbrella `Table` / `TableKind` (`src/schema.rs`), and the CLI flags `--table` / `--ignore-tables` accept view names. The flags are a public contract and stay; the code is drift to be renamed to Entity.

### Filtering

**Introspection Filter**:
One of the three criteria narrowing which Entities are introspected: `--schema`, `--table`, `--ignore-tables`. They select by name only — never by Entity Kind — and are recorded in the Lock File, because a Schema Hash only means something alongside the filters that produced it. System schemas are excluded in all cases.
_Avoid_: selector, scope

**Output Mode**:
How much detail the diagram carries: `minimal`, `default`, or `full`. Purely presentational — it never reaches the Schema Hash.
_Avoid_: verbosity, detail level, format

### Drift detection

**Schema Hash**:
A `sha256` digest of a database's schema, computed from its Canonical Form. Two runs against schemas with the same structure produce the same Schema Hash, regardless of rendering options.
_Avoid_: fingerprint, checksum

**Canonical Form**:
A deterministic text serialization of the introspected schema model (Entities, columns, keys, constraints, indexes), built solely to be hashed. It is independent from the Mermaid rendering and from Rust struct field names: changing how the diagram looks, or refactoring the code, must never change it.
_Avoid_: serialization, representation

**Lock File**:
The `erdify.lock` file written by `--lock`, holding the Schema Hash alongside the Introspection Filters used to produce it. Committed to the repository so other tooling can detect schema drift without connecting to the database.
_Avoid_: cache file, snapshot

**Incomparable**:
The outcome of `--check` when the existing Lock File cannot be meaningfully compared to the current run: its format `version` is unrecognized, or its stored Introspection Filters differ from the ones used for this run. Distinct from both "unchanged" and "moved": answering either would misrepresent what was actually verified.
_Avoid_: invalid, stale
