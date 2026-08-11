use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::GlpiServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListFollowupsParams {
    pub ticket_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddFollowupParams {
    pub ticket_id: i64,
    pub content: String,
    #[schemars(description = "True for a followup visible to technicians only")]
    #[serde(default)]
    pub is_private: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFollowupParams {
    pub followup_id: i64,
}

#[tool_router(router = followups_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List all followups of a ticket")]
    pub async fn list_followups(&self, Parameters(params): Parameters<ListFollowupsParams>) -> Result<Json<Value>, String> {
        self.client
            .get(&format!("/Ticket/{}/ITILFollowup", params.ticket_id), None)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Add a followup to a ticket")]
    pub async fn add_followup(&self, Parameters(params): Parameters<AddFollowupParams>) -> Result<Json<Value>, String> {
        self.client
            .post(
                "/ITILFollowup",
                &json!({ "input": {
                    "items_id": params.ticket_id,
                    "itemtype": "Ticket",
                    "content": params.content,
                    "is_private": params.is_private as i32,
                } }),
            )
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Get details of a specific followup")]
    pub async fn get_followup(&self, Parameters(params): Parameters<GetFollowupParams>) -> Result<Json<Value>, String> {
        self.client
            .get(&format!("/ITILFollowup/{}", params.followup_id), None)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}
