use anyhow::{Context, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::config::ProviderConfig;
use crate::profile::ReasoningSetting;
use crate::spinner::InlineSpinner;
use crate::stream::{StreamRenderer, UsageStats};

const STREAM_IDLE_TIMEOUT_SECS: u64 = 120;
const CONNECT_TIMEOUT_SECS: u64 = 15;

pub enum StreamEvent<'a> {
    Chunk(&'a str, bool),
    ToolStart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Value::String(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Value::String(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: Value::String(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_with_reasoning(content: impl Into<String>, reasoning: Option<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: Value::String(content.into()),
            reasoning_content: reasoning,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool_response(tool_call_id: String, content: String) -> Self {
        Self {
            role: "tool".into(),
            content: Value::String(content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        }
    }

    pub fn content_as_str(&self) -> &str {
        self.content.as_str().unwrap_or("")
    }
}

pub struct LlmClient {
    reasoning: std::sync::Mutex<Option<ReasoningSetting>>,
    http: Client,
    config: ProviderConfig,
}

#[inline]
pub fn is_local_endpoint(url: &str) -> bool {
    let u = url.to_lowercase();
    u.contains("127.0.0.1")
        || u.contains("localhost")
        || u.contains("0.0.0.0")
        || u.contains("::1")
        || u.contains(":8080")
        || u.contains(":11434")
        || u.contains(":20128")
        || u.contains(".local")
}

/// Redacts query parameters that could leak credentials in error telemetry.
fn redact_url(url: &str) -> String {
    if let Some((base, query)) = url.split_once('?') {
        let clean: Vec<String> = query
            .split('&')
            .map(|pair| {
                if let Some((k, _)) = pair.split_once('=') {
                    if k.eq_ignore_ascii_case("key")
                        || k.eq_ignore_ascii_case("api_key")
                        || k.eq_ignore_ascii_case("token")
                        || k.eq_ignore_ascii_case("access_token")
                    {
                        return format!("{}=[REDACTED]", k);
                    }
                }
                pair.to_string()
            })
            .collect();
        format!("{}?{}", base, clean.join("&"))
    } else {
        url.to_string()
    }
}

/// Normalizes error details from JSON payloads into clamped, actionable messages.
fn format_http_error(status: reqwest::StatusCode, body_text: &str, model: &str, url: &str) -> String {
    let clean_url = redact_url(url);
    let extracted = serde_json::from_str::<Value>(body_text).ok().and_then(|v| {
        if let Some(err) = v.get("error") {
            if let Some(s) = err.as_str() {
                return Some(s.to_string());
            }
            if let Some(m) = err.get("message").and_then(|m| m.as_str()) {
                return Some(m.to_string());
            }
            if let Some(arr) = err.as_array() {
                let msgs: Vec<&str> = arr
                    .iter()
                    .filter_map(|e| e.get("message").and_then(|m| m.as_str()).or_else(|| e.as_str()))
                    .collect();
                if !msgs.is_empty() {
                    return Some(msgs.join("; "));
                }
            }
        }
        if let Some(arr) = v.get("errors").and_then(|e| e.as_array()) {
            let msgs: Vec<&str> =
                arr.iter().filter_map(|e| e.get("message").and_then(|m| m.as_str()).or_else(|| e.as_str())).collect();
            if !msgs.is_empty() {
                return Some(msgs.join("; "));
            }
        }
        v.get("message")
            .and_then(|m| m.as_str())
            .or_else(|| v.get("detail").and_then(|d| d.as_str()))
            .map(ToString::to_string)
    });

    let raw_detail = extracted.unwrap_or_else(|| {
        let t = body_text.trim();
        if t.is_empty() {
            "Empty response body".into()
        } else {
            t.to_string()
        }
    });
    let detail: String = raw_detail.chars().take(400).collect();

    match status.as_u16() {
        400 => {
            format!("HTTP 400 Bad Request from '{}': {}\nCheck model parameters or message schema.", clean_url, detail)
        }
        401 => format!("HTTP 401 Unauthorized from '{}': Invalid or missing API key.\nDetail: {}", clean_url, detail),
        403 => {
            format!("HTTP 403 Forbidden from '{}': Access denied for model '{}'.\nDetail: {}", clean_url, model, detail)
        }
        404 => format!(
            "HTTP 404 Not Found from '{}': Model '{}' or route does not exist.\nDetail: {}",
            clean_url, model, detail
        ),
        429 => format!(
            "HTTP 429 Too Many Requests from '{}': Rate limit or quota exhausted.\nDetail: {}",
            clean_url, detail
        ),
        500 => format!("HTTP 500 Internal Server Error from '{}':\nDetail: {}", clean_url, detail),
        502..=504 => format!(
            "HTTP {} Server Unavailable from '{}': Is the endpoint running?\nDetail: {}",
            status.as_u16(),
            clean_url,
            detail
        ),
        code => format!("HTTP {} Error from '{}': {}", code, clean_url, detail),
    }
}

/// Enforces tool pairing: prunes unmatched/orphaned tool responses and synthesizes cancelled responses for missing calls.
fn sanitize_tool_pairing(messages: &mut Vec<Value>) {
    let mut i = 0;
    while i < messages.len() {
        let role = messages[i].get("role").and_then(|r| r.as_str()).unwrap_or("").to_string();

        if role == "assistant" {
            let has_tool_calls = messages[i]
                .get_mut("tool_calls")
                .and_then(|tc| tc.as_array_mut())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false);

            if !has_tool_calls {
                if let Some(obj) = messages[i].as_object_mut() {
                    obj.remove("tool_calls");
                    if obj.get("content").map(|c| c.is_null()).unwrap_or(true) {
                        obj.insert("content".to_string(), Value::String(String::new()));
                    }
                }
                i += 1;
                continue;
            }

            let tool_calls = messages[i]["tool_calls"].as_array().cloned().unwrap_or_default();
            let mut expected_ids: Vec<String> = tool_calls
                .iter()
                .filter_map(|tc| tc.get("id").and_then(|id| id.as_str()).map(ToString::to_string))
                .collect();

            let mut j = i + 1;
            while j < messages.len() {
                if messages[j].get("role").and_then(|r| r.as_str()) == Some("tool") {
                    let call_id = messages[j].get("tool_call_id").and_then(|id| id.as_str()).unwrap_or("");
                    if expected_ids.iter().any(|id| id == call_id) {
                        expected_ids.retain(|id| id != call_id);
                        if let Some(obj) = messages[j].as_object_mut() {
                            if obj.get("content").map(|c| c.is_null()).unwrap_or(true) {
                                obj.insert("content".to_string(), Value::String(String::new()));
                            }
                        }
                        j += 1;
                    } else {
                        // Prune unmatched tool responses to prevent OpenAI/Gemini 400 rejection
                        messages.remove(j);
                    }
                } else {
                    break;
                }
            }

            for missing_id in expected_ids {
                messages.insert(
                    j,
                    serde_json::json!({
                        "role": "tool",
                        "tool_call_id": missing_id,
                        "content": "[Tool action cancelled or interrupted]"
                    }),
                );
                j += 1;
            }

            i = j;
        } else if role == "tool" {
            messages.remove(i);
        } else {
            if let Some(obj) = messages[i].as_object_mut() {
                if obj.get("content").map(|c| c.is_null()).unwrap_or(true) {
                    obj.insert("content".to_string(), Value::String(String::new()));
                }
            }
            i += 1;
        }
    }
}

#[inline]
fn parse_usage_and_timings(json: &Value, stats: &mut UsageStats) {
    if let Some(usage) = json.get("usage") {
        if let Some(p) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
            stats.prompt_tokens = p as usize;
        }
        if let Some(c) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
            stats.completion_tokens = c as usize;
        }
        if let Some(cached) = usage
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_u64())
            .or_else(|| usage.get("cached_tokens").and_then(|v| v.as_u64()))
        {
            stats.cached_tokens = cached as usize;
        }
        if let Some(reasoning) =
            usage.get("completion_tokens_details").and_then(|d| d.get("reasoning_tokens")).and_then(|v| v.as_u64())
        {
            stats.reasoning_tokens = reasoning as usize;
        }
    }

    if let Some(timings) = json.get("timings") {
        if let Some(cache_n) = timings.get("cache_n").and_then(|v| v.as_u64()) {
            if stats.cached_tokens == 0 {
                stats.cached_tokens = cache_n as usize;
            }
        }
    }
}

#[inline]
fn accumulate_tool_calls(delta: &Value, tool_calls_map: &mut BTreeMap<usize, (String, String, String, Option<Value>)>) {
    if let Some(tc_array) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tc_array {
            let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let entry = tool_calls_map.entry(idx).or_insert_with(|| {
                (
                    tc.get("id")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string)
                        .unwrap_or_else(|| format!("call_{idx}")),
                    String::new(),
                    String::new(),
                    None,
                )
            });

            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                entry.0 = id.to_string();
            }
            if let Some(fn_obj) = tc.get("function") {
                if let Some(name) = fn_obj.get("name").and_then(|v| v.as_str()) {
                    entry.1 = name.to_string();
                }
                if let Some(args) = fn_obj.get("arguments").and_then(|v| v.as_str()) {
                    entry.2.push_str(args);
                }
            }
            if let Some(ec) = tc.get("extra_content") {
                entry.3 = Some(ec.clone());
            } else if let Some(ec) = delta.get("extra_content") {
                entry.3 = Some(ec.clone());
            }
        }
    }
}

