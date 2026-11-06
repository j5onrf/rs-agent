use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{oneshot, Mutex};

use crate::adapter;
use crate::client::{ChatMessage, LlmClient};
use crate::config::{self, ProviderConfig, WorkspaceConfig};
use crate::profile::Profile;
use crate::security::SecurityBoundary;
use crate::skill::Skill;
use crate::tools;

struct SessionState {
    workspace: PathBuf,
    history: Vec<ChatMessage>,
    active_tools: Option<Value>,
    is_agent: bool,
    cancel_flag: Arc<AtomicBool>,
}

fn send_raw_rpc(payload: &Value) {
    let serialized = serde_json::to_string(payload).unwrap_or_default();
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", serialized);
    let _ = stdout.flush();
}

fn send_rpc(id: Option<&Value>, result: Option<Value>, error: Option<Value>) {
    let mut payload = json!({ "jsonrpc": "2.0" });
    if let Some(i) = id {
        payload["id"] = i.clone();
    }
    if let Some(e) = error {
        payload["error"] = e;
    } else {
        payload["result"] = result.unwrap_or_else(|| json!({}));
    }
    send_raw_rpc(&payload);
}

fn send_acp_chunk(session_id: &str, text: &str, is_thought: bool) {
    if text.is_empty() {
        return;
    }
    let update_type = if is_thought { "agent_thought_chunk" } else { "agent_message_chunk" };
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": update_type,
                "content": {
                    "type": "text",
                    "text": text
                }
            }
        }
    });
    send_raw_rpc(&payload);
}

fn strip_xml_block(text: &str, tag: &str) -> String {
    let mut result = text.to_string();
    let open_pattern = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);
    while let Some(start) = result.find(&open_pattern) {
        let after_tag = &result[start + open_pattern.len()..];
        if after_tag.starts_with('>') || after_tag.starts_with(' ') || after_tag.starts_with('\n') {
            if let Some(end_offset) = result[start..].find(&close_tag) {
                let end = start + end_offset + close_tag.len();
                result.replace_range(start..end, "");
                continue;
            }
        }
        break;
    }
    result
}

fn sanitize_t3_prompt(raw: &str) -> String {
    let tags = [
        "pull_request_linking",
        "t3_code_instructions",
        "runtime_info",
        "system_information",
    ];
    let mut cleaned = raw.to_string();
    for tag in tags {
        cleaned = strip_xml_block(&cleaned, tag);
    }
    cleaned.trim().to_string()
}

fn get_config_options(current_val: &str) -> Value {
    json!([
        {
            "id": "reasoning_budget",
            "name": "Reasoning Effort",
            "category": "thought_level",
            "type": "select",
            "currentValue": current_val,
            "options": [
                { "id": "0", "value": "0", "name": "Off (0 tokens)" },
                { "id": "250", "value": "250", "name": "Low (250 tokens)" },
                { "id": "500", "value": "500", "name": "Medium (500 tokens)" },
                { "id": "1000", "value": "1000", "name": "High (1000 tokens)" },
                { "id": "-1", "value": "-1", "name": "Unrestricted" }
            ]
        }
    ])
}

fn get_available_commands() -> Value {
    let mut cmds = vec![
        json!({
            "name": "task",
            "description": "Execute autonomous goal loop with verification (/task <goal>)"
        }),
        json!({
            "name": "reset",
            "description": "Reset conversation history and active turn context"
        }),
    ];

    for skill_name in Skill::list_available() {
        if let Some(sk) = Skill::load(&skill_name) {
            let desc = sk.content.lines()
                .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .unwrap_or("Custom Skill Directive")
                .trim();
            cmds.push(json!({
                "name": format!("${}", sk.name),
                "description": desc
            }));
        }
    }

    Value::Array(cmds)
}

fn is_permission_approved(res: &Value) -> bool {
    if let Some(result) = res.get("result") {
        if let Some(outcome) = result.get("outcome") {
            let outcome_type = outcome.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
            if outcome_type == "selected" {
                let opt = outcome.get("optionId").and_then(|v| v.as_str()).unwrap_or("");
                return opt != "deny" && opt != "reject" && opt != "reject_once" && opt != "reject_always" && opt != "cancel";
            }
            if outcome_type == "cancelled" || outcome_type == "dismissed" {
                return false;
            }
            return false;
        }
        return result.as_bool().unwrap_or(true);
    }
    false
}

