use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::markdown::{escape_cell, id_field, into_array, table};
use crate::server::GlpiServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindItilCategoryParams {
    #[schemars(description = "Name to search for (case-insensitive, partial match)")]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateItilCategoryParams {
    pub name: String,
    pub comment: Option<String>,
    #[schemars(
        description = "Parent category ID for the tree hierarchy; omit for a root category"
    )]
    pub parent_category_id: Option<i64>,
    #[schemars(description = "Owning entity ID (0 = root entity)")]
    #[serde(default)]
    pub entities_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateItilCategoryParams {
    pub category_id: i64,
    #[schemars(
        description = "Fields to change, e.g. name, comment, itilcategories_id (parent), ..."
    )]
    pub update_fields: Map<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteItilCategoryParams {
    pub category_id: i64,
}

#[tool_router(router = categories_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(
        description = "List all ITIL categories (Incident, Request, Change, Problem) as a Markdown table"
    )]
    pub async fn list_itil_categories(&self) -> Result<String, String> {
        let result = self
            .client
            .get("/ITILCategory", None)
            .await
            .map_err(|e| e.to_string())?;
        let items = into_array(result);
        if items.is_empty() {
            return Ok("No categories.".to_string());
        }

        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|cat| {
                let id = id_field(cat, "id");
                let name = escape_cell(
                    cat.get("completename")
                        .or_else(|| cat.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                vec![id, name]
            })
            .collect();

        Ok(format!(
            "**{} categorie(s)**\n\n{}",
            items.len(),
            table(&["ID", "Name"], &rows)
        ))
    }

    #[rmcp::tool(
        description = "Find ITIL categories by name (partial match); returns ID and full path. \
            Use this to resolve a parent category's ID before create_itil_category, or a \
            category's own ID before update_itil_category / delete_itil_category"
    )]
    pub async fn find_itil_category(
        &self,
        Parameters(params): Parameters<FindItilCategoryParams>,
    ) -> Result<String, String> {
        let query = vec![
            ("criteria[0][field]".to_string(), "14".to_string()),
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
            .get("/search/ITILCategory", Some(&query))
            .await
            .map_err(|e| e.to_string())?;
        let data = into_array(result.get("data").cloned().unwrap_or(Value::Null));
        if data.is_empty() {
            return Ok("No matching categories.".to_string());
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
            "**{} categorie(s) found**\n\n{}",
            rows.len(),
            table(&["ID", "Name"], &rows)
        ))
    }

    #[rmcp::tool(description = "Create a new ITIL category")]
    pub async fn create_itil_category(
        &self,
        Parameters(params): Parameters<CreateItilCategoryParams>,
    ) -> Result<String, String> {
        let mut input = json!({
            "name": params.name,
            "entities_id": params.entities_id,
        });
        let obj = input.as_object_mut().expect("object literal");
        if let Some(comment) = params.comment {
            obj.insert("comment".into(), json!(comment));
        }
        if let Some(parent_category_id) = params.parent_category_id {
            obj.insert("itilcategories_id".into(), json!(parent_category_id));
        }

        let result = self
            .client
            .post("/ITILCategory", &json!({ "input": input }))
            .await
            .map_err(|e| e.to_string())?;
        let id = id_field(&result, "id");
        Ok(format!("ITIL category #{id} \"{}\" created.", params.name))
    }

    #[rmcp::tool(description = "Update an ITIL category; pass only the fields to change")]
    pub async fn update_itil_category(
        &self,
        Parameters(params): Parameters<UpdateItilCategoryParams>,
    ) -> Result<String, String> {
        let mut input = params.update_fields;
        // Mirrors update_group: renaming without touching the parent field must not detach
        // the category from its hierarchy.
        if input.contains_key("name") && !input.contains_key("itilcategories_id") {
            let current = self
                .client
                .get(&format!("/ITILCategory/{}", params.category_id), None)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(parent_id) = current.get("itilcategories_id") {
                input.insert("itilcategories_id".into(), parent_id.clone());
            }
        }

        self.client
            .put(
                &format!("/ITILCategory/{}", params.category_id),
                &json!({ "input": input }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("ITIL category #{} updated.", params.category_id))
    }

    #[rmcp::tool(description = "Delete an ITIL category by ID")]
    pub async fn delete_itil_category(
        &self,
        Parameters(params): Parameters<DeleteItilCategoryParams>,
    ) -> Result<String, String> {
        self.client
            .delete(&format!("/ITILCategory/{}", params.category_id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("ITIL category #{} deleted.", params.category_id))
    }
}
