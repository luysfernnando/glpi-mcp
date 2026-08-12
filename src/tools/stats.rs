use std::collections::HashMap;

use chrono::NaiveDateTime;
use rmcp::tool_router;
use serde_json::Value;

use crate::labels::lookup;
use crate::markdown::{escape_cell, field_table, table};
use crate::server::GlpiServer;

const PAGE_SIZE: i64 = 500;
const STATUS_SOLVED: i64 = 5;
const STATUS_CLOSED: i64 = 6;
const GLPI_DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

// Stable core GLPI search-option field IDs for Ticket (unchanged across v10/v11
// in practice; only high-numbered plugin-derived options drift between versions).
const F_ID: &str = "2";
const F_NAME: &str = "1";
const F_STATUS: &str = "12";
const F_TYPE: &str = "14";
const F_PRIORITY: &str = "3";
const F_CATEGORY: &str = "7";
const F_ASSIGNED: &str = "5";
const F_OPENED: &str = "15";
const F_SOLVED: &str = "17";
const F_DEADLINE: &str = "18";

fn parse_glpi_datetime(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, GLPI_DATETIME_FORMAT).ok()
}

/// Reads a search-result row field as `&str`; GLPI's `/search/*` endpoints key
/// rows by these numeric field IDs. Text fields (names, dates) come back as
/// JSON strings; enum fields (status, type, priority) come back as bare JSON
/// numbers when only that field is requested — callers needing those should
/// use `field_i64` instead.
fn field<'a>(row: &'a Value, id: &str) -> Option<&'a str> {
    row.get(id).and_then(Value::as_str)
}

/// Reads a search-result row field as `i64`, accepting either a bare JSON
/// number or a numeric string (GLPI's `/search/*` serializes the same
/// enum field as either depending on what else is requested alongside it).
fn field_i64(row: &Value, id: &str) -> Option<i64> {
    match row.get(id) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

fn render_counts(total_label: &str, total: usize, counts: HashMap<&str, i64>) -> String {
    let mut rows: Vec<(&str, i64)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let table_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|(label, count)| vec![escape_cell(label), count.to_string()])
        .collect();
    format!(
        "**{total_label}: {total}**\n\n{}",
        table(&["Label", "Count"], &table_rows)
    )
}

impl GlpiServer {
    /// Fetches only the requested search-option fields of every ticket via
    /// `/search/Ticket`, range-paginated, instead of the full `/Ticket` object.
    /// GLPI resolves joined fields (category name, assigned technician) server
    /// side, so this both slashes the payload size and skips local ID lookups —
    /// avoiding the PHP memory/timeout crashes a full-object dump risks on large
    /// instances. Rows are returned as raw `Value` objects (no re-copy into an
    /// owned map) since `serde_json::Map` already gives O(1)-ish field lookups.
    async fn fetch_ticket_rows(&self, fields: &[&str]) -> Result<Vec<Value>, String> {
        let mut all_rows = Vec::new();
        let mut offset: i64 = 0;
        loop {
            let range = format!("{offset}-{}", offset + PAGE_SIZE - 1);
            let mut query: Vec<(String, String)> = vec![("range".to_string(), range)];
            for (idx, field) in fields.iter().enumerate() {
                query.push((format!("forcedisplay[{idx}]"), field.to_string()));
            }

            let mut result = self
                .client
                .get("/search/Ticket", Some(&query))
                .await
                .map_err(|e| e.to_string())?;
            let data = match result.get_mut("data").map(Value::take) {
                Some(Value::Array(items)) => items,
                _ => Vec::new(),
            };
            let fetched = data.len();
            all_rows.extend(data);

            if fetched < PAGE_SIZE as usize {
                break;
            }
            offset += PAGE_SIZE;
        }
        Ok(all_rows)
    }
}

