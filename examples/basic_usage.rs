// Example: Using rust-utcp to register and call tools

use std::{collections::HashMap, convert::Infallible, net::SocketAddr};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rs_utcp::UtcpClientInterface;
use serde_json::json;

#[path = "common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Spin up a tiny HTTP provider that returns a weather-like manifest.
    let addr = spawn_demo_server().await?;
    println!("Started demo HTTP provider at http://{addr}/tools");

    // Load providers via new using a temp JSON file.
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "http",
            "name": "weather_api",
            "url": format!("http://{addr}/tools"),
            "http_method": "POST"
        }]
    }))
    .await?;

    println!("✓ UTCP Client initialized with all transports");
    println!("  Providers loaded from temporary config via new\n");

    // Example 1: List available tools
    println!("📡 Example 1: Listing provider tools");
    let tools = client.search_tools("", 10).await?;
    for tool in &tools {
        println!("  - {}: {}", tool.name, tool.description);
    }

    // Example 2: Search for tools
    println!("\n🔍 Example 2: Search Tools");
    let matching = client.search_tools("weather", 5).await?;
    println!("  Found {} tools matching 'weather'", matching.len());

    // Example 3: Call a tool
    println!("\n⚡ Example 3: Call Tool");
    let mut args = HashMap::new();
    args.insert("city".to_string(), serde_json::json!("London"));
    args.insert("units".to_string(), serde_json::json!("metric"));
    let result = client
        .call_tool("weather_api.get_current_weather", args)
        .await?;
    println!("  Result: {}", serde_json::to_string_pretty(&result)?);

    // Example 4: List available transports
    println!("\n📋 Example 4: Available Transports");
    let transports = client.get_transports();
    println!("  {} transports available:", transports.len());
    for (name, _) in &transports {
        println!("    - {}", name);
    }

    println!("\n✨ Demo complete!");

    Ok(())
}

async fn spawn_demo_server() -> anyhow::Result<SocketAddr> {
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
                    "name": "get_current_weather",
                    "description": "Return a mock weather payload",
                    "inputs": { "type": "object" },
                    "outputs": { "type": "object" },
                    "tags": ["weather", "demo"]
                }]
            });
            Ok(json_response(StatusCode::OK, manifest))
        }
        (&Method::POST, "/tools") => {
            let body_bytes = hyper::body::to_bytes(req.into_body())
                .await
                .unwrap_or_default();
            let mut value: serde_json::Value =
                serde_json::from_slice(&body_bytes).unwrap_or_else(|_| json!({}));
            value["provider"] = json!("weather_api");
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
