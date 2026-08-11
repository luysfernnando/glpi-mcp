use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::markdown::{escape_cell, field_table, str_field, strip_html, table, truncate};
use crate::server::GlpiServer;

fn default_range_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListKbArticlesParams {
    #[serde(default)]
    pub range_start: i64,
    #[serde(default = "default_range_limit")]
    pub range_limit: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetKbArticleParams {
    pub article_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchKbArticlesParams {
    pub keywords: String,
    #[serde(default)]
    pub range_start: i64,
    #[serde(default = "default_range_limit")]
    pub range_limit: i64,
    #[schemars(description = "Also match against the article body; only enable when the GLPI DB has a FULLTEXT index on knowbaseitems.answer, otherwise the request is slow")]
    #[serde(default)]
    pub search_content: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateKbArticleParams {
    pub name: String,
    #[schemars(description = "Article content/solution, HTML accepted")]
    pub answer: String,
    pub category_id: Option<i64>,
    #[schemars(description = "True to publish in the public FAQ")]
    #[serde(default)]
    pub is_faq: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateKbArticleParams {
    pub article_id: i64,
    #[schemars(description = "Fields to change, e.g. name, answer, is_faq, knowbaseitemcategories_id")]
    pub update_fields: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetKbArticleVisibilityParams {
    pub article_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddKbVisibilityProfileParams {
    pub article_id: i64,
    pub profiles_id: i64,
    #[schemars(description = "0 = root entity")]
    #[serde(default)]
    pub entities_id: i64,
    #[schemars(description = "Apply to sub-entities")]
    #[serde(default)]
    pub is_recursive: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddKbVisibilityGroupParams {
    pub article_id: i64,
    pub groups_id: i64,
    #[serde(default)]
    pub entities_id: i64,
    #[serde(default)]
    pub is_recursive: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateKbVisibilityParams {
    #[schemars(description = "ID of the visibility entry, obtained via get_kb_article_visibility")]
    pub visibility_id: i64,
    pub update_fields: Value,
}

fn any_field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn render_kb_list(items: &[Value]) -> String {
    if items.is_empty() {
        return "No knowledge base articles.".to_string();
    }
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|item| {
            let id = item.get("id").and_then(Value::as_i64).map(|v| v.to_string()).unwrap_or_default();
            let name = escape_cell(&str_field(item, "name"));
            let faq = if item.get("is_faq").and_then(Value::as_i64).unwrap_or(0) == 1 { "yes" } else { "no" };
            vec![id, name, faq.to_string()]
        })
        .collect();
    format!("**{} article(s)**\n\n{}", items.len(), table(&["ID", "Name", "FAQ"], &rows))
}

fn render_visibility_table(headers: &[&str], items: &[Value], columns: &[&str]) -> String {
    if items.is_empty() {
        return "None.".to_string();
    }
    let rows: Vec<Vec<String>> = items.iter().map(|item| columns.iter().map(|c| escape_cell(&any_field(item, c))).collect()).collect();
    table(headers, &rows)
}

#[tool_router(router = kb_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(
        description = "List knowledge base articles with pagination, as a Markdown table. range_limit is auto-clamped \
            to 10 when range_start > 60 to avoid exceeding GLPI's PHP memory_limit on large HTML payloads"
    )]
    pub async fn list_kb_articles(&self, Parameters(params): Parameters<ListKbArticlesParams>) -> Result<String, String> {
        let clamped = params.range_start > 60 && params.range_limit > 10;
        let effective_limit = if clamped { 10 } else { params.range_limit };
        let range = format!("{}-{}", params.range_start, params.range_start + effective_limit - 1);

        let result = self
            .client
            .get("/KnowbaseItem", Some(&[("range".to_string(), range)]))
            .await
            .map_err(|e| e.to_string())?;

        let items = result.as_array().cloned().unwrap_or_default();
        let mut rendered = render_kb_list(&items);
        if clamped {
            rendered = format!("_{}_\n\n{rendered}", self.labels.kb_clamp_warning);
        }
        Ok(rendered)
    }

    #[rmcp::tool(description = "Get full details of a knowledge base article as Markdown, HTML stripped")]
    pub async fn get_kb_article(&self, Parameters(params): Parameters<GetKbArticleParams>) -> Result<String, String> {
        let result = self.client.get(&format!("/KnowbaseItem/{}", params.article_id), None).await.map_err(|e| e.to_string())?;
        let faq = if result.get("is_faq").and_then(Value::as_i64).unwrap_or(0) == 1 { "yes" } else { "no" };
        Ok(field_table(&[
            ("ID", any_field(&result, "id")),
            ("Name", str_field(&result, "name")),
            ("FAQ", faq.to_string()),
            ("Answer", strip_html(&str_field(&result, "answer"))),
        ]))
    }

    #[rmcp::tool(
        description = "Search knowledge base articles by keyword, as a Markdown table. Searches the title only by \
            default; set search_content to also match the HTML body. Field IDs are discovered at \
            runtime so this works on both GLPI 10 and 11"
    )]
    pub async fn search_kb_articles(&self, Parameters(params): Parameters<SearchKbArticlesParams>) -> Result<String, String> {
        let name_field = self.client.resolve_search_field_id("KnowbaseItem", "name", "6").await;
        let answer_field = self.client.resolve_search_field_id("KnowbaseItem", "answer", "7").await;
        let range = format!("{}-{}", params.range_start, params.range_start + params.range_limit - 1);

        let mut query: Vec<(String, String)> = vec![
            ("range".to_string(), range),
            ("criteria[0][field]".to_string(), name_field.clone()),
            ("criteria[0][searchtype]".to_string(), "contains".to_string()),
            ("criteria[0][value]".to_string(), params.keywords.clone()),
        ];
        if params.search_content {
            query.push(("criteria[0][link]".to_string(), "AND".to_string()));
            query.push(("criteria[1][link]".to_string(), "OR".to_string()));
            query.push(("criteria[1][field]".to_string(), answer_field.clone()));
            query.push(("criteria[1][searchtype]".to_string(), "contains".to_string()));
            query.push(("criteria[1][value]".to_string(), params.keywords));
        }

        let result = self.client.get("/search/KnowbaseItem", Some(&query)).await.map_err(|e| e.to_string())?;
        let items = result.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
        if items.is_empty() {
            return Ok("No matching articles.".to_string());
        }

        let mut labels: HashMap<&str, &str> = HashMap::new();
        labels.insert("2", "ID");
        labels.insert(name_field.as_str(), "Name");
        if params.search_content {
            labels.insert(answer_field.as_str(), "Answer");
        }
        let mut headers: Vec<&str> = vec!["ID", "Name"];
        if params.search_content {
            headers.push("Answer");
        }

        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|item| {
                let obj = item.as_object();
                headers
                    .iter()
                    .map(|header| {
                        let field_id = labels.iter().find(|(_, v)| **v == *header).map(|(k, _)| *k).unwrap_or("");
                        let raw = obj.and_then(|o| o.get(field_id)).and_then(Value::as_str).unwrap_or("");
                        let text = strip_html(raw);
                        if *header == "Answer" { truncate(&escape_cell(&text), 150) } else { escape_cell(&text) }
                    })
                    .collect()
            })
            .collect();

        Ok(format!("**{} article(s)**\n\n{}", items.len(), table(&headers, &rows)))
    }

    #[rmcp::tool(description = "Create a new knowledge base article")]
    pub async fn create_kb_article(&self, Parameters(params): Parameters<CreateKbArticleParams>) -> Result<String, String> {
        let mut input = json!({
            "name": params.name,
            "answer": params.answer,
            "is_faq": params.is_faq as i32,
        });
        if let Some(category_id) = params.category_id {
            input
                .as_object_mut()
                .expect("object literal")
                .insert("knowbaseitemcategories_id".into(), json!(category_id));
        }

        let result = self.client.post("/KnowbaseItem", &json!({ "input": input })).await.map_err(|e| e.to_string())?;
        let id = any_field(&result, "id");
        Ok(format!("KB article #{id} \"{}\" created.", params.name))
    }

    #[rmcp::tool(description = "Update a knowledge base article; pass only the fields to change")]
    pub async fn update_kb_article(&self, Parameters(params): Parameters<UpdateKbArticleParams>) -> Result<String, String> {
        self.client
            .put(&format!("/KnowbaseItem/{}", params.article_id), &json!({ "input": params.update_fields }))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("KB article #{} updated.", params.article_id))
    }

    #[rmcp::tool(description = "List all knowledge base categories as a Markdown table")]
    pub async fn list_kb_categories(&self) -> Result<String, String> {
        let result = self.client.get("/KnowbaseItemCategory", None).await.map_err(|e| e.to_string())?;
        let items = result.as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            return Ok("No categories.".to_string());
        }
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|cat| vec![any_field(cat, "id"), escape_cell(&str_field(cat, "completename"))])
            .collect();
        Ok(format!("**{} categorie(s)**\n\n{}", items.len(), table(&["ID", "Name"], &rows)))
    }

    #[rmcp::tool(
        description = "Get the visibility rules of a KB article as Markdown: profiles, groups, users and entities with access"
    )]
    pub async fn get_kb_article_visibility(&self, Parameters(params): Parameters<GetKbArticleVisibilityParams>) -> Result<String, String> {
        let id = params.article_id;
        let profiles = self.client.get(&format!("/KnowbaseItem/{id}/KnowbaseItem_Profile"), None).await.map_err(|e| e.to_string())?;
        let groups = self.client.get(&format!("/KnowbaseItem/{id}/KnowbaseItem_Group"), None).await.map_err(|e| e.to_string())?;
        let users = self.client.get(&format!("/KnowbaseItem/{id}/KnowbaseItem_User"), None).await.map_err(|e| e.to_string())?;
        let entities = self.client.get(&format!("/KnowbaseItem/{id}/Entity_KnowbaseItem"), None).await.map_err(|e| e.to_string())?;

        let profiles_md = render_visibility_table(&["ID", "Profile ID", "Entity"], &profiles.as_array().cloned().unwrap_or_default(), &["id", "profiles_id", "entities_id"]);
        let groups_md = render_visibility_table(&["ID", "Group ID", "Entity"], &groups.as_array().cloned().unwrap_or_default(), &["id", "groups_id", "entities_id"]);
        let users_md = render_visibility_table(&["ID", "User ID", "Entity"], &users.as_array().cloned().unwrap_or_default(), &["id", "users_id", "entities_id"]);
        let entities_md = render_visibility_table(&["ID", "Entity ID"], &entities.as_array().cloned().unwrap_or_default(), &["id", "entities_id"]);

        Ok(format!(
            "**Profiles**\n\n{profiles_md}\n\n**Groups**\n\n{groups_md}\n\n**Users**\n\n{users_md}\n\n**Entities**\n\n{entities_md}"
        ))
    }

    #[rmcp::tool(description = "Add a profile to a KB article's visibility rules")]
    pub async fn add_kb_article_visibility_profile(&self, Parameters(params): Parameters<AddKbVisibilityProfileParams>) -> Result<String, String> {
        self.client
            .post(
                "/KnowbaseItem_Profile",
                &json!({ "input": {
                    "knowbaseitems_id": params.article_id,
                    "profiles_id": params.profiles_id,
                    "entities_id": params.entities_id,
                    "is_recursive": params.is_recursive as i32,
                } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Profile #{} visibility added to KB article #{}.", params.profiles_id, params.article_id))
    }

    #[rmcp::tool(description = "Add a group to a KB article's visibility rules")]
    pub async fn add_kb_article_visibility_group(&self, Parameters(params): Parameters<AddKbVisibilityGroupParams>) -> Result<String, String> {
        self.client
            .post(
                "/KnowbaseItem_Group",
                &json!({ "input": {
                    "knowbaseitems_id": params.article_id,
                    "groups_id": params.groups_id,
                    "entities_id": params.entities_id,
                    "is_recursive": params.is_recursive as i32,
                } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Group #{} visibility added to KB article #{}.", params.groups_id, params.article_id))
    }

    #[rmcp::tool(description = "Update a KB article's profile-based visibility rule")]
    pub async fn update_kb_article_visibility_profile(&self, Parameters(params): Parameters<UpdateKbVisibilityParams>) -> Result<String, String> {
        self.client
            .put(&format!("/KnowbaseItem_Profile/{}", params.visibility_id), &json!({ "input": params.update_fields }))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Profile visibility rule #{} updated.", params.visibility_id))
    }

    #[rmcp::tool(description = "Update a KB article's group-based visibility rule")]
    pub async fn update_kb_article_visibility_group(&self, Parameters(params): Parameters<UpdateKbVisibilityParams>) -> Result<String, String> {
        self.client
            .put(&format!("/KnowbaseItem_Group/{}", params.visibility_id), &json!({ "input": params.update_fields }))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Group visibility rule #{} updated.", params.visibility_id))
    }
}
