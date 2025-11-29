use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported authentication mechanisms across transports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    ApiKey,
    Basic,
    OAuth2,
}

/// Contract implemented by auth configs to validate their state and report their type.
pub trait Auth: Send + Sync + std::fmt::Debug {
    fn auth_type(&self) -> AuthType;
    fn validate(&self) -> Result<(), AuthError>;
}

/// Validation errors emitted by auth implementations.
#[derive(Error, Debug)]
pub enum AuthError {
    #[error("API key must be provided")]
    MissingApiKey,
    #[error("Location must be 'header', 'query', or 'cookie'")]
    InvalidLocation,
    #[error("Username must be provided")]
    MissingUsername,
    #[error("Password must be provided")]
    MissingPassword,
    #[error("Token URL must be provided")]
    MissingTokenUrl,
    #[error("Client ID must be provided")]
    MissingClientId,
    #[error("Client secret must be provided")]
    MissingClientSecret,
}

/// API key authentication descriptor used by HTTP-like transports.
#[derive(Clone, Serialize, Deserialize)]
pub struct ApiKeyAuth {
    pub auth_type: AuthType,
    pub api_key: String,
    pub var_name: String,
    pub location: String, // "header", "query", or "cookie"
}

impl std::fmt::Debug for ApiKeyAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyAuth")
            .field("auth_type", &self.auth_type)
            .field("api_key", &"[REDACTED]")
            .field("var_name", &self.var_name)
            .field("location", &self.location)
            .finish()
    }
}

impl ApiKeyAuth {
    /// Build an API key auth config with common defaults.
    pub fn new(api_key: String) -> Self {
        Self {
            auth_type: AuthType::ApiKey,
            api_key,
            var_name: "X-Api-Key".to_string(),
            location: "header".to_string(),
        }
    }
}

impl Auth for ApiKeyAuth {
    fn auth_type(&self) -> AuthType {
        AuthType::ApiKey
    }

    fn validate(&self) -> Result<(), AuthError> {
        if self.api_key.is_empty() {
            return Err(AuthError::MissingApiKey);
        }
        match self.location.as_str() {
            "header" | "query" | "cookie" => Ok(()),
            _ => Err(AuthError::InvalidLocation),
        }
    }
}

/// Basic authentication descriptor.
#[derive(Clone, Serialize, Deserialize)]
pub struct BasicAuth {
    pub auth_type: AuthType,
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for BasicAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasicAuth")
            .field("auth_type", &self.auth_type)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

impl BasicAuth {
    /// Build a basic auth config from username/password.
    pub fn new(username: String, password: String) -> Self {
        Self {
            auth_type: AuthType::Basic,
            username,
            password,
        }
    }
}

impl Auth for BasicAuth {
    fn auth_type(&self) -> AuthType {
        AuthType::Basic
    }

    fn validate(&self) -> Result<(), AuthError> {
        if self.username.is_empty() {
            return Err(AuthError::MissingUsername);
        }
        if self.password.is_empty() {
            return Err(AuthError::MissingPassword);
        }
        Ok(())
    }
}

/// OAuth2 client credentials descriptor.
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuth2Auth {
    pub auth_type: AuthType,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl std::fmt::Debug for OAuth2Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuth2Auth")
            .field("auth_type", &self.auth_type)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("scope", &self.scope)
            .finish()
    }
}

impl OAuth2Auth {
    pub fn new(
        token_url: String,
        client_id: String,
        client_secret: String,
        scope: Option<String>,
    ) -> Self {
        Self {
            auth_type: AuthType::OAuth2,
            token_url,
            client_id,
            client_secret,
            scope,
        }
    }
}

impl Auth for OAuth2Auth {
    fn auth_type(&self) -> AuthType {
        AuthType::OAuth2
    }

    fn validate(&self) -> Result<(), AuthError> {
        if self.token_url.is_empty() {
            return Err(AuthError::MissingTokenUrl);
        }
        if self.client_id.is_empty() {
            return Err(AuthError::MissingClientId);
        }
        if self.client_secret.is_empty() {
            return Err(AuthError::MissingClientSecret);
        }
        Ok(())
    }
}

/// Untagged wrapper that allows serde to deserialize any auth config and still expose the `Auth` trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthConfig {
    ApiKey(ApiKeyAuth),
    Basic(BasicAuth),
    OAuth2(OAuth2Auth),
}

impl Auth for AuthConfig {
    fn auth_type(&self) -> AuthType {
        match self {
            AuthConfig::ApiKey(auth) => auth.auth_type(),
            AuthConfig::Basic(auth) => auth.auth_type(),
            AuthConfig::OAuth2(auth) => auth.auth_type(),
        }
    }

    fn validate(&self) -> Result<(), AuthError> {
        match self {
            AuthConfig::ApiKey(auth) => auth.validate(),
            AuthConfig::Basic(auth) => auth.validate(),
            AuthConfig::OAuth2(auth) => auth.validate(),
        }
    }
}
