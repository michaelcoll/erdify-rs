//! Renders the schema as a Mermaid `erDiagram`.
//!
//! The output is a Markdown document containing a valid ```` ```mermaid ````
//! block: entities declared with `NAME { type column KEYS "comment" }`
//! and relationships formatted as `PARENT ||--o{ CHILD : "label"`.

use crate::config::{Args, OutputMode};
use crate::schema::{Table, TableKind};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Indentation of an entity within the `erDiagram` block.
const ENTITY_INDENT: &str = "    ";
/// Indentation of an attribute within an entity's block.
const ATTR_INDENT: &str = "        ";

/// Identifying key of a table: `(schema, name)`.
type TableKey<'a> = (&'a str, &'a str);

/// Generates the full Markdown document (title + Mermaid block) for all tables.
#[must_use]
pub fn render_all(tables: &[Table], mode: OutputMode, args: &Args, database: &str) -> String {
    let mut output = String::new();

    let title = generate_title(args, database, tables);
    let _ = writeln!(output, "# {title}\n");

    // Entity names are only prefixed with the schema when multiple schemas
    // coexist: otherwise `users` is more readable than `public.users`.
    let qualify = uses_multiple_schemas(tables);
    let names: HashMap<TableKey<'_>, String> = tables
        .iter()
        .map(|t| (t.key(), entity_name(t, qualify)))
        .collect();

    output.push_str("```mermaid\nerDiagram\n");

    for table in tables {
        let Some(name) = names.get(&table.key()) else {
            continue;
        };
        output.push_str(&render_entity(table, name, mode));
    }

    // Relationships are only drawn in full mode; in default mode FKs are
    // only marked on the columns themselves.
    if mode == OutputMode::Full {
        output.push_str(&render_relationships(tables, &names));
    }

    output.push_str("```\n");

    output.push_str(&render_views_section(tables, qualify));

    if mode == OutputMode::Full {
        output.push_str(&render_extras(tables, qualify));
    }

    output
}

/// Indicates whether the tables come from multiple schemas.
fn uses_multiple_schemas(tables: &[Table]) -> bool {
    let mut schemas = tables.iter().map(|t| t.schema.as_str());
    let Some(first) = schemas.next() else {
        return false;
    };
    schemas.any(|s| s != first)
}

/// Builds a table's Mermaid entity name, quoting it if necessary.
fn entity_name(table: &Table, qualify: bool) -> String {
    let raw = if qualify {
        format!("{}.{}", table.schema, table.name)
    } else {
        table.name.clone()
    };
    quote_if_needed(&raw)
}

/// Generates the block for a single entity.
fn render_entity(table: &Table, name: &str, mode: OutputMode) -> String {
    let mut s = String::new();

    // An entity with no columns is declared without an attribute block: an
    // empty block isn't accepted by the Mermaid grammar.
    if table.columns.is_empty() {
        let _ = writeln!(s, "{ENTITY_INDENT}{name}");
        return s;
    }

    let pk: HashSet<&str> = table.primary_keys.iter().map(String::as_str).collect();
    let fk: HashSet<&str> = table
        .foreign_keys
        .iter()
        .flat_map(|f| f.from_columns.iter().map(String::as_str))
        .collect();
    let uk = single_column_unique_names(table);

    let _ = writeln!(s, "{ENTITY_INDENT}{name} {{");

    for col in &table.columns {
        let col_name = col.name.as_str();
        let mut keys: Vec<&str> = Vec::new();

        if mode != OutputMode::Minimal {
            if pk.contains(col_name) {
                keys.push("PK");
            }
            if fk.contains(col_name) {
                keys.push("FK");
            }
            if mode == OutputMode::Full && uk.contains(col_name) {
                keys.push("UK");
            }
        }

        let _ = write!(
            s,
            "{ATTR_INDENT}{} {}",
            sanitize_type(&col.data_type),
            sanitize_ident(col_name)
        );

        if !keys.is_empty() {
            let _ = write!(s, " {}", keys.join(", "));
        }

        // NOT NULL is redundant with PK, so it's only shown for other columns.
        // The default value stays useful even on a PK (e.g. a sequence), so
        // it isn't suppressed there.
        if mode == OutputMode::Full {
            let mut notes: Vec<String> = Vec::new();
            if !pk.contains(col_name) && table.not_null_cols.contains(col_name) {
                notes.push("not null".to_string());
            }
            if let Some(default) = &col.default {
                notes.push(format!("default: {}", sanitize_comment(default)));
            }
            if !notes.is_empty() {
                let _ = write!(s, " \"{}\"", notes.join(", "));
            }
        }

        s.push('\n');
    }

    let _ = writeln!(s, "{ENTITY_INDENT}}}");
    s
}

