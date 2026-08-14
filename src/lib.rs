//! Génération de diagrammes ER Mermaid depuis une base PostgreSQL.

pub mod config;
pub mod db;
pub mod errors;
pub mod mermaid;
pub mod schema;

use crate::config::Args;
use crate::errors::ErdifyError;
use std::io::Write as _;

/// Point d'entrée de l'application.
///
/// # Errors
///
/// Retourne une [ErdifyError] si l'url est invalide, si la connexion ou une
/// requête échoue, si aucune table ne correspond aux filtres, ou si l'écriture
/// du fichier de sortie échoue.
pub async fn run(args: Args) -> Result<(), ErdifyError> {
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
        // `output` se termine déjà par un saut de ligne : `print!` évite la
        // ligne vide supplémentaire qu'ajouterait `println!`.
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(output.as_bytes())?;
        stdout.flush()?;
    }

    Ok(())
}
