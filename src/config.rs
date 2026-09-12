use crate::errors::ErdifyError;
use clap::Parser;
use std::borrow::Cow;
use std::env;

/// `Mermaid` ER diagram generator from `PostgreSQL`.
#[derive(Parser, Debug)]
#[command(name = "erdify", version, about)]
pub struct Args {
    /// `PostgreSQL` connection URL (ex: `postgresql://user:pass@host:5432/dbname`)
    #[arg(short, long)]
    pub url: Option<String>,

    /// Schemas to include, comma-separated (ex: public,extended)
    #[arg(long)]
    pub schema: Option<String>,

    /// Tables to include, comma-separated (ex: `users,orders`)
    #[arg(long, conflicts_with = "ignore_tables")]
    pub table: Option<String>,

    /// Tables to exclude, comma-separated (ex: `logs,audit_trail`)
    #[arg(long, conflicts_with = "table")]
    pub ignore_tables: Option<String>,

    /// Minimal mode: columns only, without PK/FK metadata
    #[arg(long, conflicts_with = "full")]
    pub minimal: bool,

    /// Full mode: columns + PK/FK/NOT NULL + relationships + constraints + indexes
    #[arg(long, conflicts_with = "minimal")]
    pub full: bool,

    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Diagram title (default: derived from the database name)
    #[arg(long)]
    pub title: Option<String>,

    /// Write the schema hash to a lock file (default path: `./erdify.lock`)
    #[arg(long, num_args = 0..=1, default_missing_value = crate::lock::DEFAULT_LOCK_PATH)]
    pub lock: Option<String>,
}

/// Connection information extracted from a `PostgreSQL` URL.
#[derive(Debug)]
pub struct ConnectionInfo {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl Args {
    /// Parses comma-separated values into a vector of &str.
    #[must_use]
    pub fn parse_csv<'a>(&self, value: Option<&'a str>) -> Vec<&'a str> {
        match value {
            Some(v) if !v.is_empty() => v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Determines the output mode.
    #[must_use]
    pub const fn output_mode(&self) -> OutputMode {
        if self.minimal {
            OutputMode::Minimal
        } else if self.full {
            OutputMode::Full
        } else {
            OutputMode::Default
        }
    }

    /// Builds `ConnectionInfo` from `--url` or `DATABASE_URL`.
    ///
    /// # Errors
    ///
    /// Returns [`ErdifyError::InvalidUrl`] if no URL is provided and
    /// `DATABASE_URL` is unset or empty, or if the URL can't be parsed.
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
                        "no url provided; use --url or the DATABASE_URL variable".into(),
                    ));
                }
            },
        };

        parse_postgres_url(&url_str)
    }
}

/// Default `PostgreSQL` port, used when the url doesn't specify one.
const DEFAULT_PORT: u16 = 5432;

/// Parses a `PostgreSQL` URL in the `postgresql://user:pass@host:port/dbname` format.
fn parse_postgres_url(url: &str) -> Result<ConnectionInfo, ErdifyError> {
    let url = url::Url::parse(url)
        .map_err(|e| ErdifyError::InvalidUrl(format!("invalid url format: {e}")))?;

    let scheme = url.scheme();
    if scheme != "postgresql" && scheme != "postgres" && scheme != "pg" {
        return Err(ErdifyError::InvalidUrl(format!(
            "expected url scheme: postgresql/postgres/pg, got: {scheme}"
        )));
    }

    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| ErdifyError::InvalidUrl("no host in the url".into()))?
        .to_string();

    // `port_or_known_default` doesn't know the postgresql scheme.
    let port = url.port().unwrap_or(DEFAULT_PORT);

    let database = url
        .path_segments()
        .and_then(|mut segs| segs.next())
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .ok_or_else(|| ErdifyError::InvalidUrl("no database name in the url".into()))?;

    // Credentials are percent-encoded within a url: `p%40ss` must be
    // passed to PostgreSQL as `p@ss`.
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

/// Decodes the `%XX` sequences of a url component, leaving it unchanged
/// if the result isn't valid UTF-8.
fn percent_decode(raw: &str) -> String {
    percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_or_else(|_| raw.to_string(), Cow::into_owned)
}

/// Diagram output mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Columns only, without PK/FK.
    Minimal,
    /// Columns + PK/FK (default).
    Default,
    /// Everything: columns + PK/FK/NOT NULL + relationships + constraints + indexes.
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
    fn test_lock_absent_by_default() {
        let args = Args::parse_from(["erdify"]);
        assert_eq!(args.lock, None);
    }

    #[test]
    fn test_lock_defaults_path_when_given_no_value() {
        let args = Args::parse_from(["erdify", "--lock"]);
        assert_eq!(args.lock.as_deref(), Some("erdify.lock"));
    }

    #[test]
    fn test_lock_uses_given_path() {
        let args = Args::parse_from(["erdify", "--lock", "custom/schema.lock"]);
        assert_eq!(args.lock.as_deref(), Some("custom/schema.lock"));
    }

    #[test]
    fn test_parse_url_no_url_no_env() {
        // Save and restore DATABASE_URL to avoid side effects.
        let orig = env::var("DATABASE_URL").ok();
        // SAFETY: no other test reads `DATABASE_URL` while it is removed, and
        // the original value is restored below.
        unsafe {
            env::remove_var("DATABASE_URL");
        }

        let args = Args::parse_from(["erdify"]);
        let result = args.parse_url();
        assert!(result.is_err());

        // Restore.
        if let Some(val) = orig {
            // SAFETY: restores the environment to its original state; no
            // concurrent reader of `DATABASE_URL` exists in the test suite.
            unsafe {
                env::set_var("DATABASE_URL", val);
            }
        }
    }

    #[test]
    fn test_parse_url_falls_back_to_env_var() {
        let orig = env::var("DATABASE_URL").ok();
        // SAFETY: no other test reads `DATABASE_URL` while it is set, and
        // the original value is restored below.
        unsafe {
            env::set_var("DATABASE_URL", "postgresql://user@localhost:5432/from_env");
        }

        let args = Args::parse_from(["erdify"]);
        let info = args.parse_url().unwrap();
        assert_eq!(info.database, "from_env");

        // Restore.
        unsafe {
            match &orig {
                Some(val) => env::set_var("DATABASE_URL", val),
                None => env::remove_var("DATABASE_URL"),
            }
        }
    }

    #[test]
    fn test_parse_url_rejects_unknown_scheme() {
        let args = Args::parse_from(["erdify", "--url", "mysql://user@localhost:5432/mydb"]);
        let err = args.parse_url().unwrap_err();
        assert!(matches!(err, ErdifyError::InvalidUrl(_)));
    }
}
