use std::collections::HashMap;

use serde_json::Value;

use crate::client::GLPIClient;

/// Runtime discovery of GLPI search-option field IDs, isolating the numeric-ID
/// drift between GLPI 10 and 11 (e.g. KnowbaseItem's title field is ID 6 on one
/// version and a different ID on the other) behind a column-name lookup.
impl GLPIClient {
    /// Resolves a search-option field ID by stable column name, falling back to
    /// `default` (the legacy GLPI 10 numbering) when discovery fails or the
    /// column is unknown — callers never need to special-case discovery errors.
    pub async fn resolve_search_field_id(
        &self,
        itemtype: &str,
        column: &str,
        default: &str,
    ) -> String {
        self.discover_search_options(itemtype)
            .await
            .get(&column.to_lowercase())
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    async fn discover_search_options(&self, itemtype: &str) -> HashMap<String, String> {
        if let Some(cached) = self.search_options.read().await.get(itemtype) {
            return cached.clone();
        }

        let mapping = self
            .fetch_search_options(itemtype)
            .await
            .unwrap_or_default();
        self.search_options
            .write()
            .await
            .insert(itemtype.to_string(), mapping.clone());
        mapping
    }

    async fn fetch_search_options(&self, itemtype: &str) -> Option<HashMap<String, String>> {
        let result = self
            .get(&format!("/listSearchOptions/{itemtype}"), None)
            .await
            .ok()?;
        let fields = result.as_object()?;

        // GLPI reports discovery-unavailable as {"error": ..., "detail": ...} on the
        // active API prefix; treat that as "no mapping" rather than a hard failure.
        if fields.contains_key("error") && fields.contains_key("detail") {
            return None;
        }

        let mut mapping = HashMap::new();
        for (field_id, meta) in fields {
            if !field_id.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let Some(column) = meta.get("field").and_then(Value::as_str) else {
                continue;
            };
            let column = column.trim().to_lowercase();
            if !column.is_empty() {
                mapping.entry(column).or_insert_with(|| field_id.clone());
            }
        }
        Some(mapping)
    }
}
