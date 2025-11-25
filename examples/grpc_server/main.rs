use std::net::SocketAddr;
use std::sync::Arc;

use rs_utcp::{
    config::UtcpClientConfig,
    grpcpb::generated::{
        utcp_service_server::{UtcpService, UtcpServiceServer},
        Empty, Manual, Tool as PbTool, ToolCallRequest, ToolCallResponse,
    },
    providers::grpc::GrpcProvider,
    repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy,
    UtcpClient, UtcpClientInterface,
};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{transport::Server, Request, Response, Status};

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

    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let client = UtcpClient::new(UtcpClientConfig::default(), repo, search);

    let provider = GrpcProvider::new("grpc_demo".into(), "127.0.0.1".into(), addr.port(), None);
    client.register_tool_provider(Arc::new(provider)).await?;

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
