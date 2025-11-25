use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rs_utcp::{
    config::UtcpClientConfig, providers::graphql::GraphqlProvider,
    repository::in_memory::InMemoryToolRepository, tag::tag_search::TagSearchStrategy, UtcpClient,
    UtcpClientInterface,
};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_graphql_server().await?;
    println!("Started GraphQL demo at http://{addr}/graphql");

    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let client = UtcpClient::new(UtcpClientConfig::default(), repo, search);

    let provider = GraphqlProvider::new(
        "graphql_demo".into(),
        format!("http://{addr}/graphql"),
        None,
    );
    let tools = client.register_tool_provider(Arc::new(provider)).await?;
    println!(
        "Tools: {:?}",
        tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    let mut args = std::collections::HashMap::new();
    args.insert("name".into(), serde_json::json!("rust-utcp"));
    let res = client.call_tool("graphql_demo.hello", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

async fn spawn_graphql_server() -> anyhow::Result<SocketAddr> {
    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::try_bind(&([127, 0, 0, 1], 0).into())?;
    let addr = server.local_addr();
    tokio::spawn(server.serve(make_svc));
    Ok(addr)
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    if req.method() != Method::POST || req.uri().path() != "/graphql" {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap());
    }

    let body_bytes = hyper::body::to_bytes(req.into_body())
        .await
        .unwrap_or_default();
    let payload: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
    let query = payload
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if query.contains("__schema") {
        let resp = json!({
            "data": {
                "__schema": {
                    "queryType": { "fields": [ { "name": "hello", "description": "Hello field"} ] },
                    "mutationType": null,
                    "subscriptionType": null
                }
            }
        });
        return Ok(json_response(StatusCode::OK, resp));
    }

    let data = json!({ "hello": format!("Hello, {}", payload.get("variables").and_then(|v| v.get("name")).and_then(|v| v.as_str()).unwrap_or("world")) });
    Ok(json_response(StatusCode::OK, json!({ "data": data })))
}

fn json_response(status: StatusCode, body: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
