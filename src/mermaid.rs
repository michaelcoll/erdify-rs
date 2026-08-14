//! Rendu du schéma en `erDiagram` Mermaid.
//!
//! La sortie est un document Markdown contenant un bloc ```` ```mermaid ````
//! valide : entités déclarées avec `NOM { type colonne CLÉS "commentaire" }`
//! et relations au format `PARENT ||--o{ ENFANT : "label"`.

use crate::config::{Args, OutputMode};
use crate::schema::Table;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Indentation d'une entité dans le bloc `erDiagram`.
const ENTITY_INDENT: &str = "    ";
/// Indentation d'un attribut dans le bloc d'une entité.
const ATTR_INDENT: &str = "        ";

/// Clé d'identification d'une table : `(schema, nom)`.
type TableKey<'a> = (&'a str, &'a str);

/// Génère le document Markdown complet (titre + bloc Mermaid) pour toutes les tables.
#[must_use]
pub fn render_all(tables: &[Table], mode: OutputMode, args: &Args, database: &str) -> String {
    let mut output = String::new();

    let title = generate_title(args, database, tables);
    let _ = writeln!(output, "# {title}\n");

    // Les noms d'entités ne sont préfixés par le schema que si plusieurs
    // schemas coexistent : sinon `users` est plus lisible que `public.users`.
    let qualify = uses_multiple_schemas(tables);
    let names: HashMap<TableKey<'_>, String> = tables
        .iter()
        .map(|t| (t.key(), entity_name(t, qualify)))
        .collect();

    output.push_str("```mermaid\nerDiagram\n");

    for table in tables {
        let name = names.get(&table.key()).expect("entité enregistrée");
        output.push_str(&render_entity(table, name, mode));
    }

    // Les relations ne sont dessinées qu'en mode complet ; en mode défaut les
    // FK sont uniquement marquées sur les colonnes.
    if mode == OutputMode::Full {
        output.push_str(&render_relationships(tables, &names));
    }

    output.push_str("```\n");

    if mode == OutputMode::Full {
        output.push_str(&render_extras(tables, qualify));
    }

    output
}

/// Indique si les tables proviennent de plusieurs schemas.
fn uses_multiple_schemas(tables: &[Table]) -> bool {
    let mut schemas = tables.iter().map(|t| t.schema.as_str());
    let Some(first) = schemas.next() else {
        return false;
    };
    schemas.any(|s| s != first)
}

/// Construit le nom d'entité Mermaid d'une table, en le quotant si nécessaire.
fn entity_name(table: &Table, qualify: bool) -> String {
    let raw = if qualify {
        format!("{}.{}", table.schema, table.name)
    } else {
        table.name.clone()
    };
    quote_if_needed(&raw)
}

/// Génère le bloc d'une seule entité.
fn render_entity(table: &Table, name: &str, mode: OutputMode) -> String {
    let mut s = String::new();

    // Une entité sans colonne se déclare sans bloc d'attributs : un bloc vide
    // n'est pas accepté par la grammaire Mermaid.
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

        // NOT NULL est redondant avec PK, on ne l'affiche que pour les autres.
        if mode == OutputMode::Full
            && !pk.contains(col_name)
            && table.not_null_cols.contains(col_name)
        {
            s.push_str(" \"not null\"");
        }

        s.push('\n');
    }

    let _ = writeln!(s, "{ENTITY_INDENT}}}");
    s
}

/// Colonnes couvertes par une contrainte ou un index UNIQUE mono-colonne.
///
/// Les contraintes multi-colonnes sont ignorées : marquer chaque colonne `UK`
/// affirmerait à tort que chacune est unique isolément.
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

