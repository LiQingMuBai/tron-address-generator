use std::path::PathBuf;

/// Application-level error type. All fallible entry points return [`Result`].
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested configuration file does not exist on disk.
    #[error("config file not found: {0}")]
    ConfigNotFound(PathBuf),

    /// The configuration file could not be read.
    #[error("failed to read config file '{path}': {source}")]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The XML document is malformed or missing required structural elements.
    #[error("failed to parse XML config: {0}")]
    ConfigParse(String),

    /// A mandatory element is absent.
    #[error("missing required element: {0}")]
    MissingField(String),

    /// An element is present but holds an unacceptable value.
    #[error("invalid value for '{field}': {reason}")]
    InvalidValue { field: String, reason: String },

    /// A value is syntactically valid but not supported by this generator.
    #[error("unsupported value for '{field}': {value}")]
    Unsupported { field: String, value: String },

    /// Writing the exported result file failed.
    #[error("failed to write export file '{path}': {source}")]
    Export {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON serialization of the results failed.
    #[error("failed to serialize results: {0}")]
    Serialize(#[from] serde_json::Error),

    /// An HTTP request carried an unacceptable payload.
    #[error("invalid request: {0}")]
    BadRequest(String),

    /// Starting or running the embedded web server failed.
    #[error("web server error: {0}")]
    WebServer(String),

    /// Delivering a Telegram notification failed.
    #[error("telegram notification failed: {0}")]
    Telegram(String),
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, AppError>;
