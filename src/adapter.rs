use serde_json::{Map, Value};

/// Auto-heals parameter alias discrepancies from small models
pub fn normalize_params(mut map: Map<String, Value>) -> Map<String, Value> {
    // 1. Path aliases
    if !map.contains_key("path") {
        for alt in &["file", "filename", "filepath", "target", "file_path"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("path".into(), val);
                break;
            }
        }
    }

    // 2. Command aliases
    if !map.contains_key("command") {
        for alt in &["cmd", "exec", "shell_command", "bash", "script"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("command".into(), val);
                break;
            }
        }
    }

    // 3. Content / Code aliases
    if !map.contains_key("content") {
        for alt in &["text", "body", "data", "source", "code"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("content".into(), val);
                break;
            }
        }
    }

    // 4. Pattern aliases
    if !map.contains_key("pattern") {
        for alt in &["query", "regex", "search_term", "match"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("pattern".into(), val);
                break;
            }
        }
    }

    // 5. Clean leading/trailing quotes on paths and patterns
    if let Some(Value::String(p)) = map.get_mut("path") {
        *p = p.trim().trim_matches(|c| c == '\'' || c == '"' || c == '`').to_string();
    }
    if let Some(Value::String(pat)) = map.get_mut("pattern") {
        *pat = pat.trim().trim_matches(|c| c == '\'' || c == '"' || c == '`').to_string();
    }

    map
}

/// Closes unclosed quotes and brackets using a stack-based balancer
pub fn heal_json_args(raw: &str) -> Value {
    let clean = raw.trim();
    if clean.is_empty() {
        return Value::Object(Map::new());
    }

    // Pass 1: Direct JSON parse
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(clean) {
        return Value::Object(normalize_params(map));
    }

    // Pass 2: Markdown fence stripping (```json ... ```)
    let unwrap = if clean.starts_with("```") {
        let lines: Vec<&str> = clean.lines().collect();
        if lines.len() >= 2 {
            let start = 1;
            let end = if lines.last().map(|l| l.trim().starts_with("```")).unwrap_or(false) {
                lines.len() - 1
            } else {
                lines.len()
            };
            lines[start..end].join("\n")
        } else {
            clean.to_string()
        }
    } else {
        clean.to_string()
    };

    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&unwrap) {
        return Value::Object(normalize_params(map));
    }

    // Pass 3: Stack-based quote and brace closure
    let mut stack = Vec::new();
    let mut in_str = false;
    let mut esc = false;
    let mut healed = String::new();

    for ch in unwrap.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
        } else {
            match ch {
                '"' => in_str = true,
                '{' => stack.push('}'),
                '[' => stack.push(']'),
                '}' => {
                    if stack.last() == Some(&'}') {
                        stack.pop();
                    }
                }
                ']' => {
                    if stack.last() == Some(&']') {
                        stack.pop();
                    }
                }
                _ => {}
            }
        }
        healed.push(ch);
    }

    if in_str {
        healed.push('"');
    }

    while let Some(closing) = stack.pop() {
        healed.push(closing);
    }

    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&healed) {
        Value::Object(normalize_params(map))
    } else {
        Value::Object(Map::new())
    }
}
