// ai/generic.rs
// A generic provider that works with any API that follows the OpenAI
// chat completions format. This covers Anthropic (via its compatibility layer),
// Google Gemini, Mistral, Groq, Together AI, OpenRouter, NVIDIA NIM,
// and any custom endpoint the user enters.
//
// All of these accept:
//   POST /chat/completions
//   Authorization: Bearer <api_key>
//   Body: { model, messages, max_tokens }
//
// And return:
//   { choices: [{ message: { content: "..." } }] }

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::config::AiProviderConfig;
use super::{AiProvider, AiResponse, ChatMessage};

pub struct GenericProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    provider_name: String,
}

impl GenericProvider {
    pub fn new(cfg: &AiProviderConfig) -> Self {
        GenericProvider {
            client: Client::new(),
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            api_key: cfg.api_key.clone(),
            model: cfg.model.clone(),
            provider_name: cfg.provider.clone(),
        }
    }
}

#[async_trait]
impl AiProvider for GenericProvider {
    async fn chat(&self, messages: Vec<ChatMessage>, max_tokens: Option<u32>) -> Result<AiResponse> {
        // For Anthropic: inject cache_control on system message for prompt caching (~60% input savings).
        let messages_val = if self.provider_name == "anthropic" {
            let mut msgs: Vec<serde_json::Value> = messages.iter().map(|m| {
                serde_json::json!({ "role": m.role, "content": m.content })
            }).collect();
            if let Some(first) = msgs.first_mut() {
                if first["role"] == "system" {
                    if let Some(content_str) = first["content"].as_str() {
                        first["content"] = serde_json::json!([{
                            "type": "text",
                            "text": content_str,
                            "cache_control": { "type": "ephemeral" }
                        }]);
                    }
                }
            }
            serde_json::Value::Array(msgs)
        } else {
            serde_json::to_value(&messages).unwrap_or(serde_json::Value::Array(vec![]))
        };

        let body = json!({
            "model": self.model,
            "messages": messages_val,
            "max_tokens": max_tokens.unwrap_or(4096)
        });

        let url = format!("{}/chat/completions", self.base_url);

        let mut req = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body);
        if self.provider_name == "anthropic" {
            req = req
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "prompt-caching-2024-07-31");
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("Failed to reach {} API", self.provider_name))?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            let code = status.as_u16();
            if code == 429 {
                anyhow::bail!("429_RATE_LIMIT");
            }
            anyhow::bail!("{} API returned {}: {}", self.provider_name, status, text);
        }

        let val: serde_json::Value = serde_json::from_str(&text)
            .context("Failed to parse API response as JSON")?;

        let content = val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let is_tool_call = content.trim_start().starts_with('{');

        Ok(AiResponse { content, is_tool_call })
    }

    async fn ping(&self) -> Result<String> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": "hi" }],
            "max_tokens": 5
        });

        let url = format!("{}/chat/completions", self.base_url);

        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to reach {}", self.provider_name))?;

        if resp.status().is_success() {
            Ok("API key verified.".to_string())
        } else {
            let text = resp.text().await?;
            anyhow::bail!("API key check failed ({}): {}", self.provider_name, text)
        }
    }

    async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<AiResponse> {
        // For Anthropic: inject cache_control on system message for prompt caching (~60% input savings).
        let messages_val = if self.provider_name == "anthropic" {
            let mut msgs: Vec<serde_json::Value> = messages.iter().map(|m| {
                serde_json::json!({ "role": m.role, "content": m.content })
            }).collect();
            if let Some(first) = msgs.first_mut() {
                if first["role"] == "system" {
                    if let Some(content_str) = first["content"].as_str() {
                        first["content"] = serde_json::json!([{
                            "type": "text",
                            "text": content_str,
                            "cache_control": { "type": "ephemeral" }
                        }]);
                    }
                }
            }
            serde_json::Value::Array(msgs)
        } else {
            serde_json::to_value(&messages).unwrap_or(serde_json::Value::Array(vec![]))
        };

        let body = json!({
            "model": self.model,
            "messages": messages_val,
            "max_tokens": max_tokens.unwrap_or(4096),
            "stream": true
        });

        let url = format!("{}/chat/completions", self.base_url);

        let mut req = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body);
        if self.provider_name == "anthropic" {
            req = req
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "prompt-caching-2024-07-31");
        }
        let mut resp = req
            .send()
            .await
            .with_context(|| format!("Failed to reach {} API", self.provider_name))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await?;
            let code = status.as_u16();
            if code == 429 {
                anyhow::bail!("429_RATE_LIMIT");
            }
            anyhow::bail!("{} API returned {}: {}", self.provider_name, status, text);
        }

        let mut full_content = String::new();
        let mut decoder = super::LineDecoder::new();

        while let Some(chunk) = resp.chunk().await.with_context(|| format!("Failed to read {} stream chunk", self.provider_name))? {
            let lines = decoder.feed(&chunk);
            for line in lines {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !trimmed.starts_with("data: ") {
                    continue;
                }
                let data = &trimmed[6..];
                if data == "[DONE]" {
                    break;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                        if !content.is_empty() {
                            full_content.push_str(content);
                            let _ = tx.send(content.to_string()).await;
                        }
                    }
                }
            }
        }

        if let Some(line) = decoder.flush() {
            let trimmed = line.trim();
            if trimmed.starts_with("data: ") && &trimmed[6..] != "[DONE]" {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&trimmed[6..]) {
                    if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                        if !content.is_empty() {
                            full_content.push_str(content);
                            let _ = tx.send(content.to_string()).await;
                        }
                    }
                }
            }
        }

        let is_tool_call = full_content.trim_start().starts_with('{');
        Ok(AiResponse { content: full_content, is_tool_call })
    }

    fn model_name(&self) -> &str { &self.model }
    fn provider_id(&self) -> &str { &self.provider_name }
    fn api_key_mut(&mut self) -> &mut String { &mut self.api_key }
}
