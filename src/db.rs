//! Schema extraction from the PostgreSQL system catalog.

use crate::config::ConnectionInfo;
use crate::errors::ErdifyError;
use crate::schema::{
    CheckConstraint, Column, ForeignKey, IndexInfo, SYSTEM_SCHEMAS, Table, UniqueConstraint,
};
use std::collections::{HashMap, HashSet};
use tokio::time::{Duration, timeout};
use tokio_postgres::types::{Oid, ToSql};
use tokio_postgres::{Client, NoTls};

/// Maximum delay for establishing the connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Raw constraint as read from `pg_constraint`.
struct ConstraintRow {
    table_oid: Oid,
    name: String,
    /// `p` (primary key), `f` (foreign key), `u` (unique) or `c` (check).
    kind: String,
    columns: Vec<String>,
    ref_schema: Option<String>,
    ref_table: Option<String>,
    ref_columns: Vec<String>,
    definition: String,
}

/// Raw index as read from `pg_index`.
struct IndexRow {
    table_oid: Oid,
    name: String,
    columns: Vec<String>,
    is_unique: bool,
}

/// Raw column as read from `pg_attribute`.
struct ColumnRow {
    table_oid: Oid,
    name: String,
    data_type: String,
    not_null: bool,
}

/// Identity of a table in the catalog.
struct TableRow {
    oid: Oid,
    schema: String,
    name: String,
}

/// Connection to the PostgreSQL database with a timeout.
///
/// # Errors
///
/// Returns [`ErdifyError::ConnectionTimeout`] if the connection doesn't
/// succeed within the allotted time, [`ErdifyError::DatabaseConnection`] if
/// the server refuses the connection.
pub async fn connect(info: &ConnectionInfo) -> Result<Client, ErdifyError> {
    // The configuration is built field by field rather than by concatenating
    // a URL: a password containing `@`, `/` or `:` would break the
    // connection string.
    let mut config = tokio_postgres::Config::new();
    config
        .host(&info.host)
        .port(info.port)
        .dbname(&info.database)
        .connect_timeout(CONNECT_TIMEOUT);

    if !info.user.is_empty() {
        config.user(&info.user);
    }
    if !info.password.is_empty() {
        config.password(&info.password);
    }

    let fut = config.connect(NoTls);

    match timeout(CONNECT_TIMEOUT, fut).await {
        Ok(Ok((client, connection))) => {
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    eprintln!("warning: connection interrupted: {e}");
                }
            });

            ping(&client).await?;
            Ok(client)
        }
        Ok(Err(e)) => Err(ErdifyError::DatabaseConnection(e.to_string())),
        Err(_) => Err(ErdifyError::ConnectionTimeout),
    }
}

/// Validates the connection by running a simple query.
///
/// # Errors
///
/// Returns [`ErdifyError::DatabaseConnection`] if the query fails.
pub async fn ping(client: &Client) -> Result<(), ErdifyError> {
    client
        .query_one("SELECT 1", &[])
        .await
        .map_err(|e| ErdifyError::DatabaseConnection(e.to_string()))?;
    Ok(())
}

/// Fetches all tables of the specified schemas, along with their metadata.
///
/// Tables are returned sorted by `(schema, name)` so the output is
/// reproducible across runs.
///
/// # Errors
///
/// Returns [`ErdifyError::QueryError`] if one of the catalog queries fails.
pub async fn fetch_tables(
    client: &Client,
    schemas: &[&str],
    tables_filter: &[&str],
    ignore_tables: &[&str],
) -> Result<Vec<Table>, ErdifyError> {
    let table_rows = fetch_table_list(client, schemas).await?;
    warn_missing_schemas(schemas, &table_rows);

    if table_rows.is_empty() {
        return Ok(Vec::new());
    }

    let oids: Vec<Oid> = table_rows.iter().map(|t| t.oid).collect();

    // The three queries are independent, but tokio-postgres serializes
    // queries from the same client: chaining them remains the simplest approach.
    let columns = fetch_columns(client, &oids).await?;
    let constraints = fetch_constraints(client, &oids).await?;
    let indexes = fetch_indexes(client, &oids).await?;

    let tables = assemble_tables(table_rows, columns, constraints, indexes);
    let tables = crate::schema::filter_tables(tables, schemas, tables_filter, ignore_tables);
    warn_missing_tables(tables_filter, &tables);

    Ok(tables)
}

