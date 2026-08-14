use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

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
}
