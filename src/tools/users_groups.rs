use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::markdown::{escape_cell, id_field, into_array, table};
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
    #[schemars(
        description = "Fields to change, e.g. name, comment, groups_id (parent), is_assign, ..."
    )]
    pub update_fields: Map<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteGroupParams {
    pub group_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindGroupParams {
    #[schemars(description = "Name or acronym to search for (case-insensitive, partial match)")]
    pub name: String,
}

#[tool_router(router = users_groups_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List GLPI users as a compact Markdown table")]
    pub async fn get_users(&self) -> Result<String, String> {
        let result = self
            .client
            .get("/User", None)
            .await
            .map_err(|e| e.to_string())?;
        let items = into_array(result);
        if items.is_empty() {
            return Ok("No users.".to_string());
        }

        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|user| {
                let id = id_field(user, "id");
                let login = escape_cell(user.get("name").and_then(Value::as_str).unwrap_or(""));
                let realname =
                    escape_cell(user.get("realname").and_then(Value::as_str).unwrap_or(""));
                let firstname =
                    escape_cell(user.get("firstname").and_then(Value::as_str).unwrap_or(""));
                let active = if user.get("is_active").and_then(Value::as_i64).unwrap_or(1) == 1 {
                    "yes"
                } else {
                    "no"
                };
                vec![id, login, firstname, realname, active.to_string()]
            })
            .collect();

        Ok(format!(
            "**{} user(s)**\n\n{}",
            items.len(),
            table(&["ID", "Login", "First name", "Last name", "Active"], &rows)
        ))
    }

    #[rmcp::tool(description = "List GLPI groups as a compact Markdown table")]
    pub async fn get_groups(&self) -> Result<String, String> {
        let result = self
            .client
            .get("/Group", None)
            .await
            .map_err(|e| e.to_string())?;
        let items = into_array(result);
        if items.is_empty() {
            return Ok("No groups.".to_string());
        }

        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|group| {
                let id = id_field(group, "id");
                let name = escape_cell(
                    group
                        .get("completename")
                        .or_else(|| group.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                let comment =
                    escape_cell(group.get("comment").and_then(Value::as_str).unwrap_or(""));
                vec![id, name, comment]
            })
            .collect();

        Ok(format!(
            "**{} group(s)**\n\n{}",
            items.len(),
            table(&["ID", "Name", "Comment"], &rows)
        ))
    }

    #[rmcp::tool(
        description = "Find GLPI groups by name or acronym (partial match); returns ID, full \
            hierarchy path and comment. Use this to resolve a parent group's ID before create_group, \
            or a group's own ID before update_group / delete_group / find_group_rule_references"
    )]
    pub async fn find_group(
        &self,
        Parameters(params): Parameters<FindGroupParams>,
    ) -> Result<String, String> {
        let query = vec![
            ("criteria[0][field]".to_string(), "14".to_string()),
            ("criteria[0][searchtype]".to_string(), "contains".to_string()),
            ("criteria[0][value]".to_string(), params.name),
            ("forcedisplay[0]".to_string(), "2".to_string()),
            ("forcedisplay[1]".to_string(), "1".to_string()),
            ("forcedisplay[2]".to_string(), "16".to_string()),
        ];
        let result = self
            .client
            .get("/search/Group", Some(&query))
            .await
            .map_err(|e| e.to_string())?;
        let data = into_array(result.get("data").cloned().unwrap_or(Value::Null));
        if data.is_empty() {
            return Ok("No matching groups.".to_string());
        }

        let rows: Vec<Vec<String>> = data
            .iter()
            .map(|row| {
                vec![
                    id_field(row, "2"),
                    escape_cell(row.get("1").and_then(Value::as_str).unwrap_or("")),
                    escape_cell(row.get("16").and_then(Value::as_str).unwrap_or("")),
                ]
            })
            .collect();

        Ok(format!(
            "**{} group(s) found**\n\n{}",
            rows.len(),
            table(&["ID", "Path", "Comment"], &rows)
        ))
    }

    #[rmcp::tool(description = "Create a new GLPI group")]
    pub async fn create_group(
        &self,
        Parameters(params): Parameters<CreateGroupParams>,
    ) -> Result<String, String> {
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

        let result = self
            .client
            .post("/Group", &json!({ "input": input }))
            .await
            .map_err(|e| e.to_string())?;
        let id = id_field(&result, "id");
        Ok(format!("Group #{id} \"{}\" created.", params.name))
    }

    #[rmcp::tool(description = "Update a GLPI group; pass only the fields to change")]
    pub async fn update_group(
        &self,
        Parameters(params): Parameters<UpdateGroupParams>,
    ) -> Result<String, String> {
        let mut input = params.update_fields;
        // GLPI's Group API resets groups_id (the parent) to 0 when `name` is sent without it,
        // silently detaching the group from its hierarchy. Preserve the current parent whenever
        // a rename doesn't explicitly touch groups_id.
        if input.contains_key("name") && !input.contains_key("groups_id") {
            let current = self
                .client
                .get(&format!("/Group/{}", params.group_id), None)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(groups_id) = current.get("groups_id") {
                input.insert("groups_id".into(), groups_id.clone());
            }
        }

        self.client
            .put(
                &format!("/Group/{}", params.group_id),
                &json!({ "input": input }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Group #{} updated.", params.group_id))
    }

    #[rmcp::tool(description = "Delete a GLPI group by ID")]
    pub async fn delete_group(
        &self,
        Parameters(params): Parameters<DeleteGroupParams>,
    ) -> Result<String, String> {
        self.client
            .delete(&format!("/Group/{}", params.group_id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Group #{} deleted.", params.group_id))
    }
}
