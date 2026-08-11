use secrecy::SecretString;

use crate::error::GlpiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlpiVersion {
    V10,
    V11,
}

impl GlpiVersion {
    /// REST API path prefix that differs between GLPI 10 and 11.
    pub fn api_prefix(&self) -> &'static str {
        match self {
            GlpiVersion::V10 => "/apirest.php",
            GlpiVersion::V11 => "/api.php/v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Fr,
}

#[derive(Clone)]
pub struct GlpiConfig {
    pub base_url: String,
    pub app_token: SecretString,
    pub user_token: SecretString,
    pub version: GlpiVersion,
    pub verify_tls: bool,
    pub language: Language,
}

impl GlpiConfig {
    pub fn from_env() -> Result<Self, GlpiError> {
        let _ = dotenvy::dotenv();

        let base_url = require_env("GLPI_URL")?
            .trim_end_matches('/')
            .to_string();
        let app_token = require_env("GLPI_APP_TOKEN")?.into();
        let user_token = require_env("GLPI_USER_TOKEN")?.into();

        let version = match std::env::var("GLPI_VERSION").as_deref() {
            Ok("11") => GlpiVersion::V11,
            Ok("10") | Err(_) => GlpiVersion::V10,
            Ok(other) => {
                return Err(GlpiError::InvalidConfig(format!(
                    "GLPI_VERSION must be \"10\" or \"11\", got \"{other}\""
                )));
            }
        };

        let verify_tls = parse_bool(std::env::var("GLPI_VERIFY_TLS").ok().as_deref(), false);

        let language = match std::env::var("GLPI_LANG").as_deref() {
            Ok("en") => Language::En,
            _ => Language::Fr,
        };

        Ok(Self {
            base_url,
            app_token,
            user_token,
            version,
            verify_tls,
            language,
        })
    }
}

fn require_env(key: &str) -> Result<String, GlpiError> {
    std::env::var(key).map_err(|_| GlpiError::MissingEnv(key.to_string()))
}

fn parse_bool(value: Option<&str>, default: bool) -> bool {
    match value.map(str::to_lowercase).as_deref() {
        Some("true" | "1" | "yes" | "on") => true,
        Some("false" | "0" | "no" | "off") => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_prefix_differs_by_version() {
        assert_eq!(GlpiVersion::V10.api_prefix(), "/apirest.php");
        assert_eq!(GlpiVersion::V11.api_prefix(), "/api.php/v1");
    }

    #[test]
    fn parse_bool_accepts_common_truthy_falsy_forms() {
        assert!(parse_bool(Some("true"), false));
        assert!(parse_bool(Some("1"), false));
        assert!(!parse_bool(Some("false"), true));
        assert!(!parse_bool(Some("0"), true));
        assert!(parse_bool(None, true));
    }
}
