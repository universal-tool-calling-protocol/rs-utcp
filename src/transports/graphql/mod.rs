// GraphQL Transport - queries, mutations, and subscriptions
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use base64::Engine;
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::graphql::GraphqlProvider;
use crate::tools::{Tool, ToolInputOutputSchema};
use crate::transports::{
    stream::{boxed_channel_stream, StreamResult},
    ClientTransport,
};

/// Transport that maps GraphQL operations to UTCP tools.
pub struct GraphQLTransport {
    client: Client,
}

impl GraphQLTransport {
    /// Create a GraphQL transport using a default reqwest client.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
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

    fn infer_operation(operation_type: &str, tool_name: &str) -> String {
        let op = operation_type.trim().to_lowercase();
        match op.as_str() {
            "query" | "mutation" | "subscription" => return op,
            _ => {}
        }

        let tool = tool_name.to_lowercase();
        if tool.starts_with("subscription")
            || tool.starts_with("subscribe")
            || tool.starts_with("on_")
        {
            return "subscription".to_string();
        }
        if tool.starts_with("mutation")
            || tool.starts_with("create")
            || tool.starts_with("update")
            || tool.starts_with("delete")
        {
            return "mutation".to_string();
        }

        "query".to_string()
    }

    fn normalize_arg_value(key: &str, value: Value) -> (String, Value) {
        match value {
            Value::Bool(_) => ("Boolean!".to_string(), value),
            Value::Number(num) => {
                if let Some(int_val) = num.as_i64() {
                    if int_val >= i64::from(i32::MIN) && int_val <= i64::from(i32::MAX) {
                        return ("Int!".to_string(), Value::Number(num));
                    }
                }
                ("Float!".to_string(), Value::Number(num))
            }
            Value::String(s) => {
                let maybe_id = key.to_ascii_lowercase().ends_with("_id");
                let type_name = if maybe_id { "ID!" } else { "String!" };
                (type_name.to_string(), Value::String(s))
            }
            Value::Array(_) | Value::Object(_) => {
                // Serialize complex values as JSON strings for portability.
                let serialized = value.to_string();
                ("String!".to_string(), Value::String(serialized))
            }
            Value::Null => ("String".to_string(), Value::Null),
        }
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
                    "cookie" => Ok(builder.header(
                        "cookie",
                        format!("{}={}", api_key.var_name, api_key.api_key),
                    )),
                    other => Err(anyhow!("Unsupported API key location: {}", other)),
                }
            }
            AuthConfig::Basic(basic) => {
                Ok(builder.basic_auth(&basic.username, Some(&basic.password)))
            }
            AuthConfig::OAuth2(_) => Err(anyhow!(
                "OAuth2 auth is not yet supported by the GraphQL transport"
            )),
        }
    }

    async fn execute_query(
        &self,
        prov: &GraphqlProvider,
        query: &str,
        variables: HashMap<String, Value>,
    ) -> Result<Value> {
        let mut req = self
            .client
            .post(&prov.url)
            .json(&json!({ "query": query, "variables": variables }));
        if let Some(headers) = &prov.headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }
        if let Some(auth) = &prov.base.auth {
            req = self.apply_auth(req, auth)?;
        }

        let response = req.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("GraphQL request failed: {}", response.status()));
        }

        let result: Value = response.json().await?;
        if let Some(errors) = result.get("errors") {
            return Err(anyhow!("GraphQL errors: {}", errors));
        }

        result
            .get("data")
            .cloned()
            .ok_or_else(|| anyhow!("No data in GraphQL response"))
    }
}

