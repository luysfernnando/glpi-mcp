use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::markdown::{escape_cell, id_field, into_array, table};
use crate::server::GlpiServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindProfileParams {
    #[schemars(description = "Name to search for (case-insensitive, partial match)")]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateProfileParams {
    pub name: String,
    #[schemars(description = "1 = central (technician), 2 = helpdesk (self-service)")]
    #[serde(default = "default_interface")]
    pub interface: i64,
    #[schemars(description = "Owning entity ID (0 = root entity)")]
    #[serde(default)]
    pub entities_id: i64,
    #[serde(default)]
    pub is_default: bool,
}

fn default_interface() -> i64 {
    1
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateProfileParams {
    pub profile_id: i64,
    #[schemars(description = "Fields to change, e.g. name, interface, is_default, ...")]
    pub update_fields: Map<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteProfileParams {
    pub profile_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DuplicateProfileParams {
    #[schemars(description = "Profile ID to copy rights from")]
    pub source_profile_id: i64,
    #[schemars(description = "Name for the new profile")]
    pub new_name: String,
}

#[tool_router(router = profiles_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(
        description = "List GLPI profiles (Admin, Technician, Self-Service, ...) as a compact Markdown table"
    )]
    pub async fn get_profiles(&self) -> Result<String, String> {
        let result = self
            .client
            .get("/Profile", None)
            .await
            .map_err(|e| e.to_string())?;
        let items = into_array(result);
        if items.is_empty() {
            return Ok("No profiles.".to_string());
        }

        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|profile| {
                let id = id_field(profile, "id");
                let name = escape_cell(profile.get("name").and_then(Value::as_str).unwrap_or(""));
                let is_default = if profile
                    .get("is_default")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    == 1
                {
                    "yes"
                } else {
                    "no"
                };
                vec![id, name, is_default.to_string()]
            })
            .collect();

        Ok(format!(
            "**{} profile(s)**\n\n{}",
            items.len(),
            table(&["ID", "Name", "Default"], &rows)
        ))
    }

    #[rmcp::tool(
        description = "Find GLPI profiles by name (partial match); returns ID and name. Use \
            this to resolve a profile's ID before update_profile / delete_profile / \
            duplicate_profile"
    )]
    pub async fn find_profile(
        &self,
        Parameters(params): Parameters<FindProfileParams>,
    ) -> Result<String, String> {
        let query = vec![
            ("criteria[0][field]".to_string(), "1".to_string()),
            (
                "criteria[0][searchtype]".to_string(),
                "contains".to_string(),
            ),
            ("criteria[0][value]".to_string(), params.name),
            ("forcedisplay[0]".to_string(), "2".to_string()),
            ("forcedisplay[1]".to_string(), "1".to_string()),
        ];
        let result = self
            .client
            .get("/search/Profile", Some(&query))
            .await
            .map_err(|e| e.to_string())?;
        let data = into_array(result.get("data").cloned().unwrap_or(Value::Null));
        if data.is_empty() {
            return Ok("No matching profiles.".to_string());
        }

        let rows: Vec<Vec<String>> = data
            .iter()
            .map(|row| {
                vec![
                    id_field(row, "2"),
                    escape_cell(row.get("1").and_then(Value::as_str).unwrap_or("")),
                ]
            })
            .collect();

        Ok(format!(
            "**{} profile(s) found**\n\n{}",
            rows.len(),
            table(&["ID", "Name"], &rows)
        ))
    }

    #[rmcp::tool(description = "Create a new GLPI profile (e.g. a new support tier or role)")]
    pub async fn create_profile(
        &self,
        Parameters(params): Parameters<CreateProfileParams>,
    ) -> Result<String, String> {
        let input = json!({
            "name": params.name,
            "interface": params.interface,
            "entities_id": params.entities_id,
            "is_default": params.is_default as i32,
        });

        let result = self
            .client
            .post("/Profile", &json!({ "input": input }))
            .await
            .map_err(|e| e.to_string())?;
        let id = id_field(&result, "id");
        Ok(format!("Profile #{id} \"{}\" created.", params.name))
    }

    #[rmcp::tool(description = "Update a GLPI profile; pass only the fields to change")]
    pub async fn update_profile(
        &self,
        Parameters(params): Parameters<UpdateProfileParams>,
    ) -> Result<String, String> {
        self.client
            .put(
                &format!("/Profile/{}", params.profile_id),
                &json!({ "input": params.update_fields }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Profile #{} updated.", params.profile_id))
    }

    #[rmcp::tool(description = "Delete a GLPI profile by ID")]
    pub async fn delete_profile(
        &self,
        Parameters(params): Parameters<DeleteProfileParams>,
    ) -> Result<String, String> {
        self.client
            .delete(&format!("/Profile/{}", params.profile_id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Profile #{} deleted.", params.profile_id))
    }

    #[rmcp::tool(
        description = "Duplicate a GLPI profile: creates a new profile with the same \
            interface/entity settings as the source and copies all its ProfileRight entries \
            (permissions), matching GLPI's UI \"Duplicate\" action"
    )]
    pub async fn duplicate_profile(
        &self,
        Parameters(params): Parameters<DuplicateProfileParams>,
    ) -> Result<String, String> {
        let source = self
            .client
            .get(&format!("/Profile/{}", params.source_profile_id), None)
            .await
            .map_err(|e| e.to_string())?;

        let interface = source.get("interface").cloned().unwrap_or(json!(1));
        let entities_id = source.get("entities_id").cloned().unwrap_or(json!(0));

        let created = self
            .client
            .post(
                "/Profile",
                &json!({ "input": {
                    "name": params.new_name,
                    "interface": interface,
                    "entities_id": entities_id,
                    "is_default": 0,
                } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        let new_id = id_field(&created, "id");

        let rights = self
            .client
            .get(
                &format!("/Profile/{}/ProfileRight", params.source_profile_id),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;

        let mut copied = 0usize;
        for right in into_array(rights) {
            let name = right.get("name").and_then(Value::as_str).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let rights_value = right.get("rights").cloned().unwrap_or(json!(0));
            self.client
                .post(
                    "/ProfileRight",
                    &json!({ "input": {
                        "profiles_id": new_id,
                        "name": name,
                        "rights": rights_value,
                    } }),
                )
                .await
                .map_err(|e| e.to_string())?;
            copied += 1;
        }

        Ok(format!(
            "Profile #{new_id} \"{}\" created from #{} ({copied} right(s) copied).",
            params.new_name, params.source_profile_id
        ))
    }
}
