// ai/openai.rs
// Implements the AiProvider trait for OpenAI (api.openai.com).
// OpenAI uses its own authentication header format (Authorization: Bearer ...)
// and its response envelope differs slightly from Ollama's.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::config::AiProviderConfig;
use super::{AiProvider, AiResponse, ChatMessage};

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(cfg: &AiProviderConfig) -> Self {
        OpenAiProvider {
            client: Client::new(),
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            api_key: cfg.api_key.clone(),
            model: cfg.model.clone(),
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    async fn chat(&self, messages: Vec<ChatMessage>, max_tokens: Option<u32>) -> Result<AiResponse> {
        let body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": max_tokens.unwrap_or(4096)
        });

        let url = format!("{}/chat/completions", self.base_url);

        let resp = self.client
            .post(&url)
            // OpenAI requires the key in the Authorization header as "Bearer <key>".
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .context("Failed to reach OpenAI API")?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            let code = status.as_u16();
            if code == 429 {
                anyhow::bail!("429_RATE_LIMIT");
            }
            anyhow::bail!("OpenAI API returned {}: {}", status, text);
        }

        let val: serde_json::Value = serde_json::from_str(&text)
            .context("Failed to parse OpenAI response")?;

        // OpenAI wraps the reply in: choices[0].message.content
        let content = val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let is_tool_call = content.trim_start().starts_with('{');

        Ok(AiResponse { content, is_tool_call })
    }

    async fn ping(&self) -> Result<String> {
        // A minimal request to verify the API key works.
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
            .context("Failed to reach OpenAI")?;

        if resp.status().is_success() {
            Ok("API key verified.".to_string())
        } else {
            let text = resp.text().await?;
            anyhow::bail!("API key check failed: {}", text)
        }
    }

    async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<AiResponse> {
        let body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": max_tokens.unwrap_or(4096),
            "stream": true
        });

        let url = format!("{}/chat/completions", self.base_url);

        let mut resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .context("Failed to reach OpenAI API")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await?;
            let code = status.as_u16();
            if code == 429 {
                anyhow::bail!("429_RATE_LIMIT");
            }
            anyhow::bail!("OpenAI API returned {}: {}", status, text);
        }

        let mut full_content = String::new();
        let mut decoder = super::LineDecoder::new();

        while let Some(chunk) = resp.chunk().await.context("Failed to read stream chunk")? {
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
    fn provider_id(&self) -> &str { "openai" }
    fn api_key_mut(&mut self) -> &mut String { &mut self.api_key }
}
