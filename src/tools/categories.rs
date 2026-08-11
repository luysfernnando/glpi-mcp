use rmcp::handler::server::wrapper::Json;
use rmcp::tool_router;
use serde_json::Value;

use crate::server::GlpiServer;

#[tool_router(router = categories_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List all ITIL categories (Incident, Request, Change, Problem)")]
    pub async fn list_itil_categories(&self) -> Result<Json<Value>, String> {
        self.client.get("/ITILCategory", None).await.map(Json).map_err(|e| e.to_string())
    }
}
