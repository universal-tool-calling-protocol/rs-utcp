use std::{convert::Infallible, net::SocketAddr, time::Duration};

use futures_util::stream::{self, StreamExt};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rs_utcp::UtcpClientInterface;
use serde_json::json;

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_sse_server().await?;
    println!("Started SSE demo at http://{addr}/tools");

    let client = common::client_from_providers(json!({
        "manual_call_templates": [{
            "call_template_type": "sse",
            "name": "sse_demo",
            "url": format!("http://{addr}/tools"),
            "allowed_communication_protocols": ["sse"]
        }]
    }))
    .await?;
    let tools = client.search_tools("", 10).await?;
    println!(
        "Tools: {:?}",
        tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    let mut args = std::collections::HashMap::new();
    args.insert("topic".into(), serde_json::json!("demo"));
    let mut stream = client.call_tool_stream("sse_demo.echo", args).await?;
    if let Some(item) = stream.next().await? {
        println!("First event: {}", serde_json::to_string_pretty(&item)?);
    }
    Ok(())
}

async fn spawn_sse_server() -> anyhow::Result<SocketAddr> {
    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::try_bind(&([127, 0, 0, 1], 0).into())?;
    let addr = server.local_addr();
    tokio::spawn(server.serve(make_svc));
    Ok(addr)
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/tools") => {
            let manifest = json!({
                "tools": [{
                    "name": "echo",
                    "description": "Stream events",
                    "inputs": { "type": "object" },
                    "outputs": { "type": "object" },
                    "tags": ["sse"]
                }]
            });
            Ok(json_response(StatusCode::OK, manifest))
        }
        (&Method::POST, "/tools/echo") => {
            // Emit a few data events
            let events = stream::iter(0..3).then(|i| async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, Infallible>(format!("data: {}\n\n", json!({"idx": i, "msg": "hello"})))
            });
            let body = Body::wrap_stream(events);
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(body)
                .unwrap())
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
