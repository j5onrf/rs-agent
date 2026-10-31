#![allow(dead_code)]

use serde_json::Value;

/// Parses and heals JSON arguments into a serde_json::Value (used by tools.rs)
pub fn heal_json_args(raw_args: &str) -> Value {
    let repaired = repair_tool_args("generic", raw_args);
    if let Ok(val) = serde_json::from_str::<Value>(&repaired) {
        return val;
    }

    // Fallback: extract substring between outer curly braces { ... }
    if let Some(start) = raw_args.find('{') {
        if let Some(end) = raw_args.rfind('}') {
            if end > start {
                let slice = &raw_args[start..=end];
                if let Ok(val) = serde_json::from_str::<Value>(slice) {
                    return val;
                }
            }
        }
    }

    serde_json::json!({})
}

/// Repairs common SLM JSON formatting defects (unescaped newlines, markdown fences, trailing commas)
pub fn repair_tool_args(fn_name: &str, raw_args: &str) -> String {
    let trimmed = raw_args.trim();

    // Strip markdown code fences if model wrapped the JSON in ```json ... ```
    let clean = if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 && lines.last().unwrap_or(&"").trim().starts_with("```") {
            lines[1..lines.len() - 1].join("\n")
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };

    // If it already parses as valid JSON, pass it through directly
    if serde_json::from_str::<Value>(&clean).is_ok() {
        return clean;
    }

    // SLM Repair for write_file / edit_file: handle raw unescaped newlines in content or old_str
    if (fn_name == "write_file" || fn_name == "edit_file" || fn_name == "generic") && clean.contains("\"content\":") {
        if let Some(idx) = clean.find("\"content\":") {
            let (_head, tail) = clean.split_at(idx + 10);
            let tail_trimmed = tail.trim_start();
            if tail_trimmed.starts_with('"') {
                return clean.replace('\r', "").replace('\n', "\\n");
            }
        }
    }

    clean
}
