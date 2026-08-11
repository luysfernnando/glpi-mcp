use std::collections::HashMap;

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
}
