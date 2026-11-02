use anyhow::Result;
use rs_agent::client::{ChatMessage, LlmClient};
use rs_agent::config::{self, ProviderConfig, WorkspaceConfig};
use rs_agent::profile::Profile;
use rs_agent::security::SecurityBoundary;
use rs_agent::skill::Skill;
use rs_agent::tools;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{oneshot, Mutex};

struct SessionState {
    workspace: PathBuf,
    history: Vec<ChatMessage>,
    active_tools: Option<Value>,
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

fn is_permission_approved(res: &Value) -> bool {
    if let Some(result) = res.get("result") {
        if let Some(outcome) = result.get("outcome") {
            let outcome_type = outcome.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
            if outcome_type == "selected" {
                let opt = outcome.get("optionId").and_then(|v| v.as_str()).unwrap_or("");
                return opt != "deny" && opt != "reject" && opt != "reject_once" && opt != "reject_always";
            }
            return false;
        }
        return result.as_bool().unwrap_or(true);
    }
    false
}

fn assemble_system_prompt(workspace: &Path) -> (String, Option<Value>) {
    let cfg = WorkspaceConfig::load_or_default(workspace);
    let inherited_profile = std::env::var("AI_ACTIVE_PROFILE").unwrap_or_else(|_| cfg.profile_name.clone());
    let active_profile = Profile::load(&inherited_profile);

    let mut prompt = if let Some(prof) = &active_profile {
        format!("{}\n\nWorkspace Path: {}", prof.system_prompt, workspace.display())
    } else {
        format!("You are an autonomous agent working in workspace: {}.", workspace.display())
    };

    if let Some(global_rules) = config::load_global_instructions() {
        prompt.push_str(&format!("\n\n---\nGLOBAL SYSTEM INSTRUCTIONS:\n{}", global_rules));
    }

    if let Some(prof) = &active_profile {
        if prof.memory {
            let mem_path = workspace.join(".rs/memory.md");
            if let Ok(c) = std::fs::read_to_string(mem_path) {
                let trimmed = c.trim();
                if !trimmed.is_empty() {
                    prompt.push_str(&format!("\n\n---\nWORKSPACE MEMORY (.rs/memory.md):\n{}", trimmed));
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

    let active_tools = Some(tools::get_tools_schema(&cfg.tier));
    (prompt, active_tools)
}

#[tokio::main]
async fn main() -> Result<()> {
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

        // Handle incoming responses to our outbound RPC requests (e.g. session/request_permission)
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
                            }
                        },
                        "agentInfo": {
                            "name": "rs-agent",
                            "version": "0.4.4"
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

                let (sys_prompt, active_tools) = assemble_system_prompt(&cwd);
                let history = vec![ChatMessage::system(sys_prompt)];

                sessions.lock().await.insert(sid.clone(), SessionState {
                    workspace: cwd.clone(),
                    history,
                    active_tools,
                    cancel_flag: Arc::new(AtomicBool::new(false)),
                });

                let cfg = WorkspaceConfig::load_or_default(&cwd);
                let prof = Profile::load(&cfg.profile_name);
                let initial_val = prof.as_ref()
                    .and_then(|p| p.reasoning_budget)
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "-1".to_string());

                client.set_reasoning_budget(prof.as_ref().and_then(|p| p.reasoning_budget));

                send_rpc(req_id, Some(json!({
                    "sessionId": sid,
                    "configOptions": get_config_options(&initial_val)
                })), None);
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
                if let Some(prompt_str) = params.get("prompt").and_then(|v| v.as_str()) {
                    raw_user_text.push_str(prompt_str);
                } else if let Some(prompt_items) = params.get("prompt").and_then(|v| v.as_array()) {
                    for item in prompt_items {
                        if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                                raw_user_text.push_str(t);
                                raw_user_text.push(' ');
                            }
                        }
                    }
                }

                let cleaned_user_text = sanitize_t3_prompt(&raw_user_text);
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
                    let (workspace, mut history, tools_schema, cancel_flag) = {
                        let mut lock = sessions_ref.lock().await;
                        if let Some(sess) = lock.get_mut(&sid) {
                            sess.cancel_flag.store(false, Ordering::SeqCst);
                            sess.history.push(ChatMessage::user(cleaned_user_text));
                            (sess.workspace.clone(), sess.history.clone(), sess.active_tools.clone(), sess.cancel_flag.clone())
                        } else {
                            send_rpc(req_id_owned.as_ref(), Some(json!({ "stopReason": "end_turn" })), None);
                            return;
                        }
                    };

                    let boundary = SecurityBoundary::new(&workspace);
                    let cfg = WorkspaceConfig::load_or_default(&workspace);

                    let max_rounds = 10;
                    for _round in 0..max_rounds {
                        if cancel_flag.load(Ordering::SeqCst) {
                            send_acp_chunk(&sid, "\n\n*(Cancelled)*", false);
                            break;
                        }

                        let sid_clone = sid.clone();
                        let stream_res = client_ref.stream_chat_raw(
                            &history,
                            tools_schema.clone(),
                            move |chunk, is_thinking| {
                                send_acp_chunk(&sid_clone, chunk, is_thinking);
                            },
                        ).await;

                        match stream_res {
                            Ok((reply, tool_calls, _stats)) => {
                                if let Some(calls) = tool_calls {
                                    if !calls.is_empty() {
                                        let mut assistant_msg = ChatMessage::assistant(reply.clone());
                                        assistant_msg.tool_calls = Some(Value::Array(calls.clone()));
                                        history.push(assistant_msg);

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

                                                let req_id = rpc_id_gen.fetch_add(1, Ordering::SeqCst);
                                                let (tx, rx) = oneshot::channel();
                                                pending_perms.lock().await.insert(req_id, tx);

                                                let perm_req = json!({
                                                    "jsonrpc": "2.0",
                                                    "id": req_id,
                                                    "method": "session/request_permission",
                                                    "params": {
                                                        "sessionId": sid,
                                                        "toolCall": {
                                                            "toolCallId": call_id,
                                                            "title": title,
                                                            "kind": kind,
                                                            "rawInput": raw_input
                                                        },
                                                        "options": [
                                                            { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
                                                            { "optionId": "deny", "name": "Deny", "kind": "reject_once" }
                                                        ]
                                                    }
                                                });
                                                send_raw_rpc(&perm_req);

                                                // Wait for T3 Code decision based on active security mode
                                                let perm_res = rx.await.unwrap_or_else(|_| json!({}));
                                                if !is_permission_approved(&perm_res) {
                                                    send_acp_chunk(&sid, &format!("\n\n*Action `{}` was declined by user.*\n", fn_name), false);
                                                    history.push(ChatMessage::tool_response(call_id, "User declined permission for this tool action.".to_string()));
                                                    continue;
                                                }
                                            }

                                            send_acp_chunk(&sid, &format!("\n\n*Running `{}`...*\n", fn_name), false);

                                            let final_args = if cfg.adapters {
                                                rs_agent::adapter::repair_tool_args(fn_name, fn_args)
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
                                    } else {
                                        history.push(ChatMessage::assistant(reply));
                                        break;
                                    }
                                } else {
                                    history.push(ChatMessage::assistant(reply));
                                    break;
                                }
                            }
                            Err(e) => {
                                send_acp_chunk(&sid, &format!("\n\n[Error: {}]\n", e), false);
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

                    send_rpc(req_id_owned.as_ref(), Some(json!({ "stopReason": "end_turn" })), None);
                });
            }
            "tools/list" => {
                let schema = tools::get_tools_schema(&rs_agent::config::ToolTier::Smol);
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