/// Lists ordinary and partitioned tables of the requested schemas.
async fn fetch_table_list(client: &Client, schemas: &[&str]) -> Result<Vec<TableRow>, ErdifyError> {
    // `$1` is NULL when no schema is requested: the predicate is then
    // neutralized and only system schemas remain excluded.
    let query = "\
        SELECT c.oid, n.nspname AS schema_name, c.relname AS table_name \
        FROM pg_class c \
        JOIN pg_namespace n ON n.oid = c.relnamespace \
        WHERE c.relkind IN ('r', 'p') \
          AND NOT c.relispartition \
          AND n.nspname <> ALL($2) \
          AND ($1::text[] IS NULL OR n.nspname = ANY($1)) \
        ORDER BY n.nspname, c.relname";

    let schemas_param: Option<Vec<&str>> = if schemas.is_empty() {
        None
    } else {
        Some(schemas.to_vec())
    };
    let system: Vec<&str> = SYSTEM_SCHEMAS.to_vec();

    let rows = client
        .query(query, &[&schemas_param, &system])
        .await
        .map_err(|e| ErdifyError::QueryError(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|row| TableRow {
            oid: row.get("oid"),
            schema: row.get("schema_name"),
            name: row.get("table_name"),
        })
        .collect())
}

/// Loads the columns (name, type, nullability) of the requested tables.
async fn fetch_columns(client: &Client, oids: &[Oid]) -> Result<Vec<ColumnRow>, ErdifyError> {
    // `format_type` returns the type as PostgreSQL displays it, including for
    // user-defined types and domains: no "unknown" type.
    let query = "\
        SELECT a.attrelid AS table_oid, \
               a.attname AS column_name, \
               format_type(a.atttypid, a.atttypmod) AS data_type, \
               a.attnotnull AS not_null \
        FROM pg_attribute a \
        WHERE a.attrelid = ANY($1) \
          AND a.attnum > 0 \
          AND NOT a.attisdropped \
        ORDER BY a.attrelid, a.attnum";

    let rows = client
        .query(query, &[&oids])
        .await
        .map_err(|e| ErdifyError::QueryError(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|row| ColumnRow {
            table_oid: row.get("table_oid"),
            name: row.get("column_name"),
            data_type: row.get("data_type"),
            not_null: row.get("not_null"),
        })
        .collect())
}

/// Loads the PK, FK, UNIQUE, and CHECK constraints of the requested tables.
async fn fetch_constraints(
    client: &Client,
    oids: &[Oid],
) -> Result<Vec<ConstraintRow>, ErdifyError> {
    // `WITH ORDINALITY` preserves the column order declared in the
    // constraint, which a plain join on `pg_attribute` doesn't guarantee.
    let query = "\
        SELECT co.conrelid AS table_oid, \
               co.conname AS constraint_name, \
               co.contype::text AS constraint_type, \
               ARRAY( \
                   SELECT a.attname \
                   FROM unnest(co.conkey) WITH ORDINALITY AS k(attnum, ord) \
                   JOIN pg_attribute a ON a.attrelid = co.conrelid AND a.attnum = k.attnum \
                   ORDER BY k.ord \
               ) AS columns, \
               rn.nspname AS ref_schema, \
               rc.relname AS ref_table, \
               ARRAY( \
                   SELECT a.attname \
                   FROM unnest(co.confkey) WITH ORDINALITY AS k(attnum, ord) \
                   JOIN pg_attribute a ON a.attrelid = co.confrelid AND a.attnum = k.attnum \
                   ORDER BY k.ord \
               ) AS ref_columns, \
               pg_get_constraintdef(co.oid) AS definition \
        FROM pg_constraint co \
        LEFT JOIN pg_class rc ON rc.oid = co.confrelid \
        LEFT JOIN pg_namespace rn ON rn.oid = rc.relnamespace \
        WHERE co.conrelid = ANY($1) \
          AND co.contype IN ('p', 'f', 'u', 'c') \
        ORDER BY co.conrelid, co.conname";

    let rows = client
        .query(query, &[&oids])
        .await
        .map_err(|e| ErdifyError::QueryError(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|row| ConstraintRow {
            table_oid: row.get("table_oid"),
            name: row.get("constraint_name"),
            kind: row.get("constraint_type"),
            columns: row.get("columns"),
            ref_schema: row.get("ref_schema"),
            ref_table: row.get("ref_table"),
            ref_columns: row.get("ref_columns"),
            definition: row.get("definition"),
        })
        .collect())
}

