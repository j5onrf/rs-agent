use anyhow::Result;
use rs_agent::client::{ChatMessage, LlmClient};
use rs_agent::config::{self, ProviderConfig, WorkspaceConfig};
use rs_agent::profile::Profile;
use rs_agent::security::SecurityBoundary;
use rs_agent::tools;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

struct SessionState {
    workspace: PathBuf,
    history: Vec<ChatMessage>,
    active_tools: Option<Value>,
    cancel_flag: Arc<AtomicBool>,
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
    let serialized = serde_json::to_string(&payload).unwrap_or_default();
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", serialized);
    let _ = stdout.flush();
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
    let serialized = serde_json::to_string(&payload).unwrap_or_default();
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", serialized);
    let _ = stdout.flush();
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

    let active_tools = Some(tools::get_tools_schema(&cfg.tier));
    (prompt, active_tools)
}

#[tokio::main]
async fn main() -> Result<()> {
    let sessions: Arc<Mutex<HashMap<String, SessionState>>> = Arc::new(Mutex::new(HashMap::new()));
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
                                "resume": {}
                            }
                        },
                        "agentInfo": {
                            "name": "rs-agent",
                            "version": "0.4.0"
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
                    workspace: cwd,
                    history,
                    active_tools,
                    cancel_flag: Arc::new(AtomicBool::new(false)),
                });

                send_rpc(req_id, Some(json!({ "sessionId": sid })), None);
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
                let prompt_items = params.get("prompt").and_then(|v| v.as_array()).cloned().unwrap_or_default();

                let mut user_text = String::new();
                for item in prompt_items {
                    if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            user_text.push_str(t);
                            user_text.push(' ');
                        }
                    }
                }

                let client_ref = client.clone();
                let sessions_ref = sessions.clone();
                let req_id_owned = req_id.cloned();

                tokio::spawn(async move {
                    let (workspace, mut history, tools_schema, cancel_flag) = {
                        let mut lock = sessions_ref.lock().await;
                        if let Some(sess) = lock.get_mut(&sid) {
                            sess.cancel_flag.store(false, Ordering::SeqCst);
                            sess.history.push(ChatMessage::user(user_text.trim()));
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

                    // Save history back to session state
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
