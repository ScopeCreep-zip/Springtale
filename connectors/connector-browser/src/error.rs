use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("browser launch failed: {0}")]
    LaunchFailed(String),

    #[error("navigation failed: {0}")]
    NavigationFailed(String),

    #[error("domain not allowed: {0}")]
    DomainNotAllowed(String),

    #[error("element not found: {0}")]
    ElementNotFound(String),

    #[error("screenshot failed: {0}")]
    ScreenshotFailed(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unknown action: {0}")]
    UnknownAction(String),
}

impl From<BrowserError> for springtale_connector::error::ConnectorError {
    fn from(e: BrowserError) -> Self {
        springtale_connector::error::ConnectorError::ExecutionFailed(e.to_string())
    }
}