/// Columns covered by a single-column UNIQUE constraint or index.
///
/// Multi-column constraints are ignored: marking each column `UK` would
/// wrongly assert that each one is unique on its own.
fn single_column_unique_names(table: &Table) -> HashSet<&str> {
    let from_constraints = table
        .unique_constraints
        .iter()
        .filter(|c| c.columns.len() == 1)
        .map(|c| c.columns[0].as_str());

    let from_indexes = table
        .indexes
        .iter()
        .filter(|i| i.is_unique && i.columns.len() == 1)
        .map(|i| i.columns[0].as_str());

    from_constraints.chain(from_indexes).collect()
}

/// Generates the Mermaid relationships between tables with inferred cardinalities.
fn render_relationships(tables: &[Table], names: &HashMap<TableKey<'_>, String>) -> String {
    let mut s = String::new();
    let mut seen: HashSet<(&str, &str, String)> = HashSet::new();

    for table in tables {
        let Some(child) = names.get(&table.key()) else {
            continue;
        };

        for fk in &table.foreign_keys {
            // An FK pointing outside the filtered scope has no target entity:
            // drawing it would create a phantom entity in the diagram.
            let Some(parent) = names.get(&(fk.to_schema.as_str(), fk.to_table.as_str())) else {
                continue;
            };

            let label = sanitize_comment(&fk.from_columns.join(", "));
            if !seen.insert((parent.as_str(), child.as_str(), label.clone())) {
                continue;
            }

            // Parent side: the child row can exist without a parent if one of
            // the FK columns is nullable.
            let all_not_null = fk
                .from_columns
                .iter()
                .all(|c| table.not_null_cols.contains(c));
            let left = if all_not_null { "||" } else { "|o" };

            // Child side: at most one row if the FK is itself unique (1:1).
            let right = if is_unique_set(table, &fk.from_columns) {
                "o|"
            } else {
                "o{"
            };

            let _ = writeln!(
                s,
                "{ENTITY_INDENT}{parent} {left}--{right} {child} : \"{label}\""
            );
        }
    }

    s
}

/// Indicates whether the column set is covered by a UNIQUE constraint or index.
fn is_unique_set(table: &Table, columns: &[String]) -> bool {
    let target: HashSet<&str> = columns.iter().map(String::as_str).collect();

    let constraint_match = table
        .unique_constraints
        .iter()
        .any(|c| c.columns.iter().map(String::as_str).collect::<HashSet<_>>() == target);

    let index_match = table.indexes.iter().any(|i| {
        i.is_unique && i.columns.iter().map(String::as_str).collect::<HashSet<_>>() == target
    });

    constraint_match || index_match
}

/// Generates the Markdown section listing views and materialized views.
///
/// Unlike [`render_extras`], this section is generated in every output mode:
/// distinguishing a view from a regular table isn't a level-of-detail
/// concern, it's part of identifying the entity. Returns an empty string if
/// no view or materialized view is present.
fn render_views_section(tables: &[Table], qualify: bool) -> String {
    let mut s = String::new();

    let views: Vec<&Table> = tables
        .iter()
        .filter(|t| t.kind != TableKind::Table)
        .collect();
    if views.is_empty() {
        return s;
    }

    s.push_str("\n## Views\n\n");

    for table in views {
        let name = if qualify {
            format!("{}.{}", table.schema, table.name)
        } else {
            table.name.clone()
        };
        let label = match table.kind {
            TableKind::View => "view",
            TableKind::MaterializedView => "materialized view",
            TableKind::Table => unreachable!("filtered to non-Table kinds above"),
        };
        let _ = writeln!(s, "- `{name}` ({label})");
    }

    s
}

