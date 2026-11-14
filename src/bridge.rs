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
use crate::profile::{self, Profile, ReasoningSetting};
use crate::security::SecurityBoundary;
use crate::skill::Skill;
use crate::tools;

struct SessionState {
    workspace: PathBuf,
    history: Vec<ChatMessage>,
    active_tools: Option<Value>,
    is_agent: bool,
    adapters: bool,
    reasoning: Option<ReasoningSetting>,
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
    let tags = ["pull_request_linking", "t3_code_instructions", "runtime_info", "system_information"];
    let mut cleaned = raw.to_string();
    for tag in tags {
        cleaned = strip_xml_block(&cleaned, tag);
    }
    cleaned.trim().to_string()
}

fn strip_skills_preserve_layout(text: &str) -> (String, Vec<Skill>) {
    if !text.contains('$') {
        return (text.to_string(), Vec::new());
    }
    let mut found_skills = Vec::new();
    let mut out_lines = Vec::new();

    for line in text.split('\n') {
        if !line.contains('$') {
            out_lines.push(line.to_string());
            continue;
        }
        let mut stripped_any = false;
        let mut rebuilt = String::with_capacity(line.len());
        let mut rest = line;

        while let Some(dollar_idx) = rest.find('$') {
            rebuilt.push_str(&rest[..dollar_idx]);
            let after = &rest[dollar_idx..];
            let token_len = after.find(char::is_whitespace).unwrap_or(after.len());
            let token = &after[..token_len];
            let clean_name = token[1..].trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');

            if !clean_name.is_empty() {
                if let Some(sk) = Skill::load(clean_name) {
                    stripped_any = true;
                    found_skills.push(sk);
                    let remainder = &after[token_len..];
                    if rebuilt.ends_with([' ', '\t']) && remainder.starts_with(' ') {
                        rebuilt.pop();
                    } else if rebuilt.is_empty() {
                        rest = remainder.strip_prefix(' ').unwrap_or(remainder);
                        continue;
                    }
                    rest = remainder;
                    continue;
                }
            }
            rebuilt.push_str(token);
            rest = &after[token_len..];
        }
        rebuilt.push_str(rest);

        if !(stripped_any && rebuilt.trim().is_empty()) {
            out_lines.push(rebuilt);
        }
    }

    (out_lines.join("\n").trim().to_string(), found_skills)
}

