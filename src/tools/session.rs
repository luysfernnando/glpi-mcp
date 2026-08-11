use rmcp::handler::server::wrapper::Json;
use rmcp::tool_router;
use serde_json::Value;

use crate::server::GlpiServer;

#[tool_router(router = session_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "Close the active GLPI session")]
    pub async fn kill_session(&self) -> Result<Json<Value>, String> {
        self.client.kill_session().await.map(Json).map_err(|e| e.to_string())
    }
}