/// Generates the Markdown section listing indexes and constraints (full mode).
///
/// The `erDiagram` grammar has no notion of notes: this information is
/// rendered as Markdown, below the Mermaid block, so it stays displayable.
fn render_extras(tables: &[Table], qualify: bool) -> String {
    let mut s = String::new();

    let has_extras = tables.iter().any(|t| {
        !t.indexes.is_empty() || !t.unique_constraints.is_empty() || !t.check_constraints.is_empty()
    });
    if !has_extras {
        return s;
    }

    s.push_str("\n## Indexes and constraints\n");

    for table in tables {
        if table.indexes.is_empty()
            && table.unique_constraints.is_empty()
            && table.check_constraints.is_empty()
        {
            continue;
        }

        let heading = if qualify {
            format!("{}.{}", table.schema, table.name)
        } else {
            table.name.clone()
        };
        let _ = writeln!(s, "\n### {heading}\n");

        for idx in &table.indexes {
            let kind = if idx.is_unique {
                "unique index"
            } else {
                "index"
            };
            let _ = writeln!(s, "- {kind} `{}` ({})", idx.name, code_list(&idx.columns));
        }

        for uni in &table.unique_constraints {
            let _ = writeln!(
                s,
                "- unique constraint `{}` ({})",
                uni.name,
                code_list(&uni.columns)
            );
        }

        for chk in &table.check_constraints {
            let _ = writeln!(s, "- check constraint `{}`: `{}`", chk.name, chk.definition);
        }
    }

    s
}

/// Formats a list of columns as Markdown code: `` `a`, `b` ``.
///
/// An expression index (`lower(name)`) has no catalog column attached to it:
/// the empty list is rendered explicitly.
fn code_list(columns: &[String]) -> String {
    if columns.is_empty() {
        return "expression".to_string();
    }
    columns
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Builds the diagram's title.
///
/// Absent `--title`, the title lists the schemas actually rendered rather
/// than the ones requested: without `--schema`, announcing "public" would be
/// wrong as soon as the database contains other schemas.
fn generate_title(args: &Args, database: &str, tables: &[Table]) -> String {
    if let Some(title) = &args.title {
        return title.clone();
    }

    let mut schemas: Vec<&str> = tables.iter().map(|t| t.schema.as_str()).collect();
    schemas.sort_unstable();
    schemas.dedup();

    match schemas.len() {
        0 => database.to_string(),
        1 => format!("{database} — {} schema", schemas[0]),
        _ => format!("{database} — {} schemas", schemas.join(", ")),
    }
}

/// Quotes an entity name if it isn't a plain Mermaid identifier.
fn quote_if_needed(raw: &str) -> String {
    let is_plain = !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !raw.starts_with(|c: char| c.is_ascii_digit());

    if is_plain {
        raw.to_string()
    } else {
        // A quoted name can't contain a double quote.
        format!("\"{}\"", raw.replace('"', "'"))
    }
}

/// Normalizes a `PostgreSQL` type into a Mermaid attribute type.
///
/// Spaces and commas (`character varying`, `numeric(10,2)`) aren't accepted
/// by the grammar and are replaced with `_`. An empty type is rendered as
/// `unknown`.
fn sanitize_type(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_underscore = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '[' | ']' | '(' | ')') {
            out.push(ch);
            pending_underscore = false;
        } else if !pending_underscore && !out.is_empty() {
            out.push('_');
            pending_underscore = true;
        }
    }

    let trimmed = out.trim_end_matches('_');
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    if trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        return format!("_{trimmed}");
    }
    trimmed.to_string()
}

