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

    fn parse_command(&self, command_name: &str) -> Result<(String, Vec<String>)> {
        let parts: Vec<String> = command_name
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if parts.is_empty() {
            return Err(anyhow!("Empty command name"));
        }

        Ok((parts[0].clone(), parts[1..].to_vec()))
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
        let (cmd_path, cmd_args) = self.parse_command(&cli_prov.command_name)?;

        // Execute discovery command
        let (stdout, stderr, exit_code) = self
            .execute_command(
                &cmd_path,
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
        let (cmd_path, mut cmd_args) = self.parse_command(&cli_prov.command_name)?;

        // Build command: <cmd> call <provider> <tool> [--flags]
        cmd_args.extend([
            "call".to_string(),
            cli_prov.base.name.clone(),
            tool_name.to_string(),
        ]);
        cmd_args.extend(self.format_arguments(&args));

        // Prepare JSON input
        let input_json = serde_json::to_string(&args)?;

        // Execute command
        let (stdout, stderr, exit_code) = self
            .execute_command(
                &cmd_path,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::{BaseProvider, ProviderType};
    use crate::providers::cli::CliProvider;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn format_arguments_handles_types_and_ordering() {
        let transport = CliTransport::new();
        let mut args = HashMap::new();
        args.insert("message".to_string(), Value::String("hello".to_string()));
        args.insert("count".to_string(), Value::Number(2.into()));
        args.insert("enabled".to_string(), Value::Bool(true));
        args.insert("skip".to_string(), Value::Bool(false));
        args.insert(
            "ids".to_string(),
            Value::Array(vec![Value::Number(1.into()), Value::Number(2.into())]),
        );

        let formatted = transport.format_arguments(&args);
        assert_eq!(
            formatted,
            vec![
                "--count",
                "2",
                "--enabled",
                "--ids",
                "1",
                "--ids",
                "2",
                "--message",
                "hello"
            ]
        );
    }

    #[test]
    fn extract_tools_from_output_parses_manifest() {
        let transport = CliTransport::new();
        let output = json!({
            "tools": [{
                "name": "example",
                "description": "example tool",
                "inputs": { "type": "object" },
                "outputs": { "type": "object" },
                "tags": []
            }]
        })
        .to_string();

        let tools = transport.extract_tools_from_output(&output);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "example");
        assert_eq!(tools[0].description, "example tool");
    }

    fn write_mock_cli(dir: &std::path::Path) -> std::path::PathBuf {
        let script_path = dir.join("mock_cli.js");
        let script = r#"#!/usr/bin/env node
const [,, mode, provider, tool, ...flags] = process.argv;
if (!mode) {
  console.log(JSON.stringify({
    tools: [{
      name: "echo",
      description: "echo tool",
      inputs: { "type": "object" },
      outputs: { "type": "object" },
      tags: []
    }]
  }));
  process.exit(0);
}

if (mode === "call") {
  let body = "";
  process.stdin.on("data", chunk => body += chunk.toString());
  process.stdin.on("end", () => {
    const args = body ? JSON.parse(body) : {};
    console.log(JSON.stringify({ provider, tool, args, flags }));
  });
}
"#;
        fs::write(&script_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }
        script_path
    }

    fn write_cli_requiring_mode_flag(dir: &std::path::Path) -> std::path::PathBuf {
        let script_path = dir.join("mock_cli_mode_flag.js");
        let script = r#"#!/usr/bin/env node
const argv = process.argv.slice(2);
const hasFlag = argv.shift() === "--cli-mode";
if (!hasFlag) {
  console.error("missing --cli-mode");
  process.exit(2);
}

const [mode, provider, tool, ...flags] = argv;
if (!mode) {
  console.log(JSON.stringify({
    tools: [{
      name: "echo",
      description: "echo tool",
      inputs: { "type": "object" },
      outputs: { "type": "object" },
      tags: []
    }]
  }));
  process.exit(0);
}

if (mode === "call") {
  let body = "";
  process.stdin.on("data", chunk => body += chunk.toString());
  process.stdin.on("end", () => {
    const args = body ? JSON.parse(body) : {};
    console.log(JSON.stringify({ provider, tool, args, flags, hadFlag: hasFlag }));
  });
}
"#;
        fs::write(&script_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }
        script_path
    }

    fn cli_provider(command: &str) -> CliProvider {
        CliProvider {
            base: BaseProvider {
                name: "cli".to_string(),
                provider_type: ProviderType::Cli,
                auth: None,
            },
            command_name: command.to_string(),
            working_dir: None,
            env_vars: None,
        }
    }

    #[tokio::test]
    async fn register_and_call_tool_via_cli_transport() {
        let dir = tempdir().unwrap();
        let script_path = write_mock_cli(dir.path());
        let command = script_path.display().to_string();

        let transport = CliTransport::new();
        let provider = cli_provider(&command);

        let tools = transport
            .register_tool_provider(&provider)
            .await
            .expect("register tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        let mut args = HashMap::new();
        args.insert("name".into(), Value::String("world".into()));
        let result = transport
            .call_tool("echo", args.clone(), &provider)
            .await
            .expect("call tool");

        assert!(
            result.get("provider").is_some(),
            "result missing provider: {}",
            result
        );
        assert_eq!(result["provider"], "cli");
        assert_eq!(result["tool"], "echo");
        assert_eq!(result["args"], json!(args));
    }

    #[tokio::test]
    async fn call_tool_respects_base_command_args() {
        let dir = tempdir().unwrap();
        let script_path = write_cli_requiring_mode_flag(dir.path());
        let command = format!("{} --cli-mode", script_path.display());

        let transport = CliTransport::new();
        let provider = cli_provider(&command);

        let tools = transport
            .register_tool_provider(&provider)
            .await
            .expect("register tools");
        assert_eq!(tools.len(), 1);

        let result = transport
            .call_tool("echo", HashMap::new(), &provider)
            .await
            .expect("call tool");

        assert_eq!(result["hadFlag"], json!(true));
    }

    #[tokio::test]
    async fn call_tool_stream_not_supported() {
        let dir = tempdir().unwrap();
        let script_path = write_mock_cli(dir.path());
        let command = format!("node {}", script_path.display());
        let transport = CliTransport::new();
        let provider = cli_provider(&command);

        let err = transport
            .call_tool_stream("echo", HashMap::new(), &provider)
            .await
            .err()
            .expect("expected streaming error");
        assert!(err.to_string().contains("Streaming not supported"));
    }
}
