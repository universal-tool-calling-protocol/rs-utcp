pub mod auth;
pub mod call_templates;
pub mod config;
pub mod errors;
pub mod grpcpb;
pub mod loader;
pub mod migration;
pub mod openapi;
pub mod plugins;
pub mod providers;
pub mod repository;
pub mod spec;
pub mod tag;
pub mod tools;
pub mod transports;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::UtcpClientConfig;
use crate::errors::UtcpError;
use crate::openapi::OpenApiConverter;
use crate::providers::base::{Provider, ProviderType};
use crate::providers::http::HttpProvider;
use crate::repository::ToolRepository;
use crate::tools::{Tool, ToolSearchStrategy};
use crate::transports::registry::{
    communication_protocols_snapshot, CommunicationProtocolRegistry,
};
use crate::transports::stream::StreamResult;
use crate::transports::CommunicationProtocol;

#[async_trait]
pub trait UtcpClientInterface: Send + Sync {
    async fn register_tool_provider(&self, prov: Arc<dyn Provider>) -> Result<Vec<Tool>>;
    async fn register_tool_provider_with_tools(
        &self,
        prov: Arc<dyn Provider>,
        tools: Vec<Tool>,
    ) -> Result<Vec<Tool>>;
    async fn deregister_tool_provider(&self, provider_name: &str) -> Result<()>;
    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value>;
    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>>;
    fn get_transports(&self) -> HashMap<String, Arc<dyn CommunicationProtocol>>;
    fn get_communication_protocols(&self) -> HashMap<String, Arc<dyn CommunicationProtocol>> {
        self.get_transports()
    }
    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<Box<dyn StreamResult>>;
}

pub struct UtcpClient {
    config: UtcpClientConfig,
    communication_protocols: CommunicationProtocolRegistry,
    tool_repository: Arc<dyn ToolRepository>,
    search_strategy: Arc<dyn ToolSearchStrategy>,

    provider_tools_cache: RwLock<HashMap<String, Vec<Tool>>>,
    resolved_tools_cache: RwLock<HashMap<String, ResolvedTool>>,
}

#[derive(Clone)]
struct ResolvedTool {
    provider: Arc<dyn Provider>,
    protocol: Arc<dyn CommunicationProtocol>,
    call_name: String,
}

impl UtcpClient {
    /// v1.0-style async factory for symmetry with other language SDKs
    pub async fn create(
        config: UtcpClientConfig,
        repo: Arc<dyn ToolRepository>,
        strat: Arc<dyn ToolSearchStrategy>,
    ) -> Result<Self> {
        Self::new(config, repo, strat).await
    }

    /// Create a new UtcpClient and automatically load providers from the JSON file specified in config
    pub async fn new(
        config: UtcpClientConfig,
        repo: Arc<dyn ToolRepository>,
        strat: Arc<dyn ToolSearchStrategy>,
    ) -> Result<Self> {
        let communication_protocols = communication_protocols_snapshot();

        let client = Self {
            config,
            communication_protocols,
            tool_repository: repo,
            search_strategy: strat,
            provider_tools_cache: RwLock::new(HashMap::new()),
            resolved_tools_cache: RwLock::new(HashMap::new()),
        };

        // Load providers if file path is specified
        if let Some(providers_path) = &client.config.providers_file_path {
            let providers =
                crate::loader::load_providers_with_tools_from_file(providers_path, &client.config)
                    .await?;

            for loaded in providers {
                let result = if let Some(tools) = loaded.tools {
                    client
                        .register_tool_provider_with_tools(loaded.provider.clone(), tools)
                        .await
                } else {
                    client.register_tool_provider(loaded.provider.clone()).await
                };

                match result {
                    Ok(tools) => {
                        println!("✓ Loaded provider with {} tools", tools.len());
                    }
                    Err(e) => {
                        eprintln!("✗ Failed to load provider: {}", e);
                    }
                }
            }
        }

        Ok(client)
    }