/// Normalizes a column name into a Mermaid attribute identifier.
fn sanitize_ident(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if out.is_empty() {
        return "_".to_string();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Normalizes text meant for a Mermaid comment or label wrapped in quotes.
fn sanitize_comment(raw: &str) -> String {
    raw.replace('"', "'").replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Args;
    use crate::schema::{
        CheckConstraint, Column, ForeignKey, IndexInfo, TableKind, UniqueConstraint,
    };
    use clap::Parser;

    fn args() -> Args {
        Args::parse_from(["erdify", "--url", "postgresql://u:p@h/d"])
    }

    fn mock_users() -> Table {
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
                    data_type: "character varying(255)".to_string(),
                    default: None,
                },
            ],
            primary_keys: vec!["id".to_string()],
            foreign_keys: Vec::new(),
            not_null_cols: HashSet::from_iter(["id".to_string(), "email".to_string()]),
            unique_constraints: vec![UniqueConstraint {
                name: "uq_users_email".to_string(),
                columns: vec!["email".to_string()],
            }],
            check_constraints: Vec::new(),
            indexes: vec![IndexInfo {
                name: "idx_users_email".to_string(),
                columns: vec!["email".to_string()],
                is_unique: true,
            }],
        }
    }

    fn mock_orders() -> Table {
        Table {
            schema: "public".to_string(),
            name: "orders".to_string(),
            kind: TableKind::Table,
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    default: None,
                },
                Column {
                    name: "user_id".to_string(),
                    data_type: "integer".to_string(),
                    default: None,
                },
                Column {
                    name: "total".to_string(),
                    data_type: "numeric(10,2)".to_string(),
                    default: None,
                },
            ],
            primary_keys: vec!["id".to_string()],
            foreign_keys: vec![ForeignKey {
                name: "fk_orders_user".to_string(),
                from_columns: vec!["user_id".to_string()],
                to_schema: "public".to_string(),
                to_table: "users".to_string(),
                to_columns: vec!["id".to_string()],
            }],
            not_null_cols: HashSet::from_iter(["id".to_string(), "user_id".to_string()]),
            unique_constraints: Vec::new(),
            check_constraints: vec![CheckConstraint {
                name: "chk_orders_total_positive".to_string(),
                definition: "CHECK ((total > (0)::numeric))".to_string(),
            }],
            indexes: vec![IndexInfo {
                name: "idx_orders_user_id".to_string(),
                columns: vec!["user_id".to_string()],
                is_unique: false,
            }],
        }
    }

    #[test]
    fn entity_uses_mermaid_block_with_type_before_name() {
        let result = render_entity(&mock_users(), "users", OutputMode::Default);

        assert!(result.contains("    users {\n"), "{result}");
        assert!(result.contains("        integer id PK\n"), "{result}");
        assert!(result.contains("    }\n"), "{result}");
        assert!(!result.contains('['), "no bracket syntax: {result}");
    }

    #[test]
    fn minimal_mode_omits_key_markers() {
        let result = render_entity(&mock_users(), "users", OutputMode::Minimal);

        assert!(result.contains("        integer id\n"), "{result}");
        assert!(!result.contains("PK"), "{result}");
        assert!(!result.contains("not null"), "{result}");
    }

    #[test]
    fn full_mode_marks_pk_fk_uk_and_not_null() {
        let users = render_entity(&mock_users(), "users", OutputMode::Full);
        assert!(
            users.contains("        character_varying(255) email UK \"not null\"\n"),
            "{users}"
        );

        let orders = render_entity(&mock_orders(), "orders", OutputMode::Full);
        assert!(
            orders.contains("        integer user_id FK \"not null\"\n"),
            "{orders}"
        );
    }

    #[test]
    fn full_mode_shows_column_default() {
        let mut users = mock_users();
        users.not_null_cols.remove("email");
        users.columns[1].default = Some("'unknown@example.com'::character varying".to_string());

        let result = render_entity(&users, "users", OutputMode::Full);

        assert!(
            result.contains("\"default: 'unknown@example.com'::character varying\"\n"),
            "{result}"
        );
    }

    #[test]
    fn full_mode_combines_not_null_and_default() {
        let mut users = mock_users();
        users.columns[1].default = Some("'unknown'::character varying".to_string());

        let result = render_entity(&users, "users", OutputMode::Full);

        assert!(
            result.contains("\"not null, default: 'unknown'::character varying\"\n"),
            "{result}"
        );
    }

    #[test]
    fn full_mode_shows_default_on_primary_key_column() {
        let mut users = mock_users();
        users.columns[0].default = Some("nextval('users_id_seq'::regclass)".to_string());

        let result = render_entity(&users, "users", OutputMode::Full);

        assert!(
            result.contains("integer id PK \"default: nextval('users_id_seq'::regclass)\"\n"),
            "{result}"
        );
        assert!(!result.contains("not null, default"), "{result}");
    }

    #[test]
    fn default_and_minimal_mode_never_show_default() {
        let mut users = mock_users();
        users.columns[1].default = Some("'unknown'::character varying".to_string());

        let default_mode = render_entity(&users, "users", OutputMode::Default);
        let minimal_mode = render_entity(&users, "users", OutputMode::Minimal);

        assert!(!default_mode.contains("default:"), "{default_mode}");
        assert!(!minimal_mode.contains("default:"), "{minimal_mode}");
    }

    #[test]
    fn column_default_with_quotes_is_sanitized() {
        let mut users = mock_users();
        users.not_null_cols.remove("email");
        users.columns[1].default = Some("'a\"b\nc'::text".to_string());

        let result = render_entity(&users, "users", OutputMode::Full);

        assert!(result.contains("\"default: 'a'b c'::text\"\n"), "{result}");
    }

    #[test]
    fn entity_without_columns_has_no_attribute_block() {
        let table = Table {
            schema: "public".to_string(),
            name: "empty".to_string(),
            ..Table::default()
        };

        let result = render_entity(&table, "empty", OutputMode::Full);

        assert_eq!(result, "    empty\n");
    }

    #[test]
    fn relationship_uses_one_to_many_cardinality() {
        let tables = vec![mock_users(), mock_orders()];
        let names = tables
            .iter()
            .map(|t| (t.key(), t.name.clone()))
            .collect::<HashMap<_, _>>();

        let result = render_relationships(&tables, &names);

        assert_eq!(result, "    users ||--o{ orders : \"user_id\"\n");
    }

    #[test]
    fn relationship_with_nullable_fk_is_optional_on_parent_side() {
        let mut orders = mock_orders();
        orders.not_null_cols.remove("user_id");
        let tables = vec![mock_users(), orders];
        let names = tables
            .iter()
            .map(|t| (t.key(), t.name.clone()))
            .collect::<HashMap<_, _>>();

        let result = render_relationships(&tables, &names);

        assert!(result.contains("users |o--o{ orders"), "{result}");
    }

    #[test]
    fn relationship_with_unique_fk_is_one_to_one() {
        let mut orders = mock_orders();
        orders.unique_constraints.push(UniqueConstraint {
            name: "uq_orders_user".to_string(),
            columns: vec!["user_id".to_string()],
        });
        let tables = vec![mock_users(), orders];
        let names = tables
            .iter()
            .map(|t| (t.key(), t.name.clone()))
            .collect::<HashMap<_, _>>();

        let result = render_relationships(&tables, &names);

        assert!(result.contains("users ||--o| orders"), "{result}");
    }

    #[test]
    fn relationship_to_filtered_out_table_is_skipped() {
        let tables = vec![mock_orders()];
        let names = tables
            .iter()
            .map(|t| (t.key(), t.name.clone()))
            .collect::<HashMap<_, _>>();

        assert!(render_relationships(&tables, &names).is_empty());
    }

    #[test]
    fn full_mode_draws_no_relationship_for_views() {
        // Views/materialized views never carry catalog foreign keys (no such
        // constraint exists on them), so `render_relationships` naturally
        // produces no line involving them, even in --full.
        let tables = vec![mock_users(), mock_view(), mock_matview()];
        let names = tables
            .iter()
            .map(|t| (t.key(), t.name.clone()))
            .collect::<HashMap<_, _>>();

        assert!(render_relationships(&tables, &names).is_empty());
    }

    #[test]
    fn full_mode_marks_uk_on_materialized_view_column() {
        let matview = mock_matview();

        let result = render_entity(&matview, "users_summary", OutputMode::Full);

        assert!(
            result.contains("character_varying(255) email UK"),
            "{result}"
        );
    }

    #[test]
    fn default_mode_does_not_draw_relationships() {
        let tables = vec![mock_users(), mock_orders()];

        let result = render_all(&tables, OutputMode::Default, &args(), "d");

        assert!(!result.contains("--o{"), "{result}");
        assert!(result.contains("integer user_id FK"), "{result}");
    }

    #[test]
    fn render_all_wraps_diagram_in_a_mermaid_fence() {
        let tables = vec![mock_users()];

        let result = render_all(&tables, OutputMode::Minimal, &args(), "d");

        assert!(result.starts_with("# d — public schema\n\n```mermaid\nerDiagram\n"));
        assert!(result.ends_with("```\n"));
    }

    #[test]
    fn multiple_schemas_produce_quoted_qualified_names() {
        let mut audit = mock_users();
        audit.schema = "extended".to_string();
        audit.name = "audit".to_string();
        let tables = vec![mock_users(), audit];

        let result = render_all(&tables, OutputMode::Default, &args(), "d");

        assert!(result.contains("    \"public.users\" {\n"), "{result}");
        assert!(result.contains("    \"extended.audit\" {\n"), "{result}");
    }

    #[test]
    fn full_mode_lists_indexes_and_constraints_after_the_fence() {
        let tables = vec![mock_orders()];

        let result = render_all(&tables, OutputMode::Full, &args(), "d");

        let (diagram, extras) = result.split_once("\n## Indexes and constraints\n").unwrap();
        assert!(diagram.ends_with("```\n"), "{diagram}");
        assert!(
            extras.contains("- index `idx_orders_user_id` (`user_id`)"),
            "{extras}"
        );
        assert!(
            extras.contains(
                "- check constraint `chk_orders_total_positive`: `CHECK ((total > (0)::numeric))`"
            ),
            "{extras}"
        );
    }

    fn mock_view() -> Table {
        let mut view = mock_users();
        view.name = "active_users".to_string();
        view.kind = TableKind::View;
        view.primary_keys = Vec::new();
        view.unique_constraints = Vec::new();
        view.indexes = Vec::new();
        view
    }

    fn mock_matview() -> Table {
        let mut matview = mock_users();
        matview.name = "users_summary".to_string();
        matview.kind = TableKind::MaterializedView;
        matview.primary_keys = Vec::new();
        matview
    }

    #[test]
    fn views_section_lists_views_and_materialized_views_in_every_mode() {
        let tables = vec![mock_users(), mock_view(), mock_matview()];

        for mode in [OutputMode::Minimal, OutputMode::Default, OutputMode::Full] {
            let result = render_all(&tables, mode, &args(), "d");
            // Views/materialized views are rendered as regular entities in
            // the diagram itself, without flag, in every mode...
            assert!(
                result.contains("    active_users {\n"),
                "{mode:?}: {result}"
            );
            assert!(
                result.contains("    users_summary {\n"),
                "{mode:?}: {result}"
            );
            // ...and are additionally identified in the dedicated section.
            assert!(result.contains("\n## Views\n"), "{mode:?}: {result}");
            assert!(
                result.contains("- `active_users` (view)"),
                "{mode:?}: {result}"
            );
            assert!(
                result.contains("- `users_summary` (materialized view)"),
                "{mode:?}: {result}"
            );
        }
    }

    #[test]
    fn views_section_is_absent_without_any_view() {
        let tables = vec![mock_users(), mock_orders()];

        let result = render_all(&tables, OutputMode::Full, &args(), "d");

        assert!(!result.contains("## Views"), "{result}");
    }

    #[test]
    fn views_section_distinguishes_view_and_materialized_view() {
        let result = render_views_section(&[mock_view(), mock_matview()], false);

        assert!(result.contains("- `active_users` (view)\n"), "{result}");
        assert!(
            result.contains("- `users_summary` (materialized view)\n"),
            "{result}"
        );
    }

    #[test]
    fn title_lists_the_schemas_actually_rendered() {
        let mut audit = mock_users();
        audit.schema = "extended".to_string();
        audit.name = "audit".to_string();

        let result = render_all(&[mock_users(), audit], OutputMode::Default, &args(), "db");

        assert!(
            result.starts_with("# db — extended, public schemas\n"),
            "{result}"
        );
    }

    #[test]
    fn expression_index_without_columns_is_labelled() {
        let mut table = mock_users();
        table.indexes = vec![IndexInfo {
            name: "idx_users_lower_email".to_string(),
            columns: Vec::new(),
            is_unique: false,
        }];

        let result = render_extras(&[table], false);

        assert!(
            result.contains("- index `idx_users_lower_email` (expression)"),
            "{result}"
        );
    }

    #[test]
    fn custom_title_overrides_the_generated_one() {
        let args = Args::parse_from([
            "erdify",
            "--url",
            "postgresql://u:p@h/mydb",
            "--title",
            "My Custom Title",
        ]);

        let result = render_all(&[mock_users()], OutputMode::Default, &args, "mydb");

        assert!(result.starts_with("# My Custom Title\n"));
    }

    #[test]
    fn sanitize_type_normalises_postgres_types() {
        assert_eq!(sanitize_type("character varying"), "character_varying");
        assert_eq!(sanitize_type("numeric(10,2)"), "numeric(10_2)");
        assert_eq!(
            sanitize_type("timestamp without time zone"),
            "timestamp_without_time_zone"
        );
        assert_eq!(sanitize_type("integer[]"), "integer[]");
        assert_eq!(sanitize_type("\"MyEnum\""), "MyEnum");
        assert_eq!(sanitize_type(""), "unknown");
        assert_eq!(sanitize_type("   "), "unknown");
    }

    #[test]
    fn sanitize_ident_replaces_invalid_characters() {
        assert_eq!(sanitize_ident("user id"), "user_id");
        assert_eq!(sanitize_ident("2fa"), "_2fa");
        assert_eq!(sanitize_ident("café\"au"), "caf__au");
    }

    #[test]
    fn quote_if_needed_only_quotes_non_plain_names() {
        assert_eq!(quote_if_needed("users"), "users");
        assert_eq!(quote_if_needed("public.users"), "\"public.users\"");
        assert_eq!(quote_if_needed("my table"), "\"my table\"");
        assert_eq!(quote_if_needed("a\"b"), "\"a'b\"");
    }
}
