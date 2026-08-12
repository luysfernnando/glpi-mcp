use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::markdown::{into_array, strip_html};
use crate::server::GlpiServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSolutionParams {
    pub ticket_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddSolutionParams {
    pub ticket_id: i64,
    pub content: String,
    pub solution_type_id: Option<i64>,
}

fn render_solution(solution: &Value) -> String {
    let date = solution
        .get("date_creation")
        .and_then(Value::as_str)
        .unwrap_or("");
    let content = strip_html(
        solution
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    format!("**{date}**\n\n{content}")
}

#[tool_router(router = solutions_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "Get the solution(s) of a ticket as Markdown, HTML stripped")]
    pub async fn get_solution(
        &self,
        Parameters(params): Parameters<GetSolutionParams>,
    ) -> Result<String, String> {
        let result = self
            .client
            .get(&format!("/Ticket/{}/ITILSolution", params.ticket_id), None)
            .await
            .map_err(|e| e.to_string())?;
        let items = into_array(result);
        if items.is_empty() {
            return Ok("No solution recorded for this ticket.".to_string());
        }
        Ok(items
            .iter()
            .map(render_solution)
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"))
    }

    #[rmcp::tool(
        description = "Post a solution on a ticket (GLPI auto-closes it depending on server config)"
    )]
    pub async fn add_solution(
        &self,
        Parameters(params): Parameters<AddSolutionParams>,
    ) -> Result<String, String> {
        let mut input = json!({
            "items_id": params.ticket_id,
            "itemtype": "Ticket",
            "content": params.content,
        });
        if let Some(solution_type_id) = params.solution_type_id {
            input
                .as_object_mut()
                .expect("object literal")
                .insert("solutiontypes_id".into(), json!(solution_type_id));
        }

        self.client
            .post("/ITILSolution", &json!({ "input": input }))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Solution added to ticket #{}.", params.ticket_id))
    }
}
