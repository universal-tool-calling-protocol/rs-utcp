pub mod in_memory;

use crate::providers::base::Provider;
use crate::tools::Tool;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Persistence abstraction for storing providers and their tools.
#[async_trait]
pub trait ToolRepository: Send + Sync {
    /// Save a provider along with the full list of tools it offers.
    async fn save_provider_with_tools(
        &self,
        prov: Arc<dyn Provider>,
        tools: Vec<Tool>,
    ) -> Result<()>;
    /// Retrieve a provider by name if it exists.
    async fn get_provider(&self, name: &str) -> Result<Option<Arc<dyn Provider>>>;
    /// Remove a provider and its tools.
    async fn remove_provider(&self, name: &str) -> Result<()>;
    /// Return all registered tools across all providers.
    async fn get_tools(&self) -> Result<Vec<Tool>>;
    /// Return tools offered by a specific provider.
    async fn get_tools_by_provider(&self, provider_name: &str) -> Result<Vec<Tool>>;
}
