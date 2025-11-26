use std::fs;
use std::sync::Arc;

use anyhow::Result;
use rs_utcp::config::UtcpClientConfig;
use rs_utcp::repository::in_memory::InMemoryToolRepository;
use rs_utcp::tag::tag_search::TagSearchStrategy;
use rs_utcp::UtcpClient;
use serde_json::Value;
use tempfile::NamedTempFile;

/// Create a UTCP client by writing the provided provider JSON to a temp file and
/// loading it through `new`.
pub async fn client_from_providers(providers: Value) -> Result<Arc<UtcpClient>> {
    client_from_providers_with_config(providers, UtcpClientConfig::default()).await
}

/// Same as `client_from_providers`, but lets callers tweak the client config first.
pub async fn client_from_providers_with_config(
    providers: Value,
    config: UtcpClientConfig,
) -> Result<Arc<UtcpClient>> {
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

    let providers_file = NamedTempFile::new()?;
    fs::write(providers_file.path(), serde_json::to_vec(&providers)?)?;
    let config = config.with_providers_file(providers_file.path().to_path_buf());

    let client = UtcpClient::new(config, repo, search).await?;
    Ok(Arc::new(client))
}
