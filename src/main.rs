use std::sync::Arc;

use glpi_mcp::client::GLPIClient;
use glpi_mcp::config::GlpiConfig;
use glpi_mcp::labels::Labels;
use glpi_mcp::server::GlpiServer;
use rmcp::ServiceExt;
use rmcp::transport::stdio;

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

    match std::env::var("GLPI_MCP_TRANSPORT").as_deref() {
        Ok("http") => serve_http(server).await,
        _ => {
            server.serve(stdio()).await?.waiting().await?;
            Ok(())
        }
    }
}

/// Opt-in remote transport: exposes the same tools over streamable HTTP instead of
/// stdio, for setups (e.g. a shared VM) where the server runs once and multiple MCP
/// clients connect to it over the network rather than each spawning a local process.
async fn serve_http(server: GlpiServer) -> anyhow::Result<()> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };

    let bind_addr =
        std::env::var("GLPI_MCP_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8000".to_string());
    let auth_token = std::env::var("GLPI_MCP_HTTP_TOKEN").ok();

    // rmcp only accepts loopback Host headers by default (DNS-rebinding protection). Behind
    // a reverse proxy on a real hostname, that must be widened explicitly.
    let mut config = StreamableHttpServerConfig::default();
    if let Ok(hosts) = std::env::var("GLPI_MCP_ALLOWED_HOSTS") {
        let hosts: Vec<String> = hosts.split(',').map(|h| h.trim().to_string()).collect();
        config = config.with_allowed_hosts(hosts);
    }

    let ct = tokio_util::sync::CancellationToken::new();
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        config.with_cancellation_token(ct.child_token()),
    );

    let mut router = axum::Router::new().nest_service("/mcp", service);
    if let Some(token) = auth_token {
        router = router.layer(axum::middleware::from_fn(move |req, next| {
            let token = token.clone();
            async move { check_bearer_token(&token, req, next).await }
        }));
    } else {
        tracing::warn!(
            "GLPI_MCP_HTTP_TOKEN not set: /mcp is reachable by anyone who can reach {bind_addr}"
        );
    }

    tracing::info!("listening on http://{bind_addr}/mcp");
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

async fn check_bearer_token(
    expected: &str,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let provided = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if provided == Some(expected) {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
