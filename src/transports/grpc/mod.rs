// gRPC Transport - Protocol Buffers RPC
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use base64::Engine;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::mpsc;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
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

pub struct GrpcTransport;

impl GrpcTransport {
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
        if prov.use_ssl {
            return Err(anyhow!("TLS for gRPC transport is not configured yet"));
        }
        let endpoint = format!("http://{}:{}", prov.host, prov.port);
        let channel = Endpoint::from_shared(endpoint)?.connect().await?;
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
                _ => {
                    return Err(anyhow!("Only basic auth is supported for gRPC providers"));
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
    use crate::auth::{ApiKeyAuth, AuthType, BasicAuth};

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
            Some(AuthConfig::ApiKey(ApiKeyAuth {
                auth_type: AuthType::ApiKey,
                api_key: "secret".to_string(),
                var_name: "X-Api-Key".to_string(),
                location: "header".to_string(),
            })),
        );

        let mut request: Request<()> = Request::new(());
        let err = transport.apply_auth(&prov, &mut request).unwrap_err();
        assert!(err.to_string().contains("Only basic auth is supported"));
    }
}
