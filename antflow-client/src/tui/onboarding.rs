// tui/onboarding.rs
// Self-contained state for the first-run onboarding wizard that runs inside
// the TUI. No console prompts, no external wizard module.
//
// Steps:
//   0 — Welcome / quick-start choice (Auto vs Manual)
//   1 — Storage location
//   2 — AI provider + model + API key
//   3 — Session memory toggle
//   4 — Done / summary
//
// The runner calls `onboarding_complete()` to check whether to save config
// and transition to Chat mode.

use crate::config::{AiProviderConfig, AppConfig, StorageLocation};

// Which step the user is on.
#[derive(Debug, Clone, PartialEq)]
pub enum OnboardingStep {
    Welcome,
    Storage,
    Provider,
    Memory,
    Done,
}

// Which sub-panel is focused inside the Provider step.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderFocus {
    ProviderList,
    ModelList,
    ApiKeyInput,
    OllamaModelList,
}

#[derive(Debug, Clone)]
pub struct OnboardingState {
    pub step: OnboardingStep,

    // Storage step
    pub storage_cursor: usize, // 0=Portable, 1=Global

    // Provider step
    pub provider_cursor: usize,  // index into ONBOARDING_PROVIDERS
    pub model_cursor: usize,
    pub model_scroll: usize,
    pub api_key_input: String,
    pub api_key_visible: bool,
    pub provider_focus: ProviderFocus,
    pub ollama_models: Vec<String>,       // populated async after Ollama detected
    pub ollama_fetched: bool,
    pub key_status: KeyStatus,

    // Memory step
    pub remember_sessions: bool,

    // Chosen values (written to config on Done)
    pub chosen_storage: StorageLocation,
    pub chosen_provider: String,
    pub chosen_base_url: String,
    pub chosen_model: String,
    pub chosen_api_key: String,
    pub chosen_vision: bool,