fn get_config_options(current_val: &str, is_local: bool) -> Value {
    let (name, options) = if is_local {
        (
            "Reasoning Budget",
            json!([
                { "id": "0", "value": "0", "name": "Off (0 tokens)" },
                { "id": "250", "value": "250", "name": "250 tokens" },
                { "id": "500", "value": "500", "name": "500 tokens" },
                { "id": "1000", "value": "1000", "name": "1000 tokens" },
                { "id": "4000", "value": "4000", "name": "4000 tokens" },
                { "id": "-1", "value": "-1", "name": "Default / Unrestricted" }
            ]),
        )
    } else {
        (
            "Reasoning Effort",
            json!([
                { "id": "low", "value": "low", "name": "Low" },
                { "id": "medium", "value": "medium", "name": "Medium" },
                { "id": "high", "value": "high", "name": "High" },
                { "id": "xhigh", "value": "xhigh", "name": "XHigh" },
                { "id": "-1", "value": "-1", "name": "Default / Auto" }
            ]),
        )
    };

    json!([
        {
            "id": "reasoning_budget",
            "name": name,
            "category": "thought_level",
            "type": "select",
            "currentValue": current_val,
            "options": options
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
            let desc = sk
                .content
                .lines()
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
                return opt != "deny"
                    && opt != "reject"
                    && opt != "reject_once"
                    && opt != "reject_always"
                    && opt != "cancel";
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

/// Safely generates edit diff previews verified against the zero-trust SecurityBoundary.
fn generate_edit_diff(
    boundary: &SecurityBoundary,
    path_str: &str,
    old_str: &str,
    new_str: &str,
) -> Option<(String, String)> {
    let full_path = boundary.resolve_path(path_str);
    if boundary.is_outside(&full_path) {
        return None;
    }

    let original = fs::read_to_string(&full_path).ok()?;
    let updated = tools::resilient_replace(&original, old_str, new_str).ok()?;
    Some((original, updated))
}

/// Safely generates write diff previews verified against the zero-trust SecurityBoundary.
fn generate_write_diff(boundary: &SecurityBoundary, path_str: &str, new_content: &str) -> (String, String) {
    let full_path = boundary.resolve_path(path_str);
    if boundary.is_outside(&full_path) {
        return (String::new(), new_content.to_string());
    }
    let old = fs::read_to_string(&full_path).unwrap_or_default();
    (old, new_content.to_string())
}

struct SessionSetup {
    sys_prompt: String,
    active_tools: Option<Value>,
    is_agent: bool,
    adapters: bool,
    reasoning: Option<ReasoningSetting>,
}

async fn assemble_session_setup(workspace: &Path) -> Result<SessionSetup> {
    let cfg = WorkspaceConfig::load_or_default(workspace);
    let inherited_profile = std::env::var("AI_ACTIVE_PROFILE").unwrap_or_else(|_| cfg.profile_name.clone());

    let is_agent_env = std::env::var("AI_IS_AGENT").ok().and_then(|v| v.parse::<usize>().ok().map(|n| n == 1));
    let is_agent = is_agent_env.unwrap_or(cfg.is_agent && inherited_profile != "chat");

    let prof = Profile::load(&inherited_profile)
        .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found in ~/.config/rs-agent/profiles/", inherited_profile))?;

    let active_skills: Vec<String> = std::env::var("AI_ACTIVE_SKILLS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let sys_prompt = profile::build_system_prompt(&prof, workspace, is_agent, &active_skills);
    let active_tools = if is_agent {
        Some(tools::get_tools_schema_with_mcp(&prof.tier, prof.delegate, &prof.mcp_servers).await)
    } else {
        None
    };

    Ok(SessionSetup {
        sys_prompt,
        active_tools,
        is_agent,
        adapters: prof.adapters,
        reasoning: prof.reasoning_budget,
    })
}

struct TurnOutcome {
    reply: String,
    had_tools: bool,
    declined: bool,
    cancelled: bool,
    errored: bool,
}

#[allow(clippy::too_many_arguments)]
async fn run_agent_round(
    sid: &str,
    workspace: &Path,
    boundary: &SecurityBoundary,
    adapters: bool,
    is_agent: bool,
    client_ref: &Arc<LlmClient>,
    reasoning: Option<ReasoningSetting>,
    tools_schema: Option<Value>,
    history: &mut Vec<ChatMessage>,
    cancel_flag: &Arc<AtomicBool>,
    pending_perms: &Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    rpc_id_gen: &Arc<AtomicU64>,
) -> TurnOutcome {
    if cancel_flag.load(Ordering::SeqCst) {
        send_acp_chunk(sid, "\n\n*(Cancelled)*", false);
        return TurnOutcome { reply: String::new(), had_tools: false, declined: false, cancelled: true, errored: false };
    }

    let sid_clone = sid.to_string();
    let stream_res = client_ref
        .stream_chat_raw_with_reasoning(
            history,
            tools_schema,
            Some(cancel_flag.clone()),
            Some(reasoning),
            move |chunk, is_thinking| {
                send_acp_chunk(&sid_clone, chunk, is_thinking);
            },
        )
        .await;

    match stream_res {
        Ok((reply, reasoning_text, tool_calls, _stats)) => {
            if cancel_flag.load(Ordering::SeqCst) {
                send_acp_chunk(sid, "\n\n*(Cancelled)*", false);
                return TurnOutcome { reply, had_tools: false, declined: false, cancelled: true, errored: false };
            }

            let Some(calls) = tool_calls.filter(|c| !c.is_empty() && is_agent) else {
                history.push(ChatMessage::assistant_with_reasoning(reply.clone(), reasoning_text));
                return TurnOutcome { reply, had_tools: false, declined: false, cancelled: false, errored: false };
            };

            let mut assistant_msg = ChatMessage::assistant_with_reasoning(reply.clone(), reasoning_text);
            assistant_msg.tool_calls = Some(Value::Array(calls.clone()));
            history.push(assistant_msg);

            let mut any_declined = false;
            let mut idx = 0;

            while idx < calls.len() {
                if cancel_flag.load(Ordering::SeqCst) {
                    for remaining_call in &calls[idx..] {
                        let rem_id = remaining_call["id"].as_str().unwrap_or("call_0").to_string();
                        history.push(ChatMessage::tool_response(rem_id, "[Tool action cancelled by user]".to_string()));
                    }
                    return TurnOutcome { reply, had_tools: true, declined: false, cancelled: true, errored: false };
                }

                let fn_name = calls[idx]["function"]["name"].as_str().unwrap_or("tool");
                if tools::is_read_only_tool(fn_name) {
                    let mut batch_end = idx + 1;
                    while batch_end < calls.len()
                        && tools::is_read_only_tool(calls[batch_end]["function"]["name"].as_str().unwrap_or("tool"))
                    {
                        batch_end += 1;
                    }

                    let mut futs = Vec::with_capacity(batch_end - idx);
                    for call in &calls[idx..batch_end] {
                        let c_id = call["id"].as_str().unwrap_or("call_0").to_string();
                        let c_name = call["function"]["name"].as_str().unwrap_or("tool").to_string();
                        let raw_a = call["function"]["arguments"].as_str().unwrap_or("{}");
                        let c_args =
                            if adapters { adapter::repair_tool_args(&c_name, raw_a) } else { raw_a.to_string() };
                        send_acp_chunk(sid, &format!("\n\n*Running `{}`...*\n", c_name), false);
                        futs.push(async move {
                            let res = tools::execute_tool_with_auth(&c_name, &c_args, workspace, boundary, false).await;
                            (c_id, res)
                        });
                    }

                    for (c_id, tool_res) in futures_util::future::join_all(futs).await {
                        let out = tool_res.unwrap_or_else(|e| format!("[tool error] {}", e));
                        history.push(ChatMessage::tool_response(c_id, out));
                    }
                    idx = batch_end;
                    continue;
                }

                let call = &calls[idx];
                let call_id = call["id"].as_str().unwrap_or("call_0").to_string();
                let fn_args = call["function"]["arguments"].as_str().unwrap_or("{}");
                let final_args =
                    if adapters { adapter::repair_tool_args(fn_name, fn_args) } else { fn_args.to_string() };

                let is_mutating = fn_name == "write_file" || fn_name == "edit_file" || fn_name == "run_command";
                let mut preauthorized_cmd = false;

                if is_mutating {
                    let kind = match fn_name {
                        "write_file" | "edit_file" => "edit",
                        "run_command" => "execute",
                        _ => "other",
                    };

                    let raw_input: Value = serde_json::from_str(&final_args).unwrap_or_else(|_| json!({}));
                    let brief = raw_input
                        .get("path")
                        .or_else(|| raw_input.get("command"))
                        .or_else(|| raw_input.get("pattern"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let cmd_gate = if fn_name == "run_command" {
                        raw_input.get("command").and_then(|v| v.as_str()).and_then(|c| boundary.check_command(c))
                    } else {
                        None
                    };

                    let mut title =
                        if brief.is_empty() { fn_name.to_string() } else { format!("{} {}", fn_name, brief) };
                    if let Some(reason) = cmd_gate {
                        title = format!("⚠ SECURITY GATE ({}): {}", reason, title);
                    }

                    let mut content_blocks = Vec::new();
                    if fn_name == "edit_file" {
                        if let (Some(p), Some(old_s), Some(new_s)) = (
                            raw_input.get("path").and_then(|v| v.as_str()),
                            raw_input.get("old_str").and_then(|v| v.as_str()),
                            raw_input.get("new_str").and_then(|v| v.as_str()),
                        ) {
                            if let Some((old_text, new_text)) = generate_edit_diff(boundary, p, old_s, new_s) {
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
                            let (old_text, new_text) = generate_write_diff(boundary, p, content_str);
                            content_blocks.push(json!({
                                "type": "diff",
                                "path": p,
                                "oldText": old_text,
                                "newText": new_text
                            }));
                        }
                    } else if fn_name == "run_command" {
                        if let Some(cmd_str) = raw_input.get("command").and_then(|v| v.as_str()) {
                            let text = if let Some(reason) = cmd_gate {
                                format!("[SECURITY GATE: {}]\n$ {}", reason, cmd_str)
                            } else {
                                format!("$ {}", cmd_str)
                            };
                            content_blocks.push(json!({ "type": "text", "text": text }));
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

                    let perm_res = tokio::select! {
                        biased;
                        _ = async {
                            while !cancel_flag.load(Ordering::Relaxed) {
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            }
                        } => {
                            pending_perms.lock().await.remove(&req_id);
                            json!({ "result": { "outcome": { "outcome": "cancelled" } } })
                        }
                        res = rx => {
                            pending_perms.lock().await.remove(&req_id);
                            res.unwrap_or_else(|_| json!({}))
                        }
                    };

                    if !is_permission_approved(&perm_res) {
                        send_acp_chunk(sid, &format!("\n\n*Action `{}` was declined by user.*\n", fn_name), false);
                        history.push(ChatMessage::tool_response(
                            call_id,
                            "User declined permission for this tool action. Do not retry this action; ask the user what to do next.".to_string(),
                        ));
                        any_declined = true;

                        for remaining_call in &calls[idx + 1..] {
                            let rem_id = remaining_call["id"].as_str().unwrap_or("call_0").to_string();
                            history.push(ChatMessage::tool_response(
                                rem_id,
                                "[Tool action skipped: preceding action declined by user]".to_string(),
                            ));
                        }
                        break;
                    }

                    if fn_name == "run_command" {
                        preauthorized_cmd = true;
                    }
                }

                send_acp_chunk(sid, &format!("\n\n*Running `{}`...*\n", fn_name), false);

                let tool_res =
                    tools::execute_tool_with_auth(fn_name, &final_args, workspace, boundary, preauthorized_cmd).await;
                let out = match tool_res {
                    Ok(o) => o,
                    Err(e) => format!("[tool error] {}", e),
                };
                history.push(ChatMessage::tool_response(call_id, out));
                idx += 1;
            }

            TurnOutcome { reply, had_tools: true, declined: any_declined, cancelled: false, errored: false }
        }
        Err(e) => {
            send_acp_chunk(sid, &format!("\n\n*(Notice: {})*\n", e), false);
            TurnOutcome { reply: String::new(), had_tools: false, declined: false, cancelled: false, errored: true }
        }
    }
}

pub async fn run_bridge() -> Result<()> {
    let sessions: Arc<Mutex<HashMap<String, SessionState>>> = Arc::new(Mutex::new(HashMap::new()));
    let pending_permissions: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_rpc_id = Arc::new(AtomicU64::new(1000));

    let provider = ProviderConfig::load().await;
    let is_local = crate::client::is_local_endpoint(&provider.base_url);
    let is_google =
        provider.base_url.contains("googleapis.com") || provider.model.to_lowercase().contains("gemini");
    let provider_model = provider.model.clone();
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
                let sid = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("rsagent-{}", std::process::id()));

                active_session_id = sid.clone();

                let cwd = params
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

                let setup = match assemble_session_setup(&cwd).await {
                    Ok(v) => v,
                    Err(e) => {
                        send_rpc(req_id, None, Some(json!({ "code": -32603, "message": e.to_string() })));
                        continue;
                    }
                };
                let history = vec![ChatMessage::system(setup.sys_prompt)];

                let initial_val = match &setup.reasoning {
                    Some(rs) if is_local => rs.to_local_tokens().to_string(),
                    Some(rs) => rs.to_cloud_tier(is_google, &provider_model),
                    None => "-1".to_string(),
                };

                client.set_reasoning(setup.reasoning.clone());

                sessions.lock().await.insert(
                    sid.clone(),
                    SessionState {
                        workspace: cwd.clone(),
                        history,
                        active_tools: setup.active_tools,
                        is_agent: setup.is_agent,
                        adapters: setup.adapters,
                        reasoning: setup.reasoning,
                        cancel_flag: Arc::new(AtomicBool::new(false)),
                    },
                );

                let available_commands = get_available_commands();

                send_rpc(
                    req_id,
                    Some(json!({
                        "sessionId": sid,
                        "configOptions": get_config_options(&initial_val, is_local),
                        "availableCommands": available_commands
                    })),
                    None,
                );

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
                let sid = params.get("sessionId").and_then(|v| v.as_str()).unwrap_or(&active_session_id);
                let config_id = params.get("configId").and_then(|v| v.as_str()).unwrap_or("");
                let value_str = params
                    .get("value")
                    .and_then(|v| {
                        v.as_str()
                            .map(|s| s.to_string())
                            .or_else(|| v.get("value").and_then(|v2| v2.as_str()).map(|s| s.to_string()))
                    })
                    .unwrap_or_else(|| "0".to_string());

                if config_id == "reasoning_budget" || config_id == "thought_level" {
                    let parsed = if value_str == "-1" { None } else { ReasoningSetting::parse(&value_str) };
                    client.set_reasoning(parsed.clone());
                    if let Some(sess) = sessions.lock().await.get_mut(sid) {
                        sess.reasoning = parsed;
                    }
                }

                send_rpc(
                    req_id,
                    Some(json!({
                        "configOptions": get_config_options(&value_str, is_local)
                    })),
                    None,
                );
            }
            "session/list" | "listSessions" => {
                let sess_lock = sessions.lock().await;
                let list: Vec<Value> = sess_lock
                    .iter()
                    .map(|(k, v)| json!({ "sessionId": k, "cwd": v.workspace.display().to_string() }))
                    .collect();
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
                                    if !raw_user_text.is_empty() && !raw_user_text.ends_with([' ', '\n']) {
                                        raw_user_text.push(' ');
                                    }
                                    raw_user_text.push_str(t);
                                }
                            }
                            Some("resource") => {
                                if let Some(res) = item.get("resource") {
                                    let uri = res.get("uri").and_then(|v| v.as_str()).unwrap_or("resource");
                                    let text = res.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                    raw_user_text
                                        .push_str(&format!("\n\n---\nContext from @{}:\n{}\n---\n", uri, text));
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

                let pre_clean = sanitize_t3_prompt(&raw_user_text);

                if pre_clean == "/reset" || pre_clean == "/c" || pre_clean == "/clear" {
                    let ws_opt = {
                        let lock = sessions.lock().await;
                        lock.get(&sid).map(|s| s.workspace.clone())
                    };

                    if let Some(ws) = ws_opt {
                        if let Ok(setup) = assemble_session_setup(&ws).await {
                            let mut lock = sessions.lock().await;
                            if let Some(sess) = lock.get_mut(&sid) {
                                sess.history = vec![ChatMessage::system(setup.sys_prompt)];
                                sess.active_tools = setup.active_tools;
                                sess.is_agent = setup.is_agent;
                                sess.adapters = setup.adapters;
                            }
                        }
                    }

                    send_acp_chunk(&sid, "✓ Session history reset.", false);
                    send_rpc(req_id, Some(json!({ "stopReason": "end_turn" })), None);
                    continue;
                }

                let (mut cleaned_user_text, found_skills) = strip_skills_preserve_layout(&pre_clean);

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

                if cleaned_user_text.is_empty() && image_parts.is_empty() && !found_skills.is_empty() {
                    let names: Vec<String> = found_skills.iter().map(|s| format!("${}", s.name)).collect();
                    send_acp_chunk(
                        &sid,
                        &format!(
                            "✓ Skill active: {}. Directives attached to system context for subsequent turns.",
                            names.join(", ")
                        ),
                        false,
                    );
                    send_rpc(req_id, Some(json!({ "stopReason": "end_turn" })), None);
                    continue;
                }

                if cleaned_user_text.is_empty() && image_parts.is_empty() {
                    send_rpc(req_id, Some(json!({ "stopReason": "end_turn" })), None);
                    continue;
                }

                let is_task_cmd = cleaned_user_text == "/task"
                    || cleaned_user_text == "/goal"
                    || cleaned_user_text.starts_with("/task ")
                    || cleaned_user_text.starts_with("/goal ");

                let client_ref = client.clone();
                let sessions_ref = sessions.clone();
                let req_id_owned = req_id.cloned();
                let pending_perms = pending_permissions.clone();
                let rpc_id_gen = next_rpc_id.clone();

                tokio::spawn(async move {
                    let (workspace, mut history, tools_schema, is_agent, adapters, sess_reasoning, cancel_flag) = {
                        let mut lock = sessions_ref.lock().await;
                        let Some(sess) = lock.get_mut(&sid) else {
                            send_rpc(req_id_owned.as_ref(), Some(json!({ "stopReason": "end_turn" })), None);
                            return;
                        };

                        if is_task_cmd {
                            if !sess.is_agent {
                                send_acp_chunk(
                                    &sid,
                                    "`/task` is only available in workspace agents (`ai init <path>`).",
                                    false,
                                );
                                send_rpc(req_id_owned.as_ref(), Some(json!({ "stopReason": "end_turn" })), None);
                                return;
                            }
                            let direct_arg = cleaned_user_text
                                .strip_prefix("/task")
                                .or_else(|| cleaned_user_text.strip_prefix("/goal"))
                                .unwrap_or("")
                                .trim();
                            let effective = if !direct_arg.is_empty() {
                                Some(direct_arg.to_string())
                            } else {
                                tools::read_task_spec(&sess.workspace)
                            };
                            let Some(goal) = effective else {
                                send_acp_chunk(
                                    &sid,
                                    "No objective provided and no `TASK.md` found. Usage: `/task <goal>`",
                                    false,
                                );
                                send_rpc(req_id_owned.as_ref(), Some(json!({ "stopReason": "end_turn" })), None);
                                return;
                            };
                            cleaned_user_text = goal;
                        }

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
                            sess.adapters,
                            sess.reasoning.clone(),
                            sess.cancel_flag.clone(),
                        )
                    };

                    let boundary = SecurityBoundary::new_headless(&workspace);
                    let max_rounds = if is_agent { config::get_max_rounds() } else { 1 };

                    for turn in 0..max_rounds {
                        if is_task_cmd {
                            send_acp_chunk(&sid, &format!("\n\n---\n**Step {}/{}**\n\n", turn + 1, max_rounds), false);
                        }

                        let outcome = run_agent_round(
                            &sid,
                            &workspace,
                            &boundary,
                            adapters,
                            is_agent,
                            &client_ref,
                            sess_reasoning.clone(),
                            tools_schema.clone(),
                            &mut history,
                            &cancel_flag,
                            &pending_perms,
                            &rpc_id_gen,
                        )
                        .await;

                        if outcome.errored || outcome.cancelled || outcome.declined {
                            break;
                        }

                        if !outcome.had_tools {
                            if !is_task_cmd {
                                break;
                            }
                            if tools::is_task_complete(&outcome.reply) {
                                send_acp_chunk(&sid, &format!("\n\n✓ **Task complete in {} step(s).**", turn + 1), false);
                                break;
                            }
                            history.push(ChatMessage::user("Continue with the next action."));
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
                let sid = params.get("sessionId").and_then(|v| v.as_str()).unwrap_or(&active_session_id);
                let sess_lock = sessions.lock().await;
                let schema = sess_lock.get(sid).and_then(|s| s.active_tools.clone()).unwrap_or_else(|| json!([]));
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
