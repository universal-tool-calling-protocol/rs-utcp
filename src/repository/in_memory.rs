use crate::providers::base::Provider;
use crate::repository::ToolRepository;
use crate::tools::Tool;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Simple in-memory repository for tests and local usage.
pub struct InMemoryToolRepository {
    tools: RwLock<HashMap<String, Vec<Tool>>>, // provider_name -> tools
    providers: RwLock<HashMap<String, Arc<dyn Provider>>>, // provider_name -> Provider
}

impl InMemoryToolRepository {
    /// Create an empty repository instance.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            providers: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ToolRepository for InMemoryToolRepository {
    async fn save_provider_with_tools(
        &self,
        provider: Arc<dyn Provider>,
        tools: Vec<Tool>,
    ) -> Result<()> {
        let provider_name = provider.name();

        let mut providers_lock = self.providers.write().await;
        providers_lock.insert(provider_name.clone(), provider);

        let mut tools_lock = self.tools.write().await;
        tools_lock.insert(provider_name, tools);

        Ok(())
    }

    async fn get_provider(&self, name: &str) -> Result<Option<Arc<dyn Provider>>> {
        let providers = self.providers.read().await;
        Ok(providers.get(name).cloned())
    }

    async fn remove_provider(&self, name: &str) -> Result<()> {
        let mut providers = self.providers.write().await;
        if providers.remove(name).is_none() {
            return Err(anyhow!("provider not found: {}", name));
        }
        let mut tools = self.tools.write().await;
        tools.remove(name);
        Ok(())
    }

    async fn get_tools(&self) -> Result<Vec<Tool>> {
        let tools_map = self.tools.read().await;
        let mut all_tools = Vec::new();
        for tools in tools_map.values() {
            all_tools.extend(tools.clone());
        }
        Ok(all_tools)
    }

    async fn get_tools_by_provider(&self, provider_name: &str) -> Result<Vec<Tool>> {
        let tools_map = self.tools.read().await;
        match tools_map.get(provider_name) {
            Some(tools) => Ok(tools.clone()),
            None => Err(anyhow!("no tools found for provider {}", provider_name)),
        }
    }
}
