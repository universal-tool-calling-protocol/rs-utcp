use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Trait for loading configuration variables from various sources.
#[async_trait]
pub trait UtcpVariablesConfig: Send + Sync {
    /// Loads all variables from the source.
    async fn load(&self) -> Result<HashMap<String, String>>;
    /// Gets a single variable by key.
    async fn get(&self, key: &str) -> Result<String>;
}

/// Configuration for the UTCP client, including variables and provider file paths.
#[derive(Clone)]
pub struct UtcpClientConfig {
    /// Map of inline variables.
    pub variables: HashMap<String, String>,
    /// Path to the providers configuration file.
    pub providers_file_path: Option<PathBuf>,
    /// List of variable loaders to use.
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
    /// Creates a new default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the path to the providers configuration file.
    pub fn with_providers_file(mut self, path: PathBuf) -> Self {
        self.providers_file_path = Some(path);
        self
    }

    /// Adds a single variable to the configuration.
    pub fn with_variable(mut self, key: String, value: String) -> Self {
        self.variables.insert(key, value);
        self
    }

    /// Adds multiple variables to the configuration.
    pub fn with_variables(mut self, vars: HashMap<String, String>) -> Self {
        self.variables.extend(vars);
        self
    }

    /// v1.0-style helper to set manual/call template path (reuses providers_file_path).
    pub fn with_manual_path(mut self, path: PathBuf) -> Self {
        self.providers_file_path = Some(path);
        self
    }

    /// Retrieves a variable value by key, checking inline variables, loaders, and environment variables in order.
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

/// A variable loader that reads from a .env file.
pub struct DotEnvLoader {
    file_path: PathBuf,
}

impl DotEnvLoader {
    /// Creates a new DotEnvLoader for the specified file path.
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
