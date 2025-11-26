use thiserror::Error;

#[derive(Error, Debug)]
pub enum UtcpError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Authentication failed: {0}")]
    Authentication(String),
    #[error("Tool call failed: {0}")]
    ToolCall(String),
    #[error("Invalid configuration: {0}")]
    Config(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