/// Génère les relations Mermaid entre les tables avec cardinalités déduites.
fn render_relationships(tables: &[Table], names: &HashMap<TableKey<'_>, String>) -> String {
    let mut s = String::new();
    let mut seen: HashSet<(&str, &str, String)> = HashSet::new();

    for table in tables {
        let Some(child) = names.get(&table.key()) else {
            continue;
        };

        for fk in &table.foreign_keys {
            // Une FK pointant hors du périmètre filtré n'a pas d'entité cible :
            // la dessiner créerait une entité fantôme dans le diagramme.
            let Some(parent) = names.get(&(fk.to_schema.as_str(), fk.to_table.as_str())) else {
                continue;
            };

            let label = sanitize_comment(&fk.from_columns.join(", "));
            if !seen.insert((parent.as_str(), child.as_str(), label.clone())) {
                continue;
            }

            // Côté parent : la ligne enfant peut exister sans parent si une des
            // colonnes de la FK est nullable.
            let all_not_null = fk
                .from_columns
                .iter()
                .all(|c| table.not_null_cols.contains(c));
            let left = if all_not_null { "||" } else { "|o" };

            // Côté enfant : au plus une ligne si la FK est elle-même unique (1:1).
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

/// Indique si l'ensemble de colonnes est couvert par une contrainte ou un index UNIQUE.
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

/// Génère la section Markdown listant indexes et contraintes (mode complet).
///
/// La grammaire `erDiagram` ne connaît pas les notes : ces informations sont
/// rendues en Markdown, sous le bloc Mermaid, pour rester affichables.
fn render_extras(tables: &[Table], qualify: bool) -> String {
    let mut s = String::new();

    let has_extras = tables.iter().any(|t| {
        !t.indexes.is_empty() || !t.unique_constraints.is_empty() || !t.check_constraints.is_empty()
    });
    if !has_extras {
        return s;
    }

    s.push_str("\n## Indexes et contraintes\n");

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
                "index unique"
            } else {
                "index"
            };
            let _ = writeln!(s, "- {kind} `{}` ({})", idx.name, code_list(&idx.columns));
        }

        for uni in &table.unique_constraints {
            let _ = writeln!(
                s,
                "- contrainte unique `{}` ({})",
                uni.name,
                code_list(&uni.columns)
            );
        }

        for chk in &table.check_constraints {
            let _ = writeln!(
                s,
                "- contrainte check `{}` : `{}`",
                chk.name, chk.definition
            );
        }
    }

    s
}

/// Formate une liste de colonnes en code Markdown : `` `a`, `b` ``.
///
/// Un index sur expression (`lower(name)`) n'a aucune colonne de catalogue
/// rattachée : la liste vide est rendue explicitement.
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

/// Construit le titre du diagramme.
///
/// À défaut de `--title`, le titre liste les schemas réellement représentés
/// plutôt que ceux demandés : sans `--schema`, annoncer « public » serait faux
/// dès que la base contient d'autres schemas.
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

/// Quote un nom d'entité s'il n'est pas un identifiant Mermaid simple.
fn quote_if_needed(raw: &str) -> String {
    let is_plain = !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !raw.starts_with(|c: char| c.is_ascii_digit());

    if is_plain {
        raw.to_string()
    } else {
        // Un nom quoté ne peut pas contenir de guillemet double.
        format!("\"{}\"", raw.replace('"', "'"))
    }
}

/// Normalise un type PostgreSQL en type d'attribut Mermaid.
///
/// Les espaces et virgules (`character varying`, `numeric(10,2)`) ne sont pas
/// acceptés par la grammaire et sont remplacés par des `_`. Un type vide est
/// rendu `unknown`.
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

/// Normalise un nom de colonne en identifiant d'attribut Mermaid.
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

/// Normalise un texte destiné à un commentaire ou un label Mermaid entre guillemets.
fn sanitize_comment(raw: &str) -> String {
    raw.replace('"', "'").replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Args;
    use crate::schema::{CheckConstraint, Column, ForeignKey, IndexInfo, UniqueConstraint};
    use clap::Parser;

    fn args() -> Args {
        Args::parse_from(["erdify", "--url", "postgresql://u:p@h/d"])
    }

    fn mock_users() -> Table {
        Table {
            schema: "public".to_string(),
            name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                },
                Column {
                    name: "email".to_string(),
                    data_type: "character varying(255)".to_string(),
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
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                },
                Column {
                    name: "user_id".to_string(),
                    data_type: "integer".to_string(),
                },
                Column {
                    name: "total".to_string(),
                    data_type: "numeric(10,2)".to_string(),
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
        assert!(!result.contains('['), "pas de syntaxe crochets : {result}");
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

        let (diagram, extras) = result.split_once("\n## Indexes et contraintes\n").unwrap();
        assert!(diagram.ends_with("```\n"), "{diagram}");
        assert!(
            extras.contains("- index `idx_orders_user_id` (`user_id`)"),
            "{extras}"
        );
        assert!(
            extras.contains(
                "- contrainte check `chk_orders_total_positive` : `CHECK ((total > (0)::numeric))`"
            ),
            "{extras}"
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
        assert_eq!(sanitize_ident("crée\"le"), "cr_e_le");
    }

    #[test]
    fn quote_if_needed_only_quotes_non_plain_names() {
        assert_eq!(quote_if_needed("users"), "users");
        assert_eq!(quote_if_needed("public.users"), "\"public.users\"");
        assert_eq!(quote_if_needed("my table"), "\"my table\"");
        assert_eq!(quote_if_needed("a\"b"), "\"a'b\"");
    }
}
