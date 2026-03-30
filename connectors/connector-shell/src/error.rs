use thiserror::Error;

/// Errors specific to the shell connector.
#[derive(Debug, Error)]
pub enum ShellError {
    /// The command failed to execute or returned a non-zero exit code.
    #[error("command execution failed: {0}")]
    ExecutionFailed(String),

    /// The command is not in the allow-list.
    #[error("command not allowed: {0}")]
    CommandNotAllowed(String),

    /// The command timed out.
    #[error("command timed out after {0} seconds")]
    Timeout(u64),

    /// The action name is not recognized.
    #[error("unknown action: {0}")]
    UnknownAction(String),

    /// Invalid input parameters for an action.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// I/O error during command execution.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<ShellError> for springtale_connector::error::ConnectorError {
    fn from(e: ShellError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
