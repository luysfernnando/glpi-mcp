use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::GlpiServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateGroupParams {
    pub name: String,
    pub comment: Option<String>,
    #[schemars(description = "Parent group ID for the tree hierarchy; omit for a root group")]
    pub parent_group_id: Option<i64>,
    #[schemars(description = "Owning entity ID (0 = root entity)")]
    #[serde(default)]
    pub entities_id: i64,
    #[schemars(description = "Visible in sub-entities")]
    #[serde(default)]
    pub is_recursive: bool,
    #[serde(default = "default_true")]
    pub is_requester: bool,
    #[serde(default = "default_true")]
    pub is_watcher: bool,
    #[serde(default = "default_true")]
    pub is_assign: bool,
    #[serde(default = "default_true")]
    pub is_task: bool,
    #[schemars(description = "Can contain items")]
    #[serde(default = "default_true")]
    pub is_itemgroup: bool,
    #[schemars(description = "Can contain users")]
    #[serde(default = "default_true")]
    pub is_usergroup: bool,
    #[serde(default = "default_true")]
    pub is_manager: bool,
    #[serde(default = "default_true")]
    pub is_notify: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateGroupParams {
    pub group_id: i64,
    #[schemars(description = "Fields to change, e.g. name, comment, groups_id (parent), is_assign, ...")]
    pub update_fields: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteGroupParams {
    pub group_id: i64,
}

#[tool_router(router = users_groups_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List GLPI users")]
    pub async fn get_users(&self) -> Result<Json<Value>, String> {
        self.client.get("/User", None).await.map(Json).map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "List GLPI groups")]
    pub async fn get_groups(&self) -> Result<Json<Value>, String> {
        self.client.get("/Group", None).await.map(Json).map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Create a new GLPI group")]
    pub async fn create_group(&self, Parameters(params): Parameters<CreateGroupParams>) -> Result<Json<Value>, String> {
        let mut input = json!({
            "name": params.name,
            "entities_id": params.entities_id,
            "is_recursive": params.is_recursive as i32,
            "is_requester": params.is_requester as i32,
            "is_watcher": params.is_watcher as i32,
            "is_assign": params.is_assign as i32,
            "is_task": params.is_task as i32,
            "is_itemgroup": params.is_itemgroup as i32,
            "is_usergroup": params.is_usergroup as i32,
            "is_manager": params.is_manager as i32,
            "is_notify": params.is_notify as i32,
        });
        let obj = input.as_object_mut().expect("object literal");
        if let Some(comment) = params.comment {
            obj.insert("comment".into(), json!(comment));
        }
        if let Some(parent_group_id) = params.parent_group_id {
            obj.insert("groups_id".into(), json!(parent_group_id));
        }

        self.client
            .post("/Group", &json!({ "input": input }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Update a GLPI group; pass only the fields to change")]
    pub async fn update_group(&self, Parameters(params): Parameters<UpdateGroupParams>) -> Result<Json<Value>, String> {
        self.client
            .put(&format!("/Group/{}", params.group_id), &json!({ "input": params.update_fields }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Delete a GLPI group by ID")]
    pub async fn delete_group(&self, Parameters(params): Parameters<DeleteGroupParams>) -> Result<Json<Value>, String> {
        self.client
            .delete(&format!("/Group/{}", params.group_id))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}
