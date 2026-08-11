use serde_json::Value;

/// Strips GLPI response noise that burns tokens without carrying information for
/// an LLM caller: the `links` HATEOAS array GLPI attaches to every entity, and the
/// `style="..."` attributes the WYSIWYG editor repeats on every single `<span>`/`<br>`
/// of rich text fields (`content`, `answer`, `comment`, ...). Applied once, recursively,
/// at the client's single response choke point so every tool benefits automatically.
pub(crate) fn compact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("links");
            for v in map.values_mut() {
                compact_value(v);
            }
        }
        Value::Array(items) => {
            for item in items {
                compact_value(item);
            }
        }
        Value::String(s) if s.contains("style=\"") => {
            *s = strip_inline_styles(s);
        }
        _ => {}
    }
}

fn strip_inline_styles(html: &str) -> String {
    const NEEDLE: &str = "style=\"";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(pos) = rest.find(NEEDLE) {
        out.push_str(rest[..pos].trim_end_matches(' '));
        let after_needle = &rest[pos + NEEDLE.len()..];
        match after_needle.find('"') {
            Some(end) => rest = &after_needle[end + 1..],
            None => {
                // Malformed/truncated attribute: keep the remainder verbatim rather than
                // risk dropping content.
                out.push_str(&rest[pos..]);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removes_links_array_from_objects() {
        let mut value = json!({ "id": 1, "links": [{"rel": "Entity", "href": "..."}] });
        compact_value(&mut value);
        assert!(value.as_object().unwrap().get("links").is_none());
        assert_eq!(value["id"], 1);
    }

    #[test]
    fn removes_links_recursively_inside_arrays() {
        let mut value = json!([{ "id": 1, "links": [] }, { "id": 2, "links": [] }]);
        compact_value(&mut value);
        assert!(value[0].as_object().unwrap().get("links").is_none());
        assert!(value[1].as_object().unwrap().get("links").is_none());
    }

    #[test]
    fn strips_inline_style_attributes_but_keeps_tags_and_text() {
        let mut value = json!({
            "content": "<p><span style=\"color: #000; font-family: monospace;\">hello</span></p>"
        });
        compact_value(&mut value);
        assert_eq!(value["content"], "<p><span>hello</span></p>");
    }

    #[test]
    fn strips_multiple_style_attributes_in_one_string() {
        let mut value = json!({
            "content": "<span style=\"color: red;\">a</span><br style=\"font-size: 16px;\"><span style=\"color: blue;\">b</span>"
        });
        compact_value(&mut value);
        assert_eq!(value["content"], "<span>a</span><br><span>b</span>");
    }

    #[test]
    fn leaves_strings_without_style_attribute_untouched() {
        let mut value = json!({ "name": "plain text, no html" });
        compact_value(&mut value);
        assert_eq!(value["name"], "plain text, no html");
    }
}
