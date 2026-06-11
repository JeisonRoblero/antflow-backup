use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct FetchedModel {
    pub id: String,
    pub vision: bool,
    pub provider_id: String,
    #[allow(dead_code)]
    pub provider_name: String,
    #[allow(dead_code)]
    pub base_url: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelObject>,
}

#[derive(Deserialize)]
struct ModelObject {
    id: String,
}

fn is_vision(id: &str) -> bool {
    let l = id.to_lowercase();
    l.contains("vision") || l.contains("gpt-4o") || l.contains("gpt-4.1")
        || l.contains("gpt-4-turbo") || l.contains("claude")
        || l.contains("gemini") || l.contains("llava")
        || l.contains("minicpm") || l.contains("bakllava")
        || l.contains("gpt-5")
}

fn should_exclude(id: &str) -> bool {
    let l = id.to_lowercase();
    l.contains("embed") || l.contains("whisper") || l.contains("tts")
        || l.contains("dall-e") || l.contains("davinci") || l.contains("babbage")
        || l.contains("ada") || l.contains("curie") || l.contains("moderat")
        || l.contains("realtime") || l.contains("audio") || l.contains("transcri")
        || l.contains("search-preview")
        || l.contains("computer-use") || l.contains("image")
        || l.contains("codex") || l.contains("sora")
}

pub struct ProviderConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub auth_header: &'static str,
}

pub const PROVIDERS: &[ProviderConfig] = &[
    ProviderConfig { id: "openai",     name: "OpenAI",     base_url: "https://api.openai.com/v1",                               auth_header: "Bearer" },
    ProviderConfig { id: "anthropic",  name: "Anthropic",  base_url: "https://api.anthropic.com/v1",                            auth_header: "x-api-key" },
    ProviderConfig { id: "gemini",     name: "Google",     base_url: "https://generativelanguage.googleapis.com/v1beta/openai", auth_header: "Bearer" },
    ProviderConfig { id: "mistral",    name: "Mistral",    base_url: "https://api.mistral.ai/v1",                               auth_header: "Bearer" },
    ProviderConfig { id: "groq",       name: "Groq",       base_url: "https://api.groq.com/openai/v1",                          auth_header: "Bearer" },
    ProviderConfig { id: "nvidia",     name: "NVIDIA",     base_url: "https://integrate.api.nvidia.com/v1",                     auth_header: "Bearer" },
    ProviderConfig { id: "openrouter", name: "OpenRouter", base_url: "https://openrouter.ai/api/v1",                            auth_header: "Bearer" },
    ProviderConfig { id: "together",   name: "Together",   base_url: "https://api.together.xyz/v1",                             auth_header: "Bearer" },
];

pub async fn fetch_models_for_provider(
    provider_id: &str,
    api_key: &str,
) -> Vec<FetchedModel> {
    let Some(p) = PROVIDERS.iter().find(|p| p.id == provider_id) else {
        return vec![];
    };

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .unwrap_or_default();

    let url = format!("{}/models", p.base_url);

    let mut req = client.get(&url);

    if p.auth_header == "x-api-key" {
        req = req
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");
    } else {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let Ok(resp) = req.send().await else { return vec![]; };
    if !resp.status().is_success() { return vec![]; }
    let Ok(body) = resp.json::<ModelsResponse>().await else { return vec![]; };

    body.data
        .into_iter()
        .filter(|m| !should_exclude(&m.id))
        .map(|m| FetchedModel {
            vision: is_vision(&m.id),
            provider_id: p.id.to_string(),
            provider_name: p.name.to_string(),
            base_url: p.base_url.to_string(),
            id: m.id,
        })
        .collect()
}

pub async fn fetch_all_known_providers(
    keys: &std::collections::HashMap<String, String>,
) -> Vec<FetchedModel> {
    let mut all = Vec::new();
    for p in PROVIDERS {
        if let Some(key) = keys.get(p.id) {
            let mut models = fetch_models_for_provider(p.id, key).await;
            all.append(&mut models);
        }
    }
    all
}