use std::net::SocketAddr;

use rs_utcp::{
    grpcpb::generated::{
        utcp_service_server::{UtcpService, UtcpServiceServer},
        Empty, Manual, Tool as PbTool, ToolCallRequest, ToolCallResponse,
    },
    UtcpClientInterface,
};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{transport::Server, Request, Response, Status};

#[path = "../common/mod.rs"]
mod common;

#[derive(Default)]
struct DemoGrpc;

#[tonic::async_trait]
impl UtcpService for DemoGrpc {
    async fn get_manual(&self, _req: Request<Empty>) -> Result<Response<Manual>, Status> {
        let tool = PbTool {
            name: "echo".into(),
            description: "Echo arguments".into(),
        };
        Ok(Response::new(Manual {
            version: "1.0".into(),
            tools: vec![tool],
        }))
    }

    async fn call_tool(
        &self,
        req: Request<ToolCallRequest>,
    ) -> Result<Response<ToolCallResponse>, Status> {
        let args = req.into_inner().args_json;
        Ok(Response::new(ToolCallResponse { result_json: args }))
    }

    type CallToolStreamStream = futures_util::stream::Empty<Result<ToolCallResponse, Status>>;
    async fn call_tool_stream(
        &self,
        _request: Request<ToolCallRequest>,
    ) -> Result<Response<Self::CallToolStreamStream>, Status> {
        Ok(Response::new(futures_util::stream::empty()))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_grpc_server().await?;
    println!("Started gRPC demo at {addr}");

    let client = common::client_from_providers(serde_json::json!({
        "providers": [{
            "provider_type": "grpc",
            "name": "grpc_demo",
            "host": "127.0.0.1",
            "port": addr.port()
        }]
    }))
    .await?;

    let mut args = std::collections::HashMap::new();
    args.insert("message".into(), serde_json::json!("hello grpc"));
    let res = client.call_tool("grpc_demo.echo", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

async fn spawn_grpc_server() -> anyhow::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        Server::builder()
            .add_service(UtcpServiceServer::new(DemoGrpc::default()))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    Ok(addr)
}
