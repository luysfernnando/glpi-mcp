use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

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

#[tool_router(router = kb_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(
        description = "List knowledge base articles with pagination. range_limit is auto-clamped \
            to 10 when range_start > 60 to avoid exceeding GLPI's PHP memory_limit on large HTML payloads"
    )]
    pub async fn list_kb_articles(&self, Parameters(params): Parameters<ListKbArticlesParams>) -> Result<Json<Value>, String> {
        let clamped = params.range_start > 60 && params.range_limit > 10;
        let effective_limit = if clamped { 10 } else { params.range_limit };
        let range = format!("{}-{}", params.range_start, params.range_start + effective_limit - 1);

        let result = self
            .client
            .get("/KnowbaseItem", Some(&[("range".to_string(), range)]))
            .await
            .map_err(|e| e.to_string())?;

        if !clamped {
            return Ok(Json(result));
        }

        let wrapped = match result {
            Value::Array(items) => json!({
                "_clamped_range_limit": 10,
                "_warning": self.labels.kb_clamp_warning,
                "items": items,
            }),
            Value::Object(mut obj) => {
                obj.insert("_clamped_range_limit".into(), json!(10));
                obj.insert("_warning".into(), json!(self.labels.kb_clamp_warning));
                Value::Object(obj)
            }
            other => other,
        };
        Ok(Json(wrapped))
    }

    #[rmcp::tool(description = "Get full details of a knowledge base article")]
    pub async fn get_kb_article(&self, Parameters(params): Parameters<GetKbArticleParams>) -> Result<Json<Value>, String> {
        self.client
            .get(&format!("/KnowbaseItem/{}", params.article_id), None)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(
        description = "Search knowledge base articles by keyword. Searches the title only by \
            default; set search_content to also match the HTML body. Field IDs are discovered at \
            runtime so this works on both GLPI 10 and 11"
    )]
    pub async fn search_kb_articles(&self, Parameters(params): Parameters<SearchKbArticlesParams>) -> Result<Json<Value>, String> {
        let name_field = self.client.resolve_search_field_id("KnowbaseItem", "name", "6").await;
        let answer_field = self.client.resolve_search_field_id("KnowbaseItem", "answer", "7").await;
        let range = format!("{}-{}", params.range_start, params.range_start + params.range_limit - 1);

        let mut query: Vec<(String, String)> = vec![
            ("range".to_string(), range),
            ("criteria[0][field]".to_string(), name_field),
            ("criteria[0][searchtype]".to_string(), "contains".to_string()),
            ("criteria[0][value]".to_string(), params.keywords.clone()),
        ];
        if params.search_content {
            query.push(("criteria[0][link]".to_string(), "AND".to_string()));
            query.push(("criteria[1][link]".to_string(), "OR".to_string()));
            query.push(("criteria[1][field]".to_string(), answer_field));
            query.push(("criteria[1][searchtype]".to_string(), "contains".to_string()));
            query.push(("criteria[1][value]".to_string(), params.keywords));
        }

        self.client
            .get("/search/KnowbaseItem", Some(&query))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Create a new knowledge base article")]
    pub async fn create_kb_article(&self, Parameters(params): Parameters<CreateKbArticleParams>) -> Result<Json<Value>, String> {
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

        self.client
            .post("/KnowbaseItem", &json!({ "input": input }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Update a knowledge base article; pass only the fields to change")]
    pub async fn update_kb_article(&self, Parameters(params): Parameters<UpdateKbArticleParams>) -> Result<Json<Value>, String> {
        self.client
            .put(&format!("/KnowbaseItem/{}", params.article_id), &json!({ "input": params.update_fields }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "List all knowledge base categories")]
    pub async fn list_kb_categories(&self) -> Result<Json<Value>, String> {
        self.client.get("/KnowbaseItemCategory", None).await.map(Json).map_err(|e| e.to_string())
    }

    #[rmcp::tool(
        description = "Get the visibility rules of a KB article: profiles, groups, users and entities with access"
    )]
    pub async fn get_kb_article_visibility(&self, Parameters(params): Parameters<GetKbArticleVisibilityParams>) -> Result<Json<Value>, String> {
        let id = params.article_id;
        let profiles = self.client.get(&format!("/KnowbaseItem/{id}/KnowbaseItem_Profile"), None).await.map_err(|e| e.to_string())?;
        let groups = self.client.get(&format!("/KnowbaseItem/{id}/KnowbaseItem_Group"), None).await.map_err(|e| e.to_string())?;
        let users = self.client.get(&format!("/KnowbaseItem/{id}/KnowbaseItem_User"), None).await.map_err(|e| e.to_string())?;
        let entities = self.client.get(&format!("/KnowbaseItem/{id}/Entity_KnowbaseItem"), None).await.map_err(|e| e.to_string())?;

        Ok(Json(json!({
            "profiles": profiles,
            "groups": groups,
            "users": users,
            "entities": entities,
        })))
    }

    #[rmcp::tool(description = "Add a profile to a KB article's visibility rules")]
    pub async fn add_kb_article_visibility_profile(&self, Parameters(params): Parameters<AddKbVisibilityProfileParams>) -> Result<Json<Value>, String> {
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
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Add a group to a KB article's visibility rules")]
    pub async fn add_kb_article_visibility_group(&self, Parameters(params): Parameters<AddKbVisibilityGroupParams>) -> Result<Json<Value>, String> {
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
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Update a KB article's profile-based visibility rule")]
    pub async fn update_kb_article_visibility_profile(&self, Parameters(params): Parameters<UpdateKbVisibilityParams>) -> Result<Json<Value>, String> {
        self.client
            .put(&format!("/KnowbaseItem_Profile/{}", params.visibility_id), &json!({ "input": params.update_fields }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[rmcp::tool(description = "Update a KB article's group-based visibility rule")]
    pub async fn update_kb_article_visibility_group(&self, Parameters(params): Parameters<UpdateKbVisibilityParams>) -> Result<Json<Value>, String> {
        self.client
            .put(&format!("/KnowbaseItem_Group/{}", params.visibility_id), &json!({ "input": params.update_fields }))
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }
}
