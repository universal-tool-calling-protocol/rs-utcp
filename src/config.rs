use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[async_trait]
pub trait UtcpVariablesConfig: Send + Sync {
    async fn load(&self) -> Result<HashMap<String, String>>;
    async fn get(&self, key: &str) -> Result<String>;
}

#[derive(Clone)]
pub struct UtcpClientConfig {
    pub variables: HashMap<String, String>,
    pub providers_file_path: Option<PathBuf>,
    pub load_variables_from: Vec<Arc<dyn UtcpVariablesConfig>>,
}

impl Default for UtcpClientConfig {
    fn default() -> Self {
        Self {
            variables: HashMap::new(),
            providers_file_path: None,
            load_variables_from: Vec::new(),
        }
    }
}

impl UtcpClientConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_providers_file(mut self, path: PathBuf) -> Self {
        self.providers_file_path = Some(path);
        self
    }

    pub fn with_variable(mut self, key: String, value: String) -> Self {
        self.variables.insert(key, value);
        self
    }

    pub fn with_variables(mut self, vars: HashMap<String, String>) -> Self {
        self.variables.extend(vars);
        self
    }

    pub async fn get_variable(&self, key: &str) -> Option<String> {
        // Check inline variables first
        if let Some(val) = self.variables.get(key) {
            return Some(val.clone());
        }

        // Check variable loaders
        for loader in &self.load_variables_from {
            if let Ok(val) = loader.get(key).await {
                return Some(val);
            }
        }

        // Check environment variables
        std::env::var(key).ok()
    }
}

// DotEnv variable loader implementation
pub struct DotEnvLoader {
    file_path: PathBuf,
}

impl DotEnvLoader {
    pub fn new(file_path: PathBuf) -> Self {
        Self { file_path }
    }
}

#[async_trait]
impl UtcpVariablesConfig for DotEnvLoader {
    async fn load(&self) -> Result<HashMap<String, String>> {
        let contents = tokio::fs::read_to_string(&self.file_path).await?;
        let mut vars = HashMap::new();

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                vars.insert(
                    key.trim().to_string(),
                    value.trim().trim_matches('"').to_string(),
                );
            }
        }

        Ok(vars)
    }

    async fn get(&self, key: &str) -> Result<String> {
        let vars = self.load().await?;
        vars.get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Variable {} not found", key))
    }
}
