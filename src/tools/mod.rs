use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputOutputSchema {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub inputs: ToolInputOutputSchema,
    pub outputs: ToolInputOutputSchema,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_response_size: Option<i64>,
    #[serde(rename = "tool_provider", skip_serializing_if = "Option::is_none")]
    pub provider: Option<serde_json::Value>,
}

#[async_trait]
pub trait ToolSearchStrategy: Send + Sync {
    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>>;
}
