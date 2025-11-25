use std::{convert::Infallible, net::SocketAddr};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rs_utcp::UtcpClientInterface;
use serde_json::json;

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Spin up an in-process HTTP server that serves a manifest and echoes back POST bodies.
    let addr = spawn_demo_server().await?;
    println!("Started demo HTTP server at http://{addr}/tools");

    // Build UTCP client from a provider config file
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "http",
            "name": "demo",
            "url": format!("http://{addr}/tools"),
            "http_method": "POST"
        }]
    }))
    .await?;
    let tools = client.search_tools("", 10).await?;
    println!(
        "Registered tools: {:?}",
        tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
    );

    // Call the tool
    let mut args = std::collections::HashMap::new();
    args.insert("message".into(), serde_json::json!("hello from rust-utcp"));
    let result: serde_json::Value = client.call_tool("demo.echo", args).await?;
    println!("Tool result: {}", serde_json::to_string_pretty(&result)?);

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
        // Serve a simple UTCP manifest
        (&Method::GET, "/tools") => {
            let manifest = json!({
                "tools": [{
                    "name": "echo",
                    "description": "Echo a message",
                    "inputs": { "type": "object" },
                    "outputs": { "type": "object" },
                    "tags": ["demo", "echo"]
                }]
            });
            Ok(json_response(StatusCode::OK, manifest))
        }
        // Accept tool calls and echo back the JSON args
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
