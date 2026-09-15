//! Deterministic, zero-LLM error parsing into 1-line actionable hints.

pub mod error_cleaner;
pub mod rules;

pub use error_cleaner::{CleanHint, ErrorCleaner};
pub use rules::{RULES, Rule};

/// Coarse classification of a tool failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    ForeignKeyViolation,
    UniqueViolation,
    NotNullViolation,
    MissingSchemaObject,
    MissingRecord,
    MissingKey,
    MissingArgument,
    NullReference,
    InvalidValue,
    UnresolvedReference,
    UnknownTool,
    FileNotFound,
    PermissionDenied,
    Authentication,
    RateLimited,
    HttpClientError,
    HttpServerError,
    Timeout,
    ConnectionFailure,
    MalformedJson,
    Unknown,
}

impl ErrorCategory {
    pub fn as_str(self) -> &'static str {
        use ErrorCategory::*;
        match self {
            ForeignKeyViolation => "foreign_key_violation",
            UniqueViolation => "unique_violation",
            NotNullViolation => "not_null_violation",
            MissingSchemaObject => "missing_schema_object",
            MissingRecord => "missing_record",
            MissingKey => "missing_key",
            MissingArgument => "missing_argument",
            NullReference => "null_reference",
            InvalidValue => "invalid_value",
            UnresolvedReference => "unresolved_reference",
            UnknownTool => "unknown_tool",
            FileNotFound => "file_not_found",
            PermissionDenied => "permission_denied",
            Authentication => "authentication",
            RateLimited => "rate_limited",
            HttpClientError => "http_client_error",
            HttpServerError => "http_server_error",
            Timeout => "timeout",
            ConnectionFailure => "connection_failure",
            MalformedJson => "malformed_json",
            Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
