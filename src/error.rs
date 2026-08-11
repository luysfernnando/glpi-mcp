use thiserror::Error;

#[derive(Debug, Error)]
pub enum GlpiError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("request timed out")]
    Timeout,

    #[error("GLPI session invalid: {code} — {message}")]
    SessionInvalid { code: String, message: String },

    #[error("GLPI API error {code}: {message}")]
    Api { code: String, message: String },

    #[error("failed to parse GLPI response: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("unexpected GLPI response shape: {0}")]
    InvalidResponse(String),

    #[error("missing required environment variable: {0}")]
    MissingEnv(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

impl GlpiError {
    /// True when GLPI reported an expired/invalid session token and a single re-auth retry is warranted.
    pub fn is_session_expired(&self) -> bool {
        matches!(self, GlpiError::SessionInvalid { .. })
    }
}
