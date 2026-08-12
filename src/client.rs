use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Method, StatusCode};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::config::GlpiConfig;
use crate::error::GlpiError;

const SESSION_EXPIRED_CODES: [&str; 2] =
    ["ERROR_SESSION_TOKEN_INVALID", "ERROR_SESSION_TOKEN_MISSING"];

/// Async GLPI REST client, compatible with GLPI 10 and 11.
///
/// Session token and search-option field-id cache are behind `RwLock`s instead of
/// the Python original's module-global `_session_token` — safe to share across
/// concurrently-running tool calls via a single `Arc<GLPIClient>`.
pub struct GLPIClient {
    config: GlpiConfig,
    http: reqwest::Client,
    session_token: RwLock<Option<String>>,
    pub(crate) search_options: RwLock<HashMap<String, Arc<HashMap<String, String>>>>,
}

enum ResponseOutcome {
    Success(Value),
    SessionExpired { code: String, message: String },
    ApiError { code: String, message: String },
    Invalid(String),
}

impl GLPIClient {
    pub fn new(config: GlpiConfig) -> Result<Self, GlpiError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .danger_accept_invalid_certs(!config.verify_tls)
            .build()?;

        Ok(Self {
            config,
            http,
            session_token: RwLock::new(None),
            search_options: RwLock::new(HashMap::new()),
        })
    }

    pub async fn get(
        &self,
        endpoint: &str,
        query: Option<&[(String, String)]>,
    ) -> Result<Value, GlpiError> {
        self.request(Method::GET, endpoint, query, None).await
    }

    pub async fn post(&self, endpoint: &str, body: &Value) -> Result<Value, GlpiError> {
        self.request(Method::POST, endpoint, None, Some(body)).await
    }

    pub async fn put(&self, endpoint: &str, body: &Value) -> Result<Value, GlpiError> {
        self.request(Method::PUT, endpoint, None, Some(body)).await
    }

    pub async fn delete(&self, endpoint: &str) -> Result<Value, GlpiError> {
        self.request(Method::DELETE, endpoint, None, None).await
    }

    /// Closes the active GLPI session and forgets the cached token.
    /// A no-op (not a lazy `initSession`) when no session was ever opened.
    pub async fn kill_session(&self) -> Result<Value, GlpiError> {
        if self.session_token.read().await.is_none() {
            return Ok(json!({ "message": "No active session." }));
        }
        let result = self.get("/killSession", None).await;
        *self.session_token.write().await = None;
        result.map(|_| json!({ "message": "Session closed." }))
    }

    async fn request(
        &self,
        method: Method,
        endpoint: &str,
        query: Option<&[(String, String)]>,
        body: Option<&Value>,
    ) -> Result<Value, GlpiError> {
        let url = format!(
            "{}{}{}",
            self.config.base_url,
            self.config.version.api_prefix(),
            endpoint
        );

        let token = self.ensure_session().await?;
        let (status, text) = self
            .send_once(method.clone(), &url, query, body, &token)
            .await?;
        let outcome = classify(status, &text);

        let outcome = if status == StatusCode::UNAUTHORIZED
            || matches!(outcome, ResponseOutcome::SessionExpired { .. })
        {
            let token = self.renew_session().await?;
            let (status, text) = self.send_once(method, &url, query, body, &token).await?;
            classify(status, &text)
        } else {
            outcome
        };

        match outcome {
            ResponseOutcome::Success(mut value) => {
                crate::compact::compact_value(&mut value);
                Ok(value)
            }
            ResponseOutcome::SessionExpired { code, message } => {
                Err(GlpiError::SessionInvalid { code, message })
            }
            ResponseOutcome::ApiError { code, message } => Err(GlpiError::Api { code, message }),
            ResponseOutcome::Invalid(detail) => Err(GlpiError::InvalidResponse(detail)),
        }
    }

    async fn send_once(
        &self,
        method: Method,
        url: &str,
        query: Option<&[(String, String)]>,
        body: Option<&Value>,
        session_token: &str,
    ) -> Result<(StatusCode, String), GlpiError> {
        let mut req = self
            .http
            .request(method, url)
            .header("App-Token", self.config.app_token.expose_secret())
            .header("Session-Token", session_token)
            .header("Content-Type", "application/json");

        if let Some(query) = query {
            req = req.query(query);
        }
        if let Some(body) = body {
            req = req.json(body);
        }

        let resp = req.send().await.map_err(|err| {
            if err.is_timeout() {
                GlpiError::Timeout
            } else {
                GlpiError::Http(err)
            }
        })?;
        let status = resp.status();
        let text = resp.text().await?;
        Ok((status, text))
    }

    async fn ensure_session(&self) -> Result<String, GlpiError> {
        if let Some(token) = self.session_token.read().await.clone() {
            return Ok(token);
        }
        self.renew_session().await
    }

    async fn renew_session(&self) -> Result<String, GlpiError> {
        let mut guard = self.session_token.write().await;
        let token = self.init_session().await?;
        *guard = Some(token.clone());
        Ok(token)
    }

    async fn init_session(&self) -> Result<String, GlpiError> {
        let url = format!(
            "{}{}/initSession",
            self.config.base_url,
            self.config.version.api_prefix()
        );

        let resp = self
            .http
            .get(&url)
            .header("App-Token", self.config.app_token.expose_secret())
            .header(
                "Authorization",
                format!("user_token {}", self.config.user_token.expose_secret()),
            )
            .header("Content-Type", "application/json")
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await?;

        let value: Value = serde_json::from_str(&text).map_err(|_| {
            GlpiError::InvalidResponse(format!(
                "initSession HTTP {status}: {}",
                truncate(&text, 500)
            ))
        })?;

        if !status.is_success() {
            return Err(GlpiError::InvalidResponse(format!(
                "initSession failed with HTTP {status}: {}",
                truncate(&text, 500)
            )));
        }

        value
            .get("session_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                GlpiError::InvalidResponse("initSession response missing session_token".into())
            })
    }
}

fn classify(status: StatusCode, text: &str) -> ResponseOutcome {
    match serde_json::from_str::<Value>(text) {
        Err(_) => {
            if status.is_success() {
                ResponseOutcome::Success(json!({ "message": "Success (empty response)" }))
            } else {
                ResponseOutcome::Invalid(format!("HTTP {status}: {}", truncate(text, 500)))
            }
        }
        Ok(Value::Array(items)) if is_error_array(&items) => {
            let code = items[0].as_str().unwrap_or_default().to_string();
            let message = items[1].as_str().unwrap_or_default().to_string();
            if SESSION_EXPIRED_CODES.contains(&code.as_str()) {
                ResponseOutcome::SessionExpired { code, message }
            } else {
                ResponseOutcome::ApiError { code, message }
            }
        }
        Ok(value) => {
            if status.is_success() {
                ResponseOutcome::Success(value)
            } else {
                ResponseOutcome::ApiError {
                    code: format!("HTTP {status}"),
                    message: value.to_string(),
                }
            }
        }
    }
}

fn is_error_array(items: &[Value]) -> bool {
    items.len() == 2
        && items[0]
            .as_str()
            .is_some_and(|code| code.starts_with("ERROR"))
}

fn truncate(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        let head: String = text.chars().take(max_len).collect();
        format!("{head}…")
    }
}
