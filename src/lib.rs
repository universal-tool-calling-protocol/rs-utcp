pub mod auth;
pub mod config;
pub mod grpcpb;
pub mod loader;
pub mod plugins;
pub mod providers;
pub mod repository;
pub mod tag;
pub mod tools;
pub mod transports;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::UtcpClientConfig;
use crate::providers::base::{Provider, ProviderType};
use crate::repository::ToolRepository;
use crate::tools::{Tool, ToolSearchStrategy};
use crate::transports::stream::StreamResult;
use crate::transports::ClientTransport;

#[async_trait]
pub trait UtcpClientInterface: Send + Sync {
    async fn register_tool_provider(&self, prov: Arc<dyn Provider>) -> Result<Vec<Tool>>;
    async fn deregister_tool_provider(&self, provider_name: &str) -> Result<()>;
    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value>;
    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>>;
    fn get_transports(&self) -> HashMap<String, Arc<dyn ClientTransport>>;
    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<Box<dyn StreamResult>>;
}

pub struct UtcpClient {
    config: UtcpClientConfig,
    transports: HashMap<String, Arc<dyn ClientTransport>>,
    tool_repository: Arc<dyn ToolRepository>,
    search_strategy: Arc<dyn ToolSearchStrategy>,

    provider_tools_cache: RwLock<HashMap<String, Vec<Tool>>>,
    resolved_tools_cache: RwLock<HashMap<String, ResolvedTool>>,
}

#[derive(Clone)]
struct ResolvedTool {
    provider: Arc<dyn Provider>,
    transport: Arc<dyn ClientTransport>,
    call_name: String,
}

impl UtcpClient {
    pub fn new(
        config: UtcpClientConfig,
        repo: Arc<dyn ToolRepository>,
        strat: Arc<dyn ToolSearchStrategy>,
    ) -> Self {
        let mut transports: HashMap<String, Arc<dyn ClientTransport>> = HashMap::new();

        // Initialize all default transports
        transports.insert(
            "http".to_string(),
            Arc::new(crate::transports::http::HttpClientTransport::new()),
        );
        transports.insert(
            "cli".to_string(),
            Arc::new(crate::transports::cli::CliTransport::new()),
        );
        transports.insert(
            "websocket".to_string(),
            Arc::new(crate::transports::websocket::WebSocketTransport::new()),
        );
        transports.insert(
            "grpc".to_string(),
            Arc::new(crate::transports::grpc::GrpcTransport::new()),
        );
        transports.insert(
            "graphql".to_string(),
            Arc::new(crate::transports::graphql::GraphQLTransport::new()),
        );
        transports.insert(
            "tcp".to_string(),
            Arc::new(crate::transports::tcp::TcpTransport::new()),
        );
        transports.insert(
            "udp".to_string(),
            Arc::new(crate::transports::udp::UdpTransport::new()),
        );
        transports.insert(
            "sse".to_string(),
            Arc::new(crate::transports::sse::SseTransport::new()),
        );
        transports.insert(
            "mcp".to_string(),
            Arc::new(crate::transports::mcp::McpTransport::new()),
        );
        transports.insert(
            "webrtc".to_string(),
            Arc::new(crate::transports::webrtc::WebRtcTransport::new()),
        );
        transports.insert(
            "http_stream".to_string(),
            Arc::new(crate::transports::http_stream::StreamableHttpTransport::new()),
        );
        transports.insert(
            "text".to_string(),
            Arc::new(crate::transports::text::TextTransport::new()),
        );

        Self {
            config,
            transports,
            tool_repository: repo,
            search_strategy: strat,
            provider_tools_cache: RwLock::new(HashMap::new()),
            resolved_tools_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new UtcpClient and automatically load providers from the JSON file specified in config
    pub async fn new_with_providers(
        config: UtcpClientConfig,
        repo: Arc<dyn ToolRepository>,
        strat: Arc<dyn ToolSearchStrategy>,
    ) -> Result<Self> {
        let client = Self::new(config, repo, strat);

        // Load providers if file path is specified
        if let Some(providers_path) = &client.config.providers_file_path {
            let providers =
                crate::loader::load_providers_from_file(providers_path, &client.config).await?;

            for provider in providers {
                match client.register_tool_provider(provider).await {
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

        let (provider_name, _tool_suffix) = tool_name.split_once('.').ok_or_else(|| {
            anyhow!(
                "Invalid tool name format. Expected 'provider.tool', got: {}",
                tool_name
            )
        })?;
        if provider_name.is_empty() {
            return Err(anyhow!(
                "Invalid tool name format. Expected 'provider.tool', got: {}",
                tool_name
            ));
        }

        let prov = self
            .tool_repository
            .get_provider(provider_name)
            .await?
            .ok_or_else(|| anyhow!("Provider not found: {}", provider_name))?;
        let provider_type = prov.type_();

        let transport_key = provider_type.as_key().to_string();
        let transport = self
            .transports
            .get(&transport_key)
            .ok_or_else(|| anyhow!("No transport found for provider type: {:?}", provider_type))?
            .clone();

        let call_name = Self::call_name_for_provider(tool_name, &provider_type);
        let resolved = ResolvedTool {
            provider: prov.clone(),
            transport: transport.clone(),
            call_name,
        };

        {
            let mut cache = self.resolved_tools_cache.write().await;
            cache.insert(tool_name.to_string(), resolved.clone());
        }

        Ok(resolved)
    }
}

#[async_trait]
impl UtcpClientInterface for UtcpClient {
    async fn register_tool_provider(&self, prov: Arc<dyn Provider>) -> Result<Vec<Tool>> {
        let provider_name = prov.name();
        let provider_type = prov.type_();

        // Check cache first
        {
            let cache = self.provider_tools_cache.read().await;
            if let Some(tools) = cache.get(&provider_name) {
                return Ok(tools.clone());
            }
        }

        // Get transport for this provider type
        let transport_key = provider_type.as_key().to_string();
        let transport = self
            .transports
            .get(&transport_key)
            .ok_or_else(|| anyhow!("No transport found for provider type: {:?}", provider_type))?
            .clone();

        // Register with transport
        let tools = transport.register_tool_provider(prov.as_ref()).await?;

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
                resolved.insert(
                    tool.name.clone(),
                    ResolvedTool {
                        provider: prov.clone(),
                        transport: transport.clone(),
                        call_name,
                    },
                );
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

        // Get transport
        let provider_type = prov.type_();
        let transport_key = provider_type.as_key().to_string();
        let transport = self
            .transports
            .get(&transport_key)
            .ok_or_else(|| anyhow!("No transport found for provider type: {:?}", provider_type))?;

        // Deregister from transport
        transport.deregister_tool_provider(prov.as_ref()).await?;

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
            .transport
            .call_tool(&resolved.call_name, args, resolved.provider.as_ref())
            .await
    }

    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>> {
        self.search_strategy.search_tools(query, limit).await
    }

    fn get_transports(&self) -> HashMap<String, Arc<dyn ClientTransport>> {
        self.transports.clone()
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<Box<dyn StreamResult>> {
        let resolved = self.resolve_tool(tool_name).await?;
        resolved
            .transport
            .call_tool_stream(&resolved.call_name, args, resolved.provider.as_ref())
            .await
    }
}
