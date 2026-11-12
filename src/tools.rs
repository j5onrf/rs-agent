// Zero-allocation character-safe string slicing for UTF-8 boundaries
fn char_slice_head(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

fn char_slice_tail(s: &str, max_chars: usize) -> &str {
    let total = s.chars().count();
    if total <= max_chars {
        return s;
    }
    let skip = total.saturating_sub(max_chars);
    match s.char_indices().nth(skip) {
        Some((idx, _)) => &s[idx..],
        None => s,
    }
}

use crate::adapter::heal_json_args;
use crate::config::{get_context_ceilings, ToolTier};
use crate::security::SecurityBoundary;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}
const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;

static SUBAGENT_DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[inline]
pub fn is_read_only_tool(name: &str) -> bool {
    matches!(name, "read_file" | "list_dir" | "search_code")
}

pub fn get_tools_schema(tier: &ToolTier, delegate: bool) -> Value {
    get_base_tools_schema(tier, delegate)
}

pub async fn get_tools_schema_with_mcp(tier: &ToolTier, delegate: bool, mcp_servers: &[String]) -> Value {
    let mut base_schema = get_base_tools_schema(tier, delegate);
    if !mcp_servers.is_empty() {
        if let Some(arr) = base_schema.as_array_mut() {
            for server in mcp_servers {
                match crate::mcp::get_server_tools_schema(server).await {
                    Ok(schemas) => {
                        arr.extend(schemas);
                    }
                    Err(e) => {
                        eprintln!("  \x1b[1;33m▲ [mcp warning]\x1b[0m Could not connect to '{}': {}", server, e);
                    }
                }
            }
        }
    }
    base_schema
}

fn get_base_tools_schema(tier: &ToolTier, delegate: bool) -> Value {
    match tier {
        ToolTier::PureBash => json!([
            {
                "type": "function",
                "function": {
                    "name": "run_command",
                    "description": "Execute a shell command inside the workspace directory.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string", "description": "Shell command to run." }
                        },
                        "required": ["command"]
                    }
                }
            }
        ]),
        ToolTier::Core4 | ToolTier::Smol => {
            let mut tools_vec = vec![
                json!({
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "description": "Read a text file from the project. Optionally specify line_start and line_end.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative path to file." },
                                "line_start": { "type": "integer", "description": "Starting line number (1-indexed)." },
                                "line_end": { "type": "integer", "description": "Ending line number (inclusive)." }
                            },
                            "required": ["path"]
                        }
                    }
                }),
                json!({
                    "type": "function",
                    "function": {
                        "name": "edit_file",
                        "description": "Surgically replace exact text (old_str) with new_str in an existing file. Confirms disk write without needing read_file.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative path to file." },
                                "old_str": { "type": "string", "description": "Exact text to find and replace." },
                                "new_str": { "type": "string", "description": "New replacement text." }
                            },
                            "required": ["path", "old_str", "new_str"]
                        }
                    }
                }),
                json!({
                    "type": "function",
                    "function": {
                        "name": "write_file",
                        "description": "Create a new file or completely overwrite an existing file. Confirms disk write without needing read_file. If file exists, pass overwrite=true.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative path to file." },
                                "content": { "type": "string", "description": "Full file content." },
                                "overwrite": { "type": "boolean", "description": "Set true to confirm overwriting existing file." }
                            },
                            "required": ["path", "content"]
                        }
                    }
                }),
            ];

            if matches!(tier, ToolTier::Smol) {
                tools_vec.push(json!({
                    "type": "function",
                    "function": {
                        "name": "list_dir",
                        "description": "List directory contents in the workspace.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative directory path. Defaults to '.' (root)." }
                            }
                        }
                    }
                }));
                tools_vec.push(json!({
                    "type": "function",
                    "function": {
                        "name": "search_code",
                        "description": "Search for text or regex pattern across workspace files.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "pattern": { "type": "string", "description": "Text or regex pattern to search for." },
                                "path": { "type": "string", "description": "Relative directory path to search within." }
                            },
                            "required": ["pattern"]
                        }
                    }
                }));
            }

            tools_vec.push(json!({
                "type": "function",
                "function": {
                    "name": "run_command",
                    "description": "Run a shell command inside the workspace directory (auto-killed after 30s timeout).",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string", "description": "Command to execute." }
                        },
                        "required": ["command"]
                    }
                }
            }));

            if delegate {
                tools_vec.push(json!({
                    "type": "function",
                    "function": {
                        "name": "delegate_task",
                        "description": "Delegate an isolated sub-task or code research query to a temporary sub-agent. Returns only a concise summary report to keep context clean.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "goal": { "type": "string", "description": "Specific isolated sub-task or code research objective." }
                            },
                            "required": ["goal"]
                        }
                    }
                }));
            }

            Value::Array(tools_vec)
        }
    }
}

