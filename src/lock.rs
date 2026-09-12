//! `erdify.lock`: canonical schema serialization, hashing, and file I/O.
//!
//! See `docs/adr/0001-erdify-lock-format-and-semantics.md` and `CONTEXT.md`
//! for why the Canonical Form is a dedicated serialization rather than the
//! Mermaid rendering or a `serde` dump, and why the hash is conservative.

use crate::errors::ErdifyError;
use crate::schema::{Table, TableKind};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::Path;

/// Format version of the lock file and its canonicalization algorithm.
///
/// Bump this whenever the canonical form below changes, so an old lock
/// file is recognized as incomparable instead of silently misread.
pub const LOCK_FORMAT_VERSION: u32 = 1;

/// Default path written by `--lock` with no value.
pub const DEFAULT_LOCK_PATH: &str = "erdify.lock";

/// Builds the Canonical Form of the introspected schema: a deterministic
/// text serialization built solely to be hashed, independent from the
/// Mermaid rendering and from these Rust structs' field names.
///
/// Tables and their columns are emitted in the order they're given: that
/// order carries schema meaning (column order contributes to the hash) and
/// introspection already returns tables ordered by schema then name, so
/// nothing is re-sorted here. `NOT NULL` is looked up per column rather
/// than iterated from `not_null_cols`, since a `HashSet`'s iteration order
/// isn't stable across runs.
#[must_use]
pub fn canonical_form(tables: &[Table]) -> String {
    let mut out = String::new();

    for table in tables {
        let kind = match table.kind {
            TableKind::Table => "TABLE",
            TableKind::View => "VIEW",
            TableKind::MaterializedView => "MATERIALIZED VIEW",
        };
        let _ = writeln!(out, "RELATION {kind} {}.{}", table.schema, table.name);

        for column in &table.columns {
            let not_null = table.not_null_cols.contains(&column.name);
            let default = column.default.as_deref().unwrap_or("-");
            let _ = writeln!(
                out,
                "  COLUMN {} {} NOT_NULL={not_null} DEFAULT={default}",
                column.name, column.data_type
            );
        }

        if !table.primary_keys.is_empty() {
            let _ = writeln!(out, "  PRIMARY KEY ({})", table.primary_keys.join(","));
        }

        for fk in &table.foreign_keys {
            let _ = writeln!(
                out,
                "  FOREIGN KEY {} ({}) -> {}.{}({})",
                fk.name,
                fk.from_columns.join(","),
                fk.to_schema,
                fk.to_table,
                fk.to_columns.join(",")
            );
        }

        for unique in &table.unique_constraints {
            let _ = writeln!(
                out,
                "  UNIQUE {} ({})",
                unique.name,
                unique.columns.join(",")
            );
        }

        for check in &table.check_constraints {
            let _ = writeln!(out, "  CHECK {} {}", check.name, check.definition);
        }

        for index in &table.indexes {
            let _ = writeln!(
                out,
                "  INDEX {} UNIQUE={} ({})",
                index.name,
                index.is_unique,
                index.columns.join(",")
            );
        }
    }

    out
}

/// Hashes the Canonical Form of `tables` with `sha256`, returning it in the
/// `sha256:<lowercase hex>` form stored in the lock file.
#[must_use]
pub fn schema_hash(tables: &[Table]) -> String {
    let digest = Sha256::digest(canonical_form(tables).as_bytes());
    let hex = digest.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!("sha256:{hex}")
}

/// Contents of an `erdify.lock` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockFile {
    pub version: u32,
    pub hash: String,
    pub schemas: Vec<String>,
    pub tables: Vec<String>,
    pub ignore_tables: Vec<String>,
}

impl LockFile {
    /// Builds a lock file for the current format version from a computed
    /// hash and the introspection filters used to produce it.
    #[must_use]
    pub fn new(hash: String, schemas: &[&str], tables: &[&str], ignore_tables: &[&str]) -> Self {
        Self {
            version: LOCK_FORMAT_VERSION,
            hash,
            schemas: schemas.iter().map(|&s| s.to_owned()).collect(),
            tables: tables.iter().map(|&s| s.to_owned()).collect(),
            ignore_tables: ignore_tables.iter().map(|&s| s.to_owned()).collect(),
        }
    }

