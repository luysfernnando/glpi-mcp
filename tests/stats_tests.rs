use std::sync::Arc;

use glpi_mcp::client::GLPIClient;
use glpi_mcp::config::{GlpiConfig, GlpiVersion, Language};
use glpi_mcp::labels::Labels;
use glpi_mcp::server::GlpiServer;
use serde_json::{Value, json};
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

fn search_row(status: i64) -> Value {
    json!({ "12": status.to_string() })
}

#[tokio::test]
async fn stats_by_status_paginates_across_multiple_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/initSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "session_token": "tok" })))
        .mount(&server)
        .await;

    // 550 tickets: a full 500-item page (PAGE_SIZE) plus a 50-item page, exercising
    // the pagination loop (fetch_ticket_rows) instead of a single unbounded range fetch.
    let page_1: Vec<Value> = (0..500).map(|_| search_row(1)).collect();
    let page_2: Vec<Value> = (0..50).map(|_| search_row(2)).collect();

    Mock::given(method("GET"))
        .and(path("/apirest.php/search/Ticket"))
        .and(query_param("range", "0-499"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": page_1 })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/search/Ticket"))
        .and(query_param("range", "500-999"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": page_2 })))
        .mount(&server)
        .await;

    let client = Arc::new(GLPIClient::new(config_for(&server)).unwrap());
    let labels = Arc::new(Labels::for_language(Language::En));
    let glpi_server = GlpiServer::new(client, labels);

    let result = glpi_server.stats_by_status().await.unwrap();
    assert!(result.contains("Total tickets: 550"));
    assert!(result.contains("| New | 500 |"));
    assert!(result.contains("| In progress (assigned) | 50 |"));
}