/// Computes Levenshtein distance similarity ratio (0.0 to 1.0)
fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();
    if n == 0 && m == 0 {
        return 1.0;
    }
    if n == 0 || m == 0 {
        return 0.0;
    }

    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    #[allow(clippy::needless_range_loop)]
    for i in 0..=n {
        dp[i][0] = i;
    }
    #[allow(clippy::needless_range_loop)]
    for j in 0..=m {
        dp[0][j] = j;
    }

    for i in 1..=n {
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1).min(dp[i][j - 1] + 1).min(dp[i - 1][j - 1] + cost);
        }
    }

    let dist = dp[n][m];
    let max_len = n.max(m);
    1.0 - (dist as f64 / max_len as f64)
}

/// 3-Stage Resilient Replacer ported from py-agent (Exact -> Indent-Delta Whitespace -> Fuzzy)
pub fn resilient_replace(original: &str, old_str: &str, new_str: &str) -> Result<String, String> {
    if original.contains(old_str) {
        if original.matches(old_str).count() > 1 {
            return Err(
                "Target 'old_str' matched multiple times. Include more surrounding lines to make it unique.".into()
            );
        }
        return Ok(original.replacen(old_str, new_str, 1));
    }

    let clean_orig = original.replace("\r\n", "\n");
    let clean_old = old_str.replace("\r\n", "\n");
    let clean_new = new_str.replace("\r\n", "\n");

    let orig_lines: Vec<&str> = clean_orig.lines().collect();
    let old_lines: Vec<&str> = clean_old.lines().collect();
    if old_lines.is_empty() {
        return Err("Parameter 'old_str' cannot be empty.".into());
    }

    let mut non_blank_orig = Vec::new();
    let mut non_blank_indices = Vec::new();
    for (idx, line) in orig_lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            non_blank_orig.push(trimmed);
            non_blank_indices.push(idx);
        }
    }

    let norm_old: Vec<&str> = old_lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let old_len = norm_old.len();
    if old_len == 0 {
        return Err("Parameter 'old_str' contains no non-whitespace content.".into());
    }

    let mut matches = Vec::new();
    if non_blank_orig.len() >= old_len {
        for i in 0..=(non_blank_orig.len() - old_len) {
            if non_blank_orig[i..i + old_len] == norm_old[..] {
                let start_idx = non_blank_indices[i];
                let end_idx = non_blank_indices[i + old_len - 1] + 1;
                matches.push((start_idx, end_idx));
            }
        }
    }

    fn reconstruct_with_indent(
        orig_lines: &[&str],
        start_idx: usize,
        end_idx: usize,
        old_first_line: &str,
        clean_new: &str,
    ) -> String {
        let orig_first_line = orig_lines[start_idx];
        let orig_indent = orig_first_line.len() - orig_first_line.trim_start().len();
        let old_indent = old_first_line.len() - old_first_line.trim_start().len();
        let indent_delta = orig_indent as isize - old_indent as isize;

        let mut adjusted_new_lines = Vec::new();
        for nl in clean_new.lines() {
            if nl.trim().is_empty() {
                adjusted_new_lines.push(String::new());
            } else if indent_delta > 0 {
                adjusted_new_lines.push(format!("{}{}", " ".repeat(indent_delta as usize), nl));
            } else if indent_delta < 0 && nl.starts_with(&" ".repeat(indent_delta.unsigned_abs())) {
                adjusted_new_lines.push(nl[indent_delta.unsigned_abs()..].to_string());
            } else {
                adjusted_new_lines.push(nl.to_string());
            }
        }

        let mut reconstructed = String::new();
        if start_idx > 0 {
            reconstructed.push_str(&orig_lines[..start_idx].join("\n"));
            reconstructed.push('\n');
        }
        reconstructed.push_str(&adjusted_new_lines.join("\n"));
        if end_idx < orig_lines.len() {
            reconstructed.push('\n');
            reconstructed.push_str(&orig_lines[end_idx..].join("\n"));
        }
        reconstructed
    }

    if matches.len() == 1 {
        let (start_idx, end_idx) = matches[0];
        return Ok(reconstruct_with_indent(&orig_lines, start_idx, end_idx, old_lines[0], &clean_new));
    } else if matches.len() > 1 {
        return Err(format!(
            "Whitespace-normalized old_str matched {} locations. Include more surrounding lines to make it unique.",
            matches.len()
        ));
    }

    let old_block_str = norm_old.join("\n");
    let mut best_ratio = 0.0;
    let mut best_window = None;

    if non_blank_orig.len() >= old_len {
        for i in 0..=(non_blank_orig.len() - old_len) {
            let cand_block_str = non_blank_orig[i..i + old_len].join("\n");
            let ratio = levenshtein_similarity(&old_block_str, &cand_block_str);
            if ratio > best_ratio {
                best_ratio = ratio;
                best_window = Some((non_blank_indices[i], non_blank_indices[i + old_len - 1] + 1));
            }
        }
    }

    if let Some((start_idx, end_idx)) = best_window.filter(|_| best_ratio >= 0.85) {
        return Ok(reconstruct_with_indent(&orig_lines, start_idx, end_idx, old_lines[0], &clean_new));
    }

    Err("Target 'old_str' not found in file (even with whitespace tolerance and fuzzy matching). Use read_file to inspect exact lines.".into())
}

