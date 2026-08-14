use rmcp::tool_router;

use crate::server::GlpiServer;

#[tool_router(router = session_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "Close the active GLPI session")]
    pub async fn kill_session(&self) -> Result<String, String> {
        self.client
            .kill_session()
            .await
            .map(|v| v.to_string())
            .map_err(|e| e.to_string())
    }
}