#[inline]
fn extract_stream_error(json: &Value) -> Option<String> {
    if let Some(err) = json.get("error") {
        if let Some(s) = err.as_str() {
            return Some(s.to_string());
        }
        if let Some(m) = err.get("message").and_then(|m| m.as_str()) {
            return Some(m.to_string());
        }
    }
    if let Some(arr) = json.get("errors").and_then(|e| e.as_array()) {
        let msgs: Vec<&str> =
            arr.iter().filter_map(|e| e.get("message").and_then(|m| m.as_str()).or_else(|| e.as_str())).collect();
        if !msgs.is_empty() {
            return Some(msgs.join("; "));
        }
    }
    None
}

impl LlmClient {
    pub fn set_reasoning(&self, setting: Option<ReasoningSetting>) {
        if let Ok(mut guard) = self.reasoning.lock() {
            *guard = setting;
        }
    }

    fn current_reasoning(&self) -> Option<ReasoningSetting> {
        self.reasoning.lock().ok().and_then(|g| g.clone())
    }

    pub fn new(config: ProviderConfig) -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .tcp_keepalive(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_default();

        Self {
            reasoning: std::sync::Mutex::new(None),
            http,
            config,
        }
    }

    #[cfg(test)]
    fn build_request_body(&self, messages: &[ChatMessage], tools: Option<Value>) -> Value {
        self.build_request_body_with_reasoning(messages, tools, None)
    }