    fn call_name_for_provider(tool_name: &str, provider_type: &ProviderType) -> String {
        match provider_type {
            ProviderType::Mcp | ProviderType::Text => tool_name
                .splitn(2, '.')
                .nth(1)
                .unwrap_or(tool_name)
                .to_string(),
            _ => tool_name.to_string(),
        }
    }

    async fn resolve_tool(&self, tool_name: &str) -> Result<ResolvedTool> {
        {
            let cache = self.resolved_tools_cache.read().await;
            if let Some(resolved) = cache.get(tool_name) {
                return Ok(resolved.clone());
            }
        }

        // Legacy qualified name flow
        if let Some((provider_name, suffix)) = tool_name.split_once('.') {
            if provider_name.is_empty() {
                return Err(UtcpError::Config(format!("Invalid tool name: {}", tool_name)).into());
            }

            let prov = self
                .tool_repository
                .get_provider(provider_name)
                .await?
                .ok_or_else(|| UtcpError::ToolNotFound(provider_name.to_string()))?;
            let provider_type = prov.type_();

            let protocol_key = provider_type.as_key().to_string();
            let protocol = self
                .communication_protocols
                .get(&protocol_key)
                .ok_or_else(|| {
                    UtcpError::Config(format!(
                        "No communication protocol found for provider type: {:?}",
                        provider_type
                    ))
                })?
                .clone();

            let call_name = Self::call_name_for_provider(tool_name, &provider_type);
            let resolved = ResolvedTool {
                provider: prov.clone(),
                protocol: protocol.clone(),
                call_name,
            };

            let mut cache = self.resolved_tools_cache.write().await;
            cache.insert(tool_name.to_string(), resolved.clone());
            cache.insert(suffix.to_string(), resolved.clone());
            return Ok(resolved);
        }

        // v1.0 bare tool names: search cached provider tools
        {
            let cache = self.provider_tools_cache.read().await;
            for (prov_name, tools) in cache.iter() {
                if tools.iter().any(|t| {
                    t.name
                        .split_once('.')
                        .map(|(_, suffix)| suffix == tool_name)
                        .unwrap_or(false)
                }) {
                    let prov = self
                        .tool_repository
                        .get_provider(prov_name)
                        .await?
                        .ok_or_else(|| UtcpError::ToolNotFound(prov_name.clone()))?;
                    let provider_type = prov.type_();
                    let protocol_key = provider_type.as_key().to_string();
                    let protocol = self
                        .communication_protocols
                        .get(&protocol_key)
                        .ok_or_else(|| {
                            UtcpError::Config(format!(
                                "No communication protocol found for provider type: {:?}",
                                provider_type
                            ))
                        })?
                        .clone();

                    let full_name = format!("{}.{}", prov_name, tool_name);
                    let call_name = Self::call_name_for_provider(&full_name, &provider_type);
                    let resolved = ResolvedTool {
                        provider: prov.clone(),
                        protocol: protocol.clone(),
                        call_name,
                    };

                    let mut rcache = self.resolved_tools_cache.write().await;
                    rcache.insert(full_name, resolved.clone());
                    rcache.insert(tool_name.to_string(), resolved.clone());
                    return Ok(resolved);
                }
            }
        }

        Err(UtcpError::ToolNotFound(tool_name.to_string()).into())
    }
}

#[async_trait]
impl UtcpClientInterface for UtcpClient {
    async fn register_tool_provider(&self, prov: Arc<dyn Provider>) -> Result<Vec<Tool>> {
        self.register_tool_provider_with_tools(prov, Vec::new())
            .await
    }

