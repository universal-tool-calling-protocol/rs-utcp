use serde::{Deserialize, Serialize};

/// v1.0 call template model (simplified to cover current transports).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CallTemplate {
    pub call_template_type: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub http_method: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub commands: Option<Vec<TemplateCommand>>,
    #[serde(default)]
    pub env_vars: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCommand {
    pub command: String,
    #[serde(default)]
    pub append_to_final_output: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualInfo {
    pub title: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualTool {
    pub name: String,
    pub description: String,
    pub inputs: serde_json::Value,
    pub outputs: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tool_call_template: Option<CallTemplate>,
    #[serde(default)]
    pub provider: Option<CallTemplate>, // legacy in-tool provider
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualV1 {
    pub manual_version: String,
    pub utcp_version: String,
    pub info: ManualInfo,
    pub tools: Vec<ManualTool>,
}
