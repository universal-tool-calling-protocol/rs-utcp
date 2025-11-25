pub mod in_memory;

use crate::providers::base::Provider;
use crate::tools::Tool;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait ToolRepository: Send + Sync {
    async fn save_provider_with_tools(
        &self,
        prov: Arc<dyn Provider>,
        tools: Vec<Tool>,
    ) -> Result<()>;
    async fn get_provider(&self, name: &str) -> Result<Option<Arc<dyn Provider>>>;
    async fn remove_provider(&self, name: &str) -> Result<()>;
    async fn get_tools(&self) -> Result<Vec<Tool>>;
    async fn get_tools_by_provider(&self, provider_name: &str) -> Result<Vec<Tool>>;
}
