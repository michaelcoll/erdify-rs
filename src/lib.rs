//! Generates Mermaid ER diagrams from a `PostgreSQL` database.

pub mod config;
pub mod db;
pub mod errors;
pub mod mermaid;
pub mod outcome;
pub mod schema;
#[cfg(test)]
mod test_support;

use crate::config::Args;
use crate::errors::ErdifyError;
use crate::outcome::Outcome;
use std::io::Write as _;

/// Application entry point.
///
/// # Errors
///
/// Returns an [`ErdifyError`] if the url is invalid, if the connection or a
/// query fails, if no table matches the filters, or if writing the output
/// file fails.
pub async fn run(args: Args) -> Result<Outcome, ErdifyError> {
    let url_info = args.parse_url()?;

    let schema_filters = args.parse_csv(args.schema.as_deref());
    let table_filters = args.parse_csv(args.table.as_deref());
    let ignore_tables = args.parse_csv(args.ignore_tables.as_deref());
    let mode = args.output_mode();

    let client = db::connect(&url_info).await?;
    let tables = db::fetch_tables(&client, &schema_filters, &table_filters, &ignore_tables).await?;

    if tables.is_empty() {
        return Err(ErdifyError::NoTablesFound);
    }

    let output = mermaid::render_all(&tables, mode, &args, &url_info.database);

    if let Some(path) = &args.output {
        tokio::fs::write(path, output).await?;
    } else {
        // `output` already ends with a newline: `print!` avoids the extra
        // blank line that `println!` would add.
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(output.as_bytes())?;
        stdout.flush()?;
    }

    Ok(Outcome::Success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::test_support::{test_connection_info, test_url};
    use clap::Parser as _;

    async fn setup_schema(schema: &str, ddl: &str) {
        let client = db::connect(&test_connection_info())
            .await
            .expect("connect to test database");
        client
            .batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}; {ddl}"
            ))
            .await
            .expect("set up test schema");
    }

    #[tokio::test]
    async fn run_returns_invalid_url_error() {
        let args = Args::parse_from(["erdify", "--url", "not-a-valid-url"]);

        let err = run(args).await.unwrap_err();

        assert!(matches!(err, ErdifyError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn run_returns_database_connection_error_when_unreachable() {
        // Port 1 is a privileged port nothing listens on: the connection is
        // refused immediately instead of hitting the 10s timeout.
        let args = Args::parse_from(["erdify", "--url", "postgresql://user@127.0.0.1:1/db"]);

        let err = run(args).await.unwrap_err();

        assert!(matches!(err, ErdifyError::DatabaseConnection(_)));
    }

    #[tokio::test]
    async fn run_returns_no_tables_found_for_empty_schema() {
        setup_schema("erdify_test_run_empty", "").await;

        let args = Args::parse_from([
            "erdify",
            "--url",
            &test_url(),
            "--schema",
            "erdify_test_run_empty",
        ]);

        let err = run(args).await.unwrap_err();

        assert!(matches!(err, ErdifyError::NoTablesFound));
    }

    #[tokio::test]
    async fn run_writes_the_diagram_to_the_requested_file() {
        setup_schema(
            "erdify_test_run_file",
            "CREATE TABLE erdify_test_run_file.widgets (id serial PRIMARY KEY, name text NOT NULL);",
        )
        .await;

        let out_path =
            std::env::temp_dir().join(format!("erdify_test_run_file_{}.md", std::process::id()));

        let args = Args::parse_from([
            "erdify",
            "--url",
            &test_url(),
            "--schema",
            "erdify_test_run_file",
            "--output",
            out_path.to_str().unwrap(),
        ]);

        let outcome = run(args).await.expect("run succeeds");
        assert_eq!(outcome, Outcome::Success);

        let content = tokio::fs::read_to_string(&out_path)
            .await
            .expect("output file written");
        assert!(content.contains("widgets"));

        tokio::fs::remove_file(&out_path).await.ok();
    }

    #[tokio::test]
    async fn run_writes_the_diagram_to_stdout() {
        setup_schema(
            "erdify_test_run_stdout",
            "CREATE TABLE erdify_test_run_stdout.gadgets (id serial PRIMARY KEY, name text NOT NULL);",
        )
        .await;

        let args = Args::parse_from([
            "erdify",
            "--url",
            &test_url(),
            "--schema",
            "erdify_test_run_stdout",
        ]);

        let outcome = run(args).await.expect("run succeeds");
        assert_eq!(outcome, Outcome::Success);
    }
}
