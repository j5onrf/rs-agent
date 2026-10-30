#![allow(dead_code)]

use anyhow::{Context, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::ProviderConfig;
use crate::spinner::InlineSpinner;
use crate::stream::{StreamRenderer, UsageStats};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool_response(tool_call_id: String, content: String) -> Self {
        Self {
            role: "tool".into(),
            content,
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        }
    }
}

pub struct LlmClient {
    http: Client,
    config: ProviderConfig,
}

impl LlmClient {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            http: Client::builder().build().unwrap_or_default(),
            config,
        }
    }

    pub async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<Value>,
        renderer: &mut StreamRenderer,
        spinner: &mut InlineSpinner,
    ) -> Result<(String, Option<Vec<Value>>, UsageStats)> {
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        if let Some(tool_specs) = tools {
            body["tools"] = tool_specs;
        }

        let mut req = self.http.post(&self.config.base_url)
            .header("Content-Type", "application/json")
            .json(&body);

        if self.config.api_key != "not-needed" && !self.config.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.config.api_key));
        }

        let response = req.send().await.context("Failed to send request to LLM endpoint")?;

        if !response.status().is_success() {
            spinner.stop();
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            anyhow::bail!("Server returned HTTP {}: {}", status, err_body);
        }

        let mut stream = response.bytes_stream().eventsource();
        let mut full_response = String::new();
        let mut in_thinking = false;
        let mut stats = UsageStats::default();
        let mut tool_calls_map: std::collections::BTreeMap<usize, (String, String, String)> = std::collections::BTreeMap::new();

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
                        }
                    }

                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            spinner.stop();
                            let mut chunk_slice = content;
                            if chunk_slice.contains("<think>") {
                                in_thinking = true;
                                chunk_slice = chunk_slice.trim_start_matches("<think>");
                            }

                            if chunk_slice.contains("</think>") {
                                let parts: Vec<&str> = chunk_slice.splitn(2, "</think>").collect();
                                renderer.on_chunk(parts[0], true);
                                in_thinking = false;
                                if parts.len() > 1 && !parts[1].is_empty() {
                                    renderer.on_chunk(parts[1], false);
                                    full_response.push_str(parts[1]);
                                }
                            } else {
                                renderer.on_chunk(chunk_slice, in_thinking);
                                if !in_thinking {
                                    full_response.push_str(chunk_slice);
                                }
                            }
                        }
                    }

                    if let Some(tc_array) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        // Tool incoming: close thinking block with newline immediately
                        renderer.close_thinking();
                        spinner.update("Drafting tool action...");

                        for tc in tc_array {
                            let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let entry = tool_calls_map.entry(idx).or_insert_with(|| (
                                tc.get("id").and_then(|v| v.as_str()).unwrap_or("call_0").to_string(),
                                String::new(),
                                String::new(),
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
                        }
                    }
                }
            }
        }

        renderer.close_thinking();
        spinner.stop();

        let tool_calls = if !tool_calls_map.is_empty() {
            let calls: Vec<Value> = tool_calls_map.into_values().map(|(id, name, args)| {
                serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args
                    }
                })
            }).collect();
            Some(calls)
        } else {
            None
        };

        Ok((full_response, tool_calls, stats))
    }
}
