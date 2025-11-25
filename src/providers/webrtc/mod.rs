use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signaling_server: Option<String>,
}

impl Provider for WebRtcProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Webrtc
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WebRtcProvider {
    pub fn new(name: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Webrtc,
                auth,
            },
            signaling_server: None,
        }
    }
}
