pub fn strip_line_gutters(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for line in text.lines() {
        if !first {
            out.push('\n');
        }
        first = false;

        let trimmed_start = line.trim_start();
        if let Some(pos) = trimmed_start.find(|c: char| c == ':' || c == '|') {
            let prefix = trimmed_start[..pos].trim();
            if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                let rest = trimmed_start[pos + 1..].strip_prefix(' ').unwrap_or(&trimmed_start[pos + 1..]);
                out.push_str(rest);
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

pub fn normalize_params(map: &mut serde_json::Map<String, serde_json::Value>) {
    let aliases = [
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

    for (alias, canonical) in aliases {
        if alias != canonical && !map.contains_key(canonical) {
            if let Some(val) = map.remove(alias) {
                map.insert(canonical.to_string(), val);
            }
        }
    }

    if let Some(serde_json::Value::String(p)) = map.get_mut("path") {
        let mut s = p.as_str();

        // Heal common Docker / SWE-bench container hallucinations from SLMs
        if let Some(stripped) = s.strip_prefix("/workspace/").or_else(|| s.strip_prefix("workspace/")) {
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix("/app/").or_else(|| s.strip_prefix("app/")) {
            s = stripped;
        } else if let Some(stripped) = s.strip_prefix("/project/").or_else(|| s.strip_prefix("project/")) {
            s = stripped;
        }

        if s.starts_with('/') && !s.starts_with("/tmp") && !s.starts_with("/home") && !s.starts_with("/dev") {
            s = s.trim_start_matches('/');
        }
        *p = s.to_string();
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
    if s.starts_with("```") {
        if let Some(first_newline) = s.find('\n') {
            s = &s[first_newline + 1..];
        } else {
            s = s.trim_start_matches('`');
        }
        if let Some(last_fence) = s.rfind("```") {
            s = &s[..last_fence];
        }
        s = s.trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            return v;
        }
    }

    let mut balanced = s.to_string();
    let mut in_str = false;
    let mut escape = false;
    let mut stack = Vec::new();

    for ch in balanced.chars() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_str = !in_str;
            continue;
        }
        if !in_str {
            match ch {
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
    }

    if in_str {
        balanced.push('"');
    }

    let mut trimmed_b = balanced.trim_end().to_string();
    if trimmed_b.ends_with(',') {
        trimmed_b.pop();
    }

    while let Some(close_delim) = stack.pop() {
        trimmed_b.push(close_delim);
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&trimmed_b) {
        return v;
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
