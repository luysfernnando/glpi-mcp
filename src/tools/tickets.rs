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
pub struct ListTicketsParams {
    #[schemars(description = "1=New 2=In progress (assigned) 3=In progress (planned) 4=Pending 5=Solved 6=Closed")]
    pub status: Option<i64>,
    #[schemars(description = "1=Incident 2=Service request")]
    pub ticket_type: Option<i64>,
    #[serde(default)]
    pub range_start: i64,
    #[serde(default = "default_range_limit")]
    pub range_limit: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetTicketParams {
    pub ticket_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchTicketsParams {
    pub keywords: Option<String>,
    #[schemars(description = "1=New 2=In progress (assigned) 3=In progress (planned) 4=Pending 5=Solved 6=Closed")]
    pub status: Option<i64>,
    #[schemars(description = "1=Incident 2=Service request")]
    pub ticket_type: Option<i64>,
    pub category_id: Option<i64>,
    pub assigned_user_id: Option<i64>,
    #[serde(default)]
    pub range_start: i64,
    #[serde(default = "default_range_limit")]
    pub range_limit: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateTicketParams {
    pub name: String,
    pub content: String,
    #[schemars(description = "1=Incident 2=Service request")]
    #[serde(default = "default_ticket_type")]
    pub ticket_type: i64,
    pub category_id: Option<i64>,
    #[schemars(description = "1 (very low) to 6 (major)")]
    #[serde(default = "default_priority")]
    pub priority: i64,
    pub assigned_user_id: Option<i64>,
    pub assigned_group_id: Option<i64>,
}

fn default_ticket_type() -> i64 {
    1
}

fn default_priority() -> i64 {
    3
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateTicketParams {
    pub ticket_id: i64,
    #[schemars(description = "Fields to change, e.g. {\"status\": 5, \"priority\": 4}")]
    pub update_fields: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteTicketParams {
    pub ticket_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LinkTicketsParams {
    pub ticket_id_1: i64,
    pub ticket_id_2: i64,
    #[schemars(description = "1=Linked to 2=Duplicates 3=Child of 4=Parent of")]
    #[serde(default = "default_link_type")]
    pub link_type: i64,
}

fn default_link_type() -> i64 {
    1
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTicketLinksParams {
    pub ticket_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MergeTicketsParams {
    pub target_ticket_id: i64,
    pub source_ticket_ids: Vec<i64>,
    #[serde(default = "default_true")]
    pub add_followups: bool,
    #[serde(default = "default_true")]
    pub close_source: bool,
}

fn default_true() -> bool {
    true
}

#[tool_router(router = tickets_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "List GLPI tickets with optional status/type filters and pagination")]
    pub async fn list_tickets(
        &self,
        Parameters(params): Parameters<ListTicketsParams>,
    ) -> Result<Json<Value>, String> {
        let range = format!(
            "{}-{}",
            params.range_start,
            params.range_start + params.range_limit - 1
        );
        let mut query: Vec<(String, String)> = vec![("range".to_string(), range)];
        if let Some(status) = params.status {
            query.push(("searchText[status]".to_string(), status.to_string()));
        }
        if let Some(ticket_type) = params.ticket_type {
            query.push(("searchText[type]".to_string(), ticket_type.to_string()));
        }

        let result = self.client.get("/Ticket", Some(&query)).await.map_err(|e| e.to_string())?;
        let enriched = match result {
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|t| self.labels.enrich_ticket(t)).collect())
            }
            other => other,
        };
        Ok(Json(enriched))
    }

    #[rmcp::tool(description = "Get full details of a ticket, with readable labels")]
    pub async fn get_ticket(&self, Parameters(params): Parameters<GetTicketParams>) -> Result<Json<Value>, String> {
        let ticket = self
            .client
            .get(&format!("/Ticket/{}", params.ticket_id), None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(self.labels.enrich_ticket(ticket)))
    }

    #[rmcp::tool(description = "Advanced ticket search via GLPI's /search/Ticket, all filters optional and combinable")]
    pub async fn search_tickets(
        &self,
        Parameters(params): Parameters<SearchTicketsParams>,
    ) -> Result<Json<Value>, String> {
        let range = format!(
            "{}-{}",
            params.range_start,
            params.range_start + params.range_limit - 1
        );
        let mut query: Vec<(String, String)> = vec![("range".to_string(), range)];
        let mut idx = 0;

        let mut push_criterion = |query: &mut Vec<(String, String)>, field: &str, searchtype: &str, value: String| {
            query.push((format!("criteria[{idx}][field]"), field.to_string()));
            query.push((format!("criteria[{idx}][searchtype]"), searchtype.to_string()));
            query.push((format!("criteria[{idx}][value]"), value));
            idx += 1;
        };

        if let Some(keywords) = &params.keywords {
            push_criterion(&mut query, "1", "contains", keywords.clone());
        }
        if let Some(status) = params.status {
            push_criterion(&mut query, "12", "equals", status.to_string());
        }
        if let Some(ticket_type) = params.ticket_type {
            push_criterion(&mut query, "14", "equals", ticket_type.to_string());
        }
        if let Some(category_id) = params.category_id {
            push_criterion(&mut query, "7", "equals", category_id.to_string());
        }
        if let Some(assigned_user_id) = params.assigned_user_id {
            push_criterion(&mut query, "5", "equals", assigned_user_id.to_string());
        }

        let result = self
            .client
            .get("/search/Ticket", Some(&query))
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(result))
    }

    #[rmcp::tool(description = "Create a new ticket")]
    pub async fn create_ticket(
        &self,
        Parameters(params): Parameters<CreateTicketParams>,
    ) -> Result<Json<Value>, String> {
        let mut input = json!({
            "name": params.name,
            "content": params.content,
            "type": params.ticket_type,
            "priority": params.priority,
        });
        let obj = input.as_object_mut().expect("object literal");
        if let Some(category_id) = params.category_id {
            obj.insert("itilcategories_id".into(), json!(category_id));
        }
        if let Some(assigned_user_id) = params.assigned_user_id {
            obj.insert("_users_id_assign".into(), json!(assigned_user_id));
        }
        if let Some(assigned_group_id) = params.assigned_group_id {
            obj.insert("_groups_id_assign".into(), json!(assigned_group_id));
        }

        let result = self
            .client
            .post("/Ticket", &json!({ "input": input }))
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(result))
    }

    #[rmcp::tool(description = "Update a ticket; pass only the fields to change")]
    pub async fn update_ticket(
        &self,
        Parameters(params): Parameters<UpdateTicketParams>,
    ) -> Result<Json<Value>, String> {
        let result = self
            .client
            .put(
                &format!("/Ticket/{}", params.ticket_id),
                &json!({ "input": params.update_fields }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(result))
    }

    #[rmcp::tool(description = "Delete a ticket by ID")]
    pub async fn delete_ticket(
        &self,
        Parameters(params): Parameters<DeleteTicketParams>,
    ) -> Result<Json<Value>, String> {
        let result = self
            .client
            .delete(&format!("/Ticket/{}", params.ticket_id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(result))
    }

    #[rmcp::tool(description = "Link two tickets together")]
    pub async fn link_tickets(
        &self,
        Parameters(params): Parameters<LinkTicketsParams>,
    ) -> Result<Json<Value>, String> {
        let result = self
            .client
            .post(
                "/Ticket_Ticket",
                &json!({ "input": {
                    "tickets_id_1": params.ticket_id_1,
                    "tickets_id_2": params.ticket_id_2,
                    "link": params.link_type,
                } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(result))
    }

    #[rmcp::tool(description = "List all links between a ticket and other tickets")]
    pub async fn list_ticket_links(
        &self,
        Parameters(params): Parameters<ListTicketLinksParams>,
    ) -> Result<Json<Value>, String> {
        let result = self
            .client
            .get(&format!("/Ticket/{}/Ticket_Ticket", params.ticket_id), None)
            .await
            .map_err(|e| e.to_string())?;
        let enriched = match result {
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|mut link| {
                        if let Some(obj) = link.as_object_mut() {
                            let link_type = obj.get("link").and_then(Value::as_i64);
                            obj.insert("_link_label".into(), json!(self.labels.ticket_link_label(link_type)));
                        }
                        link
                    })
                    .collect(),
            ),
            other => other,
        };
        Ok(Json(enriched))
    }

    #[rmcp::tool(
        description = "Merge one or more source tickets into a target ticket: links each source as a \
            duplicate, optionally copies its followups to the target, and optionally closes it"
    )]
    pub async fn merge_tickets(
        &self,
        Parameters(params): Parameters<MergeTicketsParams>,
    ) -> Result<Json<Value>, String> {
        let mut merged = Vec::new();
        let mut errors = Vec::new();

        for source_id in params.source_ticket_ids {
            match self.merge_one_ticket(params.target_ticket_id, source_id, params.add_followups, params.close_source).await {
                Ok(entry) => merged.push(entry),
                Err(err) => errors.push(json!({ "source_ticket_id": source_id, "error": err })),
            }
        }

        Ok(Json(json!({
            "target_ticket_id": params.target_ticket_id,
            "merged": merged,
            "errors": errors,
        })))
    }
}

impl GlpiServer {
    async fn merge_one_ticket(
        &self,
        target_ticket_id: i64,
        source_id: i64,
        add_followups: bool,
        close_source: bool,
    ) -> Result<Value, String> {
        let link = self
            .client
            .post(
                "/Ticket_Ticket",
                &json!({ "input": { "tickets_id_1": source_id, "tickets_id_2": target_ticket_id, "link": 2 } }),
            )
            .await
            .map_err(|e| e.to_string())?;

        let mut followups_copied = 0;
        if add_followups {
            let followups = self
                .client
                .get(&format!("/Ticket/{source_id}/ITILFollowup"), None)
                .await
                .map_err(|e| e.to_string())?;
            if let Value::Array(items) = followups {
                for followup in items {
                    let content = followup.get("content").and_then(Value::as_str).unwrap_or_default();
                    let is_private = followup.get("is_private").cloned().unwrap_or(json!(0));
                    self.client
                        .post(
                            "/ITILFollowup",
                            &json!({ "input": {
                                "items_id": target_ticket_id,
                                "itemtype": "Ticket",
                                "content": format!("[Merged from ticket #{source_id}] {content}"),
                                "is_private": is_private,
                            } }),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    followups_copied += 1;
                }
            }
        }

        let mut closed = false;
        if close_source {
            self.client
                .post(
                    "/ITILFollowup",
                    &json!({ "input": {
                        "items_id": source_id,
                        "itemtype": "Ticket",
                        "content": format!("Merged into ticket #{target_ticket_id}."),
                        "is_private": 0,
                    } }),
                )
                .await
                .map_err(|e| e.to_string())?;
            self.client
                .put(&format!("/Ticket/{source_id}"), &json!({ "input": { "status": 6 } }))
                .await
                .map_err(|e| e.to_string())?;
            closed = true;
        }

        Ok(json!({
            "source_ticket_id": source_id,
            "link": link,
            "followups_copied": followups_copied,
            "closed": closed,
        }))
    }
}
