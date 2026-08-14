/// Types d'erreur pour erdify.
#[derive(Debug, thiserror::Error)]
pub enum ErdifyError {
    #[error("erreur d'url invalide : {0}")]
    InvalidUrl(String),

    #[error("erreur de connexion à la base : {0}")]
    DatabaseConnection(String),

    #[error("erreur lors de la requête : {0}")]
    QueryError(String),

    #[error("erreur d'écriture de fichier : {0}")]
    IoError(#[from] std::io::Error),

    #[error("aucune table trouvée pour les filtres spécifiés")]
    NoTablesFound,

    #[error("timeout de connexion (10s dépassé)")]
    ConnectionTimeout,

    #[error("aucun schéma valide trouvé")]
    NoValidSchemas,
}

impl From<tokio_postgres::error::Error> for ErdifyError {
    fn from(err: tokio_postgres::error::Error) -> Self {
        Self::DatabaseConnection(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_url_displays_the_reason() {
        let err = ErdifyError::InvalidUrl("pas d'hôte dans l'url".to_string());
        assert_eq!(
            err.to_string(),
            "erreur d'url invalide : pas d'hôte dans l'url"
        );
    }

    #[test]
    fn database_connection_displays_the_reason() {
        let err = ErdifyError::DatabaseConnection("connexion refusée".to_string());
        assert_eq!(
            err.to_string(),
            "erreur de connexion à la base : connexion refusée"
        );
    }

    #[test]
    fn query_error_displays_the_reason() {
        let err = ErdifyError::QueryError("relation inconnue".to_string());
        assert_eq!(
            err.to_string(),
            "erreur lors de la requête : relation inconnue"
        );
    }

    #[test]
    fn no_tables_found_has_a_fixed_message() {
        let err = ErdifyError::NoTablesFound;
        assert_eq!(
            err.to_string(),
            "aucune table trouvée pour les filtres spécifiés"
        );
    }

    #[test]
    fn connection_timeout_has_a_fixed_message() {
        let err = ErdifyError::ConnectionTimeout;
        assert_eq!(err.to_string(), "timeout de connexion (10s dépassé)");
    }

    #[test]
    fn no_valid_schemas_has_a_fixed_message() {
        let err = ErdifyError::NoValidSchemas;
        assert_eq!(err.to_string(), "aucun schéma valide trouvé");
    }

    #[test]
    fn io_error_is_wrapped_and_displayed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "fichier introuvable");
        let err: ErdifyError = io_err.into();
        assert_eq!(
            err.to_string(),
            "erreur d'écriture de fichier : fichier introuvable"
        );
    }
}
