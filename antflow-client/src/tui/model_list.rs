// tui/model_list.rs - Model list builder
use ratatui::style::Color;

#[derive(Clone)]
pub struct ModelEntry {
    pub name: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub vision: bool,
    pub is_installed: bool,
    pub is_cloud: bool,
    pub size_gb: f32,
    pub category: Option<String>,
}

#[derive(Clone)]
pub struct ProviderGroup {
    pub provider_label: String,
    pub provider_color: Color,
    pub has_key: bool,
    pub models: Vec<ModelEntry>,
}

#[derive(Clone)]
pub struct ProviderSection {
    pub label: String,
    pub header_color: Color,
    pub count_label: String,
    pub providers: Vec<ProviderGroup>,
}

use super::app::{App, CLOUD_MODELS, OLLAMA_CATALOG};

pub fn build_model_list(app: &App) -> Vec<super::model_fetch::FetchedModel> {
    vec![]
}

pub fn build_model_list_grouped(app: &App) -> Vec<ProviderSection> {
    let search = app.model_search.to_lowercase();

    let mut ollama_models: Vec<ModelEntry> = app.ollama_model_infos.iter()
        .filter(|m| search.is_empty() || m.name.to_lowercase().contains(&search))
        .map(|m| ModelEntry {
            name: m.name.clone(),
            provider_id: "ollama".to_string(),
            provider_name: "Ollama".to_string(),
            model_id: m.name.clone(),
            vision: m.vision,
            is_installed: m.installed,
            is_cloud: false,
            size_gb: m.size_gb,
            category: Some("chat".to_string()),
        })
        .collect();

    for k in OLLAMA_CATALOG {
        let already = app.ollama_model_infos.iter().any(|m| m.name == k.name);
        if !already && (search.is_empty() || k.name.to_lowercase().contains(&search)) {
            ollama_models.push(ModelEntry {
                name: k.name.to_string(),
                provider_id: "ollama".to_string(),
                provider_name: "Ollama".to_string(),
                model_id: k.name.to_string(),
                vision: k.vision,
                is_installed: false,
                is_cloud: false,
                size_gb: k.size_gb,
                category: Some("chat".to_string()),
            });
        }
    }

    let mut localai_models: Vec<ModelEntry> = Vec::new();
    let la_catalog = crate::ai::localai::get_catalog();
    for m in la_catalog {
        if search.is_empty() || m.name.to_lowercase().contains(&search) {
            let is_installed = crate::ai::localai::is_model_installed(&app.storage_dir, &m.name);
            localai_models.push(ModelEntry {
                name: m.name.clone(),
                provider_id: "localai".to_string(),
                provider_name: "LocalAI".to_string(),
                model_id: m.name.clone(),
                vision: m.vision,
                is_installed,
                is_cloud: false,
                size_gb: m.size_gb,
                category: Some(m.category.clone()),
            });
        }
    }

    let mut sections: Vec<ProviderSection> = Vec::new();

    if !ollama_models.is_empty() {
        let installed_count = ollama_models.iter().filter(|m| m.is_installed).count();
        sections.push(ProviderSection {
            label: "󰣆  Local — Ollama".to_string(),
            header_color: Color::Rgb(80, 200, 120),
            count_label: format!("{} installed", installed_count),
            providers: vec![ProviderGroup {
                provider_label: String::new(),
                provider_color: Color::Rgb(80, 200, 120),
                has_key: true,
                models: ollama_models,
            }],
        });
    }

    if !localai_models.is_empty() {
        let installed_count = localai_models.iter().filter(|m| m.is_installed).count();
        sections.push(ProviderSection {
            label: "󰣆  Local — LocalAI".to_string(),
            header_color: Color::Rgb(100, 160, 255),
            count_label: format!("{} installed", installed_count),
            providers: vec![ProviderGroup {
                provider_label: String::new(),
                provider_color: Color::Rgb(100, 160, 255),
                has_key: true,
                models: localai_models,
            }],
        });
    }

    let mut provider_order: Vec<&str> = Vec::new();
    let mut provider_map: std::collections::HashMap<&str, ProviderGroup> = std::collections::HashMap::new();

    for pm in CLOUD_MODELS {
        if search.is_empty() || pm.model_id.to_lowercase().contains(&search)
            || pm.provider_name.to_lowercase().contains(&search) {

            let has_key = app.provider_keys_multi
                .get(pm.provider_id)
                .map(|v| !v.is_empty())
                .unwrap_or(false);

            let entry = provider_map.entry(pm.provider_id).or_insert_with(|| {
                provider_order.push(pm.provider_id);
                ProviderGroup {
                    provider_label: pm.provider_name.to_string(),
                    provider_color: provider_color(pm.provider_id),
                    has_key,
                    models: Vec::new(),
                }
            });

            entry.models.push(ModelEntry {
                name: pm.model_id.to_string(),
                provider_id: pm.provider_id.to_string(),
                provider_name: pm.provider_name.to_string(),
                model_id: pm.model_id.to_string(),
                vision: pm.vision,
                is_installed: false,
                is_cloud: true,
                size_gb: 0.0,
                category: Some("chat".to_string()),
            });
        }
    }

    if !provider_map.is_empty() {
        let groups: Vec<ProviderGroup> = provider_order.iter()
            .filter_map(|id| provider_map.remove(id))
            .collect();
        let total: usize = groups.iter().map(|g| g.models.len()).sum();
        sections.push(ProviderSection {
            label: "☁  Cloud".to_string(),
            header_color: Color::Rgb(120, 80, 200),
            count_label: format!("{} models", total),
            providers: groups,
        });
    }

    // Add online Hugging Face models if they exist in search results
    if !app.hf_search_results.is_empty() {
        let hf_models: Vec<ModelEntry> = app.hf_search_results.iter().map(|m| {
            let full_name = format!("hf.co/{}", m.id);
            let vision = m.id.to_lowercase().contains("vision")
                || m.id.to_lowercase().contains("llava")
                || m.tags.iter().any(|tag| tag.to_lowercase().contains("vision"));
            
            let is_installed = app.ollama_model_infos.iter().any(|info| {
                info.name == full_name 
                || info.name == format!("{}:latest", full_name)
                || info.name.contains(&m.id)
            });

            ModelEntry {
                name: full_name.clone(),
                provider_id: "ollama".to_string(),
                provider_name: "Ollama (Hugging Face)".to_string(),
                model_id: full_name,
                vision,
                is_installed,
                is_cloud: false,
                size_gb: m.size_bytes as f32 / (1024.0 * 1024.0 * 1024.0),
                category: Some("chat".to_string()),
            }
        }).collect();

        sections.push(ProviderSection {
            label: "󰚩  Online — Hugging Face GGUF".to_string(),
            header_color: Color::Rgb(160, 100, 255),
            count_label: format!("{} found", hf_models.len()),
            providers: vec![ProviderGroup {
                provider_label: String::new(),
                provider_color: Color::Rgb(160, 100, 255),
                has_key: true,
                models: hf_models,
            }],
        });
    }

    // Add custom Ollama model if query doesn't match any listed models
    if !search.is_empty() {
        let already_listed = sections.iter().any(|sec| {
            sec.providers.iter().any(|pg| {
                pg.models.iter().any(|m| m.name.to_lowercase() == search)
            })
        });

        if !already_listed {
            let vision = crate::ai::ollama::is_vision_model(&app.model_search);
            sections.push(ProviderSection {
                label: "󰣆  Custom Ollama Model".to_string(),
                header_color: Color::Rgb(80, 160, 240),
                count_label: "direct pull".to_string(),
                providers: vec![ProviderGroup {
                    provider_label: String::new(),
                    provider_color: Color::Rgb(80, 160, 240),
                    has_key: true,
                    models: vec![ModelEntry {
                        name: app.model_search.clone(),
                        provider_id: "ollama".to_string(),
                        provider_name: "Ollama".to_string(),
                        model_id: app.model_search.clone(),
                        vision,
                        is_installed: false,
                        is_cloud: false,
                        size_gb: 0.0,
                        category: Some("chat".to_string()),
                    }],
                }],
            });
        }
    }

    sections
}

fn provider_color(id: &str) -> Color {
    match id {
        "openai"     => Color::Rgb(16, 163, 127),
        "anthropic"  => Color::Rgb(205, 92, 92),
        "gemini"     => Color::Rgb(66, 133, 244),
        "mistral"    => Color::Rgb(255, 140, 0),
        "groq"       => Color::Rgb(240, 80, 120),
        "nvidia"     => Color::Rgb(118, 185, 0),
        "openrouter" => Color::Rgb(160, 100, 255),
        "together"   => Color::Rgb(0, 180, 200),
        "localai"    => Color::Rgb(100, 160, 255),
        _            => Color::White,
    }
}