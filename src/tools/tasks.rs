use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::GlpiServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTasksParams {
    pub ticket_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddTaskParams {
    pub ticket_id: i64,
    pub content: String,
    pub assigned_user_id: Option<i64>,
    #[schemars(description = "Duration in seconds, e.g. 3600 = 1h")]
    pub duration_seconds: Option<i64>,
    #[serde(default)]
    pub is_private: bool,
    #[schemars(description = "1=To do 2=Done")]
    #[serde(default = "default_status")]
    pub status: i64,
}

fn default_status() -> i64 {
    1
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTaskParams {
    pub task_id: i64,
    #[schemars(description = "Fields to change, e.g. state (1/2), content, actiontime, users_id_tech")]
    pub update_fields: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteTaskParams {
    pub task_id: i64,
}

#[tool_router(router = tasks_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List all tasks of a ticket")]
    pub async fn list_tasks(&self, Parameters(params): Parameters<ListTasksParams>) -> Result<Json<Value>, String> {
        self.client
            .get(&format!("/Ticket/{}/TicketTask", params.ticket_id), None)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Create a task on a ticket")]
    pub async fn add_task(&self, Parameters(params): Parameters<AddTaskParams>) -> Result<Json<Value>, String> {
        let mut input = json!({
            "tickets_id": params.ticket_id,
            "content": params.content,
            "is_private": params.is_private as i32,
            "state": params.status,
        });
        let obj = input.as_object_mut().expect("object literal");
        if let Some(assigned_user_id) = params.assigned_user_id {
            obj.insert("users_id_tech".into(), json!(assigned_user_id));
        }
        if let Some(duration_seconds) = params.duration_seconds {
            obj.insert("actiontime".into(), json!(duration_seconds));
        }

        self.client
            .post("/TicketTask", &json!({ "input": input }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Update a task; pass only the fields to change")]
    pub async fn update_task(&self, Parameters(params): Parameters<UpdateTaskParams>) -> Result<Json<Value>, String> {
        self.client
            .put(&format!("/TicketTask/{}", params.task_id), &json!({ "input": params.update_fields }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Delete a task")]
    pub async fn delete_task(&self, Parameters(params): Parameters<DeleteTaskParams>) -> Result<Json<Value>, String> {
        self.client
            .delete(&format!("/TicketTask/{}", params.task_id))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}
