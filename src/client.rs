use anyhow::{Context, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::config::ProviderConfig;
use crate::spinner::InlineSpinner;
use crate::stream::{StreamRenderer, UsageStats};

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
    pub reasoning_budget: std::sync::atomic::AtomicI64,
    http: Client,
    config: ProviderConfig,
}

#[inline]
fn is_local_endpoint(url: &str) -> bool {
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

impl LlmClient {
    pub fn set_reasoning_budget(&self, budget: Option<usize>) {
        let val = budget.map(|b| b as i64).unwrap_or(-1);
        self.reasoning_budget.store(val, Ordering::Relaxed);
    }

    pub fn new(config: ProviderConfig) -> Self {
        Self {
            reasoning_budget: std::sync::atomic::AtomicI64::new(-1),
            http: Client::builder().build().unwrap_or_default(),
            config,
        }
    }

    fn build_request_body(&self, messages: &[ChatMessage], tools: Option<Value>) -> Value {
        let is_local = is_local_endpoint(&self.config.base_url);
        let is_google = self.config.base_url.contains("googleapis.com")
            || self.config.model.to_lowercase().contains("gemini");

        let mut sanitized_messages = serde_json::to_value(messages).unwrap_or_else(|_| serde_json::json!([]));

        // Google Gemini strict multi-turn tool verification:
        // Ensure all assistant tool_calls carry either their real thought_signature or the official bypass sentinel
        if is_google {
            if let Some(arr) = sanitized_messages.as_array_mut() {
                for msg in arr {
                    if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                        if let Some(tcs) = msg.get_mut("tool_calls").and_then(|v| v.as_array_mut()) {
                            for tc in tcs {
                                let has_sig = tc.get("extra_content")
                                    .and_then(|ec| ec.get("google"))
                                    .and_then(|g| g.get("thought_signature"))
                                    .and_then(|s| s.as_str())
                                    .map(|s| !s.is_empty())
                                    .unwrap_or(false);

                                if !has_sig {
                                    if let Some(obj) = tc.as_object_mut() {
                                        obj.insert(
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

        // Vendor-specific extensions are only injected for local engines (llama.cpp, vLLM, Ollama)
        if is_local {
            body["cache_prompt"] = serde_json::json!(true);

            let rb = self.reasoning_budget.load(Ordering::Relaxed);
            if rb >= 0 {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("reasoning_budget".to_string(), serde_json::json!(rb));
                    obj.insert("reasoning_budget_tokens".to_string(), serde_json::json!(rb));
                    obj.insert("thinking_budget_tokens".to_string(), serde_json::json!(rb));
                    if rb == 0 {
                        obj.insert("chat_template_kwargs".to_string(), serde_json::json!({ "enable_thinking": false }));
                    }
                }
            }
        }

        body
    }

    fn prepare_request(&self, body: &Value) -> reqwest::RequestBuilder {
        let mut req = self.http.post(&self.config.base_url)
            .header("Content-Type", "application/json")
            .json(body);

        if self.config.api_key != "not-needed" && !self.config.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.config.api_key));
        }

        if self.config.base_url.contains("openrouter.ai") {
            req = req.header("HTTP-Referer", "https://github.com/j5onrf/rs-agent");
            req = req.header("X-Title", "rs-agent");
        }

        req
    }

    pub async fn stream_chat_raw<F>(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        cancel_flag: Option<Arc<AtomicBool>>,
        mut on_chunk: F,
    ) -> Result<(String, Option<String>, Option<Vec<Value>>, UsageStats)>
    where
        F: FnMut(&str, bool) + Send,
    {
        let body = self.build_request_body(messages, tools);
        let req = self.prepare_request(&body);
        let response = req.send().await.context("Failed to send request to LLM endpoint")?;

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            anyhow::bail!("Server returned HTTP {}: {}", status, err_body);
        }

        let mut stream = response.bytes_stream().eventsource();
        let mut full_response = String::new();
        let mut full_reasoning = String::new();
        let mut in_thinking = false;
        let mut stats = UsageStats::default();
        let mut tool_calls_map: std::collections::BTreeMap<usize, (String, String, String, Option<Value>)> = std::collections::BTreeMap::new();

        while let Some(event_res) = stream.next().await {
            if let Some(ref flag) = cancel_flag {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
            }

            let event = event_res.context("Error reading SSE event")?;
            if event.data == "[DONE]" {
                break;
            }

            if let Ok(json) = serde_json::from_str::<Value>(&event.data) {
                if let Some(usage) = json.get("usage") {
                    if let Some(p) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                        stats.prompt_tokens = p as usize;
                    }
                    if let Some(c) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                        stats.completion_tokens = c as usize;
                    }
                    if let Some(cached) = usage.get("prompt_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(|v| v.as_u64())
                        .or_else(|| usage.get("cached_tokens").and_then(|v| v.as_u64()))
                    {
                        stats.cached_tokens = cached as usize;
                    }
                    if let Some(reasoning) = usage.get("completion_tokens_details")
                        .and_then(|d| d.get("reasoning_tokens"))
                        .and_then(|v| v.as_u64())
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

                if let Some(delta) = json["choices"].get(0).and_then(|c| c.get("delta")) {
                    if let Some(reasoning) = delta.get("reasoning_content")
                        .or_else(|| delta.get("thinking"))
                        .and_then(|r| r.as_str())
                    {
                        if !reasoning.is_empty() {
                            on_chunk(reasoning, true);
                            full_reasoning.push_str(reasoning);
                        }
                    }

                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            let mut chunk_slice = content;
                            if chunk_slice.contains("<think>") || chunk_slice.contains("<thought>") {
                                in_thinking = true;
                                chunk_slice = chunk_slice
                                    .trim_start_matches("<think>")
                                    .trim_start_matches("<thought>");
                            }

                            if chunk_slice.contains("</think>") || chunk_slice.contains("</thought>") {
                                let delimiter = if chunk_slice.contains("</think>") { "</think>" } else { "</thought>" };
                                let parts: Vec<&str> = chunk_slice.splitn(2, delimiter).collect();
                                on_chunk(parts[0], true);
                                full_reasoning.push_str(parts[0]);
                                in_thinking = false;
                                if parts.len() > 1 && !parts[1].is_empty() {
                                    on_chunk(parts[1], false);
                                    full_response.push_str(parts[1]);
                                }
                            } else {
                                on_chunk(chunk_slice, in_thinking);
                                if in_thinking {
                                    full_reasoning.push_str(chunk_slice);
                                } else {
                                    full_response.push_str(chunk_slice);
                                }
                            }
                        }
                    }

                    if let Some(tc_array) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tc_array {
                            let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let entry = tool_calls_map.entry(idx).or_insert_with(|| (
                                tc.get("id").and_then(|v| v.as_str()).unwrap_or("call_0").to_string(),
                                String::new(),
                                String::new(),
                                None,
                            ));

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
                            // Preserve incoming extra_content / thought_signature
                            if let Some(ec) = tc.get("extra_content") {
                                entry.3 = Some(ec.clone());
                            } else if let Some(ec) = delta.get("extra_content") {
                                entry.3 = Some(ec.clone());
                            }
                        }
                    }
                }
            }
        }

        let tool_calls = if !tool_calls_map.is_empty() {
            let calls: Vec<Value> = tool_calls_map.into_values().map(|(id, name, args, extra_content)| {
                let mut call = serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args
                    }
                });
                if let Some(ec) = extra_content {
                    call["extra_content"] = ec;
                }
                call
            }).collect();
            Some(calls)
        } else {
            None
        };

        let opt_reasoning = if !full_reasoning.is_empty() {
            Some(full_reasoning)
        } else {
            None
        };

        Ok((full_response, opt_reasoning, tool_calls, stats))
    }

    pub async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        renderer: &mut StreamRenderer,
        spinner: &mut InlineSpinner,
    ) -> Result<(String, Option<String>, Option<Vec<Value>>, UsageStats)> {
        let body = self.build_request_body(messages, tools);
        let req = self.prepare_request(&body);
        let response = req.send().await.context("Failed to send request to LLM endpoint")?;

        if !response.status().is_success() {
            spinner.stop();
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            anyhow::bail!("Server returned HTTP {}: {}", status, err_body);
        }

        let mut stream = response.bytes_stream().eventsource();
        let mut full_response = String::new();
        let mut full_reasoning = String::new();
        let mut in_thinking = false;
        let mut stats = UsageStats::default();
        let mut tool_calls_map: std::collections::BTreeMap<usize, (String, String, String, Option<Value>)> = std::collections::BTreeMap::new();

        while let Some(event_res) = stream.next().await {
            let event = event_res.context("Error reading SSE event")?;
            if event.data == "[DONE]" {
                break;
            }

            if let Ok(json) = serde_json::from_str::<Value>(&event.data) {
                if let Some(usage) = json.get("usage") {
                    if let Some(p) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                        stats.prompt_tokens = p as usize;
                    }
                    if let Some(c) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                        stats.completion_tokens = c as usize;
                    }
                    if let Some(cached) = usage.get("prompt_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(|v| v.as_u64())
                        .or_else(|| usage.get("cached_tokens").and_then(|v| v.as_u64()))
                    {
                        stats.cached_tokens = cached as usize;
                    }
                    if let Some(reasoning) = usage.get("completion_tokens_details")
                        .and_then(|d| d.get("reasoning_tokens"))
                        .and_then(|v| v.as_u64())
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

                if let Some(delta) = json["choices"].get(0).and_then(|c| c.get("delta")) {
                    if let Some(reasoning) = delta.get("reasoning_content")
                        .or_else(|| delta.get("thinking"))
                        .and_then(|r| r.as_str())
                    {
                        if !reasoning.is_empty() {
                            spinner.stop();
                            renderer.on_chunk(reasoning, true);
                            full_reasoning.push_str(reasoning);
                        }
                    }

                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            spinner.stop();
                            let mut chunk_slice = content;
                            if chunk_slice.contains("<think>") || chunk_slice.contains("<thought>") {
                                in_thinking = true;
                                chunk_slice = chunk_slice
                                    .trim_start_matches("<think>")
                                    .trim_start_matches("<thought>");
                            }

                            if chunk_slice.contains("</think>") || chunk_slice.contains("</thought>") {
                                let delimiter = if chunk_slice.contains("</think>") { "</think>" } else { "</thought>" };
                                let parts: Vec<&str> = chunk_slice.splitn(2, delimiter).collect();
                                renderer.on_chunk(parts[0], true);
                                full_reasoning.push_str(parts[0]);
                                in_thinking = false;
                                if parts.len() > 1 && !parts[1].is_empty() {
                                    renderer.on_chunk(parts[1], false);
                                    full_response.push_str(parts[1]);
                                }
                            } else {
                                renderer.on_chunk(chunk_slice, in_thinking);
                                if in_thinking {
                                    full_reasoning.push_str(chunk_slice);
                                } else {
                                    full_response.push_str(chunk_slice);
                                }
                            }
                        }
                    }

                    if let Some(tc_array) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        renderer.close_thinking();
                        spinner.update("Drafting tool action...");

                        for tc in tc_array {
                            let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let entry = tool_calls_map.entry(idx).or_insert_with(|| (
                                tc.get("id").and_then(|v| v.as_str()).unwrap_or("call_0").to_string(),
                                String::new(),
                                String::new(),
                                None,
                            ));

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
                            // Preserve incoming extra_content / thought_signature
                            if let Some(ec) = tc.get("extra_content") {
                                entry.3 = Some(ec.clone());
                            } else if let Some(ec) = delta.get("extra_content") {
                                entry.3 = Some(ec.clone());
                            }
                        }
                    }
                }
            }
        }

        renderer.close_thinking();
        spinner.stop();

        let tool_calls = if !tool_calls_map.is_empty() {
            let calls: Vec<Value> = tool_calls_map.into_values().map(|(id, name, args, extra_content)| {
                let mut call = serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args
                    }
                });
                if let Some(ec) = extra_content {
                    call["extra_content"] = ec;
                }
                call
            }).collect();
            Some(calls)
        } else {
            None
        };

        let opt_reasoning = if !full_reasoning.is_empty() {
            Some(full_reasoning)
        } else {
            None
        };

        Ok((full_response, opt_reasoning, tool_calls, stats))
    }
}
