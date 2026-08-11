use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::labels::lookup;
use crate::markdown::{escape_cell, strip_html, table, truncate};
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
    #[rmcp::tool(description = "List all tasks of a ticket as a compact Markdown table")]
    pub async fn list_tasks(&self, Parameters(params): Parameters<ListTasksParams>) -> Result<String, String> {
        let result = self
            .client
            .get(&format!("/Ticket/{}/TicketTask", params.ticket_id), None)
            .await
            .map_err(|e| e.to_string())?;
        let items = result.as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            return Ok("No tasks.".to_string());
        }

        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|task| {
                let id = task.get("id").and_then(Value::as_i64).map(|v| v.to_string()).unwrap_or_default();
                let status = lookup(&self.labels.task_status, task.get("state").and_then(Value::as_i64), self.labels.unknown).to_string();
                let assignee = task
                    .get("users_id_tech")
                    .and_then(Value::as_i64)
                    .filter(|id| *id != 0)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| self.labels.unassigned.to_string());
                let duration = task.get("actiontime").and_then(Value::as_i64).map(|s| format!("{}min", s / 60)).unwrap_or_default();
                let content = truncate(&escape_cell(&strip_html(task.get("content").and_then(Value::as_str).unwrap_or(""))), 100);
                vec![id, status, assignee, duration, content]
            })
            .collect();

        Ok(format!(
            "**{} task(s)**\n\n{}",
            items.len(),
            table(&["ID", "Status", "Assigned to", "Duration", "Content"], &rows)
        ))
    }

    #[rmcp::tool(description = "Create a task on a ticket")]
    pub async fn add_task(&self, Parameters(params): Parameters<AddTaskParams>) -> Result<String, String> {
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

        let result = self.client.post("/TicketTask", &json!({ "input": input })).await.map_err(|e| e.to_string())?;
        let id = result.get("id").and_then(Value::as_i64).map(|v| v.to_string()).unwrap_or_default();
        Ok(format!("Task #{id} added to ticket #{}.", params.ticket_id))
    }

    #[rmcp::tool(description = "Update a task; pass only the fields to change")]
    pub async fn update_task(&self, Parameters(params): Parameters<UpdateTaskParams>) -> Result<String, String> {
        self.client
            .put(&format!("/TicketTask/{}", params.task_id), &json!({ "input": params.update_fields }))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Task #{} updated.", params.task_id))
    }

    #[rmcp::tool(description = "Delete a task")]
    pub async fn delete_task(&self, Parameters(params): Parameters<DeleteTaskParams>) -> Result<String, String> {
        self.client.delete(&format!("/TicketTask/{}", params.task_id)).await.map_err(|e| e.to_string())?;
        Ok(format!("Task #{} deleted.", params.task_id))
    }
}
