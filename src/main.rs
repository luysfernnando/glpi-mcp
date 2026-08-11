use std::sync::Arc;

use glpi_mcp::client::GLPIClient;
use glpi_mcp::config::GlpiConfig;
use glpi_mcp::labels::Labels;
use glpi_mcp::server::GlpiServer;
use rmcp::transport::stdio;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = GlpiConfig::from_env()?;
    let labels = Arc::new(Labels::for_language(config.language));
    let client = Arc::new(GLPIClient::new(config)?);
    let server = GlpiServer::new(client, labels);

    server.serve(stdio()).await?.waiting().await?;
    Ok(())
}
