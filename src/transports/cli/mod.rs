// CLI Transport - executes command-line tools
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::providers::base::Provider;
use crate::providers::cli::CliProvider;
use crate::tools::Tool;
use crate::transports::{stream::StreamResult, ClientTransport};

pub struct CliTransport;

impl CliTransport {
    pub fn new() -> Self {
        Self
    }

    async fn execute_command(
        &self,
        cmd_path: &str,
        args: &[String],
        env_vars: &Option<HashMap<String, String>>,
        working_dir: &Option<String>,
        stdin_input: Option<&str>,
    ) -> Result<(String, String, i32)> {
        let mut cmd = Command::new(cmd_path);
        cmd.args(args);

        // Set environment variables
        if let Some(env) = env_vars {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }

        // Set working directory
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Configure stdin/stdout/stderr
        cmd.stdin(if stdin_input.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        });
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

        // Write stdin if provided
        if let Some(input) = stdin_input {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes()).await?;
                drop(stdin); // Close stdin
            }
        }

        // Wait for completion with timeout
        let output =
            tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await??;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(1);

        Ok((stdout, stderr, exit_code))
    }

    fn format_arguments(&self, args: &HashMap<String, Value>) -> Vec<String> {
        let mut result = Vec::new();
        let mut keys: Vec<_> = args.keys().collect();
        keys.sort(); // Deterministic ordering

        for key in keys {
            if let Some(value) = args.get(key) {
                match value {
                    Value::Bool(true) => {
                        result.push(format!("--{}", key));
                    }
                    Value::Bool(false) => {
                        // Skip false booleans
                    }
                    Value::Array(arr) => {
                        for item in arr {
                            result.push(format!("--{}", key));
                            result.push(item.to_string());
                        }
                    }
                    _ => {
                        result.push(format!("--{}", key));
                        result.push(value.to_string().trim_matches('"').to_string());
                    }
                }
            }
        }

        result
    }

    fn extract_tools_from_output(&self, output: &str) -> Vec<Tool> {
        // Try to parse as UTCP manifest
        if let Ok(manifest) = serde_json::from_str::<Value>(output) {
            if let Some(tools_array) = manifest.get("tools").and_then(|v| v.as_array()) {
                let mut tools = Vec::new();
                for tool_value in tools_array {
                    if let Ok(tool) = serde_json::from_value::<Tool>(tool_value.clone()) {
                        tools.push(tool);
                    }
                }
                return tools;
            }
        }

        // Try line-by-line parsing
        let mut tools = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.starts_with('{') && line.ends_with('}') {
                if let Ok(tool) = serde_json::from_str::<Tool>(line) {
                    tools.push(tool);
                }
            }
        }

        tools
    }
}

#[async_trait]
impl ClientTransport for CliTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        let cli_prov = prov
            .as_any()
            .downcast_ref::<CliProvider>()
            .ok_or_else(|| anyhow!("Provider is not a CliProvider"))?;

        // Parse command name into command and args
        let parts: Vec<String> = cli_prov
            .command_name
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if parts.is_empty() {
            return Err(anyhow!("Empty command name"));
        }

        let cmd_path = &parts[0];
        let cmd_args = if parts.len() > 1 {
            parts[1..].to_vec()
        } else {
            Vec::new()
        };

        // Execute discovery command
        let (stdout, stderr, exit_code) = self
            .execute_command(
                cmd_path,
                &cmd_args,
                &cli_prov.env_vars,
                &cli_prov.working_dir,
                None,
            )
            .await?;

        let output = if exit_code == 0 { stdout } else { stderr };

        if output.trim().is_empty() {
            return Ok(vec![]);
        }

        Ok(self.extract_tools_from_output(&output))
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        // CLI transport is stateless
        Ok(())
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value> {
        let cli_prov = prov
            .as_any()
            .downcast_ref::<CliProvider>()
            .ok_or_else(|| anyhow!("Provider is not a CliProvider"))?;

        // Parse command name
        let parts: Vec<String> = cli_prov
            .command_name
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if parts.is_empty() {
            return Err(anyhow!("Empty command name"));
        }

        let cmd_path = &parts[0];

        // Build command: <cmd> call <provider> <tool> [--flags]
        let mut cmd_args = vec![
            "call".to_string(),
            cli_prov.base.name.clone(),
            tool_name.to_string(),
        ];
        cmd_args.extend(self.format_arguments(&args));

        // Prepare JSON input
        let input_json = serde_json::to_string(&args)?;

        // Execute command
        let (stdout, stderr, exit_code) = self
            .execute_command(
                cmd_path,
                &cmd_args,
                &cli_prov.env_vars,
                &cli_prov.working_dir,
                Some(&input_json),
            )
            .await?;

        let output = if exit_code == 0 { stdout } else { stderr };

        if output.trim().is_empty() {
            return Ok(Value::String(String::new()));
        }

        // Try to parse as JSON
        if let Ok(result) = serde_json::from_str::<Value>(&output) {
            Ok(result)
        } else {
            // Return as string if not JSON
            Ok(Value::String(output.trim().to_string()))
        }
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        Err(anyhow!("Streaming not supported by CliTransport"))
    }
}
