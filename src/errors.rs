use thiserror::Error;

/// Represents errors that can occur within the UTCP client.
#[derive(Error, Debug)]
pub enum UtcpError {
    /// Error when a requested tool is not found.
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    /// Error related to authentication failures.
    #[error("Authentication failed: {0}")]
    Authentication(String),
    /// Error occurring during a tool call execution.
    #[error("Tool call failed: {0}")]
    ToolCall(String),
    /// Error related to invalid configuration.
    #[error("Invalid configuration: {0}")]
    Config(String),
    /// Other errors wrapped by anyhow.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
