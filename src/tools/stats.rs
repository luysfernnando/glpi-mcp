use std::collections::HashMap;

use chrono::NaiveDateTime;
use rmcp::handler::server::wrapper::Json;
use rmcp::tool_router;
use serde_json::{json, Value};

use crate::labels::lookup;
use crate::server::GlpiServer;

const PAGE_SIZE: i64 = 200;
const STATUS_SOLVED: i64 = 5;
const STATUS_CLOSED: i64 = 6;
const GLPI_DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

fn parse_glpi_datetime(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, GLPI_DATETIME_FORMAT).ok()
}

impl GlpiServer {
    /// Fetches every item of `endpoint` via range-paginated GET requests instead of a
    /// single unbounded `range=0-9999` call, so large GLPI instances don't risk hitting
    /// the PHP memory_limit the Python original was prone to.
    async fn fetch_all(&self, endpoint: &str) -> Result<Vec<Value>, String> {
        let mut all_items = Vec::new();
        let mut offset: i64 = 0;
        loop {
            let range = format!("{offset}-{}", offset + PAGE_SIZE - 1);
            let batch = self
                .client
                .get(endpoint, Some(&[("range".to_string(), range)]))
                .await
                .map_err(|e| e.to_string())?;
            let Value::Array(items) = batch else {
                break;
            };
            let fetched = items.len();
            all_items.extend(items);
            if fetched < PAGE_SIZE as usize {
                break;
            }
            offset += PAGE_SIZE;
        }
        Ok(all_items)
    }
}

