// WebRTC Transport - peer-to-peer data channels
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_credential_type::RTCIceCredentialType;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::webrtc::WebRtcProvider;
use crate::tools::{Tool, ToolInputOutputSchema};
use crate::transports::{
    stream::{boxed_channel_stream, StreamResult},
    ClientTransport,
};

/// Peer-to-peer transport that relays tool calls over WebRTC data channels.
pub struct WebRtcTransport {
    // Cache of active peer connections
    connections: Arc<Mutex<HashMap<String, Arc<RTCPeerConnection>>>>,
}

impl WebRtcTransport {
    /// Create an empty transport with no cached connections.
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
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

    async fn create_peer_connection(
        &self,
        prov: &WebRtcProvider,
    ) -> Result<Arc<RTCPeerConnection>> {
        // Configure ICE servers
        let ice_servers: Vec<RTCIceServer> = prov
            .ice_servers
            .iter()
            .map(|server| {
                let credential_type = if server.username.is_some() {
                    RTCIceCredentialType::Password
                } else {
                    RTCIceCredentialType::Unspecified
                };

                RTCIceServer {
                    urls: server.urls.clone(),
                    username: server.username.clone().unwrap_or_default(),
                    credential: server.credential.clone().unwrap_or_default(),
                    credential_type,
                }
            })
            .collect();

        // Use the default API to create peer connection
        let api = APIBuilder::new().build();

        // Create peer connection configuration
        let config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        // Create the peer connection
        let peer_connection = Arc::new(api.new_peer_connection(config).await?);

        Ok(peer_connection)
    }

    async fn exchange_sdp(
        &self,
        prov: &WebRtcProvider,
        offer: RTCSessionDescription,
    ) -> Result<RTCSessionDescription> {
        // Send offer to signaling server and get answer
        let client = reqwest::Client::new();

        let mut request = client
            .post(&prov.signaling_server)
            .json(&serde_json::json!({
                "type": "offer",
                "sdp": offer.sdp,
            }));

        // Apply authentication if configured
        if let Some(auth) = &prov.base.auth {
            request = self.apply_auth(request, auth)?;
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Signaling server returned error: {}",
                response.status()
            ));
        }

        let answer_json: Value = response.json().await?;

        let answer_sdp = answer_json
            .get("sdp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Invalid answer from signaling server"))?;

        Ok(RTCSessionDescription::answer(answer_sdp.to_string())?)
    }

    fn apply_auth(
        &self,
        builder: reqwest::RequestBuilder,
        auth: &AuthConfig,
    ) -> Result<reqwest::RequestBuilder> {
        match auth {
            AuthConfig::ApiKey(api_key) => {
                let location = api_key.location.to_ascii_lowercase();
                match location.as_str() {
                    "header" => Ok(builder.header(&api_key.var_name, &api_key.api_key)),
                    "query" => {
                        Ok(builder.query(&[(api_key.var_name.clone(), api_key.api_key.clone())]))
                    }
                    other => Err(anyhow!("Unsupported API key location: {}", other)),
                }
            }
            AuthConfig::Basic(basic) => {
                Ok(builder.basic_auth(&basic.username, Some(&basic.password)))
            }
            AuthConfig::OAuth2(_) => {
                Err(anyhow!("OAuth2 auth not yet supported by WebRTC transport"))
            }
        }
    }

    async fn create_data_channel(
        &self,
        prov: &WebRtcProvider,
    ) -> Result<(Arc<RTCPeerConnection>, Arc<RTCDataChannel>)> {
        let peer_connection = self.create_peer_connection(prov).await?;

        // Create data channel configuration
        let mut init = webrtc::data_channel::data_channel_init::RTCDataChannelInit {
            ordered: Some(prov.ordered),
            ..Default::default()
        };

        if let Some(max_retransmits) = prov.max_retransmits {
            init.max_retransmits = Some(max_retransmits);
        }

        if let Some(max_packet_life_time) = prov.max_packet_life_time {
            init.max_packet_life_time = Some(max_packet_life_time);
        }

        // Create data channel
        let data_channel = peer_connection
            .create_data_channel(&prov.channel_label, Some(init))
            .await?;

        // Create offer
        let offer = peer_connection.create_offer(None).await?;
        peer_connection.set_local_description(offer.clone()).await?;

        // Exchange SDP with signaling server
        let answer = self.exchange_sdp(prov, offer).await?;
        peer_connection.set_remote_description(answer).await?;

        // Wait for data channel to open
        let (open_tx, mut open_rx) = mpsc::channel::<()>(1);
        let open_tx = Arc::new(Mutex::new(Some(open_tx)));

        data_channel.on_open(Box::new(move || {
            let open_tx = open_tx.clone();
            Box::pin(async move {
                if let Some(tx) = open_tx.lock().await.take() {
                    let _ = tx.send(()).await;
                }
            })
        }));

        // Wait for channel to open with timeout
        tokio::time::timeout(std::time::Duration::from_secs(10), open_rx.recv())
            .await
            .map_err(|_| anyhow!("Timeout waiting for data channel to open"))?;

        Ok((peer_connection, data_channel))
    }

    async fn send_and_receive(
        &self,
        data_channel: &Arc<RTCDataChannel>,
        request: Value,
    ) -> Result<Value> {
        let request_bytes = serde_json::to_vec(&request)?;

        // Set up receiver before sending
        let (response_tx, mut response_rx) = mpsc::channel::<Result<Value>>(1);
        let response_tx = Arc::new(Mutex::new(Some(response_tx)));

        data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
            let response_tx = response_tx.clone();
            Box::pin(async move {
                if let Some(tx) = response_tx.lock().await.take() {
                    let result = serde_json::from_slice::<Value>(&msg.data)
                        .map_err(|e| anyhow!("Failed to parse response: {}", e));
                    let _ = tx.send(result).await;
                }
            })
        }));

        // Send request
        data_channel.send(&request_bytes.into()).await?;

        // Wait for response with timeout
        let response_result =
            tokio::time::timeout(std::time::Duration::from_secs(30), response_rx.recv())
                .await
                .map_err(|_| anyhow!("Timeout waiting for response"))?;

        let response = match response_result {
            Some(Ok(value)) => value,
            Some(Err(e)) => return Err(e),
            None => return Err(anyhow!("No response received")),
        };

        Ok(response)
    }
}

