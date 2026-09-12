/// Error types for erdify.
#[derive(Debug, thiserror::Error)]
pub enum ErdifyError {
    #[error("invalid url error: {0}")]
    InvalidUrl(String),

    #[error("database connection error: {0}")]
    DatabaseConnection(String),

    #[error("query error: {0}")]
    QueryError(String),

    #[error("file write error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("no table found for the specified filters")]
    NoTablesFound,

    #[error("connection timeout (10s exceeded)")]
    ConnectionTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_url_displays_the_reason() {
        let err = ErdifyError::InvalidUrl("no host in the url".to_string());
        assert_eq!(err.to_string(), "invalid url error: no host in the url");
    }

    #[test]
    fn database_connection_displays_the_reason() {
        let err = ErdifyError::DatabaseConnection("connection refused".to_string());
        assert_eq!(
            err.to_string(),
            "database connection error: connection refused"
        );
    }

    #[test]
    fn query_error_displays_the_reason() {
        let err = ErdifyError::QueryError("unknown relation".to_string());
        assert_eq!(err.to_string(), "query error: unknown relation");
    }

    #[test]
    fn no_tables_found_has_a_fixed_message() {
        let err = ErdifyError::NoTablesFound;
        assert_eq!(err.to_string(), "no table found for the specified filters");
    }

    #[test]
    fn connection_timeout_has_a_fixed_message() {
        let err = ErdifyError::ConnectionTimeout;
        assert_eq!(err.to_string(), "connection timeout (10s exceeded)");
    }

    #[test]
    fn io_error_is_wrapped_and_displayed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: ErdifyError = io_err.into();
        assert_eq!(err.to_string(), "file write error: file not found");
    }
}