    /// Serializes to the TOML layout documented in the README.
    #[must_use]
    pub fn to_toml(&self) -> String {
        format!(
            "version = {}\nhash = \"{}\"\nschemas = {}\ntables = {}\nignore_tables = {}\n",
            self.version,
            self.hash,
            toml_string_array(&self.schemas),
            toml_string_array(&self.tables),
            toml_string_array(&self.ignore_tables),
        )
    }
}

/// Formats a list of plain identifiers as a TOML array of strings.
fn toml_string_array(values: &[String]) -> String {
    let quoted: Vec<String> = values.iter().map(|v| toml_string(v)).collect();
    format!("[{}]", quoted.join(", "))
}

/// Escapes a value as a TOML basic string: backslashes and double quotes
/// are backslash-escaped, everything else is passed through as-is
/// (`schemas`/`tables`/`ignore_tables` only ever hold `PostgreSQL`
/// identifiers, never control characters).
fn toml_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Reads back just the `hash` field of an existing lock file, if the file
/// exists and has one. Used only to decide whether a rewrite is needed;
/// comparing the full contents against a fresh run is `--check`'s job.
async fn read_existing_hash(path: &Path) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    content.lines().find_map(|line| {
        let rest = line.strip_prefix("hash")?.trim_start();
        let rest = rest.strip_prefix('=')?.trim();
        rest.strip_prefix('"')?.strip_suffix('"').map(str::to_owned)
    })
}

/// Parses the TOML layout written by [`LockFile::to_toml`].
///
/// Returns `None` if any expected field is missing or malformed — a
/// deliberately loose notion of "corrupt", since a hand-edited or
/// truncated lock file is exactly the case `--check` must recognize as
/// incomparable rather than crash on.
#[must_use]
fn parse_toml(content: &str) -> Option<LockFile> {
    let mut version = None;
    let mut hash = None;
    let mut schemas = None;
    let mut tables = None;
    let mut ignore_tables = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("version") {
            version = rest
                .trim_start()
                .strip_prefix('=')?
                .trim()
                .parse::<u32>()
                .ok();
        } else if let Some(rest) = line.strip_prefix("hash") {
            hash = parse_toml_string(rest.trim_start().strip_prefix('=')?.trim());
        } else if let Some(rest) = line.strip_prefix("ignore_tables") {
            ignore_tables = parse_toml_string_array(rest.trim_start().strip_prefix('=')?.trim());
        } else if let Some(rest) = line.strip_prefix("tables") {
            tables = parse_toml_string_array(rest.trim_start().strip_prefix('=')?.trim());
        } else if let Some(rest) = line.strip_prefix("schemas") {
            schemas = parse_toml_string_array(rest.trim_start().strip_prefix('=')?.trim());
        }
    }

    Some(LockFile {
        version: version?,
        hash: hash?,
        schemas: schemas?,
        tables: tables?,
        ignore_tables: ignore_tables?,
    })
}

/// Parses a TOML basic string (`"..."`), reversing [`toml_string`].
fn parse_toml_string(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(chars.next()?);
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Parses a TOML array of basic strings (`["a", "b"]`), reversing
/// [`toml_string_array`].
fn parse_toml_string_array(value: &str) -> Option<Vec<String>> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }

    let mut values = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == '"' {
            chars.next();
            let mut buf = String::new();
            loop {
                let c = chars.next()?;
                if c == '\\' {
                    buf.push(chars.next()?);
                } else if c == '"' {
                    break;
                } else {
                    buf.push(c);
                }
            }
            values.push(buf);
        } else if c == ',' || c.is_whitespace() {
            chars.next();
        } else {
            return None;
        }
    }
    Some(values)
}

/// Outcome of looking up a lock file on disk, ahead of comparing it against
/// a freshly introspected schema.
pub enum LockLookup {
    /// No file (or an unreadable one) at the given path.
    Missing,
    /// A file exists but doesn't parse as the documented layout.
    Corrupt,
    /// A file exists and parses.
    Found(LockFile),
}