fn generate_edit_diff(workspace: &Path, path_str: &str, old_str: &str, new_str: &str) -> Option<(String, String)> {
    let full_path = workspace.join(path_str);
    let original = fs::read_to_string(&full_path).ok()?;
    if original.contains(old_str) {
        let updated = original.replacen(old_str, new_str, 1);
        Some((original, updated))
    } else {
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
                    return Some((original, new_content));
                }
            }
        }
        None
    }
}

fn generate_write_diff(workspace: &Path, path_str: &str, new_content: &str) -> (String, String) {
    let full_path = workspace.join(path_str);
    let old = fs::read_to_string(&full_path).unwrap_or_default();
    (old, new_content.to_string())
}

fn sync_acp_skill_stubs(workspace: &Path) {
    let skill_dir = workspace.join("acp:").join("skill");
    if fs::create_dir_all(&skill_dir).is_ok() {
        for sk_name in Skill::list_available() {
            if let Some(sk) = Skill::load(&sk_name) {
                let stub = skill_dir.join(&sk.name);
                let _ = fs::write(stub, &sk.content);
            }
        }
    }
}

async fn assemble_system_prompt(workspace: &Path) -> (String, Option<Value>, bool) {
    let cfg = WorkspaceConfig::load_or_default(workspace);
    let inherited_profile = std::env::var("AI_ACTIVE_PROFILE").unwrap_or_else(|_| cfg.profile_name.clone());

    let is_agent_env = std::env::var("AI_IS_AGENT").ok().and_then(|v| v.parse::<usize>().ok().map(|n| n == 1));
    let is_agent = is_agent_env.unwrap_or(cfg.is_agent && inherited_profile != "chat");

    let active_profile = Profile::load(&inherited_profile);

    let mut prompt = if is_agent {
        if let Some(prof) = &active_profile {
            format!("{}\n\nWorkspace Path: {}", prof.system_prompt, workspace.display())
        } else {
            format!("You are an autonomous agent working in workspace: {}.", workspace.display())
        }
    } else {
        if let Some(prof) = &active_profile {
            prof.system_prompt.clone()
        } else {
            "You are an articulate, perceptive, and intellectually honest conversational partner.".to_string()
        }
    };

    if let Some(global_rules) = config::load_global_instructions() {
        prompt.push_str(&format!("\n\n---\nGLOBAL SYSTEM INSTRUCTIONS:\n{}", global_rules));
    }

    if is_agent {
        if let Some(prof) = &active_profile {
            if prof.memory {
                let mem_path = workspace.join(".rs/memory.md");
                if let Ok(c) = fs::read_to_string(mem_path) {
                    let trimmed = c.trim();
                    if !trimmed.is_empty() {
                        prompt.push_str(&format!("\n\n---\nWORKSPACE MEMORY (.rs/memory.md):\n{}", trimmed));
                    }
                }
            }
        }
    }

    if let Ok(extra) = std::env::var("AI_ACTIVE_SKILLS") {
        for sk_name in extra.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if let Some(sk) = Skill::load(sk_name) {
                prompt.push_str(&format!("\n\n---\nSKILL DIRECTIVE [{}]:\n{}", sk.name, sk.content));
            }
        }
    }

    let mcp_servers = active_profile.as_ref().map(|p| p.mcp_servers.clone()).unwrap_or_default();
    let active_tools = if is_agent {
        Some(tools::get_tools_schema_with_mcp(&cfg.tier, false, &mcp_servers).await)
    } else {
        None
    };

    (prompt, active_tools, is_agent)
}

