use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{anyhow, Result};
use rhai::{Dynamic, Engine, EvalAltResult, Map, Scope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::runtime::{Builder, RuntimeFlavor};

use crate::tools::{Tool, ToolInputOutputSchema};
use crate::UtcpClientInterface;

pub struct CodeModeUtcp {
    client: Arc<dyn UtcpClientInterface>,
}

impl CodeModeUtcp {
    pub fn new(client: Arc<dyn UtcpClientInterface>) -> Self {
        Self { client }
    }

    pub async fn execute(&self, args: CodeModeArgs) -> Result<CodeModeResult> {
        // If it's JSON already, return it directly.
        if let Ok(json) = serde_json::from_str::<Value>(&args.code) {
            return Ok(CodeModeResult {
                value: json,
                stdout: String::new(),
                stderr: String::new(),
            });
        }

        let value = self.eval_rusty_snippet(&args.code, args.timeout).await?;
        Ok(CodeModeResult {
            value,
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    fn tool_schema(&self) -> Tool {
        Tool {
            name: "codemode.run_code".to_string(),
            description: "Execute a Rust-like snippet with access to UTCP tools.".to_string(),
            inputs: ToolInputOutputSchema {
                type_: "object".to_string(),
                properties: Some(HashMap::from([
                    (
                        "code".to_string(),
                        serde_json::json!({"type": "string", "description": "Rust-like snippet"}),
                    ),
                    (
                        "timeout".to_string(),
                        serde_json::json!({"type": "integer", "description": "Timeout ms"}),
                    ),
                ])),
                required: Some(vec!["code".to_string()]),
                description: None,
                title: Some("CodeModeArgs".to_string()),
                items: None,
                enum_: None,
                minimum: None,
                maximum: None,
                format: None,
            },
            outputs: ToolInputOutputSchema {
                type_: "object".to_string(),
                properties: Some(HashMap::from([
                    ("value".to_string(), serde_json::json!({"type": "string"})),
                    ("stdout".to_string(), serde_json::json!({"type": "string"})),
                    ("stderr".to_string(), serde_json::json!({"type": "string"})),
                ])),
                required: None,
                description: None,
                title: Some("CodeModeResult".to_string()),
                items: None,
                enum_: None,
                minimum: None,
                maximum: None,
                format: None,
            },
            tags: vec!["codemode".to_string(), "utcp".to_string()],
            average_response_size: None,
            provider: None,
        }
    }

    fn build_engine(&self) -> Engine {
        let mut engine = Engine::new();
        engine.register_fn("sprintf", sprintf);

        let client = self.client.clone();
        engine.register_fn(
            "call_tool",
            move |name: &str, map: Map| -> Result<Dynamic, Box<EvalAltResult>> {
                let args_val = serde_json::to_value(map).map_err(|e| {
                    EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                })?;
                let args = value_to_map(args_val)?;

                let res = block_on_any_runtime(async { client.call_tool(name, args).await })
                    .map_err(|e| {
                        EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                    })?;

                Ok(rhai::serde::to_dynamic(res).map_err(|e| {
                    EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                })?)
            },
        );

        let client = self.client.clone();
        engine.register_fn(
            "call_tool_stream",
            move |name: &str, map: Map| -> Result<Dynamic, Box<EvalAltResult>> {
                let args_val = serde_json::to_value(map).map_err(|e| {
                    EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                })?;
                let args = value_to_map(args_val)?;

                let mut stream = block_on_any_runtime(async {
                    client.call_tool_stream(name, args).await
                })
                .map_err(|e| {
                    EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                })?;

                let mut items = Vec::new();
                loop {
                    let next = block_on_any_runtime(async { stream.next().await }).map_err(|e| {
                        EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                    })?;
                    match next {
                        Some(value) => items.push(value),
                        None => break,
                    }
                }

                if let Err(e) = block_on_any_runtime(async { stream.close().await }) {
                    return Err(
                        EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                            .into(),
                    );
                }

                Ok(rhai::serde::to_dynamic(items).map_err(|e| {
                    EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                })?)
            },
        );

        let client = self.client.clone();
        engine.register_fn(
            "search_tools",
            move |query: &str, limit: i64| -> Result<Dynamic, Box<EvalAltResult>> {
                let res = block_on_any_runtime(async {
                    client.search_tools(query, limit as usize).await
                })
                .map_err(|e| {
                    EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                })?;
                Ok(rhai::serde::to_dynamic(res).map_err(|e| {
                    EvalAltResult::ErrorRuntime(e.to_string().into(), rhai::Position::NONE)
                })?)
            },
        );

        engine
    }

    async fn eval_rusty_snippet(&self, code: &str, _timeout_ms: Option<u64>) -> Result<Value> {
        let wrapped = format!("let __out = {{ {} }};\n__out", code);
        let engine = self.build_engine();
        let mut scope = Scope::new();

        let dyn_result = engine.eval_with_scope::<Dynamic>(&mut scope, &wrapped);
        let dyn_value = dyn_result.map_err(|e| anyhow!("codemode eval error: {}", e))?;
        let value: Value = rhai::serde::from_dynamic(&dyn_value)
            .map_err(|e| anyhow!("Failed to convert result: {}", e))?;
        Ok(value)
    }

    pub fn tool(&self) -> Tool {
        self.tool_schema()
    }

    /// Convenience helpers mirroring go-utcp codemode helper exports.
    pub async fn call_tool(&self, name: &str, args: HashMap<String, Value>) -> Result<Value> {
        self.client.call_tool(name, args).await
    }

    pub async fn call_tool_stream(
        &self,
        name: &str,
        args: HashMap<String, Value>,
    ) -> Result<Box<dyn crate::transports::stream::StreamResult>> {
        self.client.call_tool_stream(name, args).await
    }

    pub async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>> {
        self.client.search_tools(query, limit).await
    }
}

#[async_trait::async_trait]
pub trait LlmModel: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<Value>;
}

/// High-level orchestrator that mirrors go-utcp's CodeMode flow:
/// 1) Decide if tools are needed
/// 2) Select tools by name
/// 3) Ask the model to emit a Rhai snippet using call_tool helpers
/// 4) Execute the snippet via CodeMode
pub struct CodemodeOrchestrator {
    codemode: Arc<CodeModeUtcp>,
    model: Arc<dyn LlmModel>,
    tool_specs_cache: RwLock<Option<String>>,
}

impl CodemodeOrchestrator {
    pub fn new(codemode: Arc<CodeModeUtcp>, model: Arc<dyn LlmModel>) -> Self {
        Self {
            codemode,
            model,
            tool_specs_cache: RwLock::new(None),
        }
    }

    /// Run the full orchestration flow. Returns Ok(None) if the model says no tools are needed
    /// or fails to pick any tools. Otherwise returns the codemode execution result.
    pub async fn call_prompt(&self, prompt: &str) -> Result<Option<Value>> {
        let specs = self.render_tool_specs().await?;

        if !self.decide_if_tools_needed(prompt, &specs).await? {
            return Ok(None);
        }

        let selected = self.select_tools(prompt, &specs).await?;
        if selected.is_empty() {
            return Ok(None);
        }

        let snippet = self.generate_snippet(prompt, &selected, &specs).await?;
        let raw = self
            .codemode
            .execute(CodeModeArgs {
                code: snippet,
                timeout: Some(20_000),
            })
            .await?;

        Ok(Some(raw.value))
    }

    async fn render_tool_specs(&self) -> Result<String> {
        {
            let cache = self.tool_specs_cache.read().await;
            if let Some(specs) = &*cache {
                return Ok(specs.clone());
            }
        }

        let tools = self
            .codemode
            .search_tools("", 200)
            .await
            .unwrap_or_default();
        let mut rendered =
            String::from("UTCP TOOL REFERENCE (use exact field names and required keys):\n");
        for tool in tools {
            rendered.push_str(&format!("TOOL: {} - {}\n", tool.name, tool.description));

            rendered.push_str("INPUTS:\n");
            match tool.inputs.properties.as_ref() {
                Some(props) if !props.is_empty() => {
                    for (key, schema) in props {
                        rendered.push_str(&format!(
                            "  - {}: {}\n",
                            key,
                            schema_type_hint(schema)
                        ));
                    }
                }
                _ => rendered.push_str("  - none\n"),
            }

            if let Some(required) = tool.inputs.required.as_ref() {
                if !required.is_empty() {
                    rendered.push_str("  REQUIRED:\n");
                    for field in required {
                        rendered.push_str(&format!("  - {}\n", field));
                    }
                }
            }

            rendered.push_str("OUTPUTS:\n");
            match tool.outputs.properties.as_ref() {
                Some(props) if !props.is_empty() => {
                    for (key, schema) in props {
                        rendered.push_str(&format!(
                            "  - {}: {}\n",
                            key,
                            schema_type_hint(schema)
                        ));
                    }
                }
                _ => {
                    if !tool.outputs.type_.is_empty() {
                        rendered.push_str(&format!("  - type: {}\n", tool.outputs.type_));
                    } else {
                        rendered.push_str("  - (shape unspecified)\n");
                    }
                }
            }

            rendered.push('\n');
        }

        let mut cache = self.tool_specs_cache.write().await;
        *cache = Some(rendered.clone());
        Ok(rendered)
    }

    async fn decide_if_tools_needed(&self, prompt: &str, specs: &str) -> Result<bool> {
        let request = format!(
            "You can call tools described below. Respond with only 'yes' or 'no'.\n\nTOOLS:\n{}\n\nUSER:\n{}",
            specs, prompt
        );
        let resp_val = self.model.complete(&request).await?;
        Ok(resp_val
            .as_str()
            .unwrap_or_default()
            .trim_start()
            .to_ascii_lowercase()
            .starts_with('y'))
    }

    async fn select_tools(&self, prompt: &str, specs: &str) -> Result<Vec<String>> {
        let request = format!(
            "Choose relevant tool names from the list. Respond with a comma-separated list of names only.\n\nTOOLS:\n{}\n\nUSER:\n{}",
            specs, prompt
        );
        let resp_val = self.model.complete(&request).await?;
        let resp = resp_val.as_str().unwrap_or_default();
        let mut out = Vec::new();
        for name in resp.split(',') {
            let n = name.trim();
            if !n.is_empty() {
                out.push(n.to_string());
            }
        }
        Ok(out)
    }

    async fn generate_snippet(
        &self,
        prompt: &str,
        tools: &[String],
        specs: &str,
    ) -> Result<String> {
        let tool_list = tools.join(", ");
        let request = format!(
            "Generate a Rhai snippet that chains UTCP tool calls to satisfy the user request.\n\
Use ONLY these tools: {tool_list}.\n\
Helpers available: call_tool(name, map), call_tool_stream(name, map) -> array of streamed chunks, search_tools(query, limit), sprintf(fmt, list).\n\
Use Rhai map syntax #{{\"field\": value}} with exact input field names; include required fields and never invent new keys.\n\
You may call multiple tools, store results in variables, and pass them into subsequent tools.\n\
When using call_tool_stream, treat the returned array as the streamed items and chain it into later calls or the final output.\n\
Return the final value as the last expression (map/list/scalar). No markdown or commentary, code only.\n\
\nUSER:\n{prompt}\n\nTOOLS (use exact field names):\n{specs}"
        );
        let resp_val = self.model.complete(&request).await?;
        Ok(resp_val.as_str().unwrap_or_default().trim().to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeModeArgs {
    pub code: String,
    #[serde(default)]
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeModeResult {
    pub value: Value,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

fn schema_type_hint(value: &Value) -> String {
    if let Some(t) = value.get("type").and_then(|v| v.as_str()) {
        t.to_string()
    } else if let Some(s) = value.as_str() {
        s.to_string()
    } else if value.is_array() {
        "array".to_string()
    } else if value.is_object() {
        "object".to_string()
    } else {
        "any".to_string()
    }
}

fn value_to_map(value: Value) -> Result<HashMap<String, Value>, Box<EvalAltResult>> {
    match value {
        Value::Object(obj) => Ok(obj.into_iter().collect()),
        _ => Err(EvalAltResult::ErrorRuntime(
            "call_tool expects object args".into(),
            rhai::Position::NONE,
        )
        .into()),
    }
}

pub fn sprintf(fmt: &str, args: &[Dynamic]) -> String {
    let mut out = fmt.to_string();
    for rendered in args.iter().map(|v| v.to_string()) {
        out = out.replacen("{}", &rendered, 1);
    }
    out
}

fn block_on_any_runtime<F, T>(fut: F) -> Result<T, anyhow::Error>
where
    F: std::future::Future<Output = Result<T, anyhow::Error>>,
    T: Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            RuntimeFlavor::MultiThread => tokio::task::block_in_place(|| handle.block_on(fut)),
            RuntimeFlavor::CurrentThread => {
                let rt = Builder::new_current_thread().enable_all().build()?;
                rt.block_on(fut)
            }
            _ => {
                let rt = Builder::new_current_thread().enable_all().build()?;
                rt.block_on(fut)
            }
        },
        Err(_) => {
            let rt = Builder::new_current_thread().enable_all().build()?;
            rt.block_on(fut)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::transports::stream::boxed_vec_stream;
    use tokio::sync::Mutex;

    #[derive(Clone)]
    struct MockClient {
        called: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl UtcpClientInterface for MockClient {
        async fn register_tool_provider(
            &self,
            _prov: Arc<dyn crate::providers::base::Provider>,
        ) -> Result<Vec<Tool>> {
            Ok(vec![])
        }

        async fn deregister_tool_provider(&self, _provider_name: &str) -> Result<()> {
            Ok(())
        }

        async fn call_tool(&self, tool_name: &str, _args: HashMap<String, Value>) -> Result<Value> {
            self.called.lock().await.push(tool_name.to_string());
            Ok(Value::Number(serde_json::Number::from(5)))
        }

        async fn search_tools(&self, query: &str, _limit: usize) -> Result<Vec<Tool>> {
            self.called.lock().await.push(format!("search:{query}"));
            Ok(vec![])
        }

        fn get_transports(&self) -> HashMap<String, Arc<dyn crate::transports::ClientTransport>> {
            HashMap::new()
        }

        async fn call_tool_stream(
            &self,
            tool_name: &str,
            _args: HashMap<String, Value>,
        ) -> Result<Box<dyn crate::transports::stream::StreamResult>> {
            self.called.lock().await.push(format!("stream:{tool_name}"));
            Ok(boxed_vec_stream(vec![Value::String("chunk".into())]))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn codemode_helpers_forward_to_client() {
        let client = Arc::new(MockClient {
            called: Arc::new(Mutex::new(Vec::new())),
        });
        let codemode = CodeModeUtcp::new(client.clone());

        codemode
            .call_tool("demo.tool", HashMap::new())
            .await
            .unwrap();
        codemode.search_tools("demo", 5).await.unwrap();
        let mut stream = codemode
            .call_tool_stream("demo.tool", HashMap::new())
            .await
            .unwrap();
        let _ = stream.next().await.unwrap();

        let calls = client.called.lock().await.clone();
        assert_eq!(calls, vec!["demo.tool", "search:demo", "stream:demo.tool"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn execute_runs_rusty_snippet_and_call_tool() {
        let client = Arc::new(MockClient {
            called: Arc::new(Mutex::new(Vec::new())),
        });
        let codemode = CodeModeUtcp::new(client);

        let code = r#"let x = 2 + 3; let y = call_tool("math.add", #{"a":1}); x + y"#;
        let args = CodeModeArgs {
            code: code.into(),
            timeout: Some(1000),
        };
        let res = codemode.execute(args).await.unwrap();
        assert_eq!(res.value, serde_json::json!(10));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn execute_collects_stream_results() {
        let client = Arc::new(MockClient {
            called: Arc::new(Mutex::new(Vec::new())),
        });
        let codemode = CodeModeUtcp::new(client.clone());

        let code = r#"let chunks = call_tool_stream("demo.tool", #{}); chunks"#;
        let args = CodeModeArgs {
            code: code.into(),
            timeout: Some(1_000),
        };
        let res = codemode.execute(args).await.unwrap();
        assert_eq!(res.value, serde_json::json!(["chunk"]));
        let calls = client.called.lock().await.clone();
        assert_eq!(calls, vec!["stream:demo.tool"]);
    }
}