    fn build_request_body_with_reasoning(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        reasoning_override: Option<Option<ReasoningSetting>>,
    ) -> Value {
        let active_reasoning = reasoning_override.unwrap_or_else(|| self.current_reasoning());
        let is_local = is_local_endpoint(&self.config.base_url);
        let is_google =
            self.config.base_url.contains("googleapis.com") || self.config.model.to_lowercase().contains("gemini");

        let mut sanitized_messages = serde_json::to_value(messages).unwrap_or_else(|_| serde_json::json!([]));

        if let Some(arr) = sanitized_messages.as_array_mut() {
            sanitize_tool_pairing(arr);
        }

        if is_google {
            if let Some(arr) = sanitized_messages.as_array_mut() {
                for msg in arr {
                    if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                        if let Some(tcs) = msg.get_mut("tool_calls").and_then(|v| v.as_array_mut()) {
                            // Gemini 4 parallel tool turns carry thought_signature only on the first call
                            let first_has_sig = tcs
                                .first()
                                .and_then(|tc| tc.get("extra_content"))
                                .and_then(|ec| ec.get("google"))
                                .and_then(|g| g.get("thought_signature"))
                                .and_then(|s| s.as_str())
                                .map(|s| !s.is_empty())
                                .unwrap_or(false);

                            if !first_has_sig {
                                if let Some(first_obj) = tcs.first_mut().and_then(|tc| tc.as_object_mut()) {
                                    first_obj.insert(
                                        "extra_content".to_string(),
                                        serde_json::json!({
                                            "google": {
                                                "thought_signature": "skip_thought_signature_validator"
                                            }
                                        }),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": sanitized_messages,
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        if let Some(tool_specs) = tools {
            body["tools"] = tool_specs;
        }

        if is_local {
            body["cache_prompt"] = serde_json::json!(true);

            if let Some(setting) = active_reasoning {
                let rb = setting.to_local_tokens();
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("reasoning_budget".to_string(), serde_json::json!(rb));
                    obj.insert("reasoning_budget_tokens".to_string(), serde_json::json!(rb));
                    obj.insert("thinking_budget_tokens".to_string(), serde_json::json!(rb));
                    if rb == 0 {
                        obj.insert("chat_template_kwargs".to_string(), serde_json::json!({ "enable_thinking": false }));
                    }
                }
            }
        } else if let Some(setting) = active_reasoning {
            let effort = setting.to_cloud_tier(is_google, &self.config.model);
            body["reasoning_effort"] = serde_json::json!(effort);
        }

        body
    }

    fn prepare_request(&self, body: &Value) -> reqwest::RequestBuilder {
        let mut req = self.http.post(&self.config.base_url).header("Content-Type", "application/json").json(body);

        if self.config.api_key != "not-needed" && !self.config.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.config.api_key));
        }

        if self.config.base_url.contains("openrouter.ai") {
            req = req.header("HTTP-Referer", "https://github.com/j5onrf/rs-agent");
            req = req.header("X-Title", "rs-agent");
        }

        req
    }

    /// Single unified streaming engine for both interactive chat and ACP bridge.
    async fn execute_stream<F>(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        cancel_flag: Option<Arc<AtomicBool>>,
        reasoning_override: Option<Option<ReasoningSetting>>,
        mut on_event: F,
    ) -> Result<(String, Option<String>, Option<Vec<Value>>, UsageStats)>
    where
        F: FnMut(StreamEvent<'_>) + Send,
    {
        let body = self.build_request_body_with_reasoning(messages, tools, reasoning_override);
        let req = self.prepare_request(&body);

        let send_res = tokio::time::timeout(Duration::from_secs(STREAM_IDLE_TIMEOUT_SECS), req.send()).await;
        let response = match send_res {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => return Err(e).context("Failed to connect to LLM endpoint"),
            Err(_) => {
                anyhow::bail!("Connection timeout waiting for response from '{}'", redact_url(&self.config.base_url));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            let err_msg = format_http_error(status, &err_body, &self.config.model, &self.config.base_url);
            anyhow::bail!("{}", err_msg);
        }

        let mut stream = response.bytes_stream().eventsource();
        let mut full_response = String::new();
        let mut full_reasoning = String::new();
        let mut think_parser = ThinkTagParser::new();
        let mut saw_done = false;
        let mut saw_finish = false;
        let mut stats = UsageStats::default();
        let mut tool_calls_map: BTreeMap<usize, (String, String, String, Option<Value>)> = BTreeMap::new();
        let mut stream_err: Option<anyhow::Error> = None;

        while let Some(event_res) = {
            tokio::select! {
                biased;
                _ = async {
                    if let Some(ref flag) = cancel_flag {
                        while !flag.load(Ordering::Relaxed) {
                            tokio::time::sleep(Duration::from_millis(30)).await;
                        }
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    None
                }
                next_opt = tokio::time::timeout(Duration::from_secs(STREAM_IDLE_TIMEOUT_SECS), stream.next()) => {
                    match next_opt {
                        Ok(Some(ev)) => Some(ev),
                        Ok(None) => None,
                        Err(_) => {
                            stream_err = Some(anyhow::anyhow!(
                                "Stream idle timeout: no data received from '{}' for {}s",
                                redact_url(&self.config.base_url),
                                STREAM_IDLE_TIMEOUT_SECS
                            ));
                            None
                        }
                    }
                }
            }
        } {
            let event = match event_res {
                Ok(ev) => ev,
                Err(e) => {
                    stream_err = Some(anyhow::anyhow!("Error reading SSE event: {}", e));
                    break;
                }
            };

            if event.data == "[DONE]" {
                saw_done = true;
                break;
            }

            if let Ok(json) = serde_json::from_str::<Value>(&event.data) {
                if let Some(err_msg) = extract_stream_error(&json) {
                    stream_err = Some(anyhow::anyhow!("Stream error from LLM endpoint: {}", err_msg));
                    break;
                }

                parse_usage_and_timings(&json, &mut stats);

                if let Some(choice) = json["choices"].get(0) {
                    if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                        saw_finish = true;
                        if fr == "length" {
                            eprintln!("\n[Notice: LLM generation reached token limit]");
                        }
                    }

                    if let Some(delta) = choice.get("delta") {
                        if let Some(reasoning) =
                            delta.get("reasoning_content").or_else(|| delta.get("thinking")).and_then(|r| r.as_str())
                        {
                            if !reasoning.is_empty() {
                                on_event(StreamEvent::Chunk(reasoning, true));
                                full_reasoning.push_str(reasoning);
                            }
                        }

                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                            if !content.is_empty() {
                                think_parser.feed(content, |slice, is_thinking| {
                                    if !slice.is_empty() {
                                        on_event(StreamEvent::Chunk(slice, is_thinking));
                                        if is_thinking {
                                            full_reasoning.push_str(slice);
                                        } else {
                                            full_response.push_str(slice);
                                        }
                                    }
                                });
                            }
                        }

                        if delta.get("tool_calls").and_then(|v| v.as_array()).is_some() {
                            on_event(StreamEvent::ToolStart);
                            accumulate_tool_calls(delta, &mut tool_calls_map);
                        }
                    }
                }
            }
        }

        think_parser.flush(|slice, is_thinking| {
            if !slice.is_empty() {
                on_event(StreamEvent::Chunk(slice, is_thinking));
                if is_thinking {
                    full_reasoning.push_str(slice);
                } else {
                    full_response.push_str(slice);
                }
            }
        });

        if let Some(err) = stream_err {
            return Err(err);
        }

        let cancelled = cancel_flag.as_ref().map(|f| f.load(Ordering::Relaxed)).unwrap_or(false);
        if !cancelled && !saw_done && !saw_finish && (!full_response.is_empty() || !tool_calls_map.is_empty()) {
            anyhow::bail!(
                "Stream from '{}' terminated abruptly without completion signal",
                redact_url(&self.config.base_url)
            );
        }

        let tool_calls = if tool_calls_map.is_empty() {
            None
        } else {
            Some(
                tool_calls_map
                    .into_values()
                    .map(|(id, name, args, extra_content)| {
                        let mut call = serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": name, "arguments": args }
                        });
                        if let Some(ec) = extra_content {
                            call["extra_content"] = ec;
                        }
                        call
                    })
                    .collect(),
            )
        };

        let opt_reasoning = if !full_reasoning.is_empty() { Some(full_reasoning) } else { None };
        Ok((full_response, opt_reasoning, tool_calls, stats))
    }

    pub async fn stream_chat_raw_with_reasoning<F>(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        cancel_flag: Option<Arc<AtomicBool>>,
        reasoning_override: Option<Option<ReasoningSetting>>,
        mut on_chunk: F,
    ) -> Result<(String, Option<String>, Option<Vec<Value>>, UsageStats)>
    where
        F: FnMut(&str, bool) + Send,
    {
        self.execute_stream(messages, tools, cancel_flag, reasoning_override, |ev| {
            if let StreamEvent::Chunk(chunk, is_thinking) = ev {
                on_chunk(chunk, is_thinking);
            }
        })
        .await
    }

    pub async fn stream_chat_raw<F>(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        cancel_flag: Option<Arc<AtomicBool>>,
        on_chunk: F,
    ) -> Result<(String, Option<String>, Option<Vec<Value>>, UsageStats)>
    where
        F: FnMut(&str, bool) + Send,
    {
        self.stream_chat_raw_with_reasoning(messages, tools, cancel_flag, None, on_chunk).await
    }

    pub async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        renderer: &mut StreamRenderer,
        spinner: &mut InlineSpinner,
    ) -> Result<(String, Option<String>, Option<Vec<Value>>, UsageStats)> {
        let mut tool_signaled = false;
        let on_event = |ev: StreamEvent<'_>| match ev {
            StreamEvent::Chunk(chunk, is_thinking) => {
                spinner.stop();
                renderer.on_chunk(chunk, is_thinking);
            }
            StreamEvent::ToolStart => {
                if !tool_signaled {
                    renderer.close_thinking();
                    spinner.start("Drafting tool action...");
                    tool_signaled = true;
                }
            }
        };

        let res = self.execute_stream(messages, tools, None, None, on_event).await;
        renderer.close_thinking();
        spinner.stop();
        res
    }
}

