use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Strips line-number gutters (e.g., "12: ", "12 | ") copied by SLMs into old_str/new_str
pub fn strip_line_gutters(text: &str) -> String {
    let re_gutter = regex::Regex::new(r"^\s*\d+[:|│\t ]\s?").unwrap();
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        if let Some(l) = lines.first() {
            return re_gutter.replace(l, "").to_string();
        }
        return text.to_string();
    }

    let mut stripped_lines = Vec::new();
    let mut gutter_count = 0;
    for line in &lines {
        if re_gutter.is_match(line) {
            gutter_count += 1;
        }
    }

    if gutter_count >= lines.len() / 2 {
        for line in &lines {
            stripped_lines.push(re_gutter.replace(line, "").to_string());
        }
        stripped_lines.join("\n")
    } else {
        text.to_string()
    }
}

/// Normalizes parameter aliases and sanitizes path / command inputs
pub fn normalize_params(map: &mut Map<String, Value>) {
    // 1. Path Aliases
    if !map.contains_key("path") {
        for alt in &["file", "filename", "filepath", "target", "file_path", "target_file"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("path".to_string(), val);
                break;
            }
        }
    }

    if let Some(path_val) = map.get_mut("path") {
        if let Some(p_str) = path_val.as_str() {
            let clean = p_str.trim().trim_matches(|c| c == '\'' || c == '"' || c == '`');
            let stripped_root = clean
                .strip_prefix("/workspace/")
                .or_else(|| clean.strip_prefix("/app/"))
                .unwrap_or(clean);
            *path_val = Value::String(stripped_root.to_string());
        }
    }

    // 2. Command Aliases & Redundant `cd <workspace> &&` Stripping
    if !map.contains_key("command") {
        for alt in &["cmd", "exec", "shell_command", "bash", "script"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("command".to_string(), val);
                break;
            }
        }
    }

    if let Some(cmd_val) = map.get_mut("command") {
        if let Some(c_str) = cmd_val.as_str() {
            let re_cd = regex::Regex::new(r"^\s*cd\s+['\x22]?(?:\./|\.|\~|/workspace|/app)[\x22']?\s*(?:&&|;)\s*").unwrap();
            let cleaned_cmd = re_cd.replace(c_str.trim(), "").to_string();
            *cmd_val = Value::String(cleaned_cmd);
        }
    }

    // 3. Content & Code Aliases
    if !map.contains_key("content") {
        for alt in &["text", "code", "body", "data", "source"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("content".to_string(), val);
                break;
            }
        }
    }

    // 4. Pattern & Query Aliases
    if !map.contains_key("pattern") {
        for alt in &["query", "regex", "search_term", "match", "find"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("pattern".to_string(), val);
                break;
            }
        }
    }

    // 5. Old String & New String Aliases for edit_file
    if !map.contains_key("old_str") {
        for alt in &["old", "old_string", "old_text", "search", "search_str", "target_str", "find", "before", "original"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("old_str".to_string(), val);
                break;
            }
        }
    }

    if !map.contains_key("new_str") {
        for alt in &["new", "new_string", "new_text", "replace_str", "replacement", "after", "update", "patch", "diff"] {
            if let Some(val) = map.remove(*alt) {
                map.insert("new_str".to_string(), val);
                break;
            }
        }
    }

    // Strip line numbers from old_str / new_str
    if let Some(old_val) = map.get_mut("old_str") {
        if let Some(s) = old_val.as_str() {
            *old_val = Value::String(strip_line_gutters(s));
        }
    }
    if let Some(new_val) = map.get_mut("new_str") {
        if let Some(s) = new_val.as_str() {
            *new_val = Value::String(strip_line_gutters(s));
        }
    }
}

/// Parses and self-heals broken JSON tool arguments from SLMs
pub fn heal_json_args(raw_args: &str) -> Value {
    let trimmed = raw_args.trim();
    if trimmed.is_empty() {
        return json!({});
    }

    // Direct JSON parse
    if let Ok(mut parsed) = serde_json::from_str::<Value>(trimmed) {
        if let Some(map) = parsed.as_object_mut() {
            normalize_params(map);
            return Value::Object(map.clone());
        }
    }

    // Markdown block unwrap
    let unescaped = if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 {
            let start = if lines[0].starts_with("```") { 1 } else { 0 };
            let end = if lines.last().map(|l| l.trim().starts_with("```")).unwrap_or(false) {
                lines.len() - 1
            } else {
                lines.len()
            };
            lines[start..end].join("\n")
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };

    if let Ok(mut parsed) = serde_json::from_str::<Value>(&unescaped) {
        if let Some(map) = parsed.as_object_mut() {
            normalize_params(map);
            return Value::Object(map.clone());
        }
    }

    // Bracket stack repair
    let mut stack = Vec::new();
    let mut in_str = false;
    let mut esc = false;
    let mut fixed = unescaped.clone();

    for ch in unescaped.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
        } else if ch == '"' {
            in_str = true;
        } else if ch == '{' || ch == '[' {
            stack.push(ch);
        } else if ch == '}' || ch == ']' {
            if let Some(top) = stack.last() {
                if (*top == '{' && ch == '}') || (*top == '[' && ch == ']') {
                    stack.pop();
                }
            }
        }
    }

    if in_str {
        fixed.push('"');
    }
    while let Some(open) = stack.pop() {
        fixed.push(if open == '{' { '}' } else { ']' });
    }

    if let Ok(mut parsed) = serde_json::from_str::<Value>(&fixed) {
        if let Some(map) = parsed.as_object_mut() {
            normalize_params(map);
            return Value::Object(map.clone());
        }
    }

    json!({})
}

pub fn repair_tool_args(_fn_name: &str, raw_args: &str) -> String {
    let healed = heal_json_args(raw_args);
    serde_json::to_string(&healed).unwrap_or_else(|_| raw_args.to_string())
}
