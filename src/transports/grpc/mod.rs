// gRPC Transport - Protocol Buffers RPC
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use base64::Engine;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::mpsc;
use tonic::metadata::{MetadataKey, MetadataValue};
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::Request;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::grpc::GrpcProvider;
use crate::tools::{Tool, ToolInputOutputSchema};
use crate::transports::{
    stream::{boxed_channel_stream, StreamResult},
    ClientTransport,
};

use crate::grpcpb::generated::utcp_service_client::UtcpServiceClient;
use crate::grpcpb::generated::{Empty, ToolCallRequest};

/// Transport implementation that communicates with UTCP servers over gRPC.
pub struct GrpcTransport;

impl GrpcTransport {
    /// Create a gRPC transport instance.
    pub fn new() -> Self {
        Self
    }

    fn default_schema() -> ToolInputOutputSchema {
        ToolInputOutputSchema {
            type_: "object".to_string(),
            properties: None,
            required: None,
            description: None,
            title: None,
            items: None,
            enum_: None,
            minimum: None,
            maximum: None,
            format: None,
        }
    }

    async fn connect(&self, prov: &GrpcProvider) -> Result<UtcpServiceClient<Channel>> {
        let scheme = if prov.use_ssl { "https" } else { "http" };
        let endpoint = format!("{}://{}:{}", scheme, prov.host, prov.port);

        let mut endpoint = Endpoint::from_shared(endpoint)?;
        if prov.use_ssl {
            endpoint = endpoint.tls_config(ClientTlsConfig::new())?;
        }

        let channel = endpoint.connect().await?;
        Ok(UtcpServiceClient::new(channel))
    }

