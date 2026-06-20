#![allow(dead_code)]
// ai/mod.rs
// Defines the AiProvider trait that all providers implement.
// Any part of antflow that needs to talk to an AI calls this trait;
// it never calls a specific provider directly.
// Adding a new provider means implementing this trait and registering
// the provider in `provider_for_config()` below.

pub mod ollama;
pub mod localai;
pub mod openai;
pub mod generic; // covers Anthropic, Gemini, Groq, Mistral, Together, OpenRouter, Nvidia, custom
pub mod embeddings;
pub mod router;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::AiProviderConfig;

// ---------------------------------------------------------------------------
// Message types sent to and received from AI providers
// ---------------------------------------------------------------------------

// A single message in the conversation, following the role/content convention
// used by the OpenAI API and adopted by most compatible providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    // "system", "user", or "assistant"
    pub role: String,

    // The content of the message. For vision requests this is a JSON array
    // of content parts; for text-only it is a plain string.
    // serde_json::Value lets this field hold either without a fixed schema.
    pub content: serde_json::Value,
}

impl ChatMessage {
    // Convenience constructor for plain text messages.
    pub fn text(role: &str, content: &str) -> Self {
        ChatMessage {
            role: role.to_string(),
            content: serde_json::Value::String(content.to_string()),
        }
    }

    pub fn has_image(&self) -> bool {
        self.content.is_array()
    }

    // Convenience constructor for a user message that includes an image.
    // `image_b64` is a base64-encoded PNG. `media_type` is "image/png".
    pub fn with_image(text: &str, image_b64: &str) -> Self {
        // Most vision-capable APIs (OpenAI, Anthropic, Ollama, etc.) accept
        // a content array with text and image_url parts.
        ChatMessage {
            role: "user".to_string(),
            content: serde_json::json!([
                { "type": "text", "text": text },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{}", image_b64)
                    }
                }
            ]),
        }
    }
}

// ---------------------------------------------------------------------------
// AI response
// ---------------------------------------------------------------------------

// The parsed response from the AI provider after a chat completion call.
#[derive(Debug, Clone)]
pub struct AiResponse {
    // The full text content of the assistant's reply.
    pub content: String,
    // True if the response looks like a JSON tool-call payload rather than
    // a plain conversational answer. The executor checks this flag.
    pub is_tool_call: bool,
}

pub struct LineDecoder {
    buffer: Vec<u8>,
}