    // Error / status line shown under the current step
    pub status_line: String,
    pub validating: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyStatus {
    Empty,
    Typing,
    Valid,
    Invalid(String),
}

// Providers shown in the onboarding list, in display order.
pub struct OnboardingProvider {
    pub id: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    pub needs_key: bool,
    pub default_model: &'static str,
    pub color: ratatui::style::Color,
}

pub const ONBOARDING_PROVIDERS: &[OnboardingProvider] = &[
    OnboardingProvider {
        id: "ollama",
        label: "Ollama  (local, free)",
        base_url: "http://localhost:11434",
        needs_key: false,
        default_model: "",
        color: ratatui::style::Color::Rgb(80, 210, 130),
    },
    OnboardingProvider {
        id: "localai",
        label: "LocalAI (local, free)",
        base_url: "",
        needs_key: false,
        default_model: "LocalAI-functioncall-llama3.2-3b-v0.5",
        color: ratatui::style::Color::Rgb(100, 160, 255),
    },
    OnboardingProvider {
        id: "openai",
        label: "OpenAI",
        base_url: "https://api.openai.com/v1",
        needs_key: true,
        default_model: "gpt-4o",
        color: ratatui::style::Color::Rgb(100, 200, 140),
    },
    OnboardingProvider {
        id: "anthropic",
        label: "Anthropic",
        base_url: "https://api.anthropic.com/v1",
        needs_key: true,
        default_model: "claude-sonnet-4-6",
        color: ratatui::style::Color::Rgb(210, 140, 90),
    },
    OnboardingProvider {
        id: "gemini",
        label: "Google Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        needs_key: true,
        default_model: "gemini-2.5-flash",
        color: ratatui::style::Color::Rgb(80, 160, 240),
    },
    OnboardingProvider {
        id: "mistral",
        label: "Mistral",
        base_url: "https://api.mistral.ai/v1",
        needs_key: true,
        default_model: "mistral-large-latest",
        color: ratatui::style::Color::Rgb(180, 100, 220),
    },
    OnboardingProvider {
        id: "groq",
        label: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        needs_key: true,
        default_model: "llama-3.3-70b-versatile",
        color: ratatui::style::Color::Rgb(240, 180, 60),
    },
    OnboardingProvider {
        id: "openrouter",
        label: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        needs_key: true,
        default_model: "google/gemini-2.5-flash",
        color: ratatui::style::Color::Rgb(160, 120, 240),
    },
    OnboardingProvider {
        id: "nvidia",
        label: "NVIDIA NIM",
        base_url: "https://integrate.api.nvidia.com/v1",
        needs_key: true,
        default_model: "meta/llama-3.2-11b-vision-instruct",
        color: ratatui::style::Color::Rgb(120, 220, 80),
    },
    OnboardingProvider {
        id: "together",
        label: "Together AI",
        base_url: "https://api.together.xyz/v1",
        needs_key: true,
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        color: ratatui::style::Color::Rgb(80, 200, 200),
    },
];

// Models shown per cloud provider in the onboarding model list.
pub struct OnboardingModel {
    pub provider_id: &'static str,
    pub model_id: &'static str,
    pub vision: bool,
}

pub const ONBOARDING_MODELS: &[OnboardingModel] = &[
    // LocalAI
    OnboardingModel { provider_id: "localai", model_id: "LocalAI-functioncall-llama3.2-3b-v0.5", vision: false },
    // OpenAI
    OnboardingModel { provider_id: "openai", model_id: "gpt-4.1",        vision: true  },
    OnboardingModel { provider_id: "openai", model_id: "gpt-4.1-mini",   vision: true  },
    OnboardingModel { provider_id: "openai", model_id: "gpt-4o",         vision: true  },
    OnboardingModel { provider_id: "openai", model_id: "gpt-4o-mini",    vision: true  },
    OnboardingModel { provider_id: "openai", model_id: "o3",             vision: false },
    OnboardingModel { provider_id: "openai", model_id: "o4-mini",        vision: false },
    // Anthropic
    OnboardingModel { provider_id: "anthropic", model_id: "claude-opus-4-6",            vision: true },
    OnboardingModel { provider_id: "anthropic", model_id: "claude-sonnet-4-6",          vision: true },
    OnboardingModel { provider_id: "anthropic", model_id: "claude-haiku-4-5-20251001",  vision: true },
    OnboardingModel { provider_id: "anthropic", model_id: "claude-3-5-sonnet-20241022", vision: true },
    // Gemini
    OnboardingModel { provider_id: "gemini", model_id: "gemini-2.5-pro",        vision: true },
    OnboardingModel { provider_id: "gemini", model_id: "gemini-2.5-flash",      vision: true },
    OnboardingModel { provider_id: "gemini", model_id: "gemini-2.5-flash-lite", vision: true },
    OnboardingModel { provider_id: "gemini", model_id: "gemini-2.0-flash",      vision: true },
    // Mistral
    OnboardingModel { provider_id: "mistral", model_id: "mistral-large-latest", vision: false },
    OnboardingModel { provider_id: "mistral", model_id: "mistral-small-latest", vision: false },
    OnboardingModel { provider_id: "mistral", model_id: "codestral-latest",     vision: false },
    // Groq
    OnboardingModel { provider_id: "groq", model_id: "llama-3.3-70b-versatile",        vision: false },
    OnboardingModel { provider_id: "groq", model_id: "llama-3.1-8b-instant",           vision: false },
    OnboardingModel { provider_id: "groq", model_id: "llama-3.2-11b-vision-preview",   vision: true  },
    OnboardingModel { provider_id: "groq", model_id: "deepseek-r1-distill-llama-70b",  vision: false },
    // OpenRouter
    OnboardingModel { provider_id: "openrouter", model_id: "google/gemini-2.5-pro-preview",      vision: true  },
    OnboardingModel { provider_id: "openrouter", model_id: "anthropic/claude-opus-4-6",           vision: true  },
    OnboardingModel { provider_id: "openrouter", model_id: "deepseek/deepseek-r1",                vision: false },
    OnboardingModel { provider_id: "openrouter", model_id: "meta-llama/llama-3.3-70b-instruct",  vision: false },
    // NVIDIA
    OnboardingModel { provider_id: "nvidia", model_id: "meta/llama-3.2-11b-vision-instruct",    vision: true  },
    OnboardingModel { provider_id: "nvidia", model_id: "nvidia/llama-3.1-nemotron-70b-instruct", vision: false },
    OnboardingModel { provider_id: "nvidia", model_id: "deepseek-ai/deepseek-r1",               vision: false },
    // Together
    OnboardingModel { provider_id: "together", model_id: "meta-llama/Llama-3.3-70B-Instruct-Turbo", vision: false },
    OnboardingModel { provider_id: "together", model_id: "deepseek-ai/DeepSeek-R1",                  vision: false },
    OnboardingModel { provider_id: "together", model_id: "Qwen/Qwen2.5-72B-Instruct-Turbo",          vision: false },
];

impl OnboardingState {
    pub fn new() -> Self {
        OnboardingState {
            step: OnboardingStep::Welcome,
            storage_cursor: 0,
            provider_cursor: 0,
            model_cursor: 0,
            model_scroll: 0,
            api_key_input: String::new(),
            api_key_visible: false,
            provider_focus: ProviderFocus::ProviderList,
            ollama_models: Vec::new(),
            ollama_fetched: false,
            key_status: KeyStatus::Empty,
            remember_sessions: true,
            chosen_storage: StorageLocation::Portable,
            chosen_provider: String::new(),
            chosen_base_url: String::new(),
            chosen_model: String::new(),
            chosen_api_key: String::new(),
            chosen_vision: false,
            status_line: String::new(),
            validating: false,
        }
    }

