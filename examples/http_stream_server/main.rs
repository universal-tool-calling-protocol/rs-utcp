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
    let addr = spawn_stream_server().await?;
    println!("Started HTTP stream demo at http://{addr}/tools");

    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "http_stream",
            "name": "http_stream_demo",
            "url": format!("http://{addr}/tools")
        }]
    }))
    .await?;

    let mut args = std::collections::HashMap::new();
    args.insert("query".into(), serde_json::json!("streaming"));
    let mut stream = client
        .call_tool_stream("http_stream_demo.echo", args)
        .await?;
    while let Some(chunk) = stream.next().await? {
        println!("Chunk: {}", serde_json::to_string_pretty(&chunk)?);
        break; // show first chunk for brevity
    }
    Ok(())
}

async fn spawn_stream_server() -> anyhow::Result<SocketAddr> {
    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::try_bind(&([127, 0, 0, 1], 0).into())?;
    let addr = server.local_addr();
    tokio::spawn(server.serve(make_svc));
    Ok(addr)
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/tools") | (&Method::POST, "/tools") => {
            let manifest = json!({
                "tools": [{
                    "name": "echo",
                    "description": "Stream back JSON chunks",
                    "inputs": { "type": "object" },
                    "outputs": { "type": "object" },
                    "tags": ["http_stream"]
                }]
            });
            Ok(json_response(StatusCode::OK, manifest))
        }
        (&Method::POST, "/tools/echo") => {
            let chunks = stream::iter(0..3).then(|i| async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, Infallible>(json!({"chunk": i, "msg": "hi"}).to_string())
            });
            let body_stream = Body::wrap_stream(chunks);
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(body_stream)
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
