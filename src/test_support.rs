//! Helpers shared by the integration tests that need a real `PostgreSQL`
//! instance (see `db.rs` and `lib.rs`).
//!
//! Connection details default to the `erdify-test-db` container started by
//! `mise run test-db-up` (a dependency of `mise run test` and
//! `mise run coverage-backend`) and can be overridden with `TEST_DB_*` env
//! vars, which is how CI points the tests at the `postgres` service
//! container instead.
#![cfg(test)]

use crate::config::ConnectionInfo;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Connection info for the `PostgreSQL` instance used by integration tests.
pub(crate) fn test_connection_info() -> ConnectionInfo {
    ConnectionInfo {
        host: env_or("TEST_DB_HOST", "localhost"),
        port: env_or("TEST_DB_PORT", "5433").parse().expect("valid port"),
        database: env_or("TEST_DB_NAME", "postgres"),
        user: env_or("TEST_DB_USER", "postgres"),
        password: env_or("TEST_DB_PASSWORD", "password"),
    }
}

/// Same connection info as [`test_connection_info`], as a `postgresql://` url.
pub(crate) fn test_url() -> String {
    let info = test_connection_info();
    format!(
        "postgresql://{}:{}@{}:{}/{}",
        info.user, info.password, info.host, info.port, info.database
    )
}