    fn apply_auth<T>(&self, prov: &GrpcProvider, req: &mut Request<T>) -> Result<()> {
        if let Some(auth) = &prov.base.auth {
            match auth {
                AuthConfig::Basic(auth) => {
                    let basic = base64::engine::general_purpose::STANDARD
                        .encode(format!("{}:{}", auth.username, auth.password));
                    let value = MetadataValue::from_str(&format!("Basic {}", basic))?;
                    req.metadata_mut().insert("authorization", value);
                }
                AuthConfig::ApiKey(api_key) => {
                    if api_key.location.to_ascii_lowercase() != "header" {
                        return Err(anyhow!(
                            "gRPC API key auth only supports the 'header' location"
                        ));
                    }
                    let key = MetadataKey::from_str(&api_key.var_name.to_ascii_lowercase())?;
                    let value = MetadataValue::from_str(&api_key.api_key)?;
                    req.metadata_mut().insert(key, value);
                }
                _ => {
                    return Err(anyhow!(
                        "Only basic and api key auth are supported for gRPC providers"
                    ));
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ClientTransport for GrpcTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        let grpc_prov = prov
            .as_any()
            .downcast_ref::<GrpcProvider>()
            .ok_or_else(|| anyhow!("Provider is not a GrpcProvider"))?;

        let mut client = self.connect(grpc_prov).await?;
        let mut request = Request::new(Empty {});
        self.apply_auth(grpc_prov, &mut request)?;

        let manual = client.get_manual(request).await?.into_inner();
        let default_schema = Self::default_schema();

        let tools = manual
            .tools
            .into_iter()
            .map(|t| Tool {
                name: t.name,
                description: t.description,
                inputs: default_schema.clone(),
                outputs: default_schema.clone(),
                tags: vec!["grpc".to_string()],
                average_response_size: None,
                provider: None,
            })
            .collect();

        Ok(tools)
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        Ok(())
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value> {
        let grpc_prov = prov
            .as_any()
            .downcast_ref::<GrpcProvider>()
            .ok_or_else(|| anyhow!("Provider is not a GrpcProvider"))?;

        let mut client = self.connect(grpc_prov).await?;
        let args_json = serde_json::to_string(&args)?;

        let mut request = Request::new(ToolCallRequest {
            tool: tool_name.to_string(),
            args_json,
        });
        self.apply_auth(grpc_prov, &mut request)?;

        let response = client.call_tool(request).await?.into_inner();
        if response.result_json.is_empty() {
            return Ok(Value::Null);
        }

        Ok(serde_json::from_str(&response.result_json)
            .unwrap_or_else(|_| Value::String(response.result_json.clone())))
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let grpc_prov = prov
            .as_any()
            .downcast_ref::<GrpcProvider>()
            .ok_or_else(|| anyhow!("Provider is not a GrpcProvider"))?;

        let mut client = self.connect(grpc_prov).await?;
        let args_json = serde_json::to_string(&args)?;

        let mut request = Request::new(ToolCallRequest {
            tool: tool_name.to_string(),
            args_json,
        });
        self.apply_auth(grpc_prov, &mut request)?;

        let mut stream = client.call_tool_stream(request).await?.into_inner();
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            while let Some(item) = stream.message().await.transpose() {
                match item {
                    Ok(resp) => {
                        let parsed = if resp.result_json.is_empty() {
                            Ok(Value::Null)
                        } else {
                            serde_json::from_str::<Value>(&resp.result_json)
                                .map_err(|e| anyhow!("Failed to parse stream item: {}", e))
                        };
                        if tx.send(parsed).await.is_err() {
                            return;
                        }
                    }
                    Err(status) => {
                        let _ = tx.send(Err(anyhow!("gRPC stream error: {}", status))).await;
                        return;
                    }
                }
            }
        });

        Ok(boxed_channel_stream(rx, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ApiKeyAuth, AuthType, BasicAuth, OAuth2Auth};
    use crate::providers::base::{BaseProvider, ProviderType};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
    use tonic::transport::Server;

    use crate::grpcpb::generated::utcp_service_server::{UtcpService, UtcpServiceServer};
    use crate::grpcpb::generated::{Manual, Tool as GrpcTool, ToolCallResponse};

    #[test]
    fn apply_auth_sets_basic_header() {
        let transport = GrpcTransport::new();
        let prov = GrpcProvider::new(
            "grpc".to_string(),
            "localhost".to_string(),
            50051,
            Some(AuthConfig::Basic(BasicAuth {
                auth_type: AuthType::Basic,
                username: "user".to_string(),
                password: "pass".to_string(),
            })),
        );

        let mut request: Request<()> = Request::new(());
        transport.apply_auth(&prov, &mut request).unwrap();

        let header = request.metadata().get("authorization").unwrap();
        assert_eq!(header.to_str().unwrap(), "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn apply_auth_rejects_non_basic() {
        let transport = GrpcTransport::new();
        let prov = GrpcProvider::new(
            "grpc".to_string(),
            "localhost".to_string(),
            50051,
            Some(AuthConfig::OAuth2(OAuth2Auth {
                auth_type: AuthType::OAuth2,
                token_url: "https://example.com".to_string(),
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
                scope: None,
            })),
        );

        let mut request: Request<()> = Request::new(());
        let err = transport.apply_auth(&prov, &mut request).unwrap_err();
        assert!(err
            .to_string()
            .contains("Only basic and api key auth are supported"));
    }

    #[test]
    fn apply_auth_sets_api_key_header() {
        let transport = GrpcTransport::new();
        let prov = GrpcProvider::new(
            "grpc".to_string(),
            "localhost".to_string(),
            50051,
            Some(AuthConfig::ApiKey(ApiKeyAuth {
                auth_type: AuthType::ApiKey,
                api_key: "token".to_string(),
                var_name: "x-api-key".to_string(),
                location: "header".to_string(),
            })),
        );

        let mut request: Request<()> = Request::new(());
        transport.apply_auth(&prov, &mut request).unwrap();
        let header = request.metadata().get("x-api-key").unwrap();
        assert_eq!(header.to_str().unwrap(), "token");
    }

    #[derive(Default)]
    struct MockGrpc;

    #[tonic::async_trait]
    impl UtcpService for MockGrpc {
        async fn get_manual(
            &self,
            _request: Request<Empty>,
        ) -> Result<tonic::Response<Manual>, tonic::Status> {
            Ok(tonic::Response::new(Manual {
                version: "1.0".to_string(),
                tools: vec![GrpcTool {
                    name: "echo".to_string(),
                    description: "echo tool".to_string(),
                }],
            }))
        }

        async fn call_tool(
            &self,
            request: Request<ToolCallRequest>,
        ) -> Result<tonic::Response<ToolCallResponse>, tonic::Status> {
            let inner = request.into_inner();
            let args_value: Value =
                serde_json::from_str(&inner.args_json).unwrap_or_else(|_| Value::Null);
            Ok(tonic::Response::new(ToolCallResponse {
                result_json: json!({
                    "tool": inner.tool,
                    "args": args_value
                })
                .to_string(),
            }))
        }

        type CallToolStreamStream = ReceiverStream<Result<ToolCallResponse, tonic::Status>>;

        async fn call_tool_stream(
            &self,
            _request: Request<ToolCallRequest>,
        ) -> Result<tonic::Response<Self::CallToolStreamStream>, tonic::Status> {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            tx.send(Ok(ToolCallResponse {
                result_json: json!({ "idx": 1 }).to_string(),
            }))
            .await
            .unwrap();
            tx.send(Ok(ToolCallResponse {
                result_json: json!({ "idx": 2 }).to_string(),
            }))
            .await
            .unwrap();
            Ok(tonic::Response::new(ReceiverStream::new(rx)))
        }
    }

    #[tokio::test]
    async fn register_call_and_stream_over_grpc() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            Server::builder()
                .add_service(UtcpServiceServer::new(MockGrpc::default()))
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        let prov = GrpcProvider {
            base: BaseProvider {
                name: "grpc".to_string(),
                provider_type: ProviderType::Grpc,
                auth: None,
                allowed_communication_protocols: None,
            },
            host: addr.ip().to_string(),
            port: addr.port(),
            use_ssl: false,
        };

        let transport = GrpcTransport::new();

        let tools = transport
            .register_tool_provider(&prov)
            .await
            .expect("register");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        let mut args = HashMap::new();
        args.insert("msg".into(), Value::String("hi".into()));
        let call_value = transport
            .call_tool("echo", args.clone(), &prov)
            .await
            .expect("call");
        assert_eq!(call_value, json!({ "tool": "echo", "args": json!(args) }));

        let mut stream = transport
            .call_tool_stream("echo", args, &prov)
            .await
            .expect("call stream");
        assert_eq!(stream.next().await.unwrap().unwrap(), json!({ "idx": 1 }));
        assert_eq!(stream.next().await.unwrap().unwrap(), json!({ "idx": 2 }));
        stream.close().await.unwrap();

        let _ = shutdown_tx.send(());
    }
}