/// Reads and parses the lock file at `path` for `--check`.
///
/// Any I/O error (missing file, permission denied, ...) is reported as
/// [`LockLookup::Missing`]: `--check` only needs to distinguish "nothing to
/// compare against" from "found something", not diagnose the I/O failure.
pub async fn read_lock_file(path: &Path) -> LockLookup {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => parse_toml(&content).map_or(LockLookup::Corrupt, LockLookup::Found),
        Err(_) => LockLookup::Missing,
    }
}

/// Result of comparing a looked-up lock file against a freshly introspected
/// schema, with the actionable detail `--check` prints for a non-zero exit.
pub enum CheckResult {
    /// The introspected schema still hashes to the value stored in the lock.
    Unchanged,
    /// The introspected schema hashes differently: the schema moved.
    Changed,
    /// No lock file was found at the given path.
    Missing,
    /// The lock and the current run can't be compared; carries why.
    Incomparable(String),
}

/// Compares a [`LockLookup`] against the hash and filters of the current
/// run.
///
/// Comparability is checked before equality: an unknown format version or
/// filters that differ from the current run yield [`CheckResult::Incomparable`],
/// never [`CheckResult::Changed`] — a hash computed under different filters
/// describes a different subset of the schema, so it isn't meaningfully
/// comparable at all.
#[must_use]
pub fn check(
    lookup: &LockLookup,
    computed_hash: &str,
    schemas: &[&str],
    tables: &[&str],
    ignore_tables: &[&str],
) -> CheckResult {
    let lock = match lookup {
        LockLookup::Missing => return CheckResult::Missing,
        LockLookup::Corrupt => {
            return CheckResult::Incomparable(
                "the lock file is corrupt or not in the expected format".to_string(),
            );
        }
        LockLookup::Found(lock) => lock,
    };

    if lock.version != LOCK_FORMAT_VERSION {
        return CheckResult::Incomparable(format!(
            "the lock file's format version ({}) is unknown to this build (expected {LOCK_FORMAT_VERSION})",
            lock.version
        ));
    }

    let filters_match = str_eq(&lock.schemas, schemas)
        && str_eq(&lock.tables, tables)
        && str_eq(&lock.ignore_tables, ignore_tables);
    if !filters_match {
        return CheckResult::Incomparable(
            "the lock file's --schema/--table/--ignore-tables filters differ from this run's"
                .to_string(),
        );
    }

    if lock.hash == computed_hash {
        CheckResult::Unchanged
    } else {
        CheckResult::Changed
    }
}

/// Compares a `Vec<String>` against a `&[&str]` for equality, element by
/// element and in order.
fn str_eq(stored: &[String], current: &[&str]) -> bool {
    stored.len() == current.len() && stored.iter().zip(current).all(|(s, c)| s == c)
}

