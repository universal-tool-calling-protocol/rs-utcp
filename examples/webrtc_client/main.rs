// WebRTC Server Example
// This server demonstrates how to implement a WebRTC-based tool provider
// It includes a signaling server and WebRTC peer connection handling

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

// HTTP server for signaling
use axum::{extract::State, http::StatusCode, response::Json, routing::post, Router};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalingOffer {
    #[serde(rename = "type")]
    offer_type: String,
    sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalingAnswer {
    sdp: String,
}

#[derive(Clone)]
struct AppState {
    peer_connections: Arc<Mutex<HashMap<String, Arc<RTCPeerConnection>>>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== WebRTC Tool Provider Server ===\n");

    // Shared state
    let state = AppState {
        peer_connections: Arc::new(Mutex::new(HashMap::new())),
    };

    // Build HTTP server for signaling
    let app = Router::new()
        .route("/offer", post(handle_offer))
        .route("/health", axum::routing::get(|| async { "OK" }))
        .with_state(state);

    let addr = "127.0.0.1:8080";
    println!("🚀 Starting signaling server on {}", addr);
    println!("📡 Clients can connect to: http://{}/offer\n", addr);

    println!("✓ Server ready!\n");
    println!("Available tools:");
    println!("  - echo: Echoes back the input");
    println!("  - uppercase: Converts text to uppercase");
    println!("  - stream_numbers: Streams numbers from 1 to N\n");

    let addr_socket: std::net::SocketAddr = addr.parse()?;
    axum::Server::bind(&addr_socket)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn handle_offer(
    State(state): State<AppState>,
    Json(offer): Json<SignalingOffer>,
) -> Result<Json<SignalingAnswer>, (StatusCode, String)> {
    println!("📥 Received WebRTC offer");

    match create_peer_connection_and_answer(&state, &offer.sdp).await {
        Ok(answer_sdp) => {
            println!("✓ Created answer and established connection\n");
            Ok(Json(SignalingAnswer { sdp: answer_sdp }))
        }
        Err(e) => {
            eprintln!("✗ Failed to create answer: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create answer: {}", e),
            ))
        }
    }
}

async fn create_peer_connection_and_answer(state: &AppState, offer_sdp: &str) -> Result<String> {
    // Configure WebRTC
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create API and peer connection
    let api = APIBuilder::new().build();
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    // Set up data channel handler
    // pc_clone removed as it was unused
    peer_connection.on_data_channel(Box::new(move |data_channel: Arc<RTCDataChannel>| {
        println!("📨 Data channel opened: {}", data_channel.label());
        let dc = data_channel.clone();

        Box::pin(async move {
            // Handle incoming messages
            dc.on_message(Box::new(move |msg: DataChannelMessage| {
                let dc_clone = data_channel.clone();
                Box::pin(async move {
                    if let Err(e) = handle_tool_call(&dc_clone, &msg.data).await {
                        eprintln!("Error handling tool call: {}", e);
                    }
                })
            }));
        })
    }));

    // Set remote description (offer)
    let offer = RTCSessionDescription::offer(offer_sdp.to_string())?;
    peer_connection.set_remote_description(offer).await?;

    // Create answer
    let answer = peer_connection.create_answer(None).await?;
    peer_connection
        .set_local_description(answer.clone())
        .await?;

    // Store peer connection
    let connection_id = uuid::Uuid::new_v4().to_string();
    state
        .peer_connections
        .lock()
        .await
        .insert(connection_id, peer_connection);

    Ok(answer.sdp)
}

async fn handle_tool_call(channel: &Arc<RTCDataChannel>, data: &[u8]) -> Result<()> {
    // Parse request
    let request: Value = serde_json::from_slice(data)?;

    println!("🔧 Received tool call: {}", request);

    let method = request
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing method"))?;

    let response = match method {
        "list_tools" => handle_list_tools(),
        "call_tool" => handle_call_tool(&request).await?,
        "call_tool_stream" => {
            handle_call_tool_stream(channel, &request).await?;
            return Ok(()); // Streaming handled separately
        }
        _ => json!({
            "error": format!("Unknown method: {}", method)
        }),
    };

    // Send response
    let response_bytes = serde_json::to_vec(&response)?;
    channel.send(&response_bytes.into()).await?;

    Ok(())
}

fn handle_list_tools() -> Value {
    json!({
        "tools": [
            {
                "name": "echo",
                "description": "Echoes back the input text",
                "inputs": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Text to echo"
                        }
                    },
                    "required": ["text"]
                },
                "outputs": {
                    "type": "object"
                }
            },
            {
                "name": "uppercase",
                "description": "Converts text to uppercase",
                "inputs": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Text to convert"
                        }
                    },
                    "required": ["text"]
                },
                "outputs": {
                    "type": "object"
                }
            },
            {
                "name": "stream_numbers",
                "description": "Streams numbers from 1 to count",
                "inputs": {
                    "type": "object",
                    "properties": {
                        "count": {
                            "type": "integer",
                            "description": "How many numbers to stream"
                        }
                    },
                    "required": ["count"]
                },
                "outputs": {
                    "type": "object"
                }
            }
        ]
    })
}

async fn handle_call_tool(request: &Value) -> Result<Value> {
    let params = request
        .get("params")
        .ok_or_else(|| anyhow!("Missing params"))?;

    let tool_name = params
        .get("tool")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing tool name"))?;

    let args = params
        .get("args")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow!("Missing args"))?;

    println!("  Tool: {}", tool_name);
    println!("  Args: {}", serde_json::to_string(args)?);

    let result = match tool_name {
        "echo" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            json!({ "result": text })
        }
        "uppercase" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            json!({ "result": text.to_uppercase() })
        }
        _ => json!({ "error": format!("Unknown tool: {}", tool_name) }),
    };

    println!("  ✓ Result: {}\n", result);
    Ok(json!({ "result": result }))
}

async fn handle_call_tool_stream(channel: &Arc<RTCDataChannel>, request: &Value) -> Result<()> {
    let params = request
        .get("params")
        .ok_or_else(|| anyhow!("Missing params"))?;

    let tool_name = params
        .get("tool")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing tool name"))?;

    let args = params
        .get("args")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow!("Missing args"))?;

    println!("  🌊 Streaming tool: {}", tool_name);

    match tool_name {
        "stream_numbers" => {
            let count = args.get("count").and_then(|v| v.as_i64()).unwrap_or(5);

            for i in 1..=count {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                let item = json!({ "number": i });
                let item_bytes = serde_json::to_vec(&item)?;
                channel.send(&item_bytes.into()).await?;
                println!("    → Sent: {}", i);
            }
            println!("  ✓ Stream complete\n");
        }
        _ => {
            let error = json!({ "error": format!("Tool {} doesn't support streaming", tool_name) });
            let error_bytes = serde_json::to_vec(&error)?;
            channel.send(&error_bytes.into()).await?;
        }
    }

    Ok(())
}
