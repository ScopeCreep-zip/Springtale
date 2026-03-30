use thiserror::Error;

/// Errors specific to the filesystem connector.
#[derive(Debug, Error)]
pub enum FilesystemError {
    /// Filesystem watcher failed to initialize or encountered a runtime error.
    #[error("watcher failed: {0}")]
    WatcherFailed(String),

    /// The requested path is outside the configured allow-list.
    #[error("path not allowed: {0}")]
    PathNotAllowed(String),

    /// The requested path does not exist.
    #[error("path not found: {0}")]
    PathNotFound(String),

    /// A filesystem I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The trigger name is not recognized by this connector.
    #[error("unknown trigger: {0}")]
    UnknownTrigger(String),

    /// The action name is not recognized by this connector.
    #[error("unknown action: {0}")]
    UnknownAction(String),

    /// Invalid input parameters for an action.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<FilesystemError> for springtale_connector::error::ConnectorError {
    fn from(e: FilesystemError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