pub async fn execute_tool(
    name: &str,
    raw_args: &str,
    workspace: &Path,
    boundary: &SecurityBoundary,
) -> Result<String, String> {
    let args = heal_json_args(raw_args);
    let ceilings = get_context_ceilings();

    if let Some(rest) = name.strip_prefix("mcp_") {
        if let Some((server, tool_name)) = rest.split_once('_') {
            return crate::mcp::call_tool(server, tool_name, &args).await;
        } else {
            return Err(format!("Malformed MCP tool name '{}'. Expected mcp_<server>_<tool>", name));
        }
    }

    match name {
        "delegate_task" => {
            let goal = args["goal"].as_str().ok_or("Missing 'goal' parameter")?;
            if SUBAGENT_DEPTH.load(std::sync::atomic::Ordering::SeqCst) >= 1 {
                return Err("[error] Sub-agents cannot recursively delegate tasks. Execute direct tools.".into());
            }

            SUBAGENT_DEPTH.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let provider = crate::config::ProviderConfig::load().await;
            let client = crate::client::LlmClient::new(provider);
            let sub_boundary = SecurityBoundary::new(workspace);

            let sub_tools = get_base_tools_schema(&ToolTier::Smol, false);
            let sys_prompt = format!(
                "You are an isolated sub-agent in workspace: {}.\nGoal: {}\nExecute direct tools (read_file, search_code, list_dir, run_command, edit_file, write_file) to accomplish the goal.\nWhen finished, output ONLY a concise factual summary report.",
                workspace.display(), goal
            );

            let mut sub_history = vec![
                crate::client::ChatMessage::system(sys_prompt),
                crate::client::ChatMessage::user(format!("Execute sub-task: {}", goal)),
            ];

            let mut final_report = String::new();
            for _step in 0..5 {
                let stream_res = client.stream_chat_raw(&sub_history, Some(sub_tools.clone()), None, |_, _| {}).await;
                match stream_res {
                    Ok((ans, opt_reasoning, tool_calls, _)) => {
                        final_report = ans.clone();
                        if let Some(calls) = tool_calls {
                            if calls.is_empty() {
                                break;
                            }
                            sub_history.push(crate::client::ChatMessage {
                                role: "assistant".into(),
                                content: serde_json::Value::String(ans),
                                reasoning_content: opt_reasoning,
                                tool_calls: Some(serde_json::Value::Array(calls.clone())),
                                tool_call_id: None,
                            });
                            let mut idx = 0;
                            while idx < calls.len() {
                                let fn_name = calls[idx]["function"]["name"].as_str().unwrap_or("tool");
                                if is_read_only_tool(fn_name) {
                                    let mut batch_end = idx + 1;
                                    while batch_end < calls.len()
                                        && is_read_only_tool(
                                            calls[batch_end]["function"]["name"].as_str().unwrap_or("tool"),
                                        )
                                    {
                                        batch_end += 1;
                                    }
                                    let mut futs = Vec::with_capacity(batch_end - idx);
                                    for call in &calls[idx..batch_end] {
                                        let c_id = call["id"].as_str().unwrap_or("call_sub").to_string();
                                        let c_name = call["function"]["name"].as_str().unwrap_or("tool").to_string();
                                        let c_args = crate::adapter::repair_tool_args(
                                            &c_name,
                                            call["function"]["arguments"].as_str().unwrap_or("{}"),
                                        );
                                        let ws = workspace;
                                        let bnd = &sub_boundary;
                                        futs.push(async move {
                                            let res = Box::pin(execute_tool(&c_name, &c_args, ws, bnd)).await;
                                            (c_id, res)
                                        });
                                    }
                                    for (c_id, res) in futures_util::future::join_all(futs).await {
                                        let res_str = res.unwrap_or_else(|e| format!("[error] {}", e));
                                        sub_history.push(crate::client::ChatMessage::tool_response(c_id, res_str));
                                    }
                                    idx = batch_end;
                                } else {
                                    let call = &calls[idx];
                                    let c_id = call["id"].as_str().unwrap_or("call_sub").to_string();
                                    let c_args = crate::adapter::repair_tool_args(
                                        fn_name,
                                        call["function"]["arguments"].as_str().unwrap_or("{}"),
                                    );
                                    let res = Box::pin(execute_tool(fn_name, &c_args, workspace, &sub_boundary)).await;
                                    let res_str = res.unwrap_or_else(|e| format!("[error] {}", e));
                                    sub_history.push(crate::client::ChatMessage::tool_response(c_id, res_str));
                                    idx += 1;
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    Err(e) => {
                        final_report = format!("[sub-agent error: {}]", e);
                        break;
                    }
                }
            }
            SUBAGENT_DEPTH.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            Ok(if final_report.is_empty() { "Sub-agent completed task with no output.".into() } else { final_report })
        }

        "read_file" => {
            let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
            let safe_path = boundary.resolve_path(path_str);
            if boundary.is_outside(&safe_path) && !boundary.authorize(&format!("OUT-OF-BOUNDS READ: '{}'", path_str)) {
                return Err("[denied] Out-of-bounds read cancelled by user.".into());
            }

            if !safe_path.is_file() {
                return Err(format!("File not found: '{}'", path_str));
            }

            let content = fs::read_to_string(&safe_path).map_err(|e| e.to_string())?;
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();

            let l_start = args.get("line_start").and_then(|v| v.as_u64()).map(|v| v as usize);
            let l_end = args.get("line_end").and_then(|v| v.as_u64()).map(|v| v as usize);

            if l_start.is_some() || l_end.is_some() {
                let s = l_start.unwrap_or(1).max(1);
                let start_idx = s.saturating_sub(1);
                if total == 0 || start_idx >= total {
                    return Err(format!("Invalid line range starting at {} for file with {} lines", s, total));
                }
                let end_idx = l_end
                    .map(|e| e.min(total))
                    .unwrap_or_else(|| (start_idx + ceilings.line_ceiling).min(total));
                if start_idx >= end_idx {
                    return Err(format!("Invalid line range {}-{} for file with {} lines", s, end_idx, total));
                }
                let slice = lines[start_idx..end_idx].join("\n");
                let note = if l_end.is_none() && end_idx < total {
                    format!("\n... [Clamped to {}-line ceiling; pass line_end to read further]", ceilings.line_ceiling)
                } else {
                    String::new()
                };
                Ok(format!("### File: {} (Lines {}-{} of {})\n{}{}", path_str, s, end_idx, total, slice, note))
            } else if total > ceilings.line_ceiling {
                let preview_count = ceilings.preview_lines.min(total);
                let preview = lines[..preview_count].join("\n");

                let scratchpad_dir = workspace.join(".rs");
                let _ = fs::create_dir_all(&scratchpad_dir);
                let _ = fs::write(scratchpad_dir.join("scratchpad.log"), &content);

                Ok(format!(
                    "### File Preview: {} (Showing 1-{} of {} lines)\n{}\n\n... [File truncated: {} lines exceeds {}-line ceiling]\nFull content saved to .rs/scratchpad.log.",
                    path_str, preview_count, total, preview, total, ceilings.line_ceiling
                ))
            } else {
                Ok(format!("### File: {} ({} lines)\n{}", path_str, total, content))
            }
        }

        "edit_file" => {
            let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
            let old_str = args["old_str"].as_str().ok_or("Missing 'old_str' parameter")?;
            let new_str = args["new_str"].as_str().ok_or("Missing 'new_str' parameter")?;

            let safe_path = boundary.resolve_path(path_str);
            if boundary.is_outside(&safe_path) && !boundary.authorize(&format!("OUT-OF-BOUNDS EDIT: '{}'", path_str)) {
                return Err("[denied] Out-of-bounds edit cancelled by user.".into());
            }

            if !safe_path.is_file() {
                return Err(format!("File not found: '{}'", path_str));
            }

            let original = fs::read_to_string(&safe_path).map_err(|e| e.to_string())?;
            let updated = resilient_replace(&original, old_str, new_str)?;
            fs::write(&safe_path, updated).map_err(|e| e.to_string())?;
            Ok(format!("✓ File updated: {}. Disk state confirmed. Halt if goal complete.", path_str))
        }

        "write_file" => {
            let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
            let content = args["content"].as_str().ok_or("Missing 'content' parameter")?;
            let _overwrite = args.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false);

            let safe_path = boundary.resolve_path(path_str);
            if boundary.is_outside(&safe_path) && !boundary.authorize(&format!("OUT-OF-BOUNDS WRITE: '{}'", path_str)) {
                return Err("[denied] Out-of-bounds write cancelled by user.".into());
            }

            // Direct overwrite allowed in autonomous workspace

            if let Some(parent) = safe_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }

            // TOCTOU re-validation before commit
            if boundary.is_outside(&safe_path) {
                return Err("[denied] Path escaped boundary after directory resolution.".into());
            }

            fs::write(&safe_path, content).map_err(|e| e.to_string())?;
            Ok(format!("✓ File written to {}. Disk state confirmed. Halt if goal complete.", path_str))
        }

        "list_dir" => {
            let sub = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let safe_path = boundary.resolve_path(sub);
            if boundary.is_outside(&safe_path) && !boundary.authorize(&format!("OUT-OF-BOUNDS LIST DIR: '{}'", sub)) {
                return Err("[denied] Out-of-bounds list_dir cancelled by user.".into());
            }

            if !safe_path.is_dir() {
                return Err(format!("'{}' is not a directory.", sub));
            }

            let mut entries = Vec::new();
            for entry in fs::read_dir(safe_path).map_err(|e| e.to_string())?.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') {
                    if entry.path().is_dir() {
                        entries.push(format!("{}/", name));
                    } else {
                        entries.push(name);
                    }
                }
            }
            entries.sort();
            Ok(entries.join("\n"))
        }

        "search_code" => {
            let pattern = args["pattern"].as_str().ok_or("Missing 'pattern' parameter")?;
            let sub = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let safe_path = boundary.resolve_path(sub);
            if boundary.is_outside(&safe_path) && !boundary.authorize(&format!("OUT-OF-BOUNDS SEARCH: '{}'", sub)) {
                return Err("[denied] Out-of-bounds search cancelled by user.".into());
            }

            let re = regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .unwrap_or_else(|_| regex::Regex::new(&regex::escape(pattern)).unwrap());

            let mut matches = Vec::new();
            fn walk_search(dir: &Path, re: &regex::Regex, root: &Path, matches: &mut Vec<String>, limit: usize) {
                if matches.len() >= limit {
                    return;
                }
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let fname = entry.file_name().to_string_lossy().to_string();
                        if fname.starts_with('.') || fname == "target" || fname == "node_modules" {
                            continue;
                        }
                        if path.is_dir() {
                            walk_search(&path, re, root, matches, limit);
                        } else if path.is_file() {
                            if let Ok(c) = fs::read_to_string(&path) {
                                for (idx, line) in c.lines().enumerate() {
                                    if re.is_match(line) {
                                        let rel = path.strip_prefix(root).unwrap_or(&path).display();
                                        matches.push(format!("{}:{}: {}", rel, idx + 1, line.trim()));
                                        if matches.len() >= limit {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            walk_search(&safe_path, &re, workspace, &mut matches, ceilings.search_cap);
            if matches.is_empty() {
                Ok(format!("No matches found for '{}'", pattern))
            } else {
                Ok(format!(
                    "### Code Search for '{}' (first {} matches):\n{}",
                    pattern,
                    matches.len(),
                    matches.join("\n")
                ))
            }
        }

        "run_command" => {
            let cmd = args["command"].as_str().ok_or("Missing 'command' parameter")?;

            if let Some(forbidden) = boundary.check_command(cmd) {
                if !boundary.authorize(&format!("MUTATING SYSTEM COMMAND: '{}' ({})", cmd, forbidden)) {
                    return Err(format!("[denied] Execution of forbidden command '{}' cancelled by user.", forbidden));
                }
            }

            let child = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(workspace)
                .process_group(0)
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("Command spawn failed: {}", e))?;

            let pid = child.id().unwrap_or(0) as i32;
            let run_res = timeout(Duration::from_secs(30), child.wait_with_output()).await;

            let output = match run_res {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => return Err(format!("Command execution failed: {}", e)),
                Err(_) => {
                    if pid > 0 {
                        unsafe {
                            kill(-pid, SIGTERM);
                        }
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        unsafe {
                            kill(-pid, SIGKILL);
                        }
                    }
                    return Err(format!("Command timed out after 30 seconds: '{}'. Process group terminated.", cmd));
                }
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}{}", stdout, stderr);
            let trimmed = combined.trim();

            let total_chars = trimmed.chars().count();
            let result = if total_chars > ceilings.char_cap {
                let scratchpad_dir = workspace.join(".rs");
                let _ = fs::create_dir_all(&scratchpad_dir);
                let _ = fs::write(scratchpad_dir.join("scratchpad.log"), trimmed);

                let head_max = ceilings.char_cap / 3;
                let tail_max = (ceilings.char_cap * 2) / 3;
                let head = char_slice_head(trimmed, head_max);
                let tail = char_slice_tail(trimmed, tail_max);

                format!(
                    "{}\n\n... [Output truncated: {} chars exceeds {} cap. Full raw output saved to .rs/scratchpad.log] ...\n\n{}",
                    head, total_chars, ceilings.char_cap, tail
                )
            } else if trimmed.is_empty() {
                "(no output)".into()
            } else {
                trimmed.to_string()
            };

            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(sig) = output.status.signal() {
                    return Ok(format!("(killed by signal {})\n{}", sig, result));
                }
            }

            match output.status.code() {
                Some(0) => Ok(if trimmed.is_empty() { "(exit 0, no output)".into() } else { result }),
                Some(code) => Ok(format!("(exit {})\n{}", code, result)),
                None => Ok(format!("(terminated abnormally)\n{}", result)),
            }
        }

        _ => Err(format!("Unknown tool '{}'", name)),
    }
}
