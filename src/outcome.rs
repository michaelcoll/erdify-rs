//! The result of a successful [`crate::run`], and how it maps to a process
//! exit code.

/// Outcome of a successful run, carrying the process exit code it maps to.
///
/// [`Outcome::SchemaChanged`], [`Outcome::LockFileMissing`] and
/// [`Outcome::Incomparable`] are produced by the `--check` flow; see
/// [`crate::lock::check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The run completed normally (diagram generated, or schema unchanged).
    Success,
    /// The introspected schema differs from the lock file.
    SchemaChanged,
    /// No lock file was found at the given path.
    LockFileMissing,
    /// The lock file and the current run cannot be compared.
    Incomparable,
}

impl Outcome {
    /// Maps the outcome to the process exit code documented in the README.
    #[must_use]
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::SchemaChanged => 2,
            Self::LockFileMissing => 3,
            Self::Incomparable => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_exits_zero() {
        assert_eq!(Outcome::Success.exit_code(), 0);
    }

    #[test]
    fn schema_changed_exits_two() {
        assert_eq!(Outcome::SchemaChanged.exit_code(), 2);
    }

    #[test]
    fn lock_file_missing_exits_three() {
        assert_eq!(Outcome::LockFileMissing.exit_code(), 3);
    }

    #[test]
    fn incomparable_exits_four() {
        assert_eq!(Outcome::Incomparable.exit_code(), 4);
    }
}