pub async fn run_bridge() -> Result<()> {
    let sessions: Arc<Mutex<HashMap<String, SessionState>>> = Arc::new(Mutex::new(HashMap::new()));
    let pending_permissions: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_rpc_id = Arc::new(AtomicU64::new(1000));

    let provider = ProviderConfig::load().await;
    let client = Arc::new(LlmClient::new(provider));

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    let mut active_session_id = format!("rsagent-{}", std::process::id());

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if req.get("method").is_none() {
            if let Some(id_num) = req.get("id").and_then(|v| v.as_u64()) {
                let mut pending = pending_permissions.lock().await;
                if let Some(tx) = pending.remove(&id_num) {
                    let _ = tx.send(req);
                    continue;
                }
            }
        }

        let req_id = req.get("id");
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => {
                send_rpc(
                    req_id,
                    Some(json!({
                        "protocolVersion": 1,
                        "agentCapabilities": {
                            "sessionCapabilities": {
                                "list": {},
                                "load": {},
                                "resume": {},
                                "configOptions": {}
                            },
                            "promptCapabilities": {
                                "image": true,
                                "embeddedContext": true
                            }
                        },
                        "agentInfo": {
                            "name": "rs-agent",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    })),
                    None,
                );
            }
            "authenticate" => {
                send_rpc(req_id, Some(json!({})), None);
            }
            "session/new" | "createSession" | "session/create" | "session/load" | "session/resume" => {
                let sid = params.get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("rsagent-{}", std::process::id()));

                active_session_id = sid.clone();

                let cwd = params.get("cwd")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

                sync_acp_skill_stubs(&cwd);
                let (sys_prompt, active_tools, is_agent) = assemble_system_prompt(&cwd).await;
                let history = vec![ChatMessage::system(sys_prompt)];

                sessions.lock().await.insert(sid.clone(), SessionState {
                    workspace: cwd.clone(),
                    history,
                    active_tools,
                    is_agent,
                    cancel_flag: Arc::new(AtomicBool::new(false)),
                });

                let cfg = WorkspaceConfig::load_or_default(&cwd);
                let prof = Profile::load(&cfg.profile_name);
                let initial_val = prof.as_ref()
                    .and_then(|p| p.reasoning_budget)
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "-1".to_string());

                client.set_reasoning_budget(prof.as_ref().and_then(|p| p.reasoning_budget));

                let available_commands = get_available_commands();

                send_rpc(req_id, Some(json!({
                    "sessionId": sid,
                    "configOptions": get_config_options(&initial_val),
                    "availableCommands": available_commands
                })), None);

                let notif = json!({
                    "jsonrpc": "2.0",
                    "method": "session/update",
                    "params": {
                        "sessionId": sid,
                        "update": {
                            "sessionUpdate": "available_commands_update",
                            "availableCommands": available_commands
                        }
                    }
                });
                send_raw_rpc(&notif);
            }
            "session/set_config_option" => {
                let config_id = params.get("configId").and_then(|v| v.as_str()).unwrap_or("");
                let value_str = params.get("value")
                    .and_then(|v| {
                        if let Some(s) = v.as_str() {
                            Some(s.to_string())
                        } else if let Some(val_str) = v.get("value").and_then(|v2| v2.as_str()) {
                            Some(val_str.to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "0".to_string());

                if config_id == "reasoning_budget" || config_id == "thought_level" {
                    if let Ok(b) = value_str.parse::<i64>() {
                        let budget = if b < 0 { None } else { Some(b as usize) };
                        client.set_reasoning_budget(budget);
                    }
                }

                send_rpc(req_id, Some(json!({
                    "configOptions": get_config_options(&value_str)
                })), None);
            }
            "session/list" | "listSessions" => {
                let sess_lock = sessions.lock().await;
                let list: Vec<Value> = sess_lock.iter().map(|(k, v)| {
                    json!({ "sessionId": k, "cwd": v.workspace.display().to_string() })
                }).collect();
                send_rpc(req_id, Some(json!({ "sessions": list })), None);
            }
            "session/prompt" | "prompt" => {
                let sid = params.get("sessionId").and_then(|v| v.as_str()).unwrap_or(&active_session_id).to_string();

                let mut raw_user_text = String::new();
                let mut image_parts = Vec::new();

                if let Some(prompt_str) = params.get("prompt").and_then(|v| v.as_str()) {
                    raw_user_text.push_str(prompt_str);
                } else if let Some(prompt_items) = params.get("prompt").and_then(|v| v.as_array()) {
                    for item in prompt_items {
                        match item.get("type").and_then(|v| v.as_str()) {
                            Some("text") => {
                                if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                                    raw_user_text.push_str(t);
                                    raw_user_text.push(' ');
                                }
                            }
                            Some("resource") => {
                                if let Some(res) = item.get("resource") {
                                    let uri = res.get("uri").and_then(|v| v.as_str()).unwrap_or("resource");
                                    let text = res.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                    raw_user_text.push_str(&format!("\n\n---\nContext from @{}:\n{}\n---\n", uri, text));
                                }
                            }
                            Some("file") => {
                                let p = item.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                                let c = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                raw_user_text.push_str(&format!("\n\n---\nFile @{}:\n{}\n---\n", p, c));
                            }
                            Some("image") => {
                                let mime = item.get("mimeType").and_then(|v| v.as_str()).unwrap_or("image/png");
                                if let Some(data) = item.get("data").and_then(|v| v.as_str()) {
                                    image_parts.push(json!({
                                        "type": "image_url",
                                        "image_url": {
                                            "url": format!("data:{};base64,{}", mime, data)
                                        }
                                    }));
                                }
                            }
                            _ => {}
                        }
                    }
                }

                let mut cleaned_user_text = sanitize_t3_prompt(&raw_user_text);

                if cleaned_user_text == "/reset" || cleaned_user_text == "/c" || cleaned_user_text == "/clear" {
                    let mut lock = sessions.lock().await;
                    if let Some(sess) = lock.get_mut(&sid) {
                        let (sys_prompt, _, _) = assemble_system_prompt(&sess.workspace).await;
                        sess.history = vec![ChatMessage::system(sys_prompt)];
                    }
                    send_acp_chunk(&sid, "✓ Session history reset.", false);
                    send_rpc(req_id, Some(json!({ "stopReason": "end_turn" })), None);
                    continue;
                }

                let mut found_skills = Vec::new();
                let mut remaining_words = Vec::new();

                for word in cleaned_user_text.split_whitespace() {
                    if let Some(skill_name) = word.strip_prefix('$') {
                        let clean_name = skill_name.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
                        if let Some(sk) = Skill::load(clean_name) {
                            found_skills.push(sk);
                            continue;
                        }
                    }
                    remaining_words.push(word);
                }

                if !found_skills.is_empty() {
                    let mut lock = sessions.lock().await;
                    if let Some(sess) = lock.get_mut(&sid) {
                        for sk in &found_skills {
                            let directive = format!("\n\n---\nSKILL DIRECTIVE [{}]:\n{}", sk.name, sk.content);
                            if let Some(first) = sess.history.first_mut() {
                                let curr = first.content_as_str().to_string();
                                if !curr.contains(&format!("SKILL DIRECTIVE [{}]", sk.name)) {
                                    first.content = serde_json::Value::String(format!("{}{}", curr, directive));
                                }
                            }
                        }
                    }
                }

                let clean_query = remaining_words.join(" ");

                if clean_query.trim().is_empty() && !found_skills.is_empty() {
                    let names: Vec<String> = found_skills.iter().map(|s| format!("${}", s.name)).collect();
                    send_acp_chunk(&sid, &format!("✓ Skill active: {}. Directives attached to system context for subsequent turns.", names.join(", ")), false);
                    send_rpc(req_id, Some(json!({ "stopReason": "end_turn" })), None);
                    continue;
                }

                cleaned_user_text = clean_query;

                if cleaned_user_text.is_empty() {
                    send_rpc(req_id, Some(json!({ "stopReason": "end_turn" })), None);
                    continue;
                }

                let client_ref = client.clone();
                let sessions_ref = sessions.clone();
                let req_id_owned = req_id.cloned();
                let pending_perms = pending_permissions.clone();
                let rpc_id_gen = next_rpc_id.clone();

                tokio::spawn(async move {
                    let (workspace, mut history, tools_schema, is_agent, cancel_flag) = {
                        let mut lock = sessions_ref.lock().await;
                        if let Some(sess) = lock.get_mut(&sid) {
                            sess.cancel_flag.store(false, Ordering::SeqCst);
                            if image_parts.is_empty() {
                                sess.history.push(ChatMessage::user(cleaned_user_text));
                            } else {
                                let mut parts = vec![json!({ "type": "text", "text": cleaned_user_text })];
                                parts.extend(image_parts);
                                sess.history.push(ChatMessage {
                                    role: "user".into(),
                                    content: Value::Array(parts),
                                    reasoning_content: None,
                                    tool_calls: None,
                                    tool_call_id: None,
                                });
                            }
                            (
                                sess.workspace.clone(),
                                sess.history.clone(),
                                sess.active_tools.clone(),
                                sess.is_agent,
                                sess.cancel_flag.clone(),
                            )
                        } else {
                            send_rpc(req_id_owned.as_ref(), Some(json!({ "stopReason": "end_turn" })), None);
                            return;
                        }
                    };

                    let boundary = SecurityBoundary::new(&workspace);
                    let cfg = WorkspaceConfig::load_or_default(&workspace);

                    let max_rounds = if is_agent { 10 } else { 1 };
                    for _round in 0..max_rounds {
                        if cancel_flag.load(Ordering::SeqCst) {
                            send_acp_chunk(&sid, "\n\n*(Cancelled)*", false);
                            break;
                        }

                        let sid_clone = sid.clone();
                        let stream_res = client_ref.stream_chat_raw(
                            &history,
                            tools_schema.clone(),
                            Some(cancel_flag.clone()),
                            move |chunk, is_thinking| {
                                send_acp_chunk(&sid_clone, chunk, is_thinking);
                            },
                        ).await;

                        match stream_res {
                            Ok((reply, reasoning, tool_calls, _stats)) => {
                                if cancel_flag.load(Ordering::SeqCst) {
                                    send_acp_chunk(&sid, "\n\n*(Cancelled)*", false);
                                    break;
                                }

                                if let Some(calls) = tool_calls {
                                    if !calls.is_empty() && is_agent {
                                        let mut assistant_msg = ChatMessage::assistant_with_reasoning(reply.clone(), reasoning.clone());
                                        assistant_msg.tool_calls = Some(Value::Array(calls.clone()));
                                        history.push(assistant_msg);

                                        let mut any_declined = false;

                                        for call in calls {
                                            if cancel_flag.load(Ordering::SeqCst) {
                                                break;
                                            }

                                            let call_id = call["id"].as_str().unwrap_or("call_0").to_string();
                                            let fn_name = call["function"]["name"].as_str().unwrap_or("tool");
                                            let fn_args = call["function"]["arguments"].as_str().unwrap_or("{}");

                                            let is_mutating = fn_name == "write_file" || fn_name == "edit_file" || fn_name == "run_command";
                                            if is_mutating {
                                                let kind = match fn_name {
                                                    "write_file" | "edit_file" => "edit",
                                                    "run_command" => "execute",
                                                    _ => "other",
                                                };

                                                let raw_input: Value = serde_json::from_str(fn_args).unwrap_or_else(|_| json!({}));
                                                let brief = raw_input.get("path")
                                                    .or_else(|| raw_input.get("command"))
                                                    .or_else(|| raw_input.get("pattern"))
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("");

                                                let title = if brief.is_empty() {
                                                    fn_name.to_string()
                                                } else {
                                                    format!("{} {}", fn_name, brief)
                                                };

                                                let mut content_blocks = Vec::new();
                                                if fn_name == "edit_file" {
                                                    if let (Some(p), Some(old_s), Some(new_s)) = (
                                                        raw_input.get("path").and_then(|v| v.as_str()),
                                                        raw_input.get("old_str").and_then(|v| v.as_str()),
                                                        raw_input.get("new_str").and_then(|v| v.as_str()),
                                                    ) {
                                                        if let Some((old_text, new_text)) = generate_edit_diff(&workspace, p, old_s, new_s) {
                                                            content_blocks.push(json!({
                                                                "type": "diff",
                                                                "path": p,
                                                                "oldText": old_text,
                                                                "newText": new_text
                                                            }));
                                                        }
                                                    }
                                                } else if fn_name == "write_file" {
                                                    if let (Some(p), Some(content_str)) = (
                                                        raw_input.get("path").and_then(|v| v.as_str()),
                                                        raw_input.get("content").and_then(|v| v.as_str()),
                                                    ) {
                                                        let (old_text, new_text) = generate_write_diff(&workspace, p, content_str);
                                                        content_blocks.push(json!({
                                                            "type": "diff",
                                                            "path": p,
                                                            "oldText": old_text,
                                                            "newText": new_text
                                                        }));
                                                    }
                                                } else if fn_name == "run_command" {
                                                    if let Some(cmd_str) = raw_input.get("command").and_then(|v| v.as_str()) {
                                                        content_blocks.push(json!({
                                                            "type": "text",
                                                            "text": format!("$ {}", cmd_str)
                                                        }));
                                                    }
                                                }

                                                let req_id = rpc_id_gen.fetch_add(1, Ordering::SeqCst);
                                                let (tx, rx) = oneshot::channel();
                                                pending_perms.lock().await.insert(req_id, tx);

                                                let tool_call_obj = json!({
                                                    "toolCallId": call_id,
                                                    "title": title,
                                                    "kind": kind,
                                                    "rawInput": raw_input,
                                                    "content": content_blocks
                                                });

                                                let perm_req = json!({
                                                    "jsonrpc": "2.0",
                                                    "id": req_id,
                                                    "method": "session/request_permission",
                                                    "params": {
                                                        "sessionId": sid,
                                                        "toolCall": tool_call_obj,
                                                        "options": [
                                                            { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
                                                            { "optionId": "deny", "name": "Deny", "kind": "reject_once" }
                                                        ]
                                                    }
                                                });
                                                send_raw_rpc(&perm_req);

                                                let perm_res = rx.await.unwrap_or_else(|_| json!({}));
                                                if !is_permission_approved(&perm_res) {
                                                    send_acp_chunk(&sid, &format!("\n\n*Action `{}` was declined by user.*\n", fn_name), false);
                                                    history.push(ChatMessage::tool_response(call_id, "User declined permission for this tool action. Do not retry this action; ask the user what to do next.".to_string()));
                                                    any_declined = true;
                                                    continue;
                                                }
                                            }

                                            send_acp_chunk(&sid, &format!("\n\n*Running `{}`...*\n", fn_name), false);

                                            let final_args = if cfg.adapters {
                                                adapter::repair_tool_args(fn_name, fn_args)
                                            } else {
                                                fn_args.to_string()
                                            };

                                            let tool_res = tools::execute_tool(fn_name, &final_args, &workspace, &boundary).await;
                                            let out = match tool_res {
                                                Ok(o) => o,
                                                Err(e) => format!("[tool error] {}", e),
                                            };
                                            history.push(ChatMessage::tool_response(call_id, out));
                                        }

                                        // If user explicitly denied permission, break loop cleanly instead of letting agent loop indefinitely
                                        if any_declined {
                                            break;
                                        }
                                    } else {
                                        history.push(ChatMessage::assistant_with_reasoning(reply, reasoning));
                                        break;
                                    }
                                } else {
                                    history.push(ChatMessage::assistant_with_reasoning(reply, reasoning));
                                    break;
                                }
                            }
                            Err(e) => {
                                // Graceful non-crashing error output to T3 stream
                                send_acp_chunk(&sid, &format!("\n\n*(Notice: {})*\n", e), false);
                                break;
                            }
                        }
                    }

                    {
                        let mut lock = sessions_ref.lock().await;
                        if let Some(sess) = lock.get_mut(&sid) {
                            sess.history = history;
                        }
                    }

                    // Turn terminates cleanly with end_turn stopReason
                    send_rpc(req_id_owned.as_ref(), Some(json!({ "stopReason": "end_turn" })), None);
                });
            }
            "tools/list" => {
                let sess_lock = sessions.lock().await;
                let is_agent = sess_lock.get(&active_session_id).map(|s| s.is_agent).unwrap_or(true);
                let schema = if is_agent {
                    tools::get_tools_schema(&config::ToolTier::Smol, false)
                } else {
                    json!([])
                };
                send_rpc(req_id, Some(json!({ "tools": schema })), None);
            }
            "session/cancel" | "cancel" => {
                let sid = params.get("sessionId").and_then(|v| v.as_str()).unwrap_or(&active_session_id);
                let lock = sessions.lock().await;
                if let Some(sess) = lock.get(sid) {
                    sess.cancel_flag.store(true, Ordering::SeqCst);
                }
                send_rpc(req_id, Some(json!({})), None);
            }
            "shutdown" => {
                send_rpc(req_id, Some(json!({ "status": "ok" })), None);
                break;
            }
            _ => {
                send_rpc(req_id, Some(json!({})), None);
            }
        }
    }

    Ok(())
}
