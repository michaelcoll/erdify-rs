# erdify

CLI tool that introspects a PostgreSQL database and generates Mermaid ER diagrams from it.

## Language

**Schema Hash**:
A `sha256` digest of a database's schema, computed from its Canonical Form. Two runs against schemas with the same structure produce the same Schema Hash, regardless of rendering options.
_Avoid_: fingerprint, checksum

**Canonical Form**:
A deterministic text serialization of the introspected schema model (tables, views, columns, keys, constraints, indexes), built solely to be hashed. It is independent from the Mermaid rendering and from Rust struct field names: changing how the diagram looks, or refactoring the code, must never change it.
_Avoid_: serialization, representation

**Lock File**:
The `erdify.lock` file written by `--lock`, holding the Schema Hash alongside the introspection filters used to produce it. Committed to the repository so other tooling can detect schema drift without connecting to the database.
_Avoid_: cache file, snapshot

**Incomparable**:
The outcome of `--check` when the existing Lock File cannot be meaningfully compared to the current run: its format `version` is unrecognized, or its stored filters differ from the ones used for this run. Distinct from both "unchanged" and "moved": answering either would misrepresent what was actually verified.
_Avoid_: invalid, stale