/// Loads the indexes of the requested tables, excluding the primary key index.
async fn fetch_indexes(client: &Client, oids: &[Oid]) -> Result<Vec<IndexRow>, ErdifyError> {
    // `indkey` is an `int2vector`: the explicit cast to `smallint[]` allows
    // using `unnest ... WITH ORDINALITY`. Entries of 0 (expression columns)
    // don't join any row and are naturally ignored.
    let query = "\
        SELECT ix.indrelid AS table_oid, \
               i.relname AS index_name, \
               ix.indisunique AS is_unique, \
               ARRAY( \
                   SELECT a.attname \
                   FROM unnest(ix.indkey::smallint[]) WITH ORDINALITY AS k(attnum, ord) \
                   JOIN pg_attribute a ON a.attrelid = ix.indrelid AND a.attnum = k.attnum \
                   ORDER BY k.ord \
               ) AS columns \
        FROM pg_index ix \
        JOIN pg_class i ON i.oid = ix.indexrelid \
        WHERE ix.indrelid = ANY($1) \
          AND NOT ix.indisprimary \
        ORDER BY ix.indrelid, i.relname";

    let rows = client
        .query(query, &[&oids])
        .await
        .map_err(|e| ErdifyError::QueryError(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|row| IndexRow {
            table_oid: row.get("table_oid"),
            name: row.get("index_name"),
            columns: row.get("columns"),
            is_unique: row.get("is_unique"),
        })
        .collect())
}

/// Assembles the raw catalog rows into [`Table`]s, sorted by `(schema, name)`.
fn assemble_tables(
    table_rows: Vec<TableRow>,
    columns: Vec<ColumnRow>,
    constraints: Vec<ConstraintRow>,
    indexes: Vec<IndexRow>,
) -> Vec<Table> {
    // The order of `table_rows` comes from an `ORDER BY`: it's preserved by
    // indexing positions rather than iterating a `HashMap`.
    let mut tables: Vec<Table> = Vec::with_capacity(table_rows.len());
    let mut position: HashMap<Oid, usize> = HashMap::with_capacity(table_rows.len());

    for row in table_rows {
        position.insert(row.oid, tables.len());
        tables.push(Table {
            schema: row.schema,
            name: row.name,
            ..Table::default()
        });
    }

    for col in columns {
        let Some(&idx) = position.get(&col.table_oid) else {
            continue;
        };
        let table = &mut tables[idx];
        if col.not_null {
            table.not_null_cols.insert(col.name.clone());
        }
        table.columns.push(Column {
            name: col.name,
            data_type: col.data_type,
        });
    }

    for c in constraints {
        let Some(&idx) = position.get(&c.table_oid) else {
            continue;
        };
        let table = &mut tables[idx];

        match c.kind.as_str() {
            "p" => table.primary_keys = c.columns,
            "f" => {
                // `ref_schema` / `ref_table` are NULL if the target table
                // disappeared between two queries: the FK is then unusable.
                if let (Some(to_schema), Some(to_table)) = (c.ref_schema, c.ref_table) {
                    table.foreign_keys.push(ForeignKey {
                        name: c.name,
                        from_columns: c.columns,
                        to_schema,
                        to_table,
                        to_columns: c.ref_columns,
                    });
                }
            }
            "u" => table.unique_constraints.push(UniqueConstraint {
                name: c.name,
                columns: c.columns,
            }),
            "c" => table.check_constraints.push(CheckConstraint {
                name: c.name,
                definition: c.definition,
            }),
            _ => {}
        }
    }

    for idx_row in indexes {
        let Some(&idx) = position.get(&idx_row.table_oid) else {
            continue;
        };
        tables[idx].indexes.push(IndexInfo {
            name: idx_row.name,
            columns: idx_row.columns,
            is_unique: idx_row.is_unique,
        });
    }

    tables
}

