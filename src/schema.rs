use std::collections::HashSet;

/// Represents a table with all its metadata.
#[derive(Debug, Clone, Default)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub primary_keys: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub not_null_cols: HashSet<String>,
    pub unique_constraints: Vec<UniqueConstraint>,
    pub check_constraints: Vec<CheckConstraint>,
    pub indexes: Vec<IndexInfo>,
}

impl Table {
    /// Unique identifying key of a table: `(schema, name)`.
    #[must_use]
    pub fn key(&self) -> (&str, &str) {
        (&self.schema, &self.name)
    }
}

/// Column of a table.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub data_type: String,
}

/// Foreign key constraint (potentially multi-column).
#[derive(Debug, Clone)]
pub struct ForeignKey {
    pub name: String,
    pub from_columns: Vec<String>,
    pub to_schema: String,
    pub to_table: String,
    pub to_columns: Vec<String>,
}

/// UNIQUE constraint.
#[derive(Debug, Clone)]
pub struct UniqueConstraint {
    pub name: String,
    pub columns: Vec<String>,
}

/// CHECK constraint.
#[derive(Debug, Clone)]
pub struct CheckConstraint {
    pub name: String,
    pub definition: String,
}

/// Information about an index.
#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
}

/// System schemas always excluded from generation.
pub const SYSTEM_SCHEMAS: [&str; 2] = ["information_schema", "pg_catalog"];

/// Filters tables based on the specified schemas and tables.
///
/// An empty filter list means "no filtering" on the corresponding criterion;
/// system schemas remain excluded in all cases. `ignore_tables` removes the
/// named tables from the result, regardless of their schema; `tables_filter`
/// and `ignore_tables` are mutually exclusive on the CLI side (clap
/// `conflicts_with`), so at most one of the two is non-empty here.
#[must_use]
pub fn filter_tables(
    tables: Vec<Table>,
    schemas: &[&str],
    tables_filter: &[&str],
    ignore_tables: &[&str],
) -> Vec<Table> {
    tables
        .into_iter()
        .filter(|t| {
            let schema_ok = if schemas.is_empty() {
                !SYSTEM_SCHEMAS.contains(&t.schema.as_str())
            } else {
                schemas.contains(&t.schema.as_str())
            };

            let table_ok = tables_filter.is_empty() || tables_filter.contains(&t.name.as_str());
            let not_ignored = !ignore_tables.contains(&t.name.as_str());

            schema_ok && table_ok && not_ignored
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(schema: &str, name: &str) -> Table {
        Table {
            schema: schema.to_string(),
            name: name.to_string(),
            ..Table::default()
        }
    }

    #[test]
    fn filter_tables_excludes_system_schemas_by_default() {
        let tables = vec![
            table("public", "users"),
            table("pg_catalog", "pg_class"),
            table("information_schema", "columns"),
        ];

        let result = filter_tables(tables, &[], &[], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "users");
    }

    #[test]
    fn filter_tables_keeps_only_requested_schemas() {
        let tables = vec![table("public", "users"), table("extended", "audit")];

        let result = filter_tables(tables, &["extended"], &[], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].schema, "extended");
    }

    #[test]
    fn filter_tables_combines_schema_and_table_filters() {
        let tables = vec![
            table("public", "users"),
            table("public", "orders"),
            table("extended", "users"),
        ];

        let result = filter_tables(tables, &["public"], &["users"], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key(), ("public", "users"));
    }

    #[test]
    fn filter_tables_excludes_ignored_tables() {
        let tables = vec![
            table("public", "users"),
            table("public", "logs"),
            table("extended", "logs"),
        ];

        let result = filter_tables(tables, &[], &[], &["logs"]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "users");
    }

    #[test]
    fn filter_tables_combines_schema_and_ignore_filters() {
        let tables = vec![
            table("public", "users"),
            table("public", "logs"),
            table("extended", "logs"),
        ];

        let result = filter_tables(tables, &["public"], &[], &["logs"]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key(), ("public", "users"));
    }
}
