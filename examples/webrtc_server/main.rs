// WebRTC Transport Example
// This example demonstrates how to use the WebRTC transport for peer-to-peer tool calling

use anyhow::Result;
use rs_utcp::auth::{ApiKeyAuth, AuthConfig};
use rs_utcp::providers::base::{BaseProvider, ProviderType};
use rs_utcp::providers::webrtc::{IceServer, WebRtcProvider};
use rs_utcp::transports::webrtc::WebRtcTransport;
use rs_utcp::transports::ClientTransport;
use serde_json::json;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== WebRTC Transport Example ===\n");

    // 1. Configure ICE servers (STUN/TURN)
    let ice_servers = vec![
        // Public STUN server
        IceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            username: None,
            credential: None,
        },
        // Optional: Add TURN server for NAT traversal
        // IceServer {
        //     urls: vec!["turn:turn.example.com:3478".to_string()],
        //     username: Some("username".to_string()),
        //     credential: Some("password".to_string()),
        // },
    ];

    // 2. Create WebRTC provider with signaling server
    let provider = WebRtcProvider {
        base: BaseProvider {
            name: "example-webrtc-provider".to_string(),
            provider_type: ProviderType::Webrtc,
            allowed_communication_protocols: Some(vec!["webrtc".to_string()]),
            // Optional: Add authentication for signaling server
            auth: Some(AuthConfig::ApiKey(ApiKeyAuth {
                auth_type: rs_utcp::auth::AuthType::ApiKey,
                api_key: "your-api-key".to_string(),
                var_name: "Authorization".to_string(),
                location: "header".to_string(),
            })),
        },
        // Signaling server endpoint (points to the local webrtc_server example)
        signaling_server: "http://127.0.0.1:8080/offer".to_string(),
        ice_servers,
        channel_label: "utcp-data".to_string(),
        ordered: true, // Ordered delivery
        max_packet_life_time: None,
        max_retransmits: None,
    };

    // 3. Create WebRTC transport
    let transport = WebRtcTransport::new();
    println!("✓ WebRTC transport created");

    // 4. Register provider and discover available tools
    println!("\n📡 Establishing WebRTC connection and discovering tools...");
    match transport.register_tool_provider(&provider).await {
        Ok(tools) => {
            println!("✓ Connection established successfully!");
            println!("✓ Discovered {} tools:", tools.len());
            for tool in &tools {
                println!("  - {}: {}", tool.name, tool.description);
            }

            // 5. Call a tool (example)
            if !tools.is_empty() {
                let tool_name = &tools[0].name;
                println!("\n🔧 Calling tool: {}", tool_name);

                let mut args = HashMap::new();
                args.insert("input".to_string(), json!("Hello from WebRTC!"));

                match transport.call_tool(tool_name, args, &provider).await {
                    Ok(result) => {
                        println!("✓ Tool result: {}", serde_json::to_string_pretty(&result)?);
                    }
                    Err(e) => {
                        eprintln!("✗ Tool call failed: {}", e);
                    }
                }

                // 6. Demonstrate streaming (if supported)
                println!("\n🌊 Testing streaming capability...");
                let mut stream_args = HashMap::new();
                stream_args.insert("count".to_string(), json!(5));

                match transport
                    .call_tool_stream(tool_name, stream_args, &provider)
                    .await
                {
                    Ok(mut stream) => {
                        println!("✓ Stream established");
                        loop {
                            match stream.next().await {
                                Ok(Some(value)) => println!("  • Stream item: {}", value),
                                Ok(None) => break,
                                Err(e) => {
                                    eprintln!("  ✗ Stream error: {}", e);
                                    break;
                                }
                            }
                        }
                        println!("✓ Stream completed");
                    }
                    Err(e) => {
                        eprintln!("✗ Streaming failed: {}", e);
                    }
                }
            }

            // 7. Clean up
            println!("\n🧹 Cleaning up connection...");
            transport.deregister_tool_provider(&provider).await?;
            println!("✓ Connection closed");
        }
        Err(e) => {
            eprintln!("✗ Failed to establish WebRTC connection: {}", e);
            eprintln!("\nTroubleshooting:");
            eprintln!("  1. Ensure your signaling server is running and accessible");
            eprintln!("  2. Check your ICE server configuration");
            eprintln!("  3. Verify network connectivity and firewall settings");
            eprintln!("  4. Confirm authentication credentials are correct");
        }
    }

    println!("\n=== Example complete ===");
    Ok(())
}

/*
 * SIGNALING SERVER REQUIREMENTS
 *
 * Your signaling server should implement the following endpoint:
 *
 * POST /offer
 * Request:
 * {
 *   "type": "offer",
 *   "sdp": "<SDP_OFFER_STRING>"
 * }
 *
 * Response:
 * {
 *   "sdp": "<SDP_ANSWER_STRING>"
 * }
 *
 * The signaling server is responsible for:
 * 1. Accepting WebRTC offers from clients
 * 2. Forwarding offers to the peer (tool provider)
 * 3. Returning the peer's answer to the client
 *
 * Example signaling server implementations:
 * - WebSocket-based signaling
 * - HTTP REST API (as shown above)
 * - Socket.IO signaling
 *
 * TOOL PROVIDER REQUIREMENTS
 *
 * The remote peer should:
 * 1. Listen for offers via the signaling server
 * 2. Create answer and send back via signaling server
 * 3. Establish data channel with label "utcp-data"
 * 4. Listen for JSON messages on the data channel
 * 5. Respond with tool results as JSON
 *
 * Tool Call Message Format:
 * {
 *   "method": "list_tools" | "call_tool" | "call_tool_stream",
 *   "params": {
 *     "tool": "tool_name",
 *     "args": { ... }
 *   }
 * }
 *
 * Response Format:
 * {
 *   "result": { ... },
 *   "error": "error message" (optional)
 * }
 */