#[async_trait]
impl ClientTransport for WebRtcTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        let webrtc_prov = prov
            .as_any()
            .downcast_ref::<WebRtcProvider>()
            .ok_or_else(|| anyhow!("Provider is not a WebRtcProvider"))?;

        // Establish connection and request tool list
        let (_peer_connection, data_channel) = self.create_data_channel(webrtc_prov).await?;

        let request = serde_json::json!({
            "method": "list_tools",
            "params": {}
        });

        let response = self.send_and_receive(&data_channel, request).await?;

        // Parse tools from response
        let tools_array = response
            .get("tools")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("Invalid tools response"))?;

        let default_schema = Self::default_schema();
        let mut tools = Vec::new();

        for tool_value in tools_array {
            if let Ok(mut tool) = serde_json::from_value::<Tool>(tool_value.clone()) {
                // Set defaults if not present
                if tool.inputs.type_.is_empty() {
                    tool.inputs = default_schema.clone();
                }
                if tool.outputs.type_.is_empty() {
                    tool.outputs = default_schema.clone();
                }
                tools.push(tool);
            }
        }

        Ok(tools)
    }

    async fn deregister_tool_provider(&self, prov: &dyn Provider) -> Result<()> {
        let webrtc_prov = prov
            .as_any()
            .downcast_ref::<WebRtcProvider>()
            .ok_or_else(|| anyhow!("Provider is not a WebRtcProvider"))?;

        // Remove cached connection
        let mut connections = self.connections.lock().await;
        if let Some(pc) = connections.remove(&webrtc_prov.base.name) {
            pc.close().await?;
        }

        Ok(())
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value> {
        let webrtc_prov = prov
            .as_any()
            .downcast_ref::<WebRtcProvider>()
            .ok_or_else(|| anyhow!("Provider is not a WebRtcProvider"))?;

        // Get or create connection
        let connections = self.connections.lock().await;
        let connection_key = webrtc_prov.base.name.clone();

        let (_peer_connection, data_channel) = if let Some(pc) = connections.get(&connection_key) {
            // Check if connection is still alive
            if pc.connection_state() == RTCPeerConnectionState::Connected {
                // Try to get existing data channel
                // Note: In practice, you'd store the data channel ref as well
                // For now, we'll create a new connection
                drop(connections);
                self.create_data_channel(webrtc_prov).await?
            } else {
                drop(connections);
                self.create_data_channel(webrtc_prov).await?
            }
        } else {
            drop(connections);
            let (pc, dc) = self.create_data_channel(webrtc_prov).await?;
            // Cache the connection
            let mut connections = self.connections.lock().await;
            connections.insert(connection_key.clone(), pc.clone());
            (pc, dc)
        };

        // Send tool call request
        let request = serde_json::json!({
            "method": "call_tool",
            "params": {
                "tool": tool_name,
                "args": args,
            }
        });

        let response = self.send_and_receive(&data_channel, request).await?;

        // Extract result
        if let Some(error) = response.get("error") {
            return Err(anyhow!("Tool execution error: {}", error));
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("No result in response"))
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let webrtc_prov = prov
            .as_any()
            .downcast_ref::<WebRtcProvider>()
            .ok_or_else(|| anyhow!("Provider is not a WebRtcProvider"))?;

        let (_peer_connection, data_channel) = self.create_data_channel(webrtc_prov).await?;

        // Send streaming request
        let request = serde_json::json!({
            "method": "call_tool_stream",
            "params": {
                "tool": tool_name,
                "args": args,
            }
        });

        let request_bytes = serde_json::to_vec(&request)?;
        data_channel.send(&request_bytes.into()).await?;

        // Set up streaming receiver
        let (tx, rx) = mpsc::channel(16);

        data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
            let tx = tx.clone();
            Box::pin(async move {
                let parsed = serde_json::from_slice::<Value>(&msg.data)
                    .map_err(|e| anyhow!("Failed to parse stream item: {}", e));
            })
        }));

        Ok(boxed_channel_stream(rx, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ApiKeyAuth, AuthType, BasicAuth};

    #[test]
    fn test_default_schema() {
        let schema = WebRtcTransport::default_schema();
        assert_eq!(schema.type_, "object");
        assert!(schema.properties.is_none());
    }

    #[test]
    fn test_apply_auth_api_key_header() {
        let transport = WebRtcTransport::new();
        let auth = AuthConfig::ApiKey(ApiKeyAuth {
            auth_type: AuthType::ApiKey,
            api_key: "secret".to_string(),
            var_name: "X-API-Key".to_string(),
            location: "header".to_string(),
        });

        let builder = reqwest::Client::new().get("http://example.com");
        let builder = transport.apply_auth(builder, &auth).unwrap();
        let request = builder.build().unwrap();

        assert_eq!(
            request
                .headers()
                .get("X-API-Key")
                .unwrap()
                .to_str()
                .unwrap(),
            "secret"
        );
    }

    #[test]
    fn test_apply_auth_api_key_query() {
        let transport = WebRtcTransport::new();
        let auth = AuthConfig::ApiKey(ApiKeyAuth {
            auth_type: AuthType::ApiKey,
            api_key: "secret".to_string(),
            var_name: "key".to_string(),
            location: "query".to_string(),
        });

        let builder = reqwest::Client::new().get("http://example.com");
        let builder = transport.apply_auth(builder, &auth).unwrap();
        let request = builder.build().unwrap();

        assert_eq!(request.url().query(), Some("key=secret"));
    }

    #[test]
    fn test_apply_auth_basic() {
        let transport = WebRtcTransport::new();
        let auth = AuthConfig::Basic(BasicAuth {
            auth_type: AuthType::Basic,
            username: "user".to_string(),
            password: "pass".to_string(),
        });

        let builder = reqwest::Client::new().get("http://example.com");
        let builder = transport.apply_auth(builder, &auth).unwrap();
        let request = builder.build().unwrap();

        // Basic auth header is "Basic <base64(user:pass)>"
        // user:pass -> dXNlcjpwYXNz
        assert_eq!(
            request
                .headers()
                .get("Authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            "Basic dXNlcjpwYXNz"
        );
    }

    #[test]
    fn test_transport_implements_trait() {
        fn assert_client_transport<T: ClientTransport>() {}
        assert_client_transport::<WebRtcTransport>();
    }
}
