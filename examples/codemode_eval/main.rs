use std::{convert::Infallible, net::SocketAddr};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rs_utcp::plugins::codemode::{CodeModeArgs, CodeModeUtcp};
use serde_json::json;

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Spin up a tiny in-process HTTP server for the codemode demo.
    let addr = spawn_http_server().await?;
    let echo_url = format!("http://{addr}/tools");

    let client = common::client_from_providers(json!({
        "manual_call_templates": [{
            "call_template_type": "http",
            "name": "http_demo",
            "url": echo_url,
            "http_method": "POST",
            "allowed_communication_protocols": ["http"]
        }]
    }))
    .await?;

    // Create codemode orchestrator
    let codemode = CodeModeUtcp::new(client.clone());

    // Example 1: direct JSON snippet passthrough
    let args = CodeModeArgs {
        code: r#"{"hello": "world"}"#.to_string(),
        timeout: Some(2_000),
    };
    let res = codemode.execute(args).await?;
    println!(
        "JSON passthrough -> {}",
        serde_json::to_string_pretty(&res.value)?
    );

    // Example 2: Rust-like snippet with call_tool helper
    let snippet = r#"
        let a = 2 + 3;
        let b = call_tool("http_demo.echo", #{"message": "from codemode"});
        b // return value so we can print the echo result
    "#;

    let res = codemode
        .execute(CodeModeArgs {
            code: snippet.to_string(),
            timeout: Some(5_000),
        })
        .await?;
    println!(
        "Snippet result -> {}",
        serde_json::to_string_pretty(&res.value)?
    );

    Ok(())
}

async fn spawn_http_server() -> anyhow::Result<SocketAddr> {
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
                    "inputs": { "type": "object" },
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