#[async_trait]
impl ClientTransport for GraphQLTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        let gql_prov = prov
            .as_any()
            .downcast_ref::<GraphqlProvider>()
            .ok_or_else(|| anyhow!("Provider is not a GraphqlProvider"))?;

        // Basic introspection to list available operations.
        let introspection = r#"
        query IntrospectionQuery {
          __schema {
            queryType { fields { name description } }
            mutationType { fields { name description } }
            subscriptionType { fields { name description } }
          }
        }"#;

        let response = self
            .execute_query(gql_prov, introspection, HashMap::new())
            .await;

        if response.is_err() {
            return Ok(vec![]);
        }
        let response = response.unwrap_or_default();

        let mut tools = Vec::new();
        let default_schema = Self::default_schema();

        if let Some(schema) = response.get("__schema") {
            for (op_type, key) in [
                ("query", "queryType"),
                ("mutation", "mutationType"),
                ("subscription", "subscriptionType"),
            ] {
                if let Some(fields) = schema
                    .get(key)
                    .and_then(|v| v.get("fields"))
                    .and_then(|v| v.as_array())
                {
                    for field in fields {
                        if let Some(name) = field.get("name").and_then(|v| v.as_str()) {
                            let description = field
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            tools.push(Tool {
                                name: name.to_string(),
                                description,
                                inputs: default_schema.clone(),
                                outputs: default_schema.clone(),
                                tags: vec![op_type.to_string()],
                                average_response_size: None,
                                provider: None,
                            });
                        }
                    }
                }
            }
        }

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
        let gql_prov = prov
            .as_any()
            .downcast_ref::<GraphqlProvider>()
            .ok_or_else(|| anyhow!("Provider is not a GraphqlProvider"))?;

        let call_name = tool_name
            .strip_prefix(&format!("{}.", gql_prov.base.name))
            .unwrap_or(tool_name);

        let operation_type = Self::infer_operation(&gql_prov.operation_type, call_name);
        let operation_name = gql_prov
            .operation_name
            .clone()
            .unwrap_or_else(|| call_name.to_string());

        // Use simple variable typing (String) for portability.
        let mut arg_defs = Vec::new();
        let mut arg_uses = Vec::new();
        let mut variables = HashMap::new();

        for (key, value) in args {
            let (type_name, normalized_value) = Self::normalize_arg_value(&key, value);
            arg_defs.push(format!("${}: {}", key, type_name));
            arg_uses.push(format!("{}: ${}", key, key));
            variables.insert(key, normalized_value);
        }

        let query = if !arg_defs.is_empty() {
            format!(
                "{} {}({}) {{ {}({}) }}",
                operation_type,
                operation_name,
                arg_defs.join(", "),
                call_name,
                arg_uses.join(", ")
            )
        } else {
            format!("{} {{ {} }}", operation_type, call_name)
        };

        self.execute_query(gql_prov, &query, variables).await
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let gql_prov = prov
            .as_any()
            .downcast_ref::<GraphqlProvider>()
            .ok_or_else(|| anyhow!("Provider is not a GraphqlProvider"))?;

        let call_name = tool_name
            .strip_prefix(&format!("{}.", gql_prov.base.name))
            .unwrap_or(tool_name);

        let operation_type = Self::infer_operation(&gql_prov.operation_type, call_name);

        // GraphQL subscriptions must be sent over WebSocket
        if operation_type != "subscription" {
            return Err(anyhow!(
                "call_tool_stream is only for GraphQL subscriptions; '{}' is a {}",
                call_name,
                operation_type
            ));
        }

        let operation_name = gql_prov
            .operation_name
            .clone()
            .unwrap_or_else(|| call_name.to_string());

        // Build the subscription query with variables
        let mut arg_defs = Vec::new();
        let mut arg_uses = Vec::new();
        let mut variables = HashMap::new();

        for (key, value) in args {
            let (type_name, normalized_value) = Self::normalize_arg_value(&key, value);
            arg_defs.push(format!("${}: {}", key, type_name));
            arg_uses.push(format!("{}: ${}", key, key));
            variables.insert(key, normalized_value);
        }

        let subscription_query = if !arg_defs.is_empty() {
            format!(
                "{} {}({}) {{ {}({}) }}",
                operation_type,
                operation_name,
                arg_defs.join(", "),
                call_name,
                arg_uses.join(", ")
            )
        } else {
            format!("{} {{ {} }}", operation_type, call_name)
        };

        // Convert HTTP URL to WebSocket URL
        let mut ws_url = gql_prov
            .url
            .replace("http://", "ws://")
            .replace("https://", "wss://");

        // Handle query-based authentication
        if let Some(AuthConfig::ApiKey(api_key)) = &gql_prov.base.auth {
            if api_key.location.to_ascii_lowercase() == "query" {
                let separator = if ws_url.contains('?') { "&" } else { "?" };
                ws_url = format!(
                    "{}{}{}={}",
                    ws_url, separator, api_key.var_name, api_key.api_key
                );
            }
        }

        // Build the WebSocket request with proper headers
        let mut req = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("Host", ws_url.split('/').nth(2).unwrap_or("localhost"))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .header("Sec-WebSocket-Protocol", "graphql-transport-ws")
            .body(())?;

        // Apply authentication to WebSocket request (except query which was handled above)
        if let Some(auth) = &gql_prov.base.auth {
            match auth {
                AuthConfig::ApiKey(api_key) => {
                    let location = api_key.location.to_ascii_lowercase();
                    match location.as_str() {
                        "header" => {
                            use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
                            let name = HeaderName::from_bytes(api_key.var_name.as_bytes())
                                .map_err(|_| anyhow!("Invalid header name"))?;
                            let value = HeaderValue::from_str(&api_key.api_key)
                                .map_err(|_| anyhow!("Invalid header value"))?;
                            req.headers_mut().insert(name, value);
                        }
                        "cookie" => {
                            use tokio_tungstenite::tungstenite::http::HeaderValue;
                            let cookie_value = format!("{}={}", api_key.var_name, api_key.api_key);
                            let value = HeaderValue::from_str(&cookie_value)
                                .map_err(|_| anyhow!("Invalid cookie value"))?;
                            req.headers_mut().insert("cookie", value);
                        }
                        "query" => {
                            // Already handled above
                        }
                        other => {
                            return Err(anyhow!(
                                "Unsupported API key location for WebSocket: {}",
                                other
                            ))
                        }
                    }
                }
                AuthConfig::Basic(basic) => {
                    use tokio_tungstenite::tungstenite::http::HeaderValue;
                    let credentials = format!("{}:{}", basic.username, basic.password);
                    let encoded =
                        base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
                    let value = HeaderValue::from_str(&format!("Basic {}", encoded))
                        .map_err(|_| anyhow!("Invalid auth header"))?;
                    req.headers_mut().insert("authorization", value);
                }
                AuthConfig::OAuth2(_) => {
                    return Err(anyhow!(
                        "OAuth2 is not supported for GraphQL WebSocket subscriptions"
                    ));
                }
            }
        }

        // Apply custom headers if any
        if let Some(headers) = &gql_prov.headers {
            use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
            for (k, v) in headers {
                let name = HeaderName::from_bytes(k.as_bytes())
                    .map_err(|_| anyhow!("Invalid header name: {}", k))?;
                let value =
                    HeaderValue::from_str(v).map_err(|_| anyhow!("Invalid header value: {}", v))?;
                req.headers_mut().insert(name, value);
            }
        }

        let (mut ws_stream, _) = connect_async(req).await?;

        // Send connection_init message (graphql-transport-ws protocol)
        ws_stream
            .send(Message::Text(
                json!({
                    "type": "connection_init"
                })
                .to_string(),
            ))
            .await?;

        // Wait for connection_ack
        if let Some(msg) = ws_stream.next().await {
            match msg? {
                Message::Text(text) => {
                    let ack: Value = serde_json::from_str(&text)?;
                    if ack.get("type").and_then(|v| v.as_str()) != Some("connection_ack") {
                        return Err(anyhow!("Expected connection_ack, got: {}", text));
                    }
                }
                _ => return Err(anyhow!("Expected text message for connection_ack")),
            }
        }

        // Send subscription message
        let subscription_id = "1"; // Simple ID for single subscription
        let subscribe_msg = json!({
            "id": subscription_id,
            "type": "subscribe",
            "payload": {
                "query": subscription_query,
                "variables": variables,
            }
        });

        ws_stream
            .send(Message::Text(subscribe_msg.to_string()))
            .await?;

        // Create channel for streaming results
        let (tx, rx) = mpsc::channel(256);

        // Spawn task to handle incoming subscription messages
        tokio::spawn(async move {
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        let parsed = match serde_json::from_str::<Value>(&text) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = tx
                                    .send(Err(anyhow!("Failed to parse WebSocket message: {}", e)))
                                    .await;
                                break;
                            }
                        };

                        let msg_type = parsed.get("type").and_then(|v| v.as_str());
                        match msg_type {
                            Some("next") => {
                                // Extract data from payload
                                if let Some(payload) = parsed.get("payload") {
                                    if let Some(data) = payload.get("data") {
                                        if tx.send(Ok(data.clone())).await.is_err() {
                                            break;
                                        }
                                    }
                                    // Check for errors in payload
                                    if let Some(errors) = payload.get("errors") {
                                        let _ = tx
                                            .send(Err(anyhow!(
                                                "GraphQL subscription error: {}",
                                                errors
                                            )))
                                            .await;
                                        break;
                                    }
                                }
                            }
                            Some("error") => {
                                let error_msg = parsed
                                    .get("payload")
                                    .map(|p| p.to_string())
                                    .unwrap_or_else(|| "Unknown error".to_string());
                                let _ = tx
                                    .send(Err(anyhow!("GraphQL subscription error: {}", error_msg)))
                                    .await;
                                break;
                            }
                            Some("complete") => {
                                // Subscription completed normally
                                break;
                            }
                            _ => {
                                // Ignore other message types (ping, pong, etc.)
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {} // Ignore binary, ping, pong
                    Err(err) => {
                        let _ = tx.send(Err(anyhow!("WebSocket error: {}", err))).await;
                        break;
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
    use axum::{extract::Json, routing::post, Router};
    use serde_json::json;
    use std::net::TcpListener;

    #[test]
    fn infer_operation_prefers_explicit_value() {
        assert_eq!(
            GraphQLTransport::infer_operation("Mutation", "getUser"),
            "mutation"
        );
        assert_eq!(
            GraphQLTransport::infer_operation("subscription", "createUser"),
            "subscription"
        );
        assert_eq!(
            GraphQLTransport::infer_operation("QUERY", "deleteUser"),
            "query"
        );
    }

    #[test]
    fn infer_operation_derives_from_tool_name_when_unspecified() {
        assert_eq!(
            GraphQLTransport::infer_operation("", "subscription_changes"),
            "subscription"
        );
        assert_eq!(
            GraphQLTransport::infer_operation("unknown", "createItem"),
            "mutation"
        );
        assert_eq!(
            GraphQLTransport::infer_operation("  ", "listItems"),
            "query"
        );
    }

    #[test]
    fn normalize_arg_value_maps_rust_types_to_graphql_scalars() {
        let (ty, value) =
            GraphQLTransport::normalize_arg_value("user_id", Value::String("abc".into()));
        assert_eq!(ty, "ID!");
        assert_eq!(value, Value::String("abc".into()));

        let (ty, value) = GraphQLTransport::normalize_arg_value("count", Value::Number(3.into()));
        assert_eq!(ty, "Int!");
        assert_eq!(value, Value::Number(3.into()));

        let (ty, value) = GraphQLTransport::normalize_arg_value(
            "price",
            Value::Number(serde_json::Number::from_f64(1.5).unwrap()),
        );
        assert_eq!(ty, "Float!");
        assert_eq!(
            value,
            Value::Number(serde_json::Number::from_f64(1.5).unwrap())
        );

        let (ty, value) =
            GraphQLTransport::normalize_arg_value("flags", serde_json::json!({"a": 1}));
        assert_eq!(ty, "String!");
        assert_eq!(value, Value::String("{\"a\":1}".into()));
    }

    #[tokio::test]
    async fn register_and_call_graphql_transport() {
        async fn handler(Json(body): Json<Value>) -> Json<Value> {
            let query_str = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
            if query_str.contains("__schema") {
                return Json(json!({
                    "data": {
                        "__schema": {
                            "queryType": { "fields": [{ "name": "hello", "description": "hi" }] },
                            "mutationType": null,
                            "subscriptionType": null
                        }
                    }
                }));
            }

            Json(json!({
                "data": {
                    "hello": {
                        "msg": "hi"
                    }
                }
            }))
        }

        let app = Router::new().route("/", post(handler));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::Server::from_tcp(listener)
                .unwrap()
                .serve(app.into_make_service())
                .await
                .unwrap();
        });

        let prov = GraphqlProvider {
            base: crate::providers::base::BaseProvider {
                name: "gql".to_string(),
                provider_type: crate::providers::base::ProviderType::Graphql,
                auth: None,
                allowed_communication_protocols: None,
            },
            url: format!("http://{}", addr),
            operation_type: "query".to_string(),
            operation_name: None,
            headers: None,
        };

        let transport = GraphQLTransport::new();
        let tools = transport
            .register_tool_provider(&prov)
            .await
            .expect("register");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "hello");

        let result = transport
            .call_tool("hello", HashMap::new(), &prov)
            .await
            .expect("call");
        assert_eq!(result["hello"]["msg"], "hi");

        // call_tool_stream should fail for queries (non-subscriptions)
        let err = transport
            .call_tool_stream("hello", HashMap::new(), &prov)
            .await
            .err()
            .expect("stream error");
        assert!(err.to_string().contains("only for GraphQL subscriptions"));
    }

    #[tokio::test]
    async fn graphql_call_strips_provider_prefix() {
        async fn handler(Json(body): Json<Value>) -> Json<Value> {
            let query = body
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            assert!(query.contains("echo"));
            assert!(
                !query.contains("gql.echo"),
                "provider prefix should be stripped before building query"
            );
            let vars = body
                .get("variables")
                .cloned()
                .unwrap_or_else(|| json!({ "missing": true }));
            Json(json!({ "data": { "echo": vars } }))
        }

        let app = Router::new().route("/graphql", post(handler));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::Server::from_tcp(listener)
                .unwrap()
                .serve(app.into_make_service())
                .await
                .unwrap();
        });

        let prov = GraphqlProvider {
            base: crate::providers::base::BaseProvider {
                name: "gql".to_string(),
                provider_type: crate::providers::base::ProviderType::Graphql,
                auth: None,
                allowed_communication_protocols: None,
            },
            url: format!("http://{}/graphql", addr),
            operation_type: "query".to_string(),
            operation_name: None,
            headers: None,
        };

        let mut args = HashMap::new();
        args.insert("msg".into(), json!("hi"));

        let transport = GraphQLTransport::new();
        let result = transport
            .call_tool("gql.echo", args.clone(), &prov)
            .await
            .expect("call tool");

        assert_eq!(result, json!({ "echo": json!({ "msg": "hi" }) }));
    }

    #[tokio::test]
    async fn graphql_subscription_streams_data() {
        use futures_util::{SinkExt, StreamExt};
        use tokio::net::TcpListener;
        use tokio_tungstenite::accept_async;

        // Create a mock GraphQL subscription server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                // Accept any WebSocket connection
                if let Ok(mut ws) = accept_async(stream).await {
                    // Receive connection_init
                    if let Some(Ok(Message::Text(text))) = ws.next().await {
                        let init: Value = serde_json::from_str(&text).unwrap();
                        if init.get("type").and_then(|v| v.as_str()) == Some("connection_init") {
                            // Send connection_ack
                            let _ = ws
                                .send(Message::Text(
                                    json!({ "type": "connection_ack" }).to_string(),
                                ))
                                .await;

                            // Receive subscription message
                            if let Some(Ok(Message::Text(text))) = ws.next().await {
                                let sub: Value = serde_json::from_str(&text).unwrap();
                                if sub.get("type").and_then(|v| v.as_str()) == Some("subscribe") {
                                    // Send multiple streaming events
                                    for i in 1..=3 {
                                        let _ = ws
                                            .send(Message::Text(
                                                json!({
                                                    "id": "1",
                                                    "type": "next",
                                                    "payload": {
                                                        "data": {
                                                            "messageAdded": {
                                                                "id": i,
                                                                "content": format!("Message {}", i)
                                                            }
                                                        }
                                                    }
                                                })
                                                .to_string(),
                                            ))
                                            .await;
                                        tokio::time::sleep(tokio::time::Duration::from_millis(10))
                                            .await;
                                    }

                                    // Send complete message
                                    let _ = ws
                                        .send(Message::Text(
                                            json!({
                                                "id": "1",
                                                "type": "complete"
                                            })
                                            .to_string(),
                                        ))
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let prov = GraphqlProvider {
            base: crate::providers::base::BaseProvider {
                name: "gql".to_string(),
                provider_type: crate::providers::base::ProviderType::Graphql,
                auth: None,
                allowed_communication_protocols: None,
            },
            url: format!("http://{}", addr),
            operation_type: "subscription".to_string(),
            operation_name: Some("MessageAdded".to_string()),
            headers: None,
        };

        let transport = GraphQLTransport::new();
        let mut stream = transport
            .call_tool_stream("messageAdded", HashMap::new(), &prov)
            .await
            .expect("stream created");

        // Collect streaming results
        let mut results = Vec::new();
        while let Ok(Some(value)) = stream.next().await {
            results.push(value);
            if results.len() >= 3 {
                break;
            }
        }

        assert_eq!(results.len(), 3);
        for (i, result) in results.iter().enumerate() {
            let expected_id = i + 1;
            assert_eq!(
                result["messageAdded"]["id"], expected_id,
                "Expected message {} to have id {}",
                i, expected_id
            );
            assert_eq!(
                result["messageAdded"]["content"],
                format!("Message {}", expected_id)
            );
        }
    }
}
