//! Error types for `luma-serde`.

use std::fmt;

use luma_syntax::Diagnostic;

/// Crate-local result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Error type used by serialization and deserialization adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Generic Serde-driven error message.
    Custom(String),
    /// Unsupported Serde data model operation/value.
    Unsupported {
        /// Human-readable name of the unsupported operation/value.
        operation: &'static str,
    },
    /// Canonical text serialization failed with a syntax-layer diagnostic.
    Diagnostic(Diagnostic),
}

impl Error {
    /// Creates an error from a custom message.
    #[must_use]
    pub fn custom(message: impl Into<String>) -> Self {
        Self::Custom(message.into())
    }

    /// Creates a standard placeholder error for behavior that has not been implemented yet.
    #[must_use]
    pub fn unsupported(operation: &'static str) -> Self {
        Self::Unsupported { operation }
    }

    /// Converts a syntax-layer diagnostic into an `luma-serde` error.
    #[must_use]
    pub fn from_diagnostic(diagnostic: Diagnostic) -> Self {
        Self::Diagnostic(diagnostic)
    }

    /// Returns `true` when the error came from unsupported Serde data.
    #[must_use]
    pub const fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }

    /// Returns `true` when the error wraps a `luma-syntax` diagnostic.
    #[must_use]
    pub const fn is_diagnostic(&self) -> bool {
        matches!(self, Self::Diagnostic(_))
    }
}

impl From<Diagnostic> for Error {
    fn from(value: Diagnostic) -> Self {
        Self::from_diagnostic(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Custom(message) => formatter.write_str(message),
            Self::Unsupported { operation } => {
                write!(formatter, "unsupported serde data: {operation}")
            }
            Self::Diagnostic(diagnostic) => {
                write!(
                    formatter,
                    "{}: {}",
                    diagnostic.code.code(),
                    diagnostic.message
                )
            }
        }
    }
}

impl std::error::Error for Error {}

impl serde::ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self::custom(msg.to_string())
    }
}

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self::custom(msg.to_string())
    }
}
