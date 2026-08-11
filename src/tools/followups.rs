use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::markdown::{id_field, into_array, strip_html};
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

fn render_followup(followup: &Value) -> String {
    let id = id_field(followup, "id");
    let date = followup.get("date").and_then(Value::as_str).unwrap_or("");
    let privacy = if followup.get("is_private").and_then(Value::as_i64).unwrap_or(0) == 1 { " (private)" } else { "" };
    let content = strip_html(followup.get("content").and_then(Value::as_str).unwrap_or(""));
    format!("**#{id} — {date}{privacy}**\n\n{content}")
}

fn render_followups_list(items: &[Value]) -> String {
    if items.is_empty() {
        return "No followups.".to_string();
    }
    let blocks: Vec<String> = items.iter().map(render_followup).collect();
    format!("**{} followup(s)**\n\n{}", items.len(), blocks.join("\n\n---\n\n"))
}

#[tool_router(router = followups_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List all followups of a ticket as Markdown, HTML stripped")]
    pub async fn list_followups(&self, Parameters(params): Parameters<ListFollowupsParams>) -> Result<String, String> {
        let result = self
            .client
            .get(&format!("/Ticket/{}/ITILFollowup", params.ticket_id), None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(render_followups_list(&into_array(result)))
    }

    #[rmcp::tool(description = "Add a followup to a ticket")]
    pub async fn add_followup(&self, Parameters(params): Parameters<AddFollowupParams>) -> Result<String, String> {
        let result = self
            .client
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
            .map_err(|e| e.to_string())?;
        let id = id_field(&result, "id");
        Ok(format!("Followup #{id} added to ticket #{}.", params.ticket_id))
    }

    #[rmcp::tool(description = "Get details of a specific followup as Markdown, HTML stripped")]
    pub async fn get_followup(&self, Parameters(params): Parameters<GetFollowupParams>) -> Result<String, String> {
        let result = self
            .client
            .get(&format!("/ITILFollowup/{}", params.followup_id), None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(render_followup(&result))
    }
}