impl LineDecoder {
    pub fn new() -> Self {
        LineDecoder { buffer: Vec::new() }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
            let mut line = String::from_utf8_lossy(&line_bytes).into_owned();
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            lines.push(line);
        }
        lines
    }

    pub fn flush(self) -> Option<String> {
        if !self.buffer.is_empty() {
            let mut line = String::from_utf8_lossy(&self.buffer).into_owned();
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            Some(line)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

// `#[async_trait]` is a macro that allows async methods in traits.
// Rust traits do not natively support async methods; this macro rewrites
// them to return `Pin<Box<dyn Future>>` under the hood.
#[async_trait]
pub trait AiProvider: Send + Sync {
    // Sends a conversation to the AI and returns its response.
    // `messages` is the full history including the system prompt.
    // `max_tokens` controls output length; pass None to use provider default.
    async fn chat(&self, messages: Vec<ChatMessage>, max_tokens: Option<u32>) -> Result<AiResponse>;

    // Sends a conversation to the AI and streams tokens via a channel.
    async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<AiResponse>;

    // Sends a quick single-message request and returns the raw text.
    // Used for API key validation, model listing, etc.
    async fn ping(&self) -> Result<String>;

    // Returns the model name this provider is configured to use.
    fn model_name(&self) -> &str;

    // Returns the provider identifier string, e.g. "openai", "ollama".
    fn provider_id(&self) -> &str;

    // Returns a mutable reference to the API key for rotation on rate limit.
    fn api_key_mut(&mut self) -> &mut String;
}

// Check if an error is a rate limit (429).
pub fn is_rate_limit_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("429_RATE_LIMIT")
}

// ---------------------------------------------------------------------------
// Provider factory
// ---------------------------------------------------------------------------
// Provider factory
// ---------------------------------------------------------------------------

// Returns a boxed AiProvider implementation based on the config.
// `Box<dyn AiProvider>` is a heap-allocated trait object: the caller does
// not need to know the concrete type at compile time.
pub fn provider_for_config(cfg: &AiProviderConfig) -> Box<dyn AiProvider> {
    match cfg.provider.as_str() {
        "ollama" => Box::new(ollama::OllamaProvider::new(cfg)),
        "openai" => Box::new(openai::OpenAiProvider::new(cfg)),
        "localai" => {
            let mut cfg_local = cfg.clone();
            cfg_local.base_url = format!("{}/v1", localai::get_localai_base_url());
            Box::new(generic::GenericProvider::new(&cfg_local))
        }
        // All other providers use the generic OpenAI-compatible adapter.
        _ => Box::new(generic::GenericProvider::new(cfg)),
    }
}

// ---------------------------------------------------------------------------
// Pre-configured provider catalog
// ---------------------------------------------------------------------------

// Describes a well-known cloud AI provider so the wizard can present a list
// without the user needing to look up endpoints.
#[derive(Debug, Clone)]
pub struct KnownProvider {
    pub id: &'static str,
    pub display_name: &'static str,
    pub base_url: &'static str,
    // Suggested vision-capable model for each provider.
    pub recommended_model: &'static str,
    // Whether this provider is known to support vision.
    pub vision_supported: bool,
}

// All entries use OpenAI-compatible chat completion endpoints.
// The user only needs to supply their API key.
pub const KNOWN_PROVIDERS: &[KnownProvider] = &[
    KnownProvider {
        id: "openai",
        display_name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        recommended_model: "gpt-4o",
        vision_supported: true,
    },
    KnownProvider {
        id: "anthropic",
        display_name: "Anthropic",
        base_url: "https://api.anthropic.com/v1",
        recommended_model: "claude-opus-4-5",
        vision_supported: true,
    },
    KnownProvider {
        id: "gemini",
        display_name: "Google Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        recommended_model: "gemini-2.5-flash",
        vision_supported: true,
    },
    KnownProvider {
        id: "mistral",
        display_name: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        recommended_model: "pixtral-large-latest",
        vision_supported: true,
    },
    KnownProvider {
        id: "groq",
        display_name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        recommended_model: "llama-3.2-90b-vision-preview",
        vision_supported: true,
    },
    KnownProvider {
        id: "together",
        display_name: "Together AI",
        base_url: "https://api.together.xyz/v1",
        recommended_model: "meta-llama/Llama-Vision-Free",
        vision_supported: true,
    },
    KnownProvider {
        id: "openrouter",
        display_name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        recommended_model: "google/gemini-2.5-flash",
        vision_supported: true,
    },
    KnownProvider {
        id: "nvidia",
        display_name: "NVIDIA NIM",
        base_url: "https://integrate.api.nvidia.com/v1",
        recommended_model: "nvidia/llama-3.2-90b-vision-instruct",
        vision_supported: true,
    },
];

pub async fn generate_welcome_greeting(cfg: &crate::config::AiProviderConfig, lang: &str) -> Result<String> {
    let provider = provider_for_config(cfg);
    let lang_name = match lang.split('-').next().unwrap_or(lang) {
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ja" => "Japanese",
        "zh" => "Chinese",
        "ru" => "Russian",
        _ => "English",
    };
    
    let prompt = format!(
        "Write an extremely brief, friendly welcome greeting for a computer automation assistant called Antflow. \
         Limit the greeting to at most 5 words and under 30 characters. Do not use ellipsis. \
         IMPORTANT: Write the greeting in {}.", 
        lang_name
    );
    let msg = ChatMessage::text("user", &prompt);
    let resp = provider.chat(vec![msg], Some(100)).await?;
    let content = resp.content.trim();
    
    let mut cleaned = content.to_string();
    if cleaned.starts_with("```") {
        let lines: Vec<&str> = cleaned.lines().collect();
        if lines.len() >= 2 {
            let start = if lines[0].starts_with("```") { 1 } else { 0 };
            let end = if lines[lines.len() - 1].starts_with("```") { lines.len() - 1 } else { lines.len() };
            if start < end {
                cleaned = lines[start..end].join("\n").trim().to_string();
            }
        }
    }

    // Normalize smart quotes
    cleaned = cleaned
        .replace('“', "\"")
        .replace('”', "\"")
        .replace('‘', "'")
        .replace('’', "'");
    
    if cleaned.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cleaned) {
            if let Some(greeting) = val["greeting"].as_str() {
                cleaned = greeting.to_string();
            } else if let Some(msg) = val["message"].as_str() {
                cleaned = msg.to_string();
            } else if let Some(summary) = val["summary"].as_str() {
                cleaned = summary.to_string();
            } else if let Some(first_val) = val.as_object().and_then(|obj| obj.values().next()).and_then(|v| v.as_str()) {
                cleaned = first_val.to_string();
            }
        } else {
            // Regex fallback if serde fails
            if let Some(start_idx) = cleaned.find('"') {
                if let Some(end_idx) = cleaned.rfind('"') {
                    if start_idx < end_idx {
                        let inner = &cleaned[start_idx + 1..end_idx];
                        if let Some(val_start) = inner.rfind(':') {
                            let value_part = inner[val_start + 1..].trim().trim_matches('"');
                            if !value_part.is_empty() {
                                cleaned = value_part.to_string();
                            }
                        } else {
                            cleaned = inner.trim().to_string();
                        }
                    }
                }
            }
        }
    }
    
    cleaned = cleaned.trim().trim_matches('"').trim_matches('\'').trim().to_string();
    if cleaned.is_empty() {
        anyhow::bail!("Empty greeting generated");
    }
    Ok(cleaned)
}

pub fn get_system_lang() -> String {
    if let Ok(lang) = std::env::var("LANG") {
        if lang.len() >= 2 {
            return lang[..2].to_string();
        }
    }
    if let Ok(lang) = std::env::var("LC_ALL") {
        if lang.len() >= 2 {
            return lang[..2].to_string();
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", "(Get-Culture).TwoLetterISOLanguageName"]);
        cmd.creation_flags(0x08000000);
        if let Ok(output) = cmd.output() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if s.len() == 2 {
                return s;
            }
        }
    }
    "en".to_string();
    "en".to_string()
}

