use std::sync::Arc;

use glpi_mcp::client::GLPIClient;
use glpi_mcp::config::{GlpiConfig, GlpiVersion, Language};
use glpi_mcp::labels::Labels;
use glpi_mcp::server::GlpiServer;
use glpi_mcp::tools::kb::ListKbArticlesParams;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config_for(server: &MockServer) -> GlpiConfig {
    GlpiConfig {
        base_url: server.uri(),
        app_token: "app-token".to_string().into(),
        user_token: "user-token".to_string().into(),
        version: GlpiVersion::V10,
        verify_tls: false,
        language: Language::En,
    }
}

async fn build_server(mock_server: &MockServer) -> GlpiServer {
    Mock::given(method("GET"))
        .and(path("/apirest.php/initSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "session_token": "tok" })))
        .mount(mock_server)
        .await;
    let client = Arc::new(GLPIClient::new(config_for(mock_server)).unwrap());
    let labels = Arc::new(Labels::for_language(Language::En));
    GlpiServer::new(client, labels)
}

#[tokio::test]
async fn clamps_range_limit_when_offset_above_60() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/KnowbaseItem"))
        .and(query_param("range", "61-70"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": 1 }])))
        .mount(&mock_server)
        .await;

    let server = build_server(&mock_server).await;
    let result = server
        .list_kb_articles(Parameters(ListKbArticlesParams { range_start: 61, range_limit: 50 }))
        .await
        .unwrap()
        .0;

    assert_eq!(result["_clamped_range_limit"], 10);
    assert!(result["_warning"].is_string());
    assert_eq!(result["items"], json!([{ "id": 1 }]));
}

#[tokio::test]
async fn no_clamping_for_small_offset() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/KnowbaseItem"))
        .and(query_param("range", "0-49"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": 2 }])))
        .mount(&mock_server)
        .await;

    let server = build_server(&mock_server).await;
    let result = server
        .list_kb_articles(Parameters(ListKbArticlesParams { range_start: 0, range_limit: 50 }))
        .await
        .unwrap()
        .0;

    assert_eq!(result, json!([{ "id": 2 }]));
}