/// Warns on stderr for each requested schema that has no table.
fn warn_missing_schemas(requested: &[&str], found: &[TableRow]) {
    if requested.is_empty() {
        return;
    }

    let present: HashSet<&str> = found.iter().map(|t| t.schema.as_str()).collect();
    for schema in requested {
        if !present.contains(schema) {
            eprintln!("warning: no table found in schema \"{schema}\"");
        }
    }
}

/// Warns on stderr for each requested table that couldn't be found.
fn warn_missing_tables(requested: &[&str], found: &[Table]) {
    if requested.is_empty() {
        return;
    }

    let present: HashSet<&str> = found.iter().map(|t| t.name.as_str()).collect();
    let missing: Vec<&&str> = requested
        .iter()
        .filter(|t| !present.contains(**t))
        .collect();

    if !missing.is_empty() {
        let list = missing
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("warning: table(s) not found: {list}");
    }
}

/// Verifies at compile time that the query parameters remain `ToSql`.
const _: fn() = || {
    fn assert_to_sql<T: ToSql + Sync>() {}
    assert_to_sql::<Vec<Oid>>();
    assert_to_sql::<Option<Vec<&str>>>();
};

#[cfg(test)]
mod tests {
    use super::*;

    fn table_row(oid: Oid, schema: &str, name: &str) -> TableRow {
        TableRow {
            oid,
            schema: schema.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn assemble_tables_preserves_catalog_order() {
        let rows = vec![
            table_row(1, "extended", "audit"),
            table_row(2, "public", "orders"),
            table_row(3, "public", "users"),
        ];

        let tables = assemble_tables(rows, Vec::new(), Vec::new(), Vec::new());

        let keys: Vec<_> = tables.iter().map(Table::key).collect();
        assert_eq!(
            keys,
            vec![
                ("extended", "audit"),
                ("public", "orders"),
                ("public", "users"),
            ]
        );
    }

    #[test]
    fn assemble_tables_attaches_columns_and_not_null() {
        let rows = vec![table_row(1, "public", "users")];
        let columns = vec![
            ColumnRow {
                table_oid: 1,
                name: "id".to_string(),
                data_type: "integer".to_string(),
                not_null: true,
            },
            ColumnRow {
                table_oid: 1,
                name: "bio".to_string(),
                data_type: "text".to_string(),
                not_null: false,
            },
        ];

        let tables = assemble_tables(rows, columns, Vec::new(), Vec::new());

        assert_eq!(tables[0].columns.len(), 2);
        assert_eq!(tables[0].columns[0].name, "id");
        assert!(tables[0].not_null_cols.contains("id"));
        assert!(!tables[0].not_null_cols.contains("bio"));
    }

    #[test]
    fn assemble_tables_dispatches_constraints_by_type() {
        let rows = vec![table_row(1, "public", "orders")];
        let constraints = vec![
            ConstraintRow {
                table_oid: 1,
                name: "orders_pkey".to_string(),
                kind: "p".to_string(),
                columns: vec!["id".to_string()],
                ref_schema: None,
                ref_table: None,
                ref_columns: Vec::new(),
                definition: "PRIMARY KEY (id)".to_string(),
            },
            ConstraintRow {
                table_oid: 1,
                name: "orders_user_fkey".to_string(),
                kind: "f".to_string(),
                columns: vec!["user_id".to_string()],
                ref_schema: Some("public".to_string()),
                ref_table: Some("users".to_string()),
                ref_columns: vec!["id".to_string()],
                definition: "FOREIGN KEY (user_id) REFERENCES users(id)".to_string(),
            },
            ConstraintRow {
                table_oid: 1,
                name: "orders_ref_key".to_string(),
                kind: "u".to_string(),
                columns: vec!["reference".to_string()],
                ref_schema: None,
                ref_table: None,
                ref_columns: Vec::new(),
                definition: "UNIQUE (reference)".to_string(),
            },
            ConstraintRow {
                table_oid: 1,
                name: "orders_total_check".to_string(),
                kind: "c".to_string(),
                columns: vec!["total".to_string()],
                ref_schema: None,
                ref_table: None,
                ref_columns: Vec::new(),
                definition: "CHECK ((total > 0))".to_string(),
            },
        ];

        let tables = assemble_tables(rows, Vec::new(), constraints, Vec::new());
        let table = &tables[0];

        assert_eq!(table.primary_keys, vec!["id".to_string()]);
        assert_eq!(table.foreign_keys.len(), 1);
        assert_eq!(table.foreign_keys[0].to_schema, "public");
        assert_eq!(table.foreign_keys[0].to_table, "users");
        assert_eq!(table.unique_constraints[0].name, "orders_ref_key");
        assert_eq!(table.check_constraints[0].definition, "CHECK ((total > 0))");
    }

    #[test]
    fn assemble_tables_drops_foreign_key_without_target() {
        let rows = vec![table_row(1, "public", "orders")];
        let constraints = vec![ConstraintRow {
            table_oid: 1,
            name: "dangling".to_string(),
            kind: "f".to_string(),
            columns: vec!["user_id".to_string()],
            ref_schema: None,
            ref_table: None,
            ref_columns: Vec::new(),
            definition: String::new(),
        }];

        let tables = assemble_tables(rows, Vec::new(), constraints, Vec::new());

        assert!(tables[0].foreign_keys.is_empty());
    }

    #[test]
    fn assemble_tables_ignores_rows_of_unknown_tables() {
        let rows = vec![table_row(1, "public", "users")];
        let columns = vec![ColumnRow {
            table_oid: 999,
            name: "ghost".to_string(),
            data_type: "text".to_string(),
            not_null: false,
        }];

        let tables = assemble_tables(rows, columns, Vec::new(), Vec::new());

        assert!(tables[0].columns.is_empty());
    }

    #[test]
    fn assemble_tables_attaches_indexes() {
        let rows = vec![table_row(1, "public", "users")];
        let indexes = vec![IndexRow {
            table_oid: 1,
            name: "idx_users_email".to_string(),
            columns: vec!["email".to_string()],
            is_unique: true,
        }];

        let tables = assemble_tables(rows, Vec::new(), Vec::new(), indexes);

        assert_eq!(tables[0].indexes.len(), 1);
        assert_eq!(tables[0].indexes[0].name, "idx_users_email");
        assert!(tables[0].indexes[0].is_unique);
    }

    #[test]
    fn assemble_tables_ignores_indexes_of_unknown_tables() {
        let rows = vec![table_row(1, "public", "users")];
        let indexes = vec![IndexRow {
            table_oid: 999,
            name: "ghost_idx".to_string(),
            columns: Vec::new(),
            is_unique: false,
        }];

        let tables = assemble_tables(rows, Vec::new(), Vec::new(), indexes);

        assert!(tables[0].indexes.is_empty());
    }

    #[test]
    fn warn_missing_schemas_does_nothing_when_no_schema_requested() {
        warn_missing_schemas(&[], &[]);
    }

    #[test]
    fn warn_missing_schemas_does_nothing_when_all_present() {
        let found = vec![table_row(1, "public", "users")];
        warn_missing_schemas(&["public"], &found);
    }

    #[test]
    fn warn_missing_schemas_warns_on_absent_schema() {
        let found = vec![table_row(1, "public", "users")];
        // Ne panique pas : le message est écrit sur stderr, sans valeur de retour à vérifier.
        warn_missing_schemas(&["public", "extended"], &found);
    }

    #[test]
    fn warn_missing_tables_does_nothing_when_no_table_requested() {
        warn_missing_tables(&[], &[]);
    }

    #[test]
    fn warn_missing_tables_does_nothing_when_all_present() {
        let table = Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            ..Table::default()
        };
        warn_missing_tables(&["users"], &[table]);
    }

    #[test]
    fn warn_missing_tables_warns_on_absent_table() {
        let table = Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            ..Table::default()
        };
        // Ne panique pas : le message est écrit sur stderr, sans valeur de retour à vérifier.
        warn_missing_tables(&["users", "ghost"], &[table]);
    }
}
