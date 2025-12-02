use serde::{Deserialize, Serialize};

/// v1.0 call template model (simplified to cover current transports).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CallTemplate {
    /// The type of the call template (e.g., "http", "cli").
    pub call_template_type: String,
    /// Optional name for the template.
    #[serde(default)]
    pub name: Option<String>,
    /// URL for HTTP-based templates.
    #[serde(default)]
    pub url: Option<String>,
    /// HTTP method for HTTP-based templates.
    #[serde(default)]
    pub http_method: Option<String>,
    /// Command string for CLI-based templates.
    #[serde(default)]
    pub command: Option<String>,
    /// List of commands for multi-step CLI templates.
    #[serde(default)]
    pub commands: Option<Vec<TemplateCommand>>,
    /// Environment variables to set for the command.
    #[serde(default)]
    pub env_vars: Option<std::collections::HashMap<String, String>>,
    /// Working directory for the command.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// List of allowed communication protocol types (e.g., ["http", "cli"]).
    /// If undefined, null, or empty, defaults to only allowing this template's own call_template_type.
    /// This provides secure-by-default behavior where a manual can only register/call tools
    /// that use its own protocol unless explicitly configured otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub allowed_communication_protocols: Option<Vec<String>>,
}

/// Represents a single command in a multi-step CLI template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCommand {
    /// The command string to execute.
    pub command: String,
    /// Whether to append the output of this command to the final result.
    #[serde(default)]
    pub append_to_final_output: Option<bool>,
}

/// Metadata information about a manual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualInfo {
    /// Title of the manual.
    pub title: String,
    /// Version of the manual.
    pub version: String,
    #[serde(default)]
    /// Optional description of the manual.
    pub description: Option<String>,
}

/// Represents a tool definition within a manual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualTool {
    /// Name of the tool.
    pub name: String,
    /// Description of what the tool does.
    pub description: String,
    /// JSON schema for the tool's inputs.
    pub inputs: serde_json::Value,
    /// JSON schema for the tool's outputs.
    pub outputs: serde_json::Value,
    /// Tags associated with the tool.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The call template defining how to execute the tool.
    #[serde(default)]
    pub tool_call_template: Option<CallTemplate>,
    /// Legacy provider definition (deprecated).
    #[serde(default)]
    pub provider: Option<CallTemplate>, // legacy in-tool provider
}

/// Represents a v1.0 Manual structure containing tool definitions and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualV1 {
    /// Version of the manual format.
    pub manual_version: String,
    /// Version of the UTCP protocol.
    pub utcp_version: String,
    /// Metadata about the manual.
    pub info: ManualInfo,
    /// List of tools defined in the manual.
    pub tools: Vec<ManualTool>,
    /// List of allowed communication protocol types for tools in this manual.
    /// If undefined, null, or empty, defaults to only allowing each tool's own protocol type.
    /// This provides secure-by-default behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub allowed_communication_protocols: Option<Vec<String>>,
}
