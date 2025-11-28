// Text Transport (for file-based tool definitions and execution)
use crate::providers::base::Provider;
use crate::providers::text::TextProvider;
use crate::tools::Tool;
use crate::transports::{stream::StreamResult, ClientTransport};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;

/// Transport that loads tools from a directory and executes scripts locally.
pub struct TextTransport {
    base_path: Option<PathBuf>,
}

enum ScriptKind {
    Executable,
    Node,
    Bash,
    Python,
}

impl TextTransport {
    /// Create a text transport without a default base path.
    pub fn new() -> Self {
        Self { base_path: None }
    }

    /// Configure a base directory that holds tool scripts and manifests.
    pub fn with_base_path(mut self, path: PathBuf) -> Self {
        self.base_path = Some(path);
        self
    }

    async fn load_tools_from_file(&self, path: &PathBuf) -> Result<Vec<Tool>> {
        let contents = fs::read_to_string(path).await?;

        // Try to parse as JSON array of tools
        if let Ok(tools) = serde_json::from_str::<Vec<Tool>>(&contents) {
            return Ok(tools);
        }

        // Try to parse as UTCP manifest
        if let Ok(manifest) = serde_json::from_str::<Value>(&contents) {
            if let Some(tools_array) = manifest.get("tools").and_then(|v| v.as_array()) {
                let mut tools = Vec::new();
                for tool_value in tools_array {
                    if let Ok(tool) = serde_json::from_value::<Tool>(tool_value.clone()) {
                        tools.push(tool);
                    }
                }
                return Ok(tools);
            }
        }

        Ok(vec![])
    }

    fn resolve_script(&self, base_path: &Path, tool_name: &str) -> Option<(ScriptKind, PathBuf)> {
        let candidates = [
            (ScriptKind::Executable, base_path.join(tool_name)),
            (
                ScriptKind::Node,
                base_path.join(format!("{}.js", tool_name)),
            ),
            (
                ScriptKind::Bash,
                base_path.join(format!("{}.sh", tool_name)),
            ),
            (
                ScriptKind::Python,
                base_path.join(format!("{}.py", tool_name)),
            ),
        ];

        for (kind, path) in candidates {
            if path.exists() {
                return Some((kind, path));
            }
        }
        None
    }

    fn build_command(&self, kind: ScriptKind, script_path: &Path, args_json: &str) -> Command {
        let mut cmd = match kind {
            ScriptKind::Node => {
                let mut c = Command::new("node");
                c.arg(script_path);
                c
            }
            ScriptKind::Bash => {
                let mut c = Command::new("bash");
                c.arg(script_path);
                c
            }
            ScriptKind::Python => {
                let mut c = Command::new("python3");
                c.arg(script_path);
                c
            }
            ScriptKind::Executable => Command::new(script_path),
        };
        cmd.arg(args_json);
        cmd
    }
}

#[async_trait]
impl ClientTransport for TextTransport {
    async fn register_tool_provider(&self, _prov: &dyn Provider) -> Result<Vec<Tool>> {
        // Load tools from text file
        let base_path = _prov
            .as_any()
            .downcast_ref::<TextProvider>()
            .and_then(|p| p.base_path.clone())
            .or_else(|| self.base_path.clone());

        if let Some(base_path) = base_path {
            let tools_file = base_path.join("tools.json");
            if tools_file.exists() {
                return self.load_tools_from_file(&tools_file).await;
            }
        }
        Ok(vec![])
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        Ok(())
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Value> {
        let base_path = _prov
            .as_any()
            .downcast_ref::<TextProvider>()
            .and_then(|p| p.base_path.clone())
            .or_else(|| self.base_path.clone());

        let base_path = base_path.ok_or_else(|| {
            anyhow!("Text transport requires base_path configuration to execute tools")
        })?;

        let (kind, script_path) = self
            .resolve_script(&base_path, tool_name)
            .ok_or_else(|| anyhow!("Tool script not found for '{}'", tool_name))?;

        let args_json = serde_json::to_string(&args)?;
        let mut command = self.build_command(kind, &script_path, &args_json);
        let output = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?
            .wait_with_output()
            .await?;

        if output.status.success() {
            let result_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(result) = serde_json::from_str::<Value>(&result_str) {
                return Ok(result);
            }
            return Ok(Value::String(result_str.to_string()));
        }

        let error = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "Tool execution failed ({}): {}",
            script_path.display(),
            error
        ))
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        Err(anyhow!("Streaming not supported by Text transport"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs as stdfs;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_text_transport_call_tool() {
        // Create a temporary directory
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        // Create a tool script
        let tool_script_path = base_path.join("test_tool.js");
        let mut tool_script_file = File::create(&tool_script_path).unwrap();
        writeln!(
            tool_script_file,
            "{}",
            r#"
            const args = JSON.parse(process.argv[2]);
            const result = { message: `Hello, ${args.name}!` };
            console.log(JSON.stringify(result));
            "#
        )
        .unwrap();

        // Make the script executable (on Unix-like systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&tool_script_path).unwrap().permissions();
            permissions.set_mode(0o755); // rwxr-xr-x
            std::fs::set_permissions(&tool_script_path, permissions).unwrap();
        }

        // Create a TextTransport with the base path
        let transport = TextTransport::new().with_base_path(base_path.clone());

        // Define tool arguments
        let mut args = HashMap::new();
        args.insert("name".to_string(), Value::String("World".to_string()));

        // Call the tool
        let result = transport
            .call_tool("test_tool", args, &MockProvider)
            .await
            .unwrap();

        // Assert the result
        assert_eq!(result["message"], "Hello, World!");

        // Clean up the temporary directory
        temp_dir.close().unwrap();
    }

    // Mock Provider for testing
    #[derive(Debug)]
    struct MockProvider;

    impl Provider for MockProvider {
        fn type_(&self) -> crate::providers::base::ProviderType {
            crate::providers::base::ProviderType::Http
        }

        fn name(&self) -> String {
            "mock".to_string()
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[tokio::test]
    async fn register_and_call_stream_errors() {
        let temp_dir = tempdir().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        // Write tools.json
        let tools_manifest = json!({
            "tools": [{
                "name": "sample",
                "description": "sample tool",
                "inputs": { "type": "object" },
                "outputs": { "type": "object" },
                "tags": []
            }]
        });
        stdfs::write(base_path.join("tools.json"), tools_manifest.to_string()).unwrap();

        // Write script
        let script_path = base_path.join("sample.js");
        stdfs::write(
            &script_path,
            r#"const args = JSON.parse(process.argv[2]); console.log(JSON.stringify({ ok: args.value }));"#,
        )
        .unwrap();

        let transport = TextTransport::new().with_base_path(base_path.clone());
        let tools = transport
            .register_tool_provider(&MockProvider)
            .await
            .expect("register");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "sample");

        let mut args = HashMap::new();
        args.insert("value".to_string(), Value::String("v".to_string()));
        let result = transport
            .call_tool("sample", args.clone(), &MockProvider)
            .await
            .expect("call");
        assert_eq!(result, json!({ "ok": "v" }));

        let err = transport
            .call_tool_stream("sample", args, &MockProvider)
            .await
            .err()
            .expect("stream error");
        assert!(err.to_string().contains("Streaming not supported"));
    }
}
