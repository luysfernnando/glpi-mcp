use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::markdown::{escape_cell, id_field, into_array, table};
use crate::server::GlpiServer;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindGroupRuleReferencesParams {
    #[schemars(description = "GLPI group ID to search for in ticket routing rules")]
    pub group_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateRuleActionParams {
    pub rule_action_id: i64,
    #[schemars(description = "New value, e.g. the replacement group ID as a string")]
    pub value: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindRulesParams {
    #[schemars(description = "Rule name to search for (case-insensitive, partial match)")]
    pub name: String,
    #[schemars(
        description = "Filter by rule sub_type, e.g. \"RuleTicket\" (ticket routing/SLA rules) \
            or \"RuleAsset\"; omit to search all"
    )]
    pub sub_type: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateRuleParams {
    pub name: String,
    #[schemars(description = "e.g. \"RuleTicket\"")]
    pub sub_type: String,
    #[schemars(description = "\"AND\" or \"OR\" — how criteria combine")]
    #[serde(default = "default_match")]
    pub r#match: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub description: Option<String>,
    #[schemars(description = "Owning entity ID (0 = root entity)")]
    #[serde(default)]
    pub entities_id: i64,
}

fn default_match() -> String {
    "AND".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateRuleParams {
    pub rule_id: i64,
    #[schemars(description = "Fields to change, e.g. name, is_active, match, ranking, ...")]
    pub update_fields: Map<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteRuleParams {
    pub rule_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuleIdParams {
    pub rule_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateRuleCriteriaParams {
    pub rule_id: i64,
    #[schemars(description = "Field name the criterion matches on, e.g. \"category\", \"name\"")]
    pub criteria: String,
    #[schemars(description = "GLPI condition code, e.g. 0 = is, 1 = is not, 2 = contains")]
    #[serde(default)]
    pub condition: i64,
    #[schemars(description = "Value to match against")]
    pub pattern: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateRuleCriteriaParams {
    pub rule_criteria_id: i64,
    #[schemars(description = "Fields to change, e.g. criteria, condition, pattern")]
    pub update_fields: Map<String, Value>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteRuleCriteriaParams {
    pub rule_criteria_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateRuleActionParams {
    pub rule_id: i64,
    #[schemars(description = "e.g. \"assign\", \"append\", \"fromuser\"")]
    #[serde(default = "default_action_type")]
    pub action_type: String,
    #[schemars(description = "Field the action sets, e.g. \"groups_id_assign\", \"slas_id_ttr\"")]
    pub field: String,
    #[schemars(description = "Value to assign, e.g. a group/SLA ID as a string")]
    pub value: String,
}

fn default_action_type() -> String {
    "assign".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteRuleActionParams {
    pub rule_action_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DuplicateRuleParams {
    #[schemars(description = "Rule ID to copy criteria and actions from")]
    pub source_rule_id: i64,
    #[schemars(description = "Name for the new rule")]
    pub new_name: String,
    #[schemars(
        description = "Optional value overrides applied to copied actions, keyed by action \
            \"field\" name (e.g. {\"groups_id_assign\": \"42\"}) — use this to point the new \
            rule at the new unit's group/SLA instead of the source's"
    )]
    #[serde(default)]
    pub action_value_overrides: Map<String, Value>,
}

#[tool_router(router = rules_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(
        description = "Scan RuleTicket (ticket routing) rules for criteria/actions referencing a \
            group ID, e.g. before deactivating a group. Returns rule id/name, whether the rule is \
            active, and the matching criteria or action row (kind, field, value, row id) — \
            read-only, changes nothing. Follow up with update_rule_action to redirect a match to a \
            different group"
    )]
    pub async fn find_group_rule_references(
        &self,
        Parameters(params): Parameters<FindGroupRuleReferencesParams>,
    ) -> Result<String, String> {
        let target = params.group_id.to_string();
        let query = vec![
            ("criteria[0][field]".to_string(), "122".to_string()),
            (
                "criteria[0][searchtype]".to_string(),
                "contains".to_string(),
            ),
            ("criteria[0][value]".to_string(), "RuleTicket".to_string()),
            ("forcedisplay[0]".to_string(), "2".to_string()),
            ("forcedisplay[1]".to_string(), "1".to_string()),
            ("forcedisplay[2]".to_string(), "8".to_string()),
            ("range".to_string(), "0-500".to_string()),
        ];
        let result = self
            .client
            .get("/search/Rule", Some(&query))
            .await
            .map_err(|e| e.to_string())?;
        let rules = into_array(result.get("data").cloned().unwrap_or(Value::Null));

        let mut rows: Vec<Vec<String>> = Vec::new();
        for rule in &rules {
            let rule_id = id_field(rule, "2");
            let rule_name = rule
                .get("1")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let is_active = rule.get("8").and_then(Value::as_i64).unwrap_or(0) == 1;

            let criteria = self
                .client
                .get(&format!("/Rule/{rule_id}/RuleCriteria"), None)
                .await
                .map_err(|e| e.to_string())?;
            for row in into_array(criteria) {
                let field = row.get("criteria").and_then(Value::as_str).unwrap_or("");
                let value = id_field(&row, "pattern");
                if field.to_lowercase().contains("group") && value == target {
                    rows.push(vec![
                        rule_id.clone(),
                        escape_cell(&rule_name),
                        if is_active { "yes" } else { "no" }.to_string(),
                        "criteria".to_string(),
                        escape_cell(field),
                        escape_cell(&value),
                        id_field(&row, "id"),
                    ]);
                }
            }

            let actions = self
                .client
                .get(&format!("/Rule/{rule_id}/RuleAction"), None)
                .await
                .map_err(|e| e.to_string())?;
            for row in into_array(actions) {
                let field = row.get("field").and_then(Value::as_str).unwrap_or("");
                let value = id_field(&row, "value");
                if field.to_lowercase().contains("group") && value == target {
                    rows.push(vec![
                        rule_id.clone(),
                        escape_cell(&rule_name),
                        if is_active { "yes" } else { "no" }.to_string(),
                        "action".to_string(),
                        escape_cell(field),
                        escape_cell(&value),
                        id_field(&row, "id"),
                    ]);
                }
            }
        }

        if rows.is_empty() {
            return Ok(format!(
                "No RuleTicket criteria/actions reference group #{}.",
                params.group_id
            ));
        }

        Ok(format!(
            "**{} match(es) referencing group #{}**\n\n{}",
            rows.len(),
            params.group_id,
            table(
                &[
                    "Rule ID",
                    "Rule name",
                    "Active",
                    "Kind",
                    "Field",
                    "Value",
                    "Row ID"
                ],
                &rows
            )
        ))
    }

    #[rmcp::tool(
        description = "Update a GLPI RuleAction's value, e.g. to redirect a ticket-routing rule \
            found via find_group_rule_references to a different group"
    )]
    pub async fn update_rule_action(
        &self,
        Parameters(params): Parameters<UpdateRuleActionParams>,
    ) -> Result<String, String> {
        self.client
            .put(
                &format!("/RuleAction/{}", params.rule_action_id),
                &json!({ "input": { "value": params.value } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("RuleAction #{} updated.", params.rule_action_id))
    }

    #[rmcp::tool(
        description = "Find GLPI rules by name (partial match), optionally filtered by \
            sub_type (e.g. \"RuleTicket\"). Use this to resolve a rule's ID before update_rule / \
            delete_rule / list_rule_criteria / list_rule_actions / duplicate_rule"
    )]
    pub async fn find_rules(
        &self,
        Parameters(params): Parameters<FindRulesParams>,
    ) -> Result<String, String> {
        let mut query = vec![
            ("criteria[0][field]".to_string(), "1".to_string()),
            (
                "criteria[0][searchtype]".to_string(),
                "contains".to_string(),
            ),
            ("criteria[0][value]".to_string(), params.name),
            ("forcedisplay[0]".to_string(), "2".to_string()),
            ("forcedisplay[1]".to_string(), "1".to_string()),
            ("forcedisplay[2]".to_string(), "8".to_string()),
        ];
        if let Some(sub_type) = params.sub_type {
            query.push(("criteria[1][link]".to_string(), "AND".to_string()));
            query.push(("criteria[1][field]".to_string(), "122".to_string()));
            query.push((
                "criteria[1][searchtype]".to_string(),
                "contains".to_string(),
            ));
            query.push(("criteria[1][value]".to_string(), sub_type));
        }

        let result = self
            .client
            .get("/search/Rule", Some(&query))
            .await
            .map_err(|e| e.to_string())?;
        let data = into_array(result.get("data").cloned().unwrap_or(Value::Null));
        if data.is_empty() {
            return Ok("No matching rules.".to_string());
        }

        let rows: Vec<Vec<String>> = data
            .iter()
            .map(|row| {
                let is_active = row.get("8").and_then(Value::as_i64).unwrap_or(0) == 1;
                vec![
                    id_field(row, "2"),
                    escape_cell(row.get("1").and_then(Value::as_str).unwrap_or("")),
                    if is_active { "yes" } else { "no" }.to_string(),
                ]
            })
            .collect();

        Ok(format!(
            "**{} rule(s) found**\n\n{}",
            rows.len(),
            table(&["ID", "Name", "Active"], &rows)
        ))
    }

    #[rmcp::tool(
        description = "Create a new GLPI rule (e.g. a ticket-routing / SLA rule), initially \
            with no criteria or actions — add those with create_rule_criteria and \
            create_rule_action"
    )]
    pub async fn create_rule(
        &self,
        Parameters(params): Parameters<CreateRuleParams>,
    ) -> Result<String, String> {
        let mut input = json!({
            "name": params.name,
            "sub_type": params.sub_type,
            "match": params.r#match,
            "is_active": params.is_active as i32,
            "entities_id": params.entities_id,
        });
        if let Some(description) = params.description {
            input
                .as_object_mut()
                .expect("object literal")
                .insert("description".into(), json!(description));
        }

        let result = self
            .client
            .post("/Rule", &json!({ "input": input }))
            .await
            .map_err(|e| e.to_string())?;
        let id = id_field(&result, "id");
        Ok(format!("Rule #{id} \"{}\" created.", params.name))
    }

    #[rmcp::tool(description = "Update a GLPI rule; pass only the fields to change")]
    pub async fn update_rule(
        &self,
        Parameters(params): Parameters<UpdateRuleParams>,
    ) -> Result<String, String> {
        self.client
            .put(
                &format!("/Rule/{}", params.rule_id),
                &json!({ "input": params.update_fields }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Rule #{} updated.", params.rule_id))
    }

    #[rmcp::tool(description = "Delete a GLPI rule by ID (also removes its criteria/actions)")]
    pub async fn delete_rule(
        &self,
        Parameters(params): Parameters<DeleteRuleParams>,
    ) -> Result<String, String> {
        self.client
            .delete(&format!("/Rule/{}", params.rule_id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Rule #{} deleted.", params.rule_id))
    }

    #[rmcp::tool(description = "List a rule's criteria (conditions) as a Markdown table")]
    pub async fn list_rule_criteria(
        &self,
        Parameters(params): Parameters<RuleIdParams>,
    ) -> Result<String, String> {
        let result = self
            .client
            .get(&format!("/Rule/{}/RuleCriteria", params.rule_id), None)
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<Vec<String>> = into_array(result)
            .iter()
            .map(|row| {
                vec![
                    id_field(row, "id"),
                    escape_cell(row.get("criteria").and_then(Value::as_str).unwrap_or("")),
                    id_field(row, "condition"),
                    escape_cell(&id_field(row, "pattern")),
                ]
            })
            .collect();
        if rows.is_empty() {
            return Ok(format!("Rule #{} has no criteria.", params.rule_id));
        }
        Ok(table(&["ID", "Field", "Condition", "Value"], &rows))
    }

    #[rmcp::tool(
        description = "List a rule's actions (what it assigns, e.g. group/SLA/OLA/priority) as \
            a Markdown table"
    )]
    pub async fn list_rule_actions(
        &self,
        Parameters(params): Parameters<RuleIdParams>,
    ) -> Result<String, String> {
        let result = self
            .client
            .get(&format!("/Rule/{}/RuleAction", params.rule_id), None)
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<Vec<String>> = into_array(result)
            .iter()
            .map(|row| {
                vec![
                    id_field(row, "id"),
                    escape_cell(row.get("field").and_then(Value::as_str).unwrap_or("")),
                    escape_cell(row.get("action_type").and_then(Value::as_str).unwrap_or("")),
                    escape_cell(&id_field(row, "value")),
                ]
            })
            .collect();
        if rows.is_empty() {
            return Ok(format!("Rule #{} has no actions.", params.rule_id));
        }
        Ok(table(&["ID", "Field", "Action type", "Value"], &rows))
    }

    #[rmcp::tool(description = "Add a new criterion (condition) to an existing rule")]
    pub async fn create_rule_criteria(
        &self,
        Parameters(params): Parameters<CreateRuleCriteriaParams>,
    ) -> Result<String, String> {
        let result = self
            .client
            .post(
                "/RuleCriteria",
                &json!({ "input": {
                    "rules_id": params.rule_id,
                    "criteria": params.criteria,
                    "condition": params.condition,
                    "pattern": params.pattern,
                } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        let id = id_field(&result, "id");
        Ok(format!(
            "RuleCriteria #{id} added to rule #{}.",
            params.rule_id
        ))
    }

    #[rmcp::tool(description = "Update a rule criterion; pass only the fields to change")]
    pub async fn update_rule_criteria(
        &self,
        Parameters(params): Parameters<UpdateRuleCriteriaParams>,
    ) -> Result<String, String> {
        self.client
            .put(
                &format!("/RuleCriteria/{}", params.rule_criteria_id),
                &json!({ "input": params.update_fields }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "RuleCriteria #{} updated.",
            params.rule_criteria_id
        ))
    }

    #[rmcp::tool(description = "Delete a rule criterion by ID")]
    pub async fn delete_rule_criteria(
        &self,
        Parameters(params): Parameters<DeleteRuleCriteriaParams>,
    ) -> Result<String, String> {
        self.client
            .delete(&format!("/RuleCriteria/{}", params.rule_criteria_id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "RuleCriteria #{} deleted.",
            params.rule_criteria_id
        ))
    }

    #[rmcp::tool(
        description = "Add a new action to an existing rule, e.g. assign a group/SLA/OLA/\
            priority when the rule matches. Missing today's only tool was update_rule_action, \
            which requires an action to already exist — use this to add the first one"
    )]
    pub async fn create_rule_action(
        &self,
        Parameters(params): Parameters<CreateRuleActionParams>,
    ) -> Result<String, String> {
        let result = self
            .client
            .post(
                "/RuleAction",
                &json!({ "input": {
                    "rules_id": params.rule_id,
                    "action_type": params.action_type,
                    "field": params.field,
                    "value": params.value,
                } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        let id = id_field(&result, "id");
        Ok(format!(
            "RuleAction #{id} added to rule #{}.",
            params.rule_id
        ))
    }

    #[rmcp::tool(description = "Delete a rule action by ID")]
    pub async fn delete_rule_action(
        &self,
        Parameters(params): Parameters<DeleteRuleActionParams>,
    ) -> Result<String, String> {
        self.client
            .delete(&format!("/RuleAction/{}", params.rule_action_id))
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("RuleAction #{} deleted.", params.rule_action_id))
    }

    #[rmcp::tool(
        description = "Duplicate a rule (e.g. an SLA ticket-routing rule) together with all \
            its criteria and actions — the standard way to set up service levels for a newly \
            created org unit by cloning an existing unit's rule. Pass action_value_overrides to \
            point copied actions (by field name) at the new unit's group/SLA instead of the \
            source's"
    )]
    pub async fn duplicate_rule(
        &self,
        Parameters(params): Parameters<DuplicateRuleParams>,
    ) -> Result<String, String> {
        let source = self
            .client
            .get(&format!("/Rule/{}", params.source_rule_id), None)
            .await
            .map_err(|e| e.to_string())?;

        let sub_type = source.get("sub_type").cloned().unwrap_or(json!(""));
        let rule_match = source.get("match").cloned().unwrap_or(json!("AND"));
        let entities_id = source.get("entities_id").cloned().unwrap_or(json!(0));

        let created = self
            .client
            .post(
                "/Rule",
                &json!({ "input": {
                    "name": params.new_name,
                    "sub_type": sub_type,
                    "match": rule_match,
                    "is_active": 1,
                    "entities_id": entities_id,
                } }),
            )
            .await
            .map_err(|e| e.to_string())?;
        let new_rule_id = id_field(&created, "id");

        let criteria = self
            .client
            .get(
                &format!("/Rule/{}/RuleCriteria", params.source_rule_id),
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut criteria_copied = 0usize;
        for row in into_array(criteria) {
            let field = row.get("criteria").and_then(Value::as_str).unwrap_or("");
            if field.is_empty() {
                continue;
            }
            self.client
                .post(
                    "/RuleCriteria",
                    &json!({ "input": {
                        "rules_id": new_rule_id,
                        "criteria": field,
                        "condition": row.get("condition").cloned().unwrap_or(json!(0)),
                        "pattern": row.get("pattern").cloned().unwrap_or(json!("")),
                    } }),
                )
                .await
                .map_err(|e| e.to_string())?;
            criteria_copied += 1;
        }

        let actions = self
            .client
            .get(&format!("/Rule/{}/RuleAction", params.source_rule_id), None)
            .await
            .map_err(|e| e.to_string())?;
        let mut actions_copied = 0usize;
        for row in into_array(actions) {
            let field = row.get("field").and_then(Value::as_str).unwrap_or("");
            if field.is_empty() {
                continue;
            }
            let value = params
                .action_value_overrides
                .get(field)
                .cloned()
                .or_else(|| row.get("value").cloned())
                .unwrap_or(json!(""));
            self.client
                .post(
                    "/RuleAction",
                    &json!({ "input": {
                        "rules_id": new_rule_id,
                        "action_type": row.get("action_type").cloned().unwrap_or(json!("assign")),
                        "field": field,
                        "value": value,
                    } }),
                )
                .await
                .map_err(|e| e.to_string())?;
            actions_copied += 1;
        }

        Ok(format!(
            "Rule #{new_rule_id} \"{}\" created from #{} ({criteria_copied} criteria, \
                {actions_copied} action(s) copied).",
            params.new_name, params.source_rule_id
        ))
    }
}
