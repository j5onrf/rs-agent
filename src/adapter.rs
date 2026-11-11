/// Strips copied line gutters (e.g. "12: ", " 42 | ") while preserving timestamps (12:30:00) and code literals.
pub fn strip_line_gutters(text: &str) -> String {
    // Fast-path: if there are no colons or pipes, zero line parsing is needed
    if !text.contains([':', '|']) {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for line in text.lines() {
        if !first {
            out.push('\n');
        }
        first = false;

        let trimmed_start = line.trim_start();
        if let Some(pos) = trimmed_start.find([':', '|']) {
            let prefix = trimmed_start[..pos].trim();
            let is_line_num = !prefix.is_empty() && prefix.len() <= 6 && prefix.chars().all(|c| c.is_ascii_digit());

            let rest_slice = &trimmed_start[pos + 1..];
            let has_gutter_spacing = rest_slice.starts_with([' ', '\t']);

            if is_line_num && has_gutter_spacing {
                let rest = rest_slice.strip_prefix([' ', '\t']).unwrap_or(rest_slice);
                out.push_str(rest);
                continue;
            }
        }
        out.push_str(line);
    }

    // Preserve trailing newline if present in original text
    if text.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[inline]
fn trim_trailing_comma(s: &mut String) {
    let trimmed_len = s.trim_end().len();
    s.truncate(trimmed_len);
    if s.ends_with(',') {
        s.pop();
    }
}

pub fn normalize_params(map: &mut serde_json::Map<String, serde_json::Value>) {
    const ALIASES: &[(&str, &str)] = &[
        ("filepath", "path"),
        ("file_path", "path"),
        ("filename", "path"),
        ("file", "path"),
        ("target_file", "path"),
        ("source_file", "path"),
        ("text", "content"),
        ("code", "content"),
        ("body", "content"),
        ("source", "content"),
        ("cmd", "command"),
        ("shell", "command"),
        ("exec", "command"),
        ("script", "command"),
        ("run", "command"),
        ("query", "pattern"),
        ("regex", "pattern"),
        ("search", "pattern"),
        ("find", "pattern"),
        ("old", "old_str"),
        ("old_text", "old_str"),
        ("replace", "new_str"),
        ("new", "new_str"),
        ("new_text", "new_str"),
        ("replacement", "new_str"),
        ("force", "overwrite"),
    ];

    for &(alias, canonical) in ALIASES {
        if let Some(val) = map.remove(alias) {
            let needs_canonical = match map.get(canonical) {
                None | Some(serde_json::Value::Null) => true,
                Some(serde_json::Value::String(s)) => s.is_empty(),
                _ => false,
            };
            if needs_canonical {
                map.insert(canonical.to_string(), val);
            }
        }
    }

    // Coerce string booleans for overwrite ("true", "1", "yes")
    if let Some(v) = map.get("overwrite") {
        let normalized_bool = match v {
            serde_json::Value::Bool(b) => Some(*b),
            serde_json::Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            },
            serde_json::Value::Number(n) => Some(n.as_i64() == Some(1)),
            _ => None,
        };
        if let Some(b) = normalized_bool {
            map.insert("overwrite".to_string(), serde_json::Value::Bool(b));
        }
    }

    // Heal container path hallucinations
    if let Some(serde_json::Value::String(p)) = map.get_mut("path") {
        let mut s = p.as_str();
        for prefix in &["/workspace/", "workspace/", "/app/", "app/", "/project/", "project/"] {
            if let Some(stripped) = s.strip_prefix(prefix) {
                s = stripped;
                break;
            }
        }
        if s.starts_with('/') && !s.starts_with("/tmp") && !s.starts_with("/home") && !s.starts_with("/dev") {
            s = s.trim_start_matches('/');
        }
        if s != p.as_str() {
            *p = s.to_string();
        }
    }

    if let Some(serde_json::Value::String(s)) = map.get_mut("old_str") {
        *s = strip_line_gutters(s);
    }
    if let Some(serde_json::Value::String(s)) = map.get_mut("new_str") {
        *s = strip_line_gutters(s);
    }
}

pub fn heal_json_args(raw_args: &str) -> serde_json::Value {
    let trimmed = raw_args.trim();
    if trimmed.is_empty() {
        return serde_json::json!({});
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return v;
    }

    let mut s = trimmed;

    // Strip markdown code fences if wrapped
    if s.starts_with("```") {
        if let Some(first_newline) = s.find('\n') {
            s = &s[first_newline + 1..];
        } else {
            s = s.trim_start_matches('`');
        }
        if let Some(last_fence) = s.rfind("```") {
            if s[last_fence + 3..].trim().is_empty() {
                s = &s[..last_fence];
            }
        }
        s = s.trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            return v;
        }
    }

    let mut balanced = String::with_capacity(s.len() + 16);
    let mut in_str = false;
    let mut escape = false;
    let mut stack = Vec::new();

    for ch in s.chars() {
        if escape {
            escape = false;
            balanced.push(ch);
            continue;
        }
        if ch == '\\' {
            escape = true;
            balanced.push(ch);
            continue;
        }
        if ch == '"' {
            in_str = !in_str;
            balanced.push(ch);
            continue;
        }

        if in_str {
            // Escape raw control characters inside strings
            match ch {
                '\n' => balanced.push_str("\\n"),
                '\r' => balanced.push_str("\\r"),
                '\t' => balanced.push_str("\\t"),
                _ => balanced.push(ch),
            }
            continue;
        }

        match ch {
            '{' => {
                stack.push('}');
                balanced.push(ch);
            }
            '[' => {
                stack.push(']');
                balanced.push(ch);
            }
            '}' => {
                if stack.last() == Some(&'}') {
                    stack.pop();
                }
                trim_trailing_comma(&mut balanced);
                balanced.push(ch);
            }
            ']' => {
                if stack.last() == Some(&']') {
                    stack.pop();
                }
                trim_trailing_comma(&mut balanced);
                balanced.push(ch);
            }
            _ => {
                balanced.push(ch);
            }
        }
    }

    if in_str {
        if escape {
            balanced.push('\\');
        }
        balanced.push('"');
    }

    trim_trailing_comma(&mut balanced);

    while let Some(close_delim) = stack.pop() {
        balanced.push(close_delim);
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&balanced) {
        return v;
    }

    if balanced.contains('\'') {
        let mut converted = String::with_capacity(balanced.len());
        let mut esc = false;
        for ch in balanced.chars() {
            if esc {
                esc = false;
                converted.push(ch);
                continue;
            }
            if ch == '\\' {
                esc = true;
                converted.push(ch);
                continue;
            }
            if ch == '\'' {
                converted.push('"');
            } else {
                converted.push(ch);
            }
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&converted) {
            return v;
        }
    }

    serde_json::json!({})
}

