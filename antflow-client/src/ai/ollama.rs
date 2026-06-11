// ai/ollama.rs
// Implements the AiProvider trait for a locally running Ollama instance.
// Ollama exposes an OpenAI-compatible /api/chat endpoint.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::config::AiProviderConfig;
use super::{AiProvider, AiResponse, ChatMessage};

// ---------------------------------------------------------------------------
// Ollama provider struct
// ---------------------------------------------------------------------------

pub struct OllamaProvider {
    // reqwest::Client is an async HTTP client. It is cloneable and internally
    // uses a connection pool, so one instance is shared across all requests.
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(cfg: &AiProviderConfig) -> Self {
        OllamaProvider {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .unwrap_or_else(|_| Client::new()),
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            model: cfg.model.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// AiProvider implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl AiProvider for OllamaProvider {
    async fn chat(&self, messages: Vec<ChatMessage>, max_tokens: Option<u32>) -> Result<AiResponse> {
        // Ollama doesn't support image_url array format.
        // Convert any array-content messages to string + separate images field.
        let converted: Vec<serde_json::Value> = messages.iter().map(|m| {
            if let serde_json::Value::Array(parts) = &m.content {
                // Extract text and base64 images from the array format
                let mut text = String::new();
                let mut images: Vec<String> = Vec::new();
                for part in parts {
                    if part["type"] == "text" {
                        text = part["text"].as_str().unwrap_or("").to_string();
                    } else if part["type"] == "image_url" {
                        // format is "data:image/png;base64,<data>"
                        if let Some(url) = part["image_url"]["url"].as_str() {
                            if let Some(b64) = url.strip_prefix("data:image/png;base64,") {
                                images.push(b64.to_string());
                            }
                        }
                    }
                }
                if images.is_empty() {
                    serde_json::json!({ "role": m.role, "content": text })
                } else {
                    serde_json::json!({ "role": m.role, "content": text, "images": images })
                }
            } else {
                serde_json::json!({ "role": m.role, "content": m.content })
            }
        }).collect();

        let mut force_json = false;
        if let Some(last_msg) = messages.last() {
            let last_content = match &last_msg.content {
                serde_json::Value::String(s) => s.to_lowercase(),
                serde_json::Value::Array(arr) => {
                    let mut text = String::new();
                    for part in arr {
                        if part["type"] == "text" {
                            if let Some(t) = part["text"].as_str() {
                                text.push_str(t);
                            }
                        }
                    }
                    text.to_lowercase()
                }
                _ => String::new(),
            };
            if last_content.contains("[todo]")
                || last_content.contains("only raw json")
                || last_content.contains("reply with only this exact json")
            {
                force_json = true;
            }
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": converted,
            "stream": false,
        });
        if force_json {
            body["format"] = serde_json::json!("json");
        }
        if let Some(mt) = max_tokens {
            body["options"] = serde_json::json!({ "num_predict": mt });
        }

        let url = format!("{}/api/chat", self.base_url);

        let resp = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to reach Ollama. Is it running?")?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("Ollama returned {}: {}", status, text);
        }

        let val: serde_json::Value = serde_json::from_str(&text)
            .context("Failed to parse Ollama response as JSON")?;

        let content = val["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let is_tool_call = content.trim_start().starts_with('{');

        Ok(AiResponse { content, is_tool_call })
    }

    async fn ping(&self) -> Result<String> {
        // A simple request to /api/tags lists available models.
        // If Ollama is running, this succeeds immediately.
        let url = format!("{}/api/tags", self.base_url);
        let resp = self.client.get(&url).send().await
            .context("Cannot reach Ollama")?;
        let text = resp.text().await?;
        Ok(text)
    }

    async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<AiResponse> {
        let converted: Vec<serde_json::Value> = messages.iter().map(|m| {
            if let serde_json::Value::Array(parts) = &m.content {
                let mut text = String::new();
                let mut images: Vec<String> = Vec::new();
                for part in parts {
                    if part["type"] == "text" {
                        text = part["text"].as_str().unwrap_or("").to_string();
                    } else if part["type"] == "image_url" {
                        if let Some(url) = part["image_url"]["url"].as_str() {
                            if let Some(b64) = url.strip_prefix("data:image/png;base64,") {
                                images.push(b64.to_string());
                            }
                        }
                    }
                }
                if images.is_empty() {
                    serde_json::json!({ "role": m.role, "content": text })
                } else {
                    serde_json::json!({ "role": m.role, "content": text, "images": images })
                }
            } else {
                serde_json::json!({ "role": m.role, "content": m.content })
            }
        }).collect();

        let mut force_json = false;
        if let Some(last_msg) = messages.last() {
            let last_content = match &last_msg.content {
                serde_json::Value::String(s) => s.to_lowercase(),
                serde_json::Value::Array(arr) => {
                    let mut text = String::new();
                    for part in arr {
                        if part["type"] == "text" {
                            if let Some(t) = part["text"].as_str() {
                                text.push_str(t);
                            }
                        }
                    }
                    text.to_lowercase()
                }
                _ => String::new(),
            };
            if last_content.contains("[todo]")
                || last_content.contains("only raw json")
                || last_content.contains("reply with only this exact json")
            {
                force_json = true;
            }
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": converted,
            "stream": true,
        });
        if force_json {
            body["format"] = serde_json::json!("json");
        }
        if let Some(mt) = max_tokens {
            body["options"] = serde_json::json!({ "num_predict": mt });
        }

        let url = format!("{}/api/chat", self.base_url);

        let mut resp = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to reach Ollama. Is it running?")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await?;
            anyhow::bail!("Ollama returned {}: {}", status, text);
        }