/// Writes the lock file at `path` from the introspected `tables` and the
/// filters used to obtain them.
///
/// Idempotent: if the computed hash already matches the one on disk, the
/// file is left untouched (its mtime included), so build tools keying off
/// it don't see spurious changes.
///
/// # Errors
///
/// Returns [`ErdifyError::IoError`] if the file can't be written.
pub async fn write_lock_file(
    path: &Path,
    tables: &[Table],
    schemas: &[&str],
    table_filters: &[&str],
    ignore_tables: &[&str],
) -> Result<(), ErdifyError> {
    let hash = schema_hash(tables);

    if read_existing_hash(path).await.as_deref() == Some(hash.as_str()) {
        return Ok(());
    }

    let lock = LockFile::new(hash, schemas, table_filters, ignore_tables);
    tokio::fs::write(path, lock.to_toml()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{CheckConstraint, Column, ForeignKey, IndexInfo, UniqueConstraint};
    use std::collections::HashSet;

    fn base_table() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            kind: TableKind::Table,
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    default: None,
                },
                Column {
                    name: "email".to_string(),
                    data_type: "text".to_string(),
                    default: None,
                },
            ],
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![],
            not_null_cols: HashSet::from(["id".to_string()]),
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
        }
    }

    #[test]
    fn same_schema_yields_same_hash() {
        let tables = vec![base_table()];
        assert_eq!(schema_hash(&tables), schema_hash(&tables.clone()));
    }

    #[test]
    fn schema_name_contributes_to_the_hash() {
        let mut other = base_table();
        other.schema = "extended".to_string();
        assert_ne!(
            schema_hash(&[base_table()]),
            schema_hash(&[other]),
            "schema name must contribute to the hash"
        );
    }

    #[test]
    fn table_name_contributes_to_the_hash() {
        let mut other = base_table();
        other.name = "accounts".to_string();
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other]));
    }

    #[test]
    fn table_kind_contributes_to_the_hash() {
        let mut other = base_table();
        other.kind = TableKind::View;
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other]));
    }

    #[test]
    fn column_order_contributes_to_the_hash() {
        let mut reordered = base_table();
        reordered.columns.reverse();
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[reordered]));
    }

    #[test]
    fn column_name_contributes_to_the_hash() {
        let mut other = base_table();
        other.columns[0].name = "identifier".to_string();
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other]));
    }

    #[test]
    fn column_type_contributes_to_the_hash() {
        let mut other = base_table();
        other.columns[0].data_type = "bigint".to_string();
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other]));
    }

    #[test]
    fn column_default_contributes_to_the_hash() {
        let mut other = base_table();
        other.columns[1].default = Some("'unknown'".to_string());
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other]));
    }

    #[test]
    fn not_null_contributes_to_the_hash() {
        let mut other = base_table();
        other.not_null_cols.insert("email".to_string());
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other]));
    }

    #[test]
    fn primary_key_contributes_to_the_hash() {
        let mut other = base_table();
        other.primary_keys.clear();
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other]));
    }

    #[test]
    fn foreign_key_contributes_to_the_hash() {
        let mut other = base_table();
        other.foreign_keys.push(ForeignKey {
            name: "fk_users_org".to_string(),
            from_columns: vec!["id".to_string()],
            to_schema: "public".to_string(),
            to_table: "organizations".to_string(),
            to_columns: vec!["id".to_string()],
        });
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other.clone()]));

        let mut renamed = other.clone();
        renamed.foreign_keys[0].name = "fk_other_name".to_string();
        assert_ne!(
            schema_hash(&[other.clone()]),
            schema_hash(&[renamed]),
            "foreign key name must contribute to the hash"
        );

        let mut retargeted = other.clone();
        retargeted.foreign_keys[0].to_columns = vec!["uuid".to_string()];
        assert_ne!(
            schema_hash(&[other]),
            schema_hash(&[retargeted]),
            "foreign key column lists must contribute to the hash"
        );
    }

    #[test]
    fn unique_constraint_contributes_to_the_hash() {
        let mut other = base_table();
        other.unique_constraints.push(UniqueConstraint {
            name: "uq_email".to_string(),
            columns: vec!["email".to_string()],
        });
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other.clone()]));

        let mut renamed = other.clone();
        renamed.unique_constraints[0].name = "uq_other_name".to_string();
        assert_ne!(
            schema_hash(&[other.clone()]),
            schema_hash(&[renamed]),
            "unique constraint name must contribute to the hash"
        );

        let mut recolumned = other.clone();
        recolumned.unique_constraints[0].columns = vec!["id".to_string()];
        assert_ne!(
            schema_hash(&[other]),
            schema_hash(&[recolumned]),
            "unique constraint column list must contribute to the hash"
        );
    }

    #[test]
    fn check_constraint_contributes_to_the_hash() {
        let mut other = base_table();
        other.check_constraints.push(CheckConstraint {
            name: "chk_email".to_string(),
            definition: "email <> ''".to_string(),
        });
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other.clone()]));

        let mut redefined = other.clone();
        redefined.check_constraints[0].definition = "length(email) > 0".to_string();
        assert_ne!(
            schema_hash(&[other]),
            schema_hash(&[redefined]),
            "raw CHECK definition must contribute to the hash"
        );
    }

    #[test]
    fn index_contributes_to_the_hash() {
        let mut other = base_table();
        other.indexes.push(IndexInfo {
            name: "idx_email".to_string(),
            columns: vec!["email".to_string()],
            is_unique: false,
        });
        assert_ne!(schema_hash(&[base_table()]), schema_hash(&[other.clone()]));

        let mut renamed = other.clone();
        renamed.indexes[0].name = "idx_other_name".to_string();
        assert_ne!(
            schema_hash(&[other.clone()]),
            schema_hash(&[renamed]),
            "index name must contribute to the hash"
        );

        let mut uniqued = other.clone();
        uniqued.indexes[0].is_unique = true;
        assert_ne!(
            schema_hash(&[other.clone()]),
            schema_hash(&[uniqued]),
            "index uniqueness must contribute to the hash"
        );

        let mut recolumned = other.clone();
        recolumned.indexes[0].columns = vec!["id".to_string(), "email".to_string()];
        assert_ne!(
            schema_hash(&[other]),
            schema_hash(&[recolumned]),
            "index column list must contribute to the hash"
        );
    }

    /// Frozen hash vector: guards against accidental canonicalization
    /// drift. If this fails after a deliberate format change, bump
    /// [`LOCK_FORMAT_VERSION`] and update the expected hash together.
    #[test]
    fn frozen_hash_vector() {
        let tables = vec![base_table()];
        assert_eq!(
            schema_hash(&tables),
            "sha256:804b87a98b311b9bb8988707be44a7c1af5e1c5890ba65b7842ed5d3094edc5c"
        );
    }

    #[test]
    fn to_toml_matches_the_documented_layout() {
        let lock = LockFile::new("sha256:abc".to_string(), &["public"], &[], &["audit_trail"]);

        assert_eq!(
            lock.to_toml(),
            "version = 1\nhash = \"sha256:abc\"\nschemas = [\"public\"]\ntables = []\nignore_tables = [\"audit_trail\"]\n"
        );
    }

    #[tokio::test]
    async fn write_lock_file_creates_the_file() {
        let dir = tempfile_dir();
        let path = dir.join("erdify.lock");
        let tables = vec![base_table()];

        write_lock_file(&path, &tables, &["public"], &[], &[])
            .await
            .unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains(&schema_hash(&tables)));
        assert!(content.contains("schemas = [\"public\"]"));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn write_lock_file_is_idempotent_when_the_hash_is_unchanged() {
        let dir = tempfile_dir();
        let path = dir.join("erdify.lock");
        let tables = vec![base_table()];

        write_lock_file(&path, &tables, &["public"], &[], &[])
            .await
            .unwrap();
        let mtime_before = tokio::fs::metadata(&path)
            .await
            .unwrap()
            .modified()
            .unwrap();

        // Give the filesystem clock room to move before rewriting, so a
        // spurious touch wouldn't hide behind clock-resolution truncation.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        write_lock_file(&path, &tables, &["public"], &[], &[])
            .await
            .unwrap();
        let mtime_after = tokio::fs::metadata(&path)
            .await
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(
            mtime_before, mtime_after,
            "the file must be left untouched when the hash hasn't changed"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn write_lock_file_overwrites_when_the_hash_changes() {
        let dir = tempfile_dir();
        let path = dir.join("erdify.lock");

        write_lock_file(&path, &[base_table()], &["public"], &[], &[])
            .await
            .unwrap();

        let mut changed = base_table();
        changed.name = "accounts".to_string();
        write_lock_file(&path, &[changed.clone()], &["public"], &[], &[])
            .await
            .unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains(&schema_hash(&[changed])));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    /// Unique per-test scratch directory under the system temp dir.
    fn tempfile_dir() -> std::path::PathBuf {
        // A counter on top of the clock: tests run in parallel threads of
        // the same process, and the wall clock's resolution isn't fine
        // enough to keep two calls landing in the same tick from
        // colliding on the same directory name.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let dir = std::env::temp_dir().join(format!(
            "erdify-lock-test-{}-{}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_lock() -> LockFile {
        LockFile::new("sha256:abc".to_string(), &["public"], &[], &["audit_trail"])
    }

    #[test]
    fn parse_toml_round_trips_to_toml() {
        let lock = sample_lock();
        assert_eq!(parse_toml(&lock.to_toml()), Some(lock));
    }

    #[test]
    fn parse_toml_round_trips_escaped_values() {
        let lock = LockFile::new(
            "sha256:abc".to_string(),
            &["weird\"schema"],
            &["back\\slash"],
            &[],
        );
        assert_eq!(parse_toml(&lock.to_toml()), Some(lock));
    }

    #[test]
    fn parse_toml_rejects_missing_field() {
        assert_eq!(parse_toml("version = 1\nhash = \"sha256:abc\"\n"), None);
    }

    #[test]
    fn parse_toml_rejects_garbage() {
        assert_eq!(parse_toml("this is not toml at all"), None);
    }

    #[tokio::test]
    async fn read_lock_file_reports_missing_when_absent() {
        let dir = tempfile_dir();
        let path = dir.join("erdify.lock");

        assert!(matches!(read_lock_file(&path).await, LockLookup::Missing));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn read_lock_file_reports_corrupt_when_unparseable() {
        let dir = tempfile_dir();
        let path = dir.join("erdify.lock");
        tokio::fs::write(&path, "not a lock file").await.unwrap();

        assert!(matches!(read_lock_file(&path).await, LockLookup::Corrupt));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn read_lock_file_reports_found_when_valid() {
        let dir = tempfile_dir();
        let path = dir.join("erdify.lock");
        tokio::fs::write(&path, sample_lock().to_toml())
            .await
            .unwrap();

        assert!(matches!(read_lock_file(&path).await, LockLookup::Found(_)));

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[test]
    fn check_reports_missing_when_no_lock_found() {
        let result = check(&LockLookup::Missing, "sha256:abc", &["public"], &[], &[]);
        assert!(matches!(result, CheckResult::Missing));
    }

    #[test]
    fn check_reports_incomparable_when_corrupt() {
        let result = check(&LockLookup::Corrupt, "sha256:abc", &["public"], &[], &[]);
        assert!(matches!(result, CheckResult::Incomparable(_)));
    }

    #[test]
    fn check_reports_incomparable_when_version_is_unknown() {
        let mut lock = sample_lock();
        lock.version = LOCK_FORMAT_VERSION + 1;
        lock.hash = "sha256:abc".to_string();

        let result = check(
            &LockLookup::Found(lock),
            "sha256:abc",
            &["public"],
            &[],
            &["audit_trail"],
        );
        assert!(matches!(result, CheckResult::Incomparable(_)));
    }

    #[test]
    fn check_reports_incomparable_when_filters_differ() {
        let lock = sample_lock();

        let result = check(
            &LockLookup::Found(lock),
            "sha256:abc",
            &["public", "extended"],
            &[],
            &["audit_trail"],
        );
        assert!(matches!(result, CheckResult::Incomparable(_)));
    }

    #[test]
    fn check_reports_incomparable_before_checking_equality() {
        // Same hash, but filters differ: must be Incomparable, not Unchanged.
        let lock = sample_lock();

        let result = check(&LockLookup::Found(lock.clone()), &lock.hash, &[], &[], &[]);
        assert!(matches!(result, CheckResult::Incomparable(_)));
    }

    #[test]
    fn check_reports_unchanged_when_hash_and_filters_match() {
        let lock = sample_lock();

        let result = check(
            &LockLookup::Found(lock.clone()),
            &lock.hash,
            &["public"],
            &[],
            &["audit_trail"],
        );
        assert!(matches!(result, CheckResult::Unchanged));
    }

    #[test]
    fn check_reports_changed_when_hash_differs_but_filters_match() {
        let lock = sample_lock();

        let result = check(
            &LockLookup::Found(lock),
            "sha256:different",
            &["public"],
            &[],
            &["audit_trail"],
        );
        assert!(matches!(result, CheckResult::Changed));
    }
}