struct ThinkTagParser {
    in_thinking: bool,
    pending: String,
}

impl ThinkTagParser {
    fn new() -> Self {
        Self { in_thinking: false, pending: String::new() }
    }

    fn find_earliest<'a>(haystack: &str, tags: &[&'a str]) -> Option<(usize, &'a str)> {
        tags.iter().filter_map(|&t| haystack.find(t).map(|pos| (pos, t))).min_by_key(|(pos, _)| *pos)
    }

    fn partial_suffix_len(haystack: &str, tags: &[&str]) -> usize {
        let max_tag = tags.iter().map(|t| t.len()).max().unwrap_or(0);
        let start = haystack.len().saturating_sub(max_tag.saturating_sub(1));
        for i in start..haystack.len() {
            if !haystack.is_char_boundary(i) {
                continue;
            }
            let suffix = &haystack[i..];
            if tags.iter().any(|t| t.starts_with(suffix)) {
                return suffix.len();
            }
        }
        0
    }

    fn feed<F: FnMut(&str, bool)>(&mut self, chunk: &str, mut emit: F) {
        const OPEN_TAGS: &[&str] = &["<think>", "<thought>"];
        const CLOSE_TAGS: &[&str] = &["</think>", "</thought>"];

        self.pending.push_str(chunk);
        loop {
            let tags = if self.in_thinking { CLOSE_TAGS } else { OPEN_TAGS };
            if let Some((pos, tag)) = Self::find_earliest(&self.pending, tags) {
                if pos > 0 {
                    emit(&self.pending[..pos], self.in_thinking);
                }
                self.pending.drain(..pos + tag.len());
                self.in_thinking = !self.in_thinking;
            } else {
                let keep = Self::partial_suffix_len(&self.pending, tags);
                let emit_len = self.pending.len() - keep;
                if emit_len > 0 {
                    emit(&self.pending[..emit_len], self.in_thinking);
                    self.pending.drain(..emit_len);
                }
                break;
            }
        }
    }

    fn flush<F: FnMut(&str, bool)>(&mut self, mut emit: F) {
        if !self.pending.is_empty() {
            emit(&self.pending, self.in_thinking);
            self.pending.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_tool_pairing_removes_orphans_and_unmatched() {
        let mut messages = vec![
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "t1", "arguments": "{}" } }]
            }),
            serde_json::json!({ "role": "tool", "tool_call_id": "call_unmatched", "content": "err" }),
            serde_json::json!({ "role": "user", "content": "next" }),
        ];

        sanitize_tool_pairing(&mut messages);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[1]["tool_call_id"], "call_1");
        assert_eq!(messages[2]["role"], "user");
    }

    #[test]
    fn test_redact_url_scrubs_keys() {
        let clean = redact_url(
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions?key=AIzaSySecret123&alt=json",
        );
        assert!(!clean.contains("AIzaSySecret123"));
        assert!(clean.contains("key=[REDACTED]"));
        assert!(clean.contains("alt=json"));
    }

    #[test]
    fn test_format_http_error_json_message() {
        let err_json = r#"{"error": {"message": "Invalid API key", "code": "invalid_api_key"}}"#;
        let formatted = format_http_error(reqwest::StatusCode::UNAUTHORIZED, err_json, "gemini-3.5", "https://api.com");
        assert!(formatted.contains("HTTP 401 Unauthorized"));
        assert!(formatted.contains("Invalid API key"));
    }

    #[test]
    fn test_hydrogen_cloud_reasoning_and_parallel_signatures() {
        let cfg = ProviderConfig {
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".into(),
            api_key: "test".into(),
            model: "gemini-4-hydrogen".into(),
            max_tokens: 65536,
        };
        let client = LlmClient::new(cfg);
        client.set_reasoning(Some(ReasoningSetting::Tier("xhigh".into())));

        let mut assistant = ChatMessage::assistant("");
        assistant.tool_calls = Some(serde_json::json!([
            {
                "id": "call_1",
                "type": "function",
                "function": { "name": "read_file", "arguments": "{\"path\":\"a.rs\"}" },
                "extra_content": { "google": { "thought_signature": "sig_abc" } }
            },
            {
                "id": "call_2",
                "type": "function",
                "function": { "name": "read_file", "arguments": "{\"path\":\"b.rs\"}" }
            }
        ]));
        let msgs = vec![
            ChatMessage::user("check both"),
            assistant,
            ChatMessage::tool_response("call_1".into(), "a".into()),
            ChatMessage::tool_response("call_2".into(), "b".into()),
        ];

        let body = client.build_request_body(&msgs, None);
        assert_eq!(body["reasoning_effort"], "xhigh");
        assert!(body.get("reasoning_budget").is_none());
        let tcs = body["messages"][1]["tool_calls"].as_array().unwrap();
        assert_eq!(tcs[0]["extra_content"]["google"]["thought_signature"], "sig_abc");
        assert!(tcs[1].get("extra_content").is_none());

        client.set_reasoning(Some(ReasoningSetting::Tier("none".into())));
        let body_clamped = client.build_request_body(&[ChatMessage::user("hi")], None);
        assert_eq!(body_clamped["reasoning_effort"], "low");
    }

    #[test]
    fn test_think_tag_parser_handles_split_sse_chunks_and_leading_text() {
        let mut parser = ThinkTagParser::new();
        let mut thinking = String::new();
        let mut answer = String::new();
        let mut collect = |s: &str, is_t: bool| {
            if is_t {
                thinking.push_str(s);
            } else {
                answer.push_str(s);
            }
        };

        for part in ["Hi <th", "ink>step 1", "</thi", "nk>Done! a < b"] {
            parser.feed(part, &mut collect);
        }
        parser.flush(&mut collect);

        assert_eq!(thinking, "step 1");
        assert_eq!(answer, "Hi Done! a < b");
    }
}