#[tool_router(router = stats_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "Ticket count grouped by status")]
    pub async fn stats_by_status(&self) -> Result<Json<Value>, String> {
        let tickets = self.fetch_all("/Ticket").await?;
        let mut counts: HashMap<&str, i64> = self.labels.ticket_status.values().map(|&l| (l, 0)).collect();
        for ticket in &tickets {
            let status = ticket.get("status").and_then(Value::as_i64);
            let label = lookup(&self.labels.ticket_status, status, self.labels.unknown);
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(Json(json!({ "total": tickets.len(), "by_status": counts })))
    }

    #[rmcp::tool(description = "Ticket count grouped by type (Incident / Service request)")]
    pub async fn stats_by_type(&self) -> Result<Json<Value>, String> {
        let tickets = self.fetch_all("/Ticket").await?;
        let mut counts: HashMap<&str, i64> = self.labels.ticket_type.values().map(|&l| (l, 0)).collect();
        for ticket in &tickets {
            let ticket_type = ticket.get("type").and_then(Value::as_i64);
            let label = lookup(&self.labels.ticket_type, ticket_type, self.labels.unknown);
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(Json(json!({ "total": tickets.len(), "by_type": counts })))
    }

    #[rmcp::tool(description = "Open (non-solved/closed) ticket count grouped by priority")]
    pub async fn stats_by_priority(&self) -> Result<Json<Value>, String> {
        let tickets = self.fetch_all("/Ticket").await?;
        let open_tickets: Vec<&Value> = tickets
            .iter()
            .filter(|t| !matches!(t.get("status").and_then(Value::as_i64), Some(STATUS_SOLVED) | Some(STATUS_CLOSED)))
            .collect();

        let mut counts: HashMap<&str, i64> = self.labels.ticket_priority.values().map(|&l| (l, 0)).collect();
        for ticket in &open_tickets {
            let priority = ticket.get("priority").and_then(Value::as_i64);
            let label = lookup(&self.labels.ticket_priority, priority, self.labels.unknown_f);
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(Json(json!({ "total_open": open_tickets.len(), "by_priority": counts })))
    }

    #[rmcp::tool(description = "Ticket count grouped by ITIL category")]
    pub async fn stats_by_category(&self) -> Result<Json<Value>, String> {
        let tickets = self.fetch_all("/Ticket").await?;
        let categories = self.fetch_all("/ITILCategory").await?;

        let category_names: HashMap<i64, String> = categories
            .iter()
            .filter_map(|cat| {
                let id = cat.get("id").and_then(Value::as_i64)?;
                let name = cat
                    .get("completename")
                    .or_else(|| cat.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or(self.labels.unknown)
                    .to_string();
                Some((id, name))
            })
            .collect();

        let mut counts: HashMap<String, i64> = HashMap::new();
        for ticket in &tickets {
            let category_id = ticket.get("itilcategories_id").and_then(Value::as_i64).unwrap_or(0);
            let label = if category_id != 0 {
                category_names.get(&category_id).cloned().unwrap_or_else(|| self.labels.uncategorized.to_string())
            } else {
                self.labels.uncategorized.to_string()
            };
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(Json(json!({ "total": tickets.len(), "by_category": counts })))
    }

    #[rmcp::tool(description = "Ticket count grouped by assigned technician")]
    pub async fn stats_by_assignee(&self) -> Result<Json<Value>, String> {
        let tickets = self.fetch_all("/Ticket").await?;
        let users = self.fetch_all("/User").await?;

        let user_names: HashMap<i64, String> = users
            .iter()
            .filter_map(|user| {
                let id = user.get("id").and_then(Value::as_i64)?;
                let name = user
                    .get("realname")
                    .or_else(|| user.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or(self.labels.unknown)
                    .to_string();
                Some((id, name))
            })
            .collect();

        let mut counts: HashMap<String, i64> = HashMap::new();
        for ticket in &tickets {
            let fallback_uid = ticket.get("users_id_lastupdater").and_then(Value::as_i64).unwrap_or(0);
            let assigned = ticket.get("_users_id_assign");

            let mut record = |uid: i64| {
                let label = if uid != 0 {
                    user_names.get(&uid).cloned().unwrap_or_else(|| format!("User #{uid}"))
                } else {
                    self.labels.unassigned.to_string()
                };
                *counts.entry(label).or_insert(0) += 1;
            };

            match assigned {
                Some(Value::Array(items)) => {
                    for item in items {
                        let uid = item
                            .as_i64()
                            .or_else(|| item.get("users_id").and_then(Value::as_i64))
                            .unwrap_or(0);
                        record(uid);
                    }
                }
                Some(value) => record(value.as_i64().unwrap_or(0)),
                None => record(fallback_uid),
            }
        }
        Ok(Json(json!({ "total": tickets.len(), "by_assignee": counts })))
    }

    #[rmcp::tool(description = "Average resolution time (hours/days) of solved or closed tickets")]
    pub async fn stats_resolution_time(&self) -> Result<Json<Value>, String> {
        let tickets = self.fetch_all("/Ticket").await?;
        let resolved: Vec<&Value> = tickets
            .iter()
            .filter(|t| matches!(t.get("status").and_then(Value::as_i64), Some(STATUS_SOLVED) | Some(STATUS_CLOSED)))
            .collect();

        let deltas: Vec<f64> = resolved
            .iter()
            .filter_map(|t| {
                let opened = parse_glpi_datetime(t.get("date")?.as_str()?)?;
                let solved = parse_glpi_datetime(t.get("solvedate")?.as_str()?)?;
                let hours = (solved - opened).num_seconds() as f64 / 3600.0;
                (hours >= 0.0).then_some(hours)
            })
            .collect();

        let avg_hours = if deltas.is_empty() {
            0.0
        } else {
            (deltas.iter().sum::<f64>() / deltas.len() as f64 * 100.0).round() / 100.0
        };

        Ok(Json(json!({
            "resolved_count": resolved.len(),
            "with_dates": deltas.len(),
            "avg_resolution_hours": avg_hours,
            "avg_resolution_days": (avg_hours / 24.0 * 100.0).round() / 100.0,
        })))
    }

    #[rmcp::tool(
        description = "Open tickets past their time_to_resolve deadline, sorted by hours overdue"
    )]
    pub async fn stats_overdue(&self) -> Result<Json<Value>, String> {
        let tickets = self.fetch_all("/Ticket").await?;
        let now = chrono::Local::now().naive_local();

        let open_tickets: Vec<&Value> = tickets
            .iter()
            .filter(|t| !matches!(t.get("status").and_then(Value::as_i64), Some(STATUS_SOLVED) | Some(STATUS_CLOSED)))
            .collect();

        let mut overdue: Vec<Value> = open_tickets
            .iter()
            .filter_map(|ticket| {
                let deadline_str = ticket.get("time_to_resolve")?.as_str()?;
                let deadline = parse_glpi_datetime(deadline_str)?;
                if deadline >= now {
                    return None;
                }
                let overdue_hours = ((now - deadline).num_seconds() as f64 / 3600.0 * 10.0).round() / 10.0;
                let status = ticket.get("status").and_then(Value::as_i64);
                let priority = ticket.get("priority").and_then(Value::as_i64);
                Some(json!({
                    "id": ticket.get("id"),
                    "name": ticket.get("name"),
                    "deadline": deadline_str,
                    "overdue_hours": overdue_hours,
                    "_status_label": lookup(&self.labels.ticket_status, status, self.labels.unknown),
                    "_priority_label": lookup(&self.labels.ticket_priority, priority, self.labels.unknown_f),
                }))
            })
            .collect();

        overdue.sort_by(|a, b| {
            let ah = a["overdue_hours"].as_f64().unwrap_or(0.0);
            let bh = b["overdue_hours"].as_f64().unwrap_or(0.0);
            bh.total_cmp(&ah)
        });

        Ok(Json(json!({
            "total_open": open_tickets.len(),
            "overdue_count": overdue.len(),
            "overdue_tickets": overdue,
        })))
    }
}
