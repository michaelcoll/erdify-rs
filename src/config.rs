use crate::errors::ErdifyError;
use clap::Parser;
use std::env;

/// Générateur de diagrammes ER Mermaid depuis PostgreSQL.
#[derive(Parser, Debug)]
#[command(name = "erdify", version, about)]
pub struct Args {
    /// URL de connexion PostgreSQL (ex: postgresql://user:pass@host:5432/dbname)
    #[arg(short, long)]
    pub url: Option<String>,

    /// Schemas à inclure, séparés par des virgules (ex: public,extended)
    #[arg(long)]
    pub schema: Option<String>,

    /// Tables à inclure, séparées par des virgules (ex: users,orders)
    #[arg(long, conflicts_with = "ignore_tables")]
    pub table: Option<String>,

    /// Tables à exclure, séparées par des virgules (ex: logs,audit_trail)
    #[arg(long, conflicts_with = "table")]
    pub ignore_tables: Option<String>,

    /// Mode minimal : colonnes uniquement, sans métadonnées PK/FK
    #[arg(long, conflicts_with = "full")]
    pub minimal: bool,

    /// Mode complet : colonnes + PK/FK/NOT NULL + relations + contraintes + indexes
    #[arg(long, conflicts_with = "minimal")]
    pub full: bool,

    /// Fichier de sortie (par défaut : stdout)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Titre du diagramme (par défaut : extrait du nom de la BDD)
    #[arg(long)]
    pub title: Option<String>,
}

/// Information de connexion extraite d'une URL PostgreSQL.
#[derive(Debug)]
pub struct ConnectionInfo {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl Args {
    /// Parse les valeurs séparées par virgules en un vecteur de &str.
    pub fn parse_csv<'a>(&self, value: Option<&'a str>) -> Vec<&'a str> {
        match value {
            Some(v) if !v.is_empty() => v
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Détermine le mode de sortie.
    pub fn output_mode(&self) -> OutputMode {
        if self.minimal {
            OutputMode::Minimal
        } else if self.full {
            OutputMode::Full
        } else {
            OutputMode::Default
        }
    }

    /// Construit ConnectionInfo depuis --url ou DATABASE_URL.
    pub fn parse_url(&self) -> Result<ConnectionInfo, ErdifyError> {
        let url_str = match &self.url {
            Some(u) if !u.is_empty() => Some(u.clone()),
            _ => None,
        };

        let url_str = match url_str {
            Some(url) => url,
            None => match env::var("DATABASE_URL") {
                Ok(v) if !v.is_empty() => v,
                _ => {
                    return Err(ErdifyError::InvalidUrl(
                        "aucune url fournie ; utilisez --url ou la variable DATABASE_URL".into(),
                    ));
                }
            },
        };

        parse_postgres_url(&url_str)
    }
}

/// Port PostgreSQL par défaut, utilisé quand l'url n'en précise pas.
const DEFAULT_PORT: u16 = 5432;

/// Parse une URL PostgreSQL au format `postgresql://user:pass@host:port/dbname`.
fn parse_postgres_url(url: &str) -> Result<ConnectionInfo, ErdifyError> {
    let url = url::Url::parse(url)
        .map_err(|e| ErdifyError::InvalidUrl(format!("format d'url invalide : {e}")))?;

    let scheme = url.scheme();
    if scheme != "postgresql" && scheme != "postgres" && scheme != "pg" {
        return Err(ErdifyError::InvalidUrl(format!(
            "schéma d'url attendu : postgresql/postgres/pg, obtenu : {scheme}"
        )));
    }

    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| ErdifyError::InvalidUrl("pas d'hôte dans l'url".into()))?
        .to_string();

    // `port_or_known_default` ne connaît pas le scheme postgresql.
    let port = url.port().unwrap_or(DEFAULT_PORT);

    let database = url
        .path_segments()
        .and_then(|mut segs| segs.next())
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .ok_or_else(|| ErdifyError::InvalidUrl("pas de nom de base dans l'url".into()))?;

    // Les identifiants sont percent-encodés dans une url : `p%40ss` doit être
    // transmis à PostgreSQL comme `p@ss`.
    let user = percent_decode(url.username());
    let password = url.password().map(percent_decode).unwrap_or_default();

