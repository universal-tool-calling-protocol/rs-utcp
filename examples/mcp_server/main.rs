use std::{convert::Infallible, net::SocketAddr};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rs_utcp::UtcpClientInterface;
use serde_json::json;
#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_mcp_server().await?;
    println!("Started MCP demo at http://{addr}");

    let client = common::client_from_providers(json!({
        "manual_call_templates": [{
            "call_template_type": "mcp",
            "name": "mcp_demo",
            "url": format!("http://{addr}"),
            "allowed_communication_protocols": ["mcp"]

        }]
    }))
    .await?;
    let tools = client.search_tools("", 10).await?;
    println!(
        "Tools: {:?}",
        tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    let mut args = std::collections::HashMap::new();
    args.insert("name".into(), serde_json::json!("world"));
    let res = client.call_tool("mcp_demo.echo", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

async fn spawn_mcp_server() -> anyhow::Result<SocketAddr> {
    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::try_bind(&([127, 0, 0, 1], 0).into())?;
    let addr = server.local_addr();
    tokio::spawn(server.serve(make_svc));
    Ok(addr)
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    if req.method() != Method::POST {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())
            .unwrap());
    }
    let body = hyper::body::to_bytes(req.into_body())
        .await
        .unwrap_or_default();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let method = payload.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "tools/list" => {
            let resp = json!({
                "jsonrpc": "2.0",
                "result": { "tools": [{
                    "name": "echo",
                    "description": "Echo args",
                    "inputs": { "type": "object" },
                    "outputs": { "type": "object" },
                    "tags": ["mcp"]
                }]},
                "id": payload.get("id").cloned().unwrap_or(json!(1))
            });
            Ok(json_response(StatusCode::OK, resp))
        }
        "tools/call" => {
            let resp = json!({
                "jsonrpc": "2.0",
                "result": payload.get("params").cloned().unwrap_or(json!({})),
                "id": payload.get("id").cloned().unwrap_or(json!(1))
            });
            Ok(json_response(StatusCode::OK, resp))
        }
        _ => Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
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