#[tool_router(router = stats_tool_router, vis = "pub")]
impl GlpiServer {
    #[rmcp::tool(description = "Ticket count grouped by status, as a Markdown table")]
    pub async fn stats_by_status(&self) -> Result<String, String> {
        let rows = self.fetch_ticket_rows(&[F_STATUS]).await?;
        let mut counts: HashMap<&str, i64> = self
            .labels
            .ticket_status
            .values()
            .map(|&l| (l, 0))
            .collect();
        for row in &rows {
            let status = field_i64(row, F_STATUS);
            let label = lookup(&self.labels.ticket_status, status, self.labels.unknown);
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(render_counts("Total tickets", rows.len(), counts))
    }

    #[rmcp::tool(
        description = "Ticket count grouped by type (Incident / Service request), as a Markdown table"
    )]
    pub async fn stats_by_type(&self) -> Result<String, String> {
        let rows = self.fetch_ticket_rows(&[F_TYPE]).await?;
        let mut counts: HashMap<&str, i64> =
            self.labels.ticket_type.values().map(|&l| (l, 0)).collect();
        for row in &rows {
            let ticket_type = field_i64(row, F_TYPE);
            let label = lookup(&self.labels.ticket_type, ticket_type, self.labels.unknown);
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(render_counts("Total tickets", rows.len(), counts))
    }

    #[rmcp::tool(
        description = "Open (non-solved/closed) ticket count grouped by priority, as a Markdown table"
    )]
    pub async fn stats_by_priority(&self) -> Result<String, String> {
        let rows = self.fetch_ticket_rows(&[F_STATUS, F_PRIORITY]).await?;
        let open_rows: Vec<&Value> = rows
            .iter()
            .filter(|r| {
                !matches!(
                    field_i64(r, F_STATUS),
                    Some(STATUS_SOLVED) | Some(STATUS_CLOSED)
                )
            })
            .collect();

        let mut counts: HashMap<&str, i64> = self
            .labels
            .ticket_priority
            .values()
            .map(|&l| (l, 0))
            .collect();
        for row in &open_rows {
            let priority = field_i64(row, F_PRIORITY);
            let label = lookup(
                &self.labels.ticket_priority,
                priority,
                self.labels.unknown_f,
            );
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(render_counts("Total open tickets", open_rows.len(), counts))
    }

    #[rmcp::tool(description = "Ticket count grouped by ITIL category, as a Markdown table")]
    pub async fn stats_by_category(&self) -> Result<String, String> {
        let rows = self.fetch_ticket_rows(&[F_CATEGORY]).await?;
        let mut counts: HashMap<&str, i64> = HashMap::new();
        for row in &rows {
            let label = field(row, F_CATEGORY)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(self.labels.uncategorized);
            *counts.entry(label).or_insert(0) += 1;
        }
        Ok(render_counts("Total tickets", rows.len(), counts))
    }

    #[rmcp::tool(description = "Ticket count grouped by assigned technician, as a Markdown table")]
    pub async fn stats_by_assignee(&self) -> Result<String, String> {
        let rows = self.fetch_ticket_rows(&[F_ASSIGNED]).await?;
        let mut counts: HashMap<&str, i64> = HashMap::new();
        for row in &rows {
            let raw = field(row, F_ASSIGNED).map(str::trim).unwrap_or("");
            if raw.is_empty() {
                *counts.entry(self.labels.unassigned).or_insert(0) += 1;
                continue;
            }
            for name in raw.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    *counts.entry(name).or_insert(0) += 1;
                }
            }
        }
        Ok(render_counts("Total tickets", rows.len(), counts))
    }

    #[rmcp::tool(description = "Average resolution time (hours/days) of solved or closed tickets")]
    pub async fn stats_resolution_time(&self) -> Result<String, String> {
        let rows = self
            .fetch_ticket_rows(&[F_STATUS, F_OPENED, F_SOLVED])
            .await?;
        let resolved: Vec<&Value> = rows
            .iter()
            .filter(|r| {
                matches!(
                    field_i64(r, F_STATUS),
                    Some(STATUS_SOLVED) | Some(STATUS_CLOSED)
                )
            })
            .collect();

        let deltas: Vec<f64> = resolved
            .iter()
            .filter_map(|r| {
                let opened = parse_glpi_datetime(field(r, F_OPENED)?)?;
                let solved = parse_glpi_datetime(field(r, F_SOLVED)?)?;
                let hours = (solved - opened).num_seconds() as f64 / 3600.0;
                (hours >= 0.0).then_some(hours)
            })
            .collect();

        let avg_hours = if deltas.is_empty() {
            0.0
        } else {
            (deltas.iter().sum::<f64>() / deltas.len() as f64 * 100.0).round() / 100.0
        };
        let avg_days = (avg_hours / 24.0 * 100.0).round() / 100.0;

        Ok(field_table(&[
            ("Resolved count", resolved.len().to_string()),
            ("With usable dates", deltas.len().to_string()),
            ("Avg resolution (hours)", avg_hours.to_string()),
            ("Avg resolution (days)", avg_days.to_string()),
        ]))
    }

    #[rmcp::tool(
        description = "Open tickets past their time_to_resolve deadline, sorted by hours overdue, as a Markdown table"
    )]
    pub async fn stats_overdue(&self) -> Result<String, String> {
        let rows = self
            .fetch_ticket_rows(&[F_ID, F_NAME, F_DEADLINE, F_STATUS, F_PRIORITY])
            .await?;
        let now = chrono::Local::now().naive_local();

        let open_rows: Vec<&Value> = rows
            .iter()
            .filter(|r| {
                !matches!(
                    field_i64(r, F_STATUS),
                    Some(STATUS_SOLVED) | Some(STATUS_CLOSED)
                )
            })
            .collect();

        struct Overdue<'a> {
            id: &'a str,
            name: &'a str,
            deadline: &'a str,
            overdue_hours: f64,
            status_label: &'a str,
            priority_label: &'a str,
        }

        let mut overdue: Vec<Overdue> = open_rows
            .iter()
            .filter_map(|row| {
                let deadline_str = field(row, F_DEADLINE).filter(|s| !s.is_empty())?;
                let deadline = parse_glpi_datetime(deadline_str)?;
                if deadline >= now {
                    return None;
                }
                let overdue_hours =
                    ((now - deadline).num_seconds() as f64 / 3600.0 * 10.0).round() / 10.0;
                let status = field_i64(row, F_STATUS);
                let priority = field_i64(row, F_PRIORITY);
                Some(Overdue {
                    id: field(row, F_ID).unwrap_or_default(),
                    name: field(row, F_NAME).unwrap_or_default(),
                    deadline: deadline_str,
                    overdue_hours,
                    status_label: lookup(&self.labels.ticket_status, status, self.labels.unknown),
                    priority_label: lookup(
                        &self.labels.ticket_priority,
                        priority,
                        self.labels.unknown_f,
                    ),
                })
            })
            .collect();

        overdue.sort_by(|a, b| b.overdue_hours.total_cmp(&a.overdue_hours));

        if overdue.is_empty() {
            return Ok(format!(
                "**Total open tickets: {}**\n\nNo overdue tickets.",
                open_rows.len()
            ));
        }

        let rows: Vec<Vec<String>> = overdue
            .iter()
            .map(|o| {
                vec![
                    o.id.to_string(),
                    escape_cell(o.name),
                    o.deadline.to_string(),
                    format!("{}h", o.overdue_hours),
                    o.status_label.to_string(),
                    o.priority_label.to_string(),
                ]
            })
            .collect();

        Ok(format!(
            "**Total open: {}, overdue: {}**\n\n{}",
            open_rows.len(),
            overdue.len(),
            table(
                &["ID", "Name", "Deadline", "Overdue", "Status", "Priority"],
                &rows
            )
        ))
    }
}