    Ok(ConnectionInfo {
        host,
        port,
        database,
        user,
        password,
    })
}

/// Décode les séquences `%XX` d'un composant d'url, en le laissant tel quel
/// si le résultat n'est pas de l'UTF-8 valide.
fn percent_decode(raw: &str) -> String {
    percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_or_else(|_| raw.to_string(), |s| s.into_owned())
}

/// Mode de sortie du diagramme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Colonnes uniquement, sans PK/FK.
    Minimal,
    /// Colonnes + PK/FK (par défaut).
    Default,
    /// Tout : colonnes + PK/FK/NOT NULL + relations + contraintes + indexes.
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_csv_empty() {
        let args = Args::parse_from(["erdify"]);
        assert!(args.parse_csv(None).is_empty());
        assert!(args.parse_csv(Some("")).is_empty());
    }

    #[test]
    fn test_parse_csv_single() {
        let args = Args::parse_from(["erdify"]);
        let result = args.parse_csv(Some("public"));
        assert_eq!(result, vec!["public"]);
    }

    #[test]
    fn test_parse_csv_multiple() {
        let args = Args::parse_from(["erdify"]);
        let result = args.parse_csv(Some("public,extended,custom"));
        assert_eq!(result, vec!["public", "extended", "custom"]);
    }

    #[test]
    fn test_parse_csv_with_spaces() {
        let args = Args::parse_from(["erdify"]);
        let result = args.parse_csv(Some(" public , extended "));
        assert_eq!(result, vec!["public", "extended"]);
    }

    #[test]
    fn test_table_and_ignore_tables_conflict() {
        let result =
            Args::try_parse_from(["erdify", "--table", "users", "--ignore-tables", "logs"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_output_mode_default() {
        let args = Args::parse_from(["erdify"]);
        assert_eq!(args.output_mode(), OutputMode::Default);
    }

    #[test]
    fn test_output_mode_minimal() {
        let args = Args::parse_from(["erdify", "--minimal"]);
        assert_eq!(args.output_mode(), OutputMode::Minimal);
    }

    #[test]
    fn test_output_mode_full() {
        let args = Args::parse_from(["erdify", "--full"]);
        assert_eq!(args.output_mode(), OutputMode::Full);
    }

    #[test]
    fn test_parse_url_valid() {
        let args = Args::parse_from([
            "erdify",
            "--url",
            "postgresql://admin:secret@localhost:5432/mydb",
        ]);
        let info = args.parse_url().unwrap();
        assert_eq!(info.host, "localhost");
        assert_eq!(info.port, 5432);
        assert_eq!(info.database, "mydb");
        assert_eq!(info.user, "admin");
        assert_eq!(info.password, "secret");
    }

    #[test]
    fn test_parse_url_default_port() {
        let args = Args::parse_from([
            "erdify",
            "--url",
            "postgresql://user@db.example.com/production",
        ]);
        let info = args.parse_url().unwrap();
        assert_eq!(info.port, 5432);
        assert_eq!(info.database, "production");
    }

    #[test]
    fn test_parse_url_percent_encoded_credentials() {
        let args = Args::parse_from([
            "erdify",
            "--url",
            "postgresql://ad%40min:p%40ss%2Fword@localhost:5432/mydb",
        ]);
        let info = args.parse_url().unwrap();
        assert_eq!(info.user, "ad@min");
        assert_eq!(info.password, "p@ss/word");
    }

    #[test]
    fn test_parse_url_missing_database() {
        let args = Args::parse_from(["erdify", "--url", "postgresql://user@localhost:5432/"]);
        assert!(args.parse_url().is_err());
    }

    #[test]
    fn test_parse_url_missing_host() {
        let args = Args::parse_from(["erdify", "--url", "postgresql:///dbname"]);
        let result = args.parse_url();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_url_no_url_no_env() {
        // Sauvegarde et restaure DATABASE_URL pour éviter les effets de bord.
        let orig = env::var("DATABASE_URL").ok();
        unsafe {
            env::remove_var("DATABASE_URL");
        }

        let args = Args::parse_from(["erdify"]);
        let result = args.parse_url();
        assert!(result.is_err());

        // Restauration.
        if let Some(val) = orig {
            unsafe {
                env::set_var("DATABASE_URL", val);
            }
        }
    }
}