pub fn repair_tool_args(_fn_name: &str, raw_args: &str) -> String {
    let mut val = heal_json_args(raw_args);
    if let Some(map) = val.as_object_mut() {
        normalize_params(map);
    }
    serde_json::to_string(&val).unwrap_or_else(|_| raw_args.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_line_gutters_preserves_timestamps_and_strips_gutters() {
        let input = "12: fn test() {\n13:     let time = \"12:30:00\";\n14: }\n";
        let cleaned = strip_line_gutters(input);
        assert_eq!(cleaned, "fn test() {\n    let time = \"12:30:00\";\n}\n");
    }

    #[test]
    fn test_strip_line_gutters_fast_path_when_clean() {
        let input = "fn hello() -> i32 {\n    42\n}\n";
        assert_eq!(strip_line_gutters(input), input);
    }

    #[test]
    fn test_normalize_params_cleans_aliases_and_handles_empty_canonical() {
        let mut map = serde_json::Map::new();
        map.insert("path".to_string(), serde_json::Value::String("".to_string()));
        map.insert("filepath".to_string(), serde_json::Value::String("/workspace/src/lib.rs".to_string()));
        map.insert("force".to_string(), serde_json::Value::String("true".to_string()));

        normalize_params(&mut map);
        assert!(!map.contains_key("filepath"));
        assert_eq!(map.get("path").unwrap().as_str().unwrap(), "src/lib.rs");
        assert_eq!(map.get("overwrite").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn test_heal_json_args_handles_nested_trailing_commas_and_fences() {
        let malformed = "```json\n{\"command\": \"cargo test\", \"options\": [1, 2, ], }\n```";
        let healed = heal_json_args(malformed);
        assert_eq!(healed["command"], "cargo test");
        assert_eq!(healed["options"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_heal_json_args_handles_raw_multiline_strings() {
        let raw = "{\"content\": \"line 1\nline 2\nline 3\"}";
        let healed = heal_json_args(raw);
        assert_eq!(healed["content"], "line 1\nline 2\nline 3");
    }

    #[test]
    fn test_heal_json_args_handles_single_quoted_json() {
        let raw = "{'path': 'calc.py', 'overwrite': true}";
        let healed = heal_json_args(raw);
        assert_eq!(healed["path"], "calc.py");
        assert_eq!(healed["overwrite"], true);
    }

    #[test]
    fn test_heal_json_args_handles_dangling_escape_in_unterminated_string() {
        let malformed = "{\"path\": \"src\\";
        let healed = heal_json_args(malformed);
        assert_eq!(healed["path"], "src\\");
    }
}
