use crate::adapter::heal_json_args;
use crate::config::ToolTier;
use crate::security::SecurityBoundary;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn get_tools_schema(tier: &ToolTier) -> Value {
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
        ToolTier::Smol => json!([
            {
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
            },
            {
                "type": "function",
                "function": {
                    "name": "edit_file",
                    "description": "Surgically replace exact text (old_str) with new_str in an existing file.",
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
            },
            {
                "type": "function",
                "function": {
                    "name": "write_file",
                    "description": "Create a new file or completely overwrite an existing file. If file exists, pass overwrite=true.",
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
            },
            {
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
            },
            {
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
            },
            {
                "type": "function",
                "function": {
                    "name": "run_command",
                    "description": "Run a shell command inside the workspace directory.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string", "description": "Command to execute." }
                        },
                        "required": ["command"]
                    }
                }
            }
        ])
    }
}

pub fn execute_tool(
    name: &str,
    raw_args: &str,
    workspace: &Path,
    boundary: &SecurityBoundary,
    yolo: bool,
) -> Result<String, String> {
    let args = heal_json_args(raw_args);

    match name {
        "read_file" => {
            let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
            let safe_path = boundary.resolve_path(path_str);
            let is_outside = boundary.is_outside(&safe_path);

            if is_outside {
                if !boundary.authorize(&format!("OUT-OF-BOUNDS READ: '{}' (outside workspace)", path_str), true, false) {
                    return Err("[denied] Out-of-bounds read cancelled by user.".into());
                }
            } else if !boundary.authorize(&format!("read file {}", path_str), false, yolo) {
                return Err("[denied] Action cancelled by user.".into());
            }

            if !safe_path.is_file() {
                return Err(format!("File not found: '{}'", path_str));
            }

            let content = fs::read_to_string(&safe_path).map_err(|e| e.to_string())?;
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();

            let l_start = args.get("line_start").and_then(|v| v.as_u64()).map(|v| v as usize);
            let l_end = args.get("line_end").and_then(|v| v.as_u64()).map(|v| v as usize);

            if let (Some(s), Some(e)) = (l_start, l_end) {
                let start_idx = s.saturating_sub(1);
                let end_idx = e.min(total);
                if start_idx >= total || start_idx >= end_idx {
                    return Err(format!("Invalid line range {}-{} for file with {} lines", s, e, total));
                }
                let slice = lines[start_idx..end_idx].join("\n");
                Ok(format!("### File: {} (Lines {}-{} of {})\n{}", path_str, s, end_idx, total, slice))
            } else if total > 250 {
                let preview = lines[..100].join("\n");
                Ok(format!("### File Preview: {} (Showing 1-100 of {} lines)\n{}\n\n... [File truncated: use read_file with line_start/line_end]", path_str, total, preview))
            } else {
                Ok(format!("### File: {} ({} lines)\n{}", path_str, total, content))
            }
        }

        "edit_file" => {
            let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
            let old_str = args["old_str"].as_str().ok_or("Missing 'old_str' parameter")?;
            let new_str = args["new_str"].as_str().ok_or("Missing 'new_str' parameter")?;

            let safe_path = boundary.resolve_path(path_str);
            let is_outside = boundary.is_outside(&safe_path);

            if is_outside {
                if !boundary.authorize(&format!("OUT-OF-BOUNDS EDIT: '{}' (outside workspace)", path_str), true, false) {
                    return Err("[denied] Out-of-bounds edit cancelled by user.".into());
                }
            } else if !boundary.authorize(&format!("surgically edit {}", path_str), false, yolo) {
                return Err("[denied] Action cancelled by user.".into());
            }

            if !safe_path.is_file() {
                return Err(format!("File not found: '{}'", path_str));
            }

            let original = fs::read_to_string(&safe_path).map_err(|e| e.to_string())?;

            // Stage 1: Exact substring replacement
            if original.contains(old_str) {
                if original.matches(old_str).count() > 1 {
                    return Err("Target 'old_str' matched multiple times. Include more surrounding lines to make it unique.".into());
                }
                let updated = original.replacen(old_str, new_str, 1);
                fs::write(&safe_path, updated).map_err(|e| e.to_string())?;
                return Ok(format!("Successfully edited {} (replaced {} chars with {} chars).", path_str, old_str.len(), new_str.len()));
            }

            // Stage 2: Whitespace-tolerant replacement
            let clean_orig = original.replace("\r\n", "\n");
            let clean_old = old_str.replace("\r\n", "\n");
            let clean_new = new_str.replace("\r\n", "\n");

            let norm_lines_orig: Vec<String> = clean_orig.lines().map(|l| l.trim().to_string()).collect();
            let norm_lines_old: Vec<String> = clean_old.lines().map(|l| l.trim().to_string()).collect();

            if !norm_lines_old.is_empty() && norm_lines_orig.len() >= norm_lines_old.len() {
                for i in 0..=(norm_lines_orig.len() - norm_lines_old.len()) {
                    if norm_lines_orig[i..i + norm_lines_old.len()] == norm_lines_old[..] {
                        let lines_raw: Vec<&str> = clean_orig.lines().collect();
                        let mut new_content = lines_raw[..i].join("\n");
                        if !new_content.is_empty() {
                            new_content.push('\n');
                        }
                        new_content.push_str(&clean_new);
                        let tail = lines_raw[i + norm_lines_old.len()..].join("\n");
                        if !tail.is_empty() {
                            new_content.push('\n');
                            new_content.push_str(&tail);
                        }
                        fs::write(&safe_path, new_content).map_err(|e| e.to_string())?;
                        return Ok(format!("Successfully edited {} with whitespace tolerance.", path_str));
                    }
                }
            }

            Err("Target 'old_str' not found in file. Use read_file to inspect exact lines.".into())
        }

        "write_file" => {
            let path_str = args["path"].as_str().ok_or("Missing 'path' parameter")?;
            let content = args["content"].as_str().ok_or("Missing 'content' parameter")?;
            let overwrite = args.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false);

            let safe_path = boundary.resolve_path(path_str);
            let is_outside = boundary.is_outside(&safe_path);

            if is_outside {
                if !boundary.authorize(&format!("OUT-OF-BOUNDS WRITE: '{}' (outside workspace)", path_str), true, false) {
                    return Err("[denied] Out-of-bounds write cancelled by user.".into());
                }
            } else {
                let action_desc = if safe_path.exists() { format!("overwrite {}", path_str) } else { format!("create {}", path_str) };
                if !boundary.authorize(&action_desc, false, yolo) {
                    return Err("[denied] Action cancelled by user.".into());
                }
            }

            if safe_path.exists() && !overwrite {
                return Err(format!("File '{}' already exists. Pass overwrite=true to confirm replacing it.", path_str));
            }

            if let Some(parent) = safe_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }

            fs::write(&safe_path, content).map_err(|e| e.to_string())?;
            Ok(format!("wrote {} chars to {}", content.len(), path_str))
        }

        "list_dir" => {
            let sub = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let safe_path = boundary.resolve_path(sub);
            let is_outside = boundary.is_outside(&safe_path);

            if is_outside {
                if !boundary.authorize(&format!("OUT-OF-BOUNDS LIST DIR: '{}' (outside workspace)", sub), true, false) {
                    return Err("[denied] Out-of-bounds list_dir cancelled by user.".into());
                }
            } else if !boundary.authorize(&format!("list directory {}", sub), false, yolo) {
                return Err("[denied] Action cancelled by user.".into());
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
            let is_outside = boundary.is_outside(&safe_path);

            if is_outside {
                if !boundary.authorize(&format!("OUT-OF-BOUNDS SEARCH: '{}' (outside workspace)", sub), true, false) {
                    return Err("[denied] Out-of-bounds search cancelled by user.".into());
                }
            }

            let re = regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .unwrap_or_else(|_| regex::Regex::new(&regex::escape(pattern)).unwrap());

            let mut matches = Vec::new();
            fn walk_search(dir: &Path, re: &regex::Regex, root: &Path, matches: &mut Vec<String>) {
                if matches.len() >= 30 {
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
                            walk_search(&path, re, root, matches);
                        } else if path.is_file() {
                            if let Ok(c) = fs::read_to_string(&path) {
                                for (idx, line) in c.lines().enumerate() {
                                    if re.is_match(line) {
                                        let rel = path.strip_prefix(root).unwrap_or(&path).display();
                                        matches.push(format!("{}:{}: {}", rel, idx + 1, line.trim()));
                                        if matches.len() >= 30 {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            walk_search(&safe_path, &re, workspace, &mut matches);
            if matches.is_empty() {
                Ok(format!("No matches found for '{}'", pattern))
            } else {
                Ok(format!("### Code Search for '{}' (first {} matches):\n{}", pattern, matches.len(), matches.join("\n")))
            }
        }

        "run_command" => {
            let cmd = args["command"].as_str().ok_or("Missing 'command' parameter")?;

            // 1. Mutating package manager / dangerous command gate (ALWAYS PROMPTS EVEN IN YOLO)
            if let Some(forbidden) = boundary.check_command(cmd) {
                if !boundary.authorize(&format!("MUTATING SYSTEM COMMAND: '{}' ({})", cmd, forbidden), true, false) {
                    return Err(format!("[denied] Execution of forbidden command '{}' cancelled by user.", forbidden));
                }
            } else if !boundary.authorize(&format!("execute: $ {}", cmd), false, yolo) {
                // 2. Normal in-bounds command (Auto-approves in YOLO, prompts in strict)
                return Err("[denied] Action cancelled by user.".into());
            }

            let output = Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(workspace)
                .output()
                .map_err(|e| format!("Command execution failed: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}{}", stdout, stderr);
            let trimmed = combined.trim();

            let result = if trimmed.len() > 15000 {
                format!("{}\n... [output capped at 15k chars]", &trimmed[..15000])
            } else if trimmed.is_empty() {
                "(exit 0, no output)".into()
            } else {
                trimmed.to_string()
            };

            let code = output.status.code().unwrap_or(0);
            if code != 0 {
                Ok(format!("(exit {})\n{}", code, result))
            } else {
                Ok(result)
            }
        }

        _ => Err(format!("Unknown tool '{}'", name)),
    }
}
