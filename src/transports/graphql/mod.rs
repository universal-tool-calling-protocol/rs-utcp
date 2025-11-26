// GraphQL Transport - queries, mutations, and subscriptions
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::graphql::GraphqlProvider;
use crate::tools::{Tool, ToolInputOutputSchema};
use crate::transports::{stream::StreamResult, ClientTransport};

pub struct GraphQLTransport {
    client: Client,
}

impl GraphQLTransport {
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

        let operation_type = Self::infer_operation(&gql_prov.operation_type, tool_name);
        let operation_name = gql_prov
            .operation_name
            .clone()
            .unwrap_or_else(|| tool_name.to_string());

        // Use simple variable typing (String) for portability.
        let mut arg_defs = Vec::new();
        let mut arg_uses = Vec::new();
        for key in args.keys() {
            arg_defs.push(format!("${}: String", key));
            arg_uses.push(format!("{}: ${}", key, key));
        }

        let query = if !arg_defs.is_empty() {
            format!(
                "{} {}({}) {{ {}({}) }}",
                operation_type,
                operation_name,
                arg_defs.join(", "),
                tool_name,
                arg_uses.join(", ")
            )
        } else {
            format!("{} {{ {} }}", operation_type, tool_name)
        };

        self.execute_query(gql_prov, &query, args).await
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        Err(anyhow!(
            "GraphQL subscriptions require WebSocket transport; use WebSocket transport instead"
        ))
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
        assert_eq!(GraphQLTransport::infer_operation("Mutation", "getUser"), "mutation");
        assert_eq!(GraphQLTransport::infer_operation("subscription", "createUser"), "subscription");
        assert_eq!(GraphQLTransport::infer_operation("QUERY", "deleteUser"), "query");
    }

    #[test]
    fn infer_operation_derives_from_tool_name_when_unspecified() {
        assert_eq!(GraphQLTransport::infer_operation("", "subscription_changes"), "subscription");
        assert_eq!(GraphQLTransport::infer_operation("unknown", "createItem"), "mutation");
        assert_eq!(GraphQLTransport::infer_operation("  ", "listItems"), "query");
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

        let err = transport
            .call_tool_stream("hello", HashMap::new(), &prov)
            .await
            .err()
            .expect("stream error");
        assert!(err.to_string().contains("GraphQL subscriptions"));
    }
}