    async fn register_tool_provider_with_tools(
        &self,
        prov: Arc<dyn Provider>,
        tools_override: Vec<Tool>,
    ) -> Result<Vec<Tool>> {
        let provider_name = prov.name();
        let provider_type = prov.type_();

        // Check cache first
        {
            let cache = self.provider_tools_cache.read().await;
            if let Some(tools) = cache.get(&provider_name) {
                return Ok(tools.clone());
            }
        }

        // Get communication protocol for this provider type
        let protocol_key = provider_type.as_key().to_string();
        let protocol = self
            .communication_protocols
            .get(&protocol_key)
            .ok_or_else(|| {
                anyhow!(
                    "No communication protocol found for provider type: {:?}",
                    provider_type
                )
            })?
            .clone();

        // Register with protocol
        let tools = if !tools_override.is_empty() {
            tools_override
        } else if provider_type == ProviderType::Http {
            if let Some(http_prov) = prov.as_any().downcast_ref::<HttpProvider>() {
                match OpenApiConverter::new_from_url(&http_prov.url, Some(provider_name.clone()))
                    .await
                {
                    Ok(converter) => {
                        let manual = converter.convert();
                        if manual.tools.is_empty() {
                            protocol.register_tool_provider(prov.as_ref()).await?
                        } else {
                            manual.tools
                        }
                    }
                    Err(_) => protocol.register_tool_provider(prov.as_ref()).await?,
                }
            } else {
                protocol.register_tool_provider(prov.as_ref()).await?
            }
        } else {
            protocol.register_tool_provider(prov.as_ref()).await?
        };

        // Normalize tool names (prefix with provider name)
        let mut normalized_tools = Vec::new();
        for mut tool in tools {
            if !tool.name.starts_with(&format!("{}.", provider_name)) {
                tool.name = format!("{}.{}", provider_name, tool.name.trim_start_matches('.'));
            }
            normalized_tools.push(tool);
        }

        // Save to repository
        self.tool_repository
            .save_provider_with_tools(prov.clone(), normalized_tools.clone())
            .await?;

        // Update cache
        {
            let mut cache = self.provider_tools_cache.write().await;
            cache.insert(provider_name, normalized_tools.clone());
        }

        {
            let mut resolved = self.resolved_tools_cache.write().await;
            for tool in &normalized_tools {
                let call_name = Self::call_name_for_provider(&tool.name, &provider_type);
                let resolved_entry = ResolvedTool {
                    provider: prov.clone(),
                    protocol: protocol.clone(),
                    call_name,
                };

                // Full name
                resolved.insert(tool.name.clone(), resolved_entry.clone());

                // Bare name (v1.0 style)
                if let Some((_, bare)) = tool.name.split_once('.') {
                    resolved.insert(bare.to_string(), resolved_entry);
                }
            }
        }

        Ok(normalized_tools)
    }

    async fn deregister_tool_provider(&self, provider_name: &str) -> Result<()> {
        // Get provider from repository
        let prov = self
            .tool_repository
            .get_provider(provider_name)
            .await?
            .ok_or_else(|| anyhow!("Provider not found: {}", provider_name))?;

        // Get communication protocol
        let provider_type = prov.type_();
        let protocol_key = provider_type.as_key().to_string();
        let protocol = self
            .communication_protocols
            .get(&protocol_key)
            .ok_or_else(|| {
                anyhow!(
                    "No communication protocol found for provider type: {:?}",
                    provider_type
                )
            })?;

        // Deregister from protocol
        protocol.deregister_tool_provider(prov.as_ref()).await?;

        // Remove from repository
        self.tool_repository.remove_provider(provider_name).await?;

        // Clear cache
        {
            let mut cache = self.provider_tools_cache.write().await;
            cache.remove(provider_name);
        }
        {
            let mut resolved = self.resolved_tools_cache.write().await;
            resolved.retain(|tool_name, _| !tool_name.starts_with(&format!("{}.", provider_name)));
        }

        Ok(())
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let resolved = self.resolve_tool(tool_name).await?;
        resolved
            .protocol
            .call_tool(&resolved.call_name, args, resolved.provider.as_ref())
            .await
    }

    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>> {
        self.search_strategy.search_tools(query, limit).await
    }

    fn get_transports(&self) -> HashMap<String, Arc<dyn CommunicationProtocol>> {
        self.communication_protocols.as_map()
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<Box<dyn StreamResult>> {
        let resolved = self.resolve_tool(tool_name).await?;
        resolved
            .protocol
            .call_tool_stream(&resolved.call_name, args, resolved.provider.as_ref())
            .await
    }
}
