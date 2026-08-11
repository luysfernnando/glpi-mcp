use serde_json::Value;

// Shared helpers for rendering GLPI JSON as compact Markdown instead of raw JSON.
// Markdown carries the same information in far fewer tokens: a table row replaces
// a multi line object, and stripped HTML/entities replace an escaped blob nobody reads.
// Every tool response should route through these instead of hand rolling formatting.

/// Escapes characters that would break a Markdown table cell.
pub fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ").replace('\r', "")
}

/// Truncates to at most `max_chars` characters (by codepoint, never mid character), appending an ellipsis.
pub fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let head: String = value.chars().take(max_chars).collect();
    format!("{head}…")
}

/// Decodes HTML entities and drops tags, turning GLPI's WYSIWYG markup into plain,
/// readable text. Block level tags (`<p>`, `<br>`, `<div>`, list items) become newlines
/// so paragraph structure survives; everything else is dropped.
pub fn strip_html(input: &str) -> String {
    let decoded = decode_entities(input);
    let mut out = String::with_capacity(decoded.len());
    let mut tag = String::new();
    let mut in_tag = false;

    for c in decoded.chars() {
        if c == '<' {
            in_tag = true;
            tag.clear();
            continue;
        }
        if in_tag {
            if c == '>' {
                in_tag = false;
                let name = tag.trim_start_matches('/').split_whitespace().next().unwrap_or("").to_lowercase();
                if matches!(name.as_str(), "br" | "p" | "div" | "li" | "tr") {
                    out.push('\n');
                }
                continue;
            }
            tag.push(c);
            continue;
        }
        out.push(c);
    }

    collapse_blank_lines(out.trim())
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after_amp = &rest[amp + 1..];
        match after_amp.find(';').filter(|&len| len <= 10) {
            Some(len) => {
                let name = &after_amp[..len];
                match decode_entity_name(name) {
                    Some(decoded) => {
                        out.push(decoded);
                        rest = &after_amp[len + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = after_amp;
                    }
                }
            }
            None => {
                out.push('&');
                rest = after_amp;
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity_name(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix('x').or_else(|| digits.strip_prefix('X')) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Renders a Markdown table from a header row and pre-built cell rows.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(&headers.join(" | "));
    out.push_str(" |\n|");
    out.push_str(&"---|".repeat(headers.len()));
    for row in rows {
        out.push('\n');
        out.push_str("| ");
        out.push_str(&row.join(" | "));
        out.push_str(" |");
    }
    out
}

/// Renders a two column "field: value" Markdown table for a single record's detail view.
pub fn field_table(fields: &[(&str, String)]) -> String {
    let mut out = String::from("| Field | Value |\n|---|---|");
    for (label, value) in fields {
        out.push('\n');
        out.push_str("| ");
        out.push_str(label);
        out.push_str(" | ");
        out.push_str(&escape_cell(value));
        out.push_str(" |");
    }
    out
}

/// Takes ownership of a JSON array without cloning it, defaulting to empty
/// when the value isn't an array. Use on a response we already own instead of
/// `.as_array().cloned()`, which deep clones every item just to iterate once.
pub fn into_array(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        _ => Vec::new(),
    }
}

/// Reads a string field out of a JSON object, defaulting to empty.
pub fn str_field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Reads a numeric or string field out of a JSON object as a plain string,
/// defaulting to empty. Use for GLPI IDs, which come back as numbers on plain
/// item endpoints but as strings on `/search/*` endpoints.
pub fn id_field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Strips HTML, escapes table cell characters, then truncates — the combo used
/// for any free text field shown as a table cell (ticket/task descriptions,
/// KB answers).
pub fn cell(html: &str, max_chars: usize) -> String {
    truncate(&escape_cell(&strip_html(html)), max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_decodes_entities_and_converts_block_tags_to_newlines() {
        let input = "&#60;p&#62;&#60;span&#62;hello&#60;/span&#62;&#60;br&#62;&#60;span&#62;world&#60;/span&#62;&#60;/p&#62;";
        assert_eq!(strip_html(input), "hello\nworld");
    }

    #[test]
    fn strip_html_drops_inline_tags_without_adding_newlines() {
        assert_eq!(strip_html("<strong>bold</strong> and <em>italic</em>"), "bold and italic");
    }

    #[test]
    fn truncate_appends_ellipsis_only_when_over_limit() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello…");
    }

    #[test]
    fn escape_cell_neutralizes_pipes_and_newlines() {
        assert_eq!(escape_cell("a|b\nc"), "a\\|b c");
    }

    #[test]
    fn table_renders_header_separator_and_rows() {
        let rendered = table(&["ID", "Name"], &[vec!["1".into(), "Alice".into()]]);
        assert_eq!(rendered, "| ID | Name |\n|---|---|\n| 1 | Alice |");
    }
}
