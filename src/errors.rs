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
