use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

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

#[tool_router(router = solutions_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "Get the solution of a ticket")]
    pub async fn get_solution(&self, Parameters(params): Parameters<GetSolutionParams>) -> Result<Json<Value>, String> {
        self.client
            .get(&format!("/Ticket/{}/ITILSolution", params.ticket_id), None)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(
        description = "Post a solution on a ticket (GLPI auto-closes it depending on server config)"
    )]
    pub async fn add_solution(&self, Parameters(params): Parameters<AddSolutionParams>) -> Result<Json<Value>, String> {
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
            .map(Json)
            .map_err(|e| e.to_string())
    }
}
