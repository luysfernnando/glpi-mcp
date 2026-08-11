use glpi_mcp::client::GLPIClient;
use glpi_mcp::config::{GlpiConfig, GlpiVersion, Language};
use serde_json::json;
use wiremock::matchers::{method, path};
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

async fn mock_init_session(server: &MockServer, token: &str) {
    Mock::given(method("GET"))
        .and(path("/apirest.php/initSession"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "session_token": token })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn lazily_initializes_session_then_reuses_it() {
    let server = MockServer::start().await;
    mock_init_session(&server, "tok-1").await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/Ticket"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": 1 }])))
        .mount(&server)
        .await;

    let client = GLPIClient::new(config_for(&server)).unwrap();
    let result = client.get("/Ticket", None).await.unwrap();
    assert_eq!(result, json!([{ "id": 1 }]));
}

#[tokio::test]
async fn renews_session_on_401_and_retries() {
    let server = MockServer::start().await;
    mock_init_session(&server, "tok-1").await;

    Mock::given(method("GET"))
        .and(path("/apirest.php/Ticket"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/Ticket"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": 42 }])))
        .mount(&server)
        .await;

    let client = GLPIClient::new(config_for(&server)).unwrap();
    let result = client.get("/Ticket", None).await.unwrap();
    assert_eq!(result, json!([{ "id": 42 }]));
}

#[tokio::test]
async fn renews_session_on_error_array_shape() {
    let server = MockServer::start().await;
    mock_init_session(&server, "tok-1").await;

    Mock::given(method("GET"))
        .and(path("/apirest.php/Ticket"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!(["ERROR_SESSION_TOKEN_INVALID", "session gone"])),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/Ticket"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": 7 }])))
        .mount(&server)
        .await;

    let client = GLPIClient::new(config_for(&server)).unwrap();
    let result = client.get("/Ticket", None).await.unwrap();
    assert_eq!(result, json!([{ "id": 7 }]));
}

#[tokio::test]
async fn surfaces_object_shaped_api_errors() {
    let server = MockServer::start().await;
    mock_init_session(&server, "tok-1").await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/Ticket/999"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({ "error": "ERROR_ITEM_NOT_FOUND", "message": "no such ticket" })),
        )
        .mount(&server)
        .await;

    let client = GLPIClient::new(config_for(&server)).unwrap();
    let err = client.get("/Ticket/999", None).await.unwrap_err();
    assert!(matches!(err, glpi_mcp::error::GlpiError::Api { .. }));
}

#[tokio::test]
async fn resolves_search_field_id_via_discovery() {
    let server = MockServer::start().await;
    mock_init_session(&server, "tok-1").await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/listSearchOptions/KnowbaseItem"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "12": { "field": "name" },
            "13": { "field": "answer" },
        })))
        .mount(&server)
        .await;

    let client = GLPIClient::new(config_for(&server)).unwrap();
    let id = client
        .resolve_search_field_id("KnowbaseItem", "name", "6")
        .await;
    assert_eq!(id, "12");
}

#[tokio::test]
async fn falls_back_to_default_field_id_when_discovery_unavailable() {
    let server = MockServer::start().await;
    mock_init_session(&server, "tok-1").await;
    Mock::given(method("GET"))
        .and(path("/apirest.php/listSearchOptions/KnowbaseItem"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = GLPIClient::new(config_for(&server)).unwrap();
    let id = client
        .resolve_search_field_id("KnowbaseItem", "name", "6")
        .await;
    assert_eq!(id, "6");
}
