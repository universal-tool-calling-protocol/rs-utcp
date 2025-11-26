//! End-to-end demo showing how to pair the codemode orchestrator with a Gemini model.
//! Requires:
//! - GEMINI_API_KEY in env (and optional GEMINI_MODEL, defaults to gemini-pro)
//! - Network access to Google's Generative Language API
//!
//! Run:
//!   GEMINI_API_KEY=your_key_here cargo run --example orchestrator_gemini -- "Send hello via the echo tool"

use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use reqwest::Client;
use rs_utcp::plugins::codemode::{CodeModeArgs, CodeModeUtcp, CodemodeOrchestrator, LlmModel};
use serde_json::{json, Value};

#[path = "common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> Result<()> {
    let prompt = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Echo 'hello from orchestrator' back to me".to_string());

    // Spin up a tiny local HTTP provider with a single echo tool.
    let addr = spawn_http_server().await?;
    let echo_url = format!("http://{addr}/tools");

    let client = common::client_from_providers(json!({
        "manual_call_templates": [{
            "call_template_type": "http",
            "name": "http_demo",
            "url": echo_url,
            "http_method": "POST"
        }]
    }))
    .await?;

    let codemode = Arc::new(CodeModeUtcp::new(client));
    let model = Arc::new(GeminiModel::from_env()?);
    let orchestrator = CodemodeOrchestrator::new(codemode.clone(), model);

    println!("Prompt: {prompt}");
    match orchestrator.call_prompt(&prompt).await? {
        Some(value) => println!("Orchestrator result:\n{}", serde_json::to_string_pretty(&value)?),
        None => println!("Model decided no tools were needed."),
    }

    // Direct codemode call for comparison
    let snippet = r#"call_tool("http_demo.echo", #{"message": "hello from codemode directly"})"#;
    let direct = codemode
        .execute(CodeModeArgs {
            code: snippet.to_string(),
            timeout: Some(5_000),
        })
        .await?;
    println!(
        "Direct snippet result:\n{}",
        serde_json::to_string_pretty(&direct.value)?
    );

    Ok(())
}

/// Minimal Gemini model that satisfies the LlmModel trait using the Generative Language REST API.
struct GeminiModel {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
}

impl GeminiModel {
    fn from_env() -> Result<Self> {
        let api_key =
            std::env::var("GEMINI_API_KEY").map_err(|_| anyhow!("GEMINI_API_KEY is required"))?;
        let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-3-pro-preview".to_string());
        let endpoint = std::env::var("GEMINI_ENDPOINT")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
        Ok(Self {
            client: Client::builder().build()?,
            api_key,
            model,
            endpoint,
        })
    }
}

#[async_trait]
impl LlmModel for GeminiModel {
    async fn complete(&self, prompt: &str) -> Result<Value> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.endpoint, self.model, self.api_key
        );
        let body = json!({
            "contents": [{
                "parts": [{ "text": prompt }]
            }]
        });

        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "Gemini request failed: {}",
                resp.text().await.unwrap_or_default()
            ));
        }
        let value: Value = resp.json().await?;
        if let Some(text) = value["candidates"]
            .get(0)
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
        {
            return Ok(Value::String(text.to_string()));
        }
        Ok(value)
    }
}

async fn spawn_http_server() -> Result<SocketAddr> {
    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_request)) });
    let server = Server::try_bind(&([127, 0, 0, 1], 0).into())?;
    let addr = server.local_addr();
    tokio::spawn(server.serve(make_svc));
    Ok(addr)
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/tools") => {
            let manifest = json!({
                "tools": [{
                    "name": "echo",
                    "description": "Echo a message",
                    "inputs": { "type": "object", "properties": { "message": { "type": "string" } }, "required": ["message"] },
                    "outputs": { "type": "object" },
                    "tags": ["demo", "echo"]
                }]
            });
            Ok(json_response(StatusCode::OK, manifest))
        }
        (&Method::POST, "/tools") => {
            let body_bytes = hyper::body::to_bytes(req.into_body())
                .await
                .unwrap_or_default();
            let value: serde_json::Value =
                serde_json::from_slice(&body_bytes).unwrap_or_else(|_| json!({}));
            Ok(json_response(StatusCode::OK, value))
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap()),
    }
}

fn json_response(status: StatusCode, body: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