        let mut full_content = String::new();
        let mut decoder = super::LineDecoder::new();

        while let Some(chunk) = resp.chunk().await.context("Failed to read Ollama stream chunk")? {
            let lines = decoder.feed(&chunk);
            for line in lines {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(content) = val["message"]["content"].as_str() {
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
            if !trimmed.is_empty() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(content) = val["message"]["content"].as_str() {
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

    fn model_name(&self) -> &str {
        &self.model
    }

    fn provider_id(&self) -> &str {
        "ollama"
    }

    fn api_key_mut(&mut self) -> &mut String {
        // Return a placeholder that's never actually used.
        &mut self.model
    }
}

// ---------------------------------------------------------------------------
// Ollama management helpers
// ---------------------------------------------------------------------------

// Returns a list of models currently installed in Ollama.
// Each entry includes the name and size in bytes.
pub async fn list_models(base_url: &str) -> Result<Vec<OllamaModel>> {
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_millis(1500))
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .context("Failed to build HTTP client")?;
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let resp = client.get(&url).send().await.context("Cannot reach Ollama")?;
    let val: serde_json::Value = resp.json().await?;

    // The "models" array contains objects with "name" and "size" fields.
    let models = val["models"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            let name = m["name"].as_str()?.to_string();
            let size = m["size"].as_u64().unwrap_or(0);
            Some(OllamaModel { name, size_bytes: size })
        })
        .collect();

    Ok(models)
}

// Starts downloading a model from the Ollama registry.
// This calls /api/pull and returns immediately; download runs in the background.
// The caller should poll or watch the terminal for progress.
pub async fn pull_model(base_url: &str, model_name: &str) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/api/pull", base_url.trim_end_matches('/'));
    let body = json!({ "name": model_name, "stream": false });
    let resp = client.post(&url).json(&body).send().await
        .context("Failed to start model download")?;
    if !resp.status().is_success() {
        let text = resp.text().await?;
        anyhow::bail!("Pull failed: {}", text);
    }
    Ok(())
}

// Checks whether the model name is in the known vision-capable list.
// This is a best-effort check based on naming conventions.
pub fn is_vision_model(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("llava")
        || lower.contains("vision")
        || lower.contains("bakllava")
        || lower.contains("moondream")
        || lower.contains("minicpm-v")
        || lower.contains("gemma3")
        || lower.contains("gemma-3")
        || lower.contains("gemma4")
        || lower.contains("gemma-4")
        || lower.contains("qwen2-vl")
        || lower.contains("qwen2.5-vl")
        || lower.contains("qwen2.5vl")
        || lower.contains("pixtral")
        || (lower.contains("llama3.2") && (lower.contains("vision") || lower.contains("11b") || lower.contains("90b")))
}

// A simple description of an installed Ollama model.
#[derive(Debug, Clone)]
pub struct OllamaModel {
    pub name: String,
    pub size_bytes: u64,
}

impl OllamaModel {
    // Formats the model size as a human-readable string (GB or MB).
    pub fn size_human(&self) -> String {
        if self.size_bytes >= 1_000_000_000 {
            format!("{:.1} GB", self.size_bytes as f64 / 1_000_000_000.0)
        } else {
            format!("{:.0} MB", self.size_bytes as f64 / 1_000_000.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Ollama installation helpers
// ---------------------------------------------------------------------------

// Checks whether Ollama is already installed and reachable on the given port.
pub async fn is_running(base_url: &str) -> bool {
    let client = match Client::builder()
        .connect_timeout(std::time::Duration::from_millis(1500))
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

// Returns the download URL for the Ollama installer for the current platform.
pub fn installer_url() -> &'static str {
    if cfg!(target_os = "windows") {
        "https://ollama.com/download/OllamaSetup.exe"
    } else if cfg!(target_os = "macos") {
        "https://ollama.com/download/Ollama-darwin.zip"
    } else {
        "https://ollama.com/download/ollama-linux-amd64"
    }
}

// Struct representing a model returned from the online search API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HfModelInfo {
    pub id: String,
    pub downloads: u64,
    pub tags: Vec<String>,
    pub size_bytes: u64,
}

// Queries Hugging Face Hub for GGUF models matching a search query.
pub async fn search_huggingface_gguf(query: &str) -> Result<Vec<HfModelInfo>> {
    let client = Client::new();
    let resp = client.get("https://huggingface.co/api/models")
        .query(&[
            ("search", query),
            ("library", "gguf"),
            ("limit", "10"),
            ("sort", "downloads"),
            ("direction", "-1"),
        ])
        .header("User-Agent", "antflow")
        .send()
        .await
        .context("Failed to contact Hugging Face API")?;

    if !resp.status().is_success() {
        anyhow::bail!("Hugging Face API returned status {}", resp.status());
    }

    #[derive(serde::Deserialize)]
    struct HfSibling {
        rpath: String,
        size: Option<u64>,
    }

    #[derive(serde::Deserialize)]
    struct HfModelResponse {
        id: String,
        downloads: Option<u64>,
        tags: Option<Vec<String>>,
        siblings: Option<Vec<HfSibling>>,
    }

    let hf_models: Vec<HfModelResponse> = resp.json().await
        .context("Failed to parse Hugging Face JSON response")?;

    let mut out = Vec::new();
    for m in hf_models {
        let mut size_bytes = 0;
        if let Some(siblings) = &m.siblings {
            for sib in siblings {
                if sib.rpath.to_lowercase().ends_with(".gguf") {
                    if let Some(sz) = sib.size {
                        if sz > size_bytes {
                            size_bytes = sz;
                        }
                    }
                }
            }
        }
        out.push(HfModelInfo {
            id: m.id,
            downloads: m.downloads.unwrap_or(0),
            tags: m.tags.unwrap_or_default(),
            size_bytes,
        });
    }
    Ok(out)
}

// Creates/imports a model from a local GGUF file.
// Returns a streaming response from Ollama's /api/create endpoint.
pub async fn create_model(base_url: &str, model_name: &str, gguf_path: &str) -> Result<reqwest::Response> {
    let client = Client::new();
    let url = format!("{}/api/create", base_url.trim_end_matches('/'));
    
    let formatted_path = gguf_path.replace("\\", "/");
    let modelfile = format!("FROM {}\n", formatted_path);
    
    let body = json!({
        "name": model_name,
        "modelfile": modelfile,
        "stream": true
    });
    
    let resp = client.post(&url).json(&body).send().await
        .context("Failed to start GGUF import in Ollama")?;
        
    if !resp.status().is_success() {
        let text = resp.text().await?;
        anyhow::bail!("Ollama import failed: {}", text);
    }
    
    Ok(resp)
}