    pub fn current_provider(&self) -> Option<&'static OnboardingProvider> {
        ONBOARDING_PROVIDERS.get(self.provider_cursor)
    }

    pub fn models_for_current_provider(&self) -> Vec<&'static OnboardingModel> {
        let Some(p) = self.current_provider() else { return vec![] };
        if p.id == "ollama" { return vec![]; }
        ONBOARDING_MODELS.iter().filter(|m| m.provider_id == p.id).collect()
    }

    pub fn selected_model_id(&self) -> &str {
        let p = match self.current_provider() { Some(p) => p, None => return "" };
        if p.id == "ollama" {
            return self.ollama_models.get(self.model_cursor).map(|s| s.as_str()).unwrap_or("");
        }
        let models = self.models_for_current_provider();
        models.get(self.model_cursor).map(|m| m.model_id).unwrap_or(p.default_model)
    }

    pub fn selected_model_vision(&self) -> bool {
        let p = match self.current_provider() { Some(p) => p, None => return false };
        if p.id == "ollama" {
            return self.ollama_models.get(self.model_cursor)
                .map(|n| crate::ai::ollama::is_vision_model(n))
                .unwrap_or(false);
        }
        let models = self.models_for_current_provider();
        models.get(self.model_cursor).map(|m| m.vision).unwrap_or(false)
    }

    // Commit the current provider step choices into the `chosen_*` fields.
    pub fn commit_provider(&mut self) {
        let p = match self.current_provider() { Some(p) => p, None => return };
        self.chosen_provider = p.id.to_string();
        self.chosen_base_url = p.base_url.to_string();
        self.chosen_model    = self.selected_model_id().to_string();
        self.chosen_api_key  = self.api_key_input.clone();
        self.chosen_vision   = self.selected_model_vision();
    }

    // Build the final AppConfig from accumulated choices.
    pub fn build_config(&self) -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.storage = self.chosen_storage.clone();
        cfg.ai = AiProviderConfig {
            provider:      self.chosen_provider.clone(),
            base_url:      self.chosen_base_url.clone(),
            api_key:       self.chosen_api_key.clone(),
            model:         self.chosen_model.clone(),
            vision_capable: self.chosen_vision,
        };
        cfg.session.remember_sessions = self.remember_sessions;
        cfg.setup_complete = true;
        cfg
    }

    pub fn key_hint_url(&self) -> &'static str {
        match self.current_provider().map(|p| p.id).unwrap_or("") {
            "openai"     => "platform.openai.com/api-keys",
            "anthropic"  => "console.anthropic.com/keys",
            "gemini"     => "aistudio.google.com/apikey",
            "mistral"    => "console.mistral.ai/api-keys",
            "groq"       => "console.groq.com/keys",
            "openrouter" => "openrouter.ai/keys",
            "nvidia"    => "build.nvidia.com",
            "together"   => "api.together.xyz/settings/api-keys",
            _            => "",
        }
    }
}