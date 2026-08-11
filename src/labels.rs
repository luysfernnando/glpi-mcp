use std::collections::HashMap;

use serde_json::{json, Value};

use crate::config::Language;

/// Human-readable labels for GLPI's numeric enum fields, mirrored fr/en.
/// Replaces the magic status/type/priority integers scattered through the Python original.
pub struct Labels {
    pub ticket_status: HashMap<i64, &'static str>,
    pub ticket_type: HashMap<i64, &'static str>,
    pub ticket_priority: HashMap<i64, &'static str>,
    pub task_status: HashMap<i64, &'static str>,
    pub ticket_link_type: HashMap<i64, &'static str>,
    pub unknown: &'static str,
    pub unknown_f: &'static str,
    pub unassigned: &'static str,
    pub uncategorized: &'static str,
    pub http_timeout_error: &'static str,
    pub http_timeout_detail: &'static str,
    pub kb_clamp_warning: &'static str,
}

impl Labels {
    pub fn for_language(lang: Language) -> Self {
        match lang {
            Language::Fr => Self::fr(),
            Language::En => Self::en(),
        }
    }

    fn fr() -> Self {
        Self {
            ticket_status: map([
                (1, "Nouveau"),
                (2, "En cours (attribué)"),
                (3, "En cours (planifié)"),
                (4, "En attente"),
                (5, "Résolu"),
                (6, "Clos"),
            ]),
            ticket_type: map([(1, "Incident"), (2, "Demande de service")]),
            ticket_priority: map([
                (1, "Très basse"),
                (2, "Basse"),
                (3, "Moyenne"),
                (4, "Haute"),
                (5, "Très haute"),
                (6, "Majeure"),
            ]),
            task_status: map([(1, "À faire"), (2, "Terminée")]),
            ticket_link_type: map([
                (1, "Lié à"),
                (2, "Duplique"),
                (3, "Enfant de"),
                (4, "Parent de"),
            ]),
            unknown: "Inconnu",
            unknown_f: "Inconnue",
            unassigned: "Non assigné",
            uncategorized: "Sans catégorie",
            http_timeout_error: "Timeout HTTP",
            http_timeout_detail: "Requête > 30s — voir GLPI logs",
            kb_clamp_warning: "range_limit ramené à 10 car range_start > 60 \
                (évite les erreurs PHP memory_limit côté GLPI sur de gros payloads KB).",
        }
    }

    fn en() -> Self {
        Self {
            ticket_status: map([
                (1, "New"),
                (2, "In progress (assigned)"),
                (3, "In progress (planned)"),
                (4, "Pending"),
                (5, "Solved"),
                (6, "Closed"),
            ]),
            ticket_type: map([(1, "Incident"), (2, "Service request")]),
            ticket_priority: map([
                (1, "Very low"),
                (2, "Low"),
                (3, "Medium"),
                (4, "High"),
                (5, "Very high"),
                (6, "Major"),
            ]),
            task_status: map([(1, "To do"), (2, "Done")]),
            ticket_link_type: map([
                (1, "Linked to"),
                (2, "Duplicates"),
                (3, "Child of"),
                (4, "Parent of"),
            ]),
            unknown: "Unknown",
            unknown_f: "Unknown",
            unassigned: "Unassigned",
            uncategorized: "Uncategorized",
            http_timeout_error: "HTTP timeout",
            http_timeout_detail: "Request > 30s — see GLPI logs",
            kb_clamp_warning: "range_limit clamped to 10 because range_start > 60 \
                (prevents PHP memory_limit errors on large KB payloads from GLPI).",
        }
    }
}

fn map<const N: usize>(pairs: [(i64, &'static str); N]) -> HashMap<i64, &'static str> {
    HashMap::from(pairs)
}

fn lookup<'a>(table: &'a HashMap<i64, &'static str>, value: Option<i64>, fallback: &'a str) -> &'a str {
    value.and_then(|v| table.get(&v).copied()).unwrap_or(fallback)
}

impl Labels {
    /// Adds `_status_label`/`_type_label`/`_priority_label`/`_urgency_label`/`_impact_label`
    /// fields to a ticket JSON object, mirroring the Python original's `_enrich_ticket`.
    pub fn enrich_ticket(&self, mut ticket: Value) -> Value {
        if let Some(obj) = ticket.as_object_mut() {
            let field = |obj: &serde_json::Map<String, Value>, key: &str| obj.get(key).and_then(Value::as_i64);
            let status = field(obj, "status");
            let ticket_type = field(obj, "type");
            let priority = field(obj, "priority");
            let urgency = field(obj, "urgency");
            let impact = field(obj, "impact");

            obj.insert("_status_label".into(), json!(lookup(&self.ticket_status, status, self.unknown)));
            obj.insert("_type_label".into(), json!(lookup(&self.ticket_type, ticket_type, self.unknown)));
            obj.insert(
                "_priority_label".into(),
                json!(lookup(&self.ticket_priority, priority, self.unknown_f)),
            );
            obj.insert(
                "_urgency_label".into(),
                json!(lookup(&self.ticket_priority, urgency, self.unknown_f)),
            );
            obj.insert("_impact_label".into(), json!(lookup(&self.ticket_priority, impact, self.unknown)));
        }
        ticket
    }

    /// Label for a `Ticket_Ticket.link` value (1=linked, 2=duplicate, 3=child, 4=parent).
    pub fn ticket_link_label(&self, link: Option<i64>) -> &str {
        lookup(&self.ticket_link_type, link, self.unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr_and_en_expose_the_same_status_keys() {
        let fr = Labels::for_language(Language::Fr);
        let en = Labels::for_language(Language::En);
        let mut fr_keys: Vec<_> = fr.ticket_status.keys().collect();
        let mut en_keys: Vec<_> = en.ticket_status.keys().collect();
        fr_keys.sort();
        en_keys.sort();
        assert_eq!(fr_keys, en_keys);
    }

    #[test]
    fn enrich_ticket_adds_readable_labels() {
        let labels = Labels::for_language(Language::En);
        let ticket = json!({ "id": 1, "status": 2, "type": 1, "priority": 4, "urgency": 3, "impact": 1 });
        let enriched = labels.enrich_ticket(ticket);
        assert_eq!(enriched["_status_label"], "In progress (assigned)");
        assert_eq!(enriched["_type_label"], "Incident");
        assert_eq!(enriched["_priority_label"], "High");
    }

    #[test]
    fn enrich_ticket_falls_back_to_unknown_for_missing_fields() {
        let labels = Labels::for_language(Language::En);
        let enriched = labels.enrich_ticket(json!({ "id": 1 }));
        assert_eq!(enriched["_status_label"], "Unknown");
    }
}
