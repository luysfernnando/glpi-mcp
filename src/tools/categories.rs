use rmcp::tool_router;
use serde_json::Value;

use crate::markdown::{escape_cell, table};
use crate::server::GlpiServer;

#[tool_router(router = categories_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List all ITIL categories (Incident, Request, Change, Problem) as a Markdown table")]
    pub async fn list_itil_categories(&self) -> Result<String, String> {
        let result = self.client.get("/ITILCategory", None).await.map_err(|e| e.to_string())?;
        let items = result.as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            return Ok("No categories.".to_string());
        }

        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|cat| {
                let id = cat.get("id").and_then(Value::as_i64).map(|v| v.to_string()).unwrap_or_default();
                let name = escape_cell(cat.get("completename").or_else(|| cat.get("name")).and_then(Value::as_str).unwrap_or(""));
                vec![id, name]
            })
            .collect();

        Ok(format!("**{} categorie(s)**\n\n{}", items.len(), table(&["ID", "Name"], &rows)))
    }
}
