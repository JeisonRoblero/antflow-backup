use crate::config::AppConfig;
use crate::db;
use rusqlite::Connection;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: String,
    pub name: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum AppMode {
    Onboarding,
    Chat,
    CommandMenu,
    ModelSelect,
    TaskManager,
    Help,
    ConversationList,
    ConversationRename,
    ConversationDeleteConfirm,
    ProviderSetup,
    ApiKeyInput,
    DownloadConfirm,
    KeyManager,
    Clarification,
}

#[derive(Debug, Clone)]
pub struct TaskEntry {
    pub id: String,
    pub prompt: String,
    pub next_run: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProviderEntry {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size_gb: f32,
    pub vision: bool,
    pub installed: bool,
}

pub const COMMANDS: &[(&str, &str)] = &[
    ("/new",           "New conversation"),
    ("/conversations", "Browse conversations"),
    ("/model",         "Switch AI model"),
    ("/mode",          "Change agent mode (auto/chat/task)"),
    ("/keys",          "Manage API keys"),
    ("/tasks",         "Manage scheduled tasks"),
    ("/clear",         "Clear chat history"),
    ("/help",          "Show help"),
    ("/info",          "Show system info"),
    ("/import",        "Import local GGUF model: /import <name> <path>"),
    ("/quit",          "Quit Antflow"),
];

#[allow(dead_code)]
pub struct ProviderModel {
    pub provider_id: &'static str,
    pub provider_name: &'static str,
    pub base_url: &'static str,
    pub model_id: &'static str,
    pub vision: bool,
    pub custom_auth: bool,
}

pub const CLOUD_MODELS: &[ProviderModel] = &[
    // ── OpenAI ──────────────────────────────────────────────────────────────
    ProviderModel { provider_id: "openai",    provider_name: "OpenAI",    base_url: "https://api.openai.com/v1",                                  model_id: "gpt-4.1",                               vision: true,  custom_auth: false },
    ProviderModel { provider_id: "openai",    provider_name: "OpenAI",    base_url: "https://api.openai.com/v1",                                  model_id: "gpt-4.1-mini",                          vision: true,  custom_auth: false },
    ProviderModel { provider_id: "openai",    provider_name: "OpenAI",    base_url: "https://api.openai.com/v1",                                  model_id: "gpt-4o",                                vision: true,  custom_auth: false },
    ProviderModel { provider_id: "openai",    provider_name: "OpenAI",    base_url: "https://api.openai.com/v1",                                  model_id: "gpt-4o-mini",                           vision: true,  custom_auth: false },
    ProviderModel { provider_id: "openai",    provider_name: "OpenAI",    base_url: "https://api.openai.com/v1",                                  model_id: "o3",                                    vision: false, custom_auth: false },
    ProviderModel { provider_id: "openai",    provider_name: "OpenAI",    base_url: "https://api.openai.com/v1",                                  model_id: "o4-mini",                               vision: false, custom_auth: false },
    // ── Anthropic ───────────────────────────────────────────────────────────
    ProviderModel { provider_id: "anthropic", provider_name: "Anthropic", base_url: "https://api.anthropic.com/v1",                               model_id: "claude-opus-4-6",                       vision: true,  custom_auth: true  },
    ProviderModel { provider_id: "anthropic", provider_name: "Anthropic", base_url: "https://api.anthropic.com/v1",                               model_id: "claude-sonnet-4-6",                     vision: true,  custom_auth: true  },
    ProviderModel { provider_id: "anthropic", provider_name: "Anthropic", base_url: "https://api.anthropic.com/v1",                               model_id: "claude-haiku-4-5-20251001",             vision: true,  custom_auth: true  },
    ProviderModel { provider_id: "anthropic", provider_name: "Anthropic", base_url: "https://api.anthropic.com/v1",                               model_id: "claude-3-5-sonnet-20241022",            vision: true,  custom_auth: true  },
    ProviderModel { provider_id: "anthropic", provider_name: "Anthropic", base_url: "https://api.anthropic.com/v1",                               model_id: "claude-3-opus-20240229",               vision: true,  custom_auth: true  },
    // ── Google ──────────────────────────────────────────────────────────────
    ProviderModel { provider_id: "gemini",    provider_name: "Google",    base_url: "https://generativelanguage.googleapis.com/v1beta/openai",    model_id: "gemini-3-flash-preview",                vision: true,  custom_auth: false },
    ProviderModel { provider_id: "gemini",    provider_name: "Google",    base_url: "https://generativelanguage.googleapis.com/v1beta/openai",    model_id: "gemini-2.5-pro",                        vision: true,  custom_auth: false },
    ProviderModel { provider_id: "gemini",    provider_name: "Google",    base_url: "https://generativelanguage.googleapis.com/v1beta/openai",    model_id: "gemini-2.5-flash",                      vision: true,  custom_auth: false },
    ProviderModel { provider_id: "gemini",    provider_name: "Google",    base_url: "https://generativelanguage.googleapis.com/v1beta/openai",    model_id: "gemini-2.5-flash-lite",                 vision: true,  custom_auth: false },
    ProviderModel { provider_id: "gemini",    provider_name: "Google",    base_url: "https://generativelanguage.googleapis.com/v1beta/openai",    model_id: "gemini-2.0-flash",                      vision: true,  custom_auth: false },
    // ── Mistral ─────────────────────────────────────────────────────────────
    ProviderModel { provider_id: "mistral",   provider_name: "Mistral",   base_url: "https://api.mistral.ai/v1",                                  model_id: "mistral-large-latest",                  vision: false, custom_auth: false },
    ProviderModel { provider_id: "mistral",   provider_name: "Mistral",   base_url: "https://api.mistral.ai/v1",                                  model_id: "mistral-small-latest",                  vision: false, custom_auth: false },
    ProviderModel { provider_id: "mistral",   provider_name: "Mistral",   base_url: "https://api.mistral.ai/v1",                                  model_id: "codestral-latest",                      vision: false, custom_auth: false },
    // ── Groq ────────────────────────────────────────────────────────────────
    ProviderModel { provider_id: "groq",      provider_name: "Groq",      base_url: "https://api.groq.com/openai/v1",                             model_id: "llama-3.3-70b-versatile",               vision: false, custom_auth: false },
    ProviderModel { provider_id: "groq",      provider_name: "Groq",      base_url: "https://api.groq.com/openai/v1",                             model_id: "llama-3.1-8b-instant",                  vision: false, custom_auth: false },
    ProviderModel { provider_id: "groq",      provider_name: "Groq",      base_url: "https://api.groq.com/openai/v1",                             model_id: "llama-3.2-11b-vision-preview",          vision: true,  custom_auth: false },
    ProviderModel { provider_id: "groq",      provider_name: "Groq",      base_url: "https://api.groq.com/openai/v1",                             model_id: "deepseek-r1-distill-llama-70b",         vision: false, custom_auth: false },
    // ── NVIDIA NIM ──────────────────────────────────────────────────────────
    ProviderModel { provider_id: "nvidia",    provider_name: "NVIDIA",    base_url: "https://integrate.api.nvidia.com/v1",                        model_id: "deepseek-ai/deepseek-v4-pro",           vision: false, custom_auth: false },
    ProviderModel { provider_id: "nvidia",    provider_name: "NVIDIA",    base_url: "https://integrate.api.nvidia.com/v1",                        model_id: "deepseek-ai/deepseek-v4-flash",         vision: false, custom_auth: false },
    ProviderModel { provider_id: "nvidia",    provider_name: "NVIDIA",    base_url: "https://integrate.api.nvidia.com/v1",                        model_id: "deepseek-ai/deepseek-r1",               vision: false, custom_auth: false },
    ProviderModel { provider_id: "nvidia",    provider_name: "NVIDIA",    base_url: "https://integrate.api.nvidia.com/v1",                        model_id: "nvidia/llama-3.1-nemotron-70b-instruct",vision: false, custom_auth: false },
    ProviderModel { provider_id: "nvidia",    provider_name: "NVIDIA",    base_url: "https://integrate.api.nvidia.com/v1",                        model_id: "meta/llama-3.2-11b-vision-instruct",   vision: true,  custom_auth: false },
    ProviderModel { provider_id: "nvidia",    provider_name: "NVIDIA",    base_url: "https://integrate.api.nvidia.com/v1",                        model_id: "meta/llama-3.1-70b-instruct",           vision: false, custom_auth: false },
    ProviderModel { provider_id: "nvidia",    provider_name: "NVIDIA",    base_url: "https://integrate.api.nvidia.com/v1",                        model_id: "google/gemma-3-27b-it",                 vision: true,  custom_auth: false },
    // ── OpenRouter ──────────────────────────────────────────────────────────
    ProviderModel { provider_id: "openrouter",provider_name: "OpenRouter", base_url: "https://openrouter.ai/api/v1",                              model_id: "google/gemini-2.5-pro-preview",         vision: true,  custom_auth: false },
    ProviderModel { provider_id: "openrouter",provider_name: "OpenRouter", base_url: "https://openrouter.ai/api/v1",                              model_id: "anthropic/claude-opus-4-6",             vision: true,  custom_auth: false },
    ProviderModel { provider_id: "openrouter",provider_name: "OpenRouter", base_url: "https://openrouter.ai/api/v1",                              model_id: "deepseek/deepseek-r1",                  vision: false, custom_auth: false },
    ProviderModel { provider_id: "openrouter",provider_name: "OpenRouter", base_url: "https://openrouter.ai/api/v1",                              model_id: "meta-llama/llama-3.3-70b-instruct",    vision: false, custom_auth: false },
    // ── Together AI ─────────────────────────────────────────────────────────
    ProviderModel { provider_id: "together",  provider_name: "Together",  base_url: "https://api.together.xyz/v1",                                model_id: "meta-llama/Llama-3.3-70B-Instruct-Turbo", vision: false, custom_auth: false },
    ProviderModel { provider_id: "together",  provider_name: "Together",  base_url: "https://api.together.xyz/v1",                                model_id: "deepseek-ai/DeepSeek-R1",               vision: false, custom_auth: false },
    ProviderModel { provider_id: "together",  provider_name: "Together",  base_url: "https://api.together.xyz/v1",                                model_id: "Qwen/Qwen2.5-72B-Instruct-Turbo",      vision: false, custom_auth: false },
];

pub struct OllamaKnownModel {
    pub name: &'static str,
    pub size_gb: f32,
    pub vision: bool,
}

pub const OLLAMA_CATALOG: &[OllamaKnownModel] = &[
    OllamaKnownModel { name: "llama3.2:1b",          size_gb: 1.3,  vision: false },
    OllamaKnownModel { name: "llama3.2:3b",          size_gb: 2.0,  vision: false },
    OllamaKnownModel { name: "llama3.1:8b",          size_gb: 4.7,  vision: false },
    OllamaKnownModel { name: "llama3.1:70b",         size_gb: 40.0, vision: false },
    OllamaKnownModel { name: "llama3.2-vision:11b",  size_gb: 7.9,  vision: true  },
    OllamaKnownModel { name: "llama3.2-vision:90b",  size_gb: 55.0, vision: true  },
    OllamaKnownModel { name: "gemma3:1b",            size_gb: 0.8,  vision: true  },
    OllamaKnownModel { name: "gemma3:4b",            size_gb: 2.5,  vision: true  },
    OllamaKnownModel { name: "gemma3:12b",           size_gb: 7.7,  vision: true  },
    OllamaKnownModel { name: "gemma3:27b",           size_gb: 17.0, vision: true  },
    OllamaKnownModel { name: "qwen2.5:7b",           size_gb: 4.7,  vision: false },
    OllamaKnownModel { name: "qwen2.5:14b",          size_gb: 9.0,  vision: false },
    OllamaKnownModel { name: "qwen2.5:32b",          size_gb: 20.0, vision: false },
    OllamaKnownModel { name: "qwen2.5:72b",          size_gb: 47.0, vision: false },
    OllamaKnownModel { name: "qwen2.5-coder:7b",     size_gb: 4.7,  vision: false },
    OllamaKnownModel { name: "qwen2.5-coder:32b",    size_gb: 20.0, vision: false },
    OllamaKnownModel { name: "mistral:7b",           size_gb: 4.1,  vision: false },
    OllamaKnownModel { name: "mistral-nemo:12b",     size_gb: 7.1,  vision: false },
    OllamaKnownModel { name: "phi4:14b",             size_gb: 9.1,  vision: false },
    OllamaKnownModel { name: "phi4-mini:3.8b",       size_gb: 2.5,  vision: false },
    OllamaKnownModel { name: "deepseek-r1:7b",       size_gb: 4.7,  vision: false },
    OllamaKnownModel { name: "deepseek-r1:14b",      size_gb: 9.0,  vision: false },
    OllamaKnownModel { name: "deepseek-r1:32b",      size_gb: 20.0, vision: false },
    OllamaKnownModel { name: "deepseek-r1:70b",      size_gb: 43.0, vision: false },
    OllamaKnownModel { name: "llava:7b",             size_gb: 4.7,  vision: true  },
    OllamaKnownModel { name: "llava:13b",            size_gb: 8.0,  vision: true  },
    OllamaKnownModel { name: "minicpm-v:8b",         size_gb: 5.5,  vision: true  },
    OllamaKnownModel { name: "codellama:7b",         size_gb: 3.8,  vision: false },
    OllamaKnownModel { name: "codellama:13b",        size_gb: 7.4,  vision: false },
    OllamaKnownModel { name: "codellama:34b",        size_gb: 19.0, vision: false },
    OllamaKnownModel { name: "nomic-embed-text",     size_gb: 0.3,  vision: false },
    OllamaKnownModel { name: "mxbai-embed-large",    size_gb: 0.7,  vision: false },
];

#[allow(dead_code)]
pub struct App {
    pub mode: AppMode,
    pub input: String,
    pub cursor_pos: usize,
    pub messages: Vec<ChatMessage>,
    pub conversations: Vec<Conversation>,
    pub conversation_cursor: usize,
    pub current_conversation_id: String,
    pub current_conversation_name: String,
    pub rename_input: String,
    pub tasks: Vec<TaskEntry>,
    pub installed_models: Vec<String>,
    pub ollama_model_infos: Vec<OllamaModelInfo>,
    pub available_providers: Vec<ProviderEntry>,
    pub model_cursor: usize,
    pub task_cursor: usize,
    pub command_menu_cursor: usize,
    pub provider_cursor: usize,
    pub status: String,
    pub thinking: bool,
    pub thinking_expanded: bool,
    pub thinking_log: Vec<String>,
    pub scroll_offset: usize,
    pub cfg: AppConfig,
    pub confirm_quit: bool,
    pub tokens_used: u32,
    pub token_limit: u32,
    pub thinking_secs: u64,
    pub thinking_start: Option<std::time::Instant>,
    pub splash_done: bool,
    pub splash_start: std::time::Instant,
    pub run_dir: String,
    pub search_input: String,
    pub conversation_list_cursor: usize,
    pub model_search: String,
    pub conv_search: String,
    pub api_key_input: String,
    pub pending_model: String,
    pub pending_provider: String,
    pub pending_base_url: String,
    pub model_scroll_offset: usize,
    pub recent_models: Vec<String>,
    pub pending_download_model: String,
    pub provider_keys: std::collections::HashMap<String, String>,
    pub provider_keys_multi: std::collections::HashMap<String, Vec<String>>,
    pub provider_key_index: std::collections::HashMap<String, usize>,
    pub key_manager_provider: String,
    pub key_manager_cursor: usize,
    pub key_manager_adding: bool,
    pub key_manager_input: String,
    pub device_screen_w: u32,
    pub device_screen_h: u32,
    pub device_screen_scale: f64,
    pub panel_visible: bool,
    pub panel_width: u16,
    pub panel_scroll: usize,
    pub device_cpu_cores: usize,
    pub device_cpu_logical: usize,
    pub device_cpu_model: String,
    pub device_cpu_arch: String,
    pub device_cpu_freq_mhz: u64,
    pub device_ram_mb: u64,
    pub device_os_edition: String,
    pub device_gpu: String,
    pub device_touch: bool,
    pub device_hostname: String,
    pub device_timezone: String,
    pub device_screens: Vec<crate::device::ScreenInfo>,
    pub device_gpus: Vec<String>,
    pub device_changed: bool,
    pub fetched_cloud_models: Vec<crate::tui::model_fetch::FetchedModel>,
    pub fetching_models: bool,
    #[cfg(feature = "tui")]
    pub fetch_rx: Option<tokio::sync::mpsc::Receiver<crate::tui::runner::FetchResult>>,
    #[cfg(not(feature = "tui"))]
    pub fetch_rx: Option<tokio::sync::mpsc::Receiver<String>>,
    pub stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub fetch_started: Option<std::time::Instant>,
    #[cfg(feature = "tui")]
    pub onboarding: Option<crate::tui::onboarding::OnboardingState>,
    #[cfg(not(feature = "tui"))]
    pub onboarding: Option<()>,
    pub clarification_question: Option<String>,
    pub clarification_options: Vec<String>,
    pub clarification_cursor: usize,
    pub clarification_tx: Option<tokio::sync::mpsc::Sender<String>>,
    pub welcome_message: String,
    pub hf_search_results: Vec<crate::ai::ollama::HfModelInfo>,
    pub hf_searching: bool,
}

impl App {
    pub fn new(cfg: AppConfig, conn: &Connection, session_id: &str) -> Self {
        let tasks = db::load_tasks(conn).unwrap_or_default().into_iter().map(|t| {
            TaskEntry {
                id: t.id.chars().take(8).collect(),
                prompt: t.prompt.chars().take(30).collect(),
                next_run: t.next_run.format("%m-%d %H:%M").to_string(),
                enabled: t.enabled,
            }
        }).collect();

        let conversations = load_conversations(conn);
        let run_dir = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());

        let token_limit = estimate_token_limit(&cfg.ai.model);

        #[cfg(feature = "tui")]
        let onboarding = if !cfg.setup_complete {
            Some(crate::tui::onboarding::OnboardingState::new())
        } else {
            None
        };
        #[cfg(not(feature = "tui"))]
        let onboarding = None;
        let initial_mode = if !cfg.setup_complete {
            AppMode::Onboarding
        } else {
            AppMode::Chat
        };

        App {
            mode: initial_mode,
            input: String::new(),
            cursor_pos: 0,
            messages: vec![],
            conversations,
            conversation_cursor: 0,
            current_conversation_id: session_id.to_string(),
            current_conversation_name: String::new(),
            rename_input: String::new(),
            tasks,
            installed_models: Vec::new(),
            ollama_model_infos: Vec::new(),
            available_providers: Vec::new(),
            model_cursor: 0,
            task_cursor: 0,
            command_menu_cursor: 0,
            provider_cursor: 0,
            status: String::from("Ready"),
            thinking: false,
            thinking_expanded: false,
            thinking_log: Vec::new(),
            scroll_offset: 0,
            cfg,
            confirm_quit: false,
            tokens_used: 0,
            token_limit,
            thinking_secs: 0,
            thinking_start: None,
            splash_done: false,
            splash_start: std::time::Instant::now(),
            run_dir,
            search_input: String::new(),
            conversation_list_cursor: 0,
            model_search: String::new(),
            conv_search: String::new(),
            api_key_input: String::new(),
            pending_model: String::new(),
            pending_provider: String::new(),
            pending_base_url: String::new(),
            model_scroll_offset: 0,
            recent_models: Vec::new(),
            pending_download_model: String::new(),
            provider_keys: std::collections::HashMap::new(),
            provider_keys_multi: std::collections::HashMap::new(),
            provider_key_index: std::collections::HashMap::new(),
            key_manager_provider: String::new(),
            key_manager_cursor: 0,
            key_manager_adding: false,
            key_manager_input: String::new(),
            device_screen_w: 0,
            device_screen_h: 0,
            device_screen_scale: 1.0,
            panel_visible: true,
            panel_width: 26,
            panel_scroll: 0,
            device_cpu_cores: 0,
            device_cpu_logical: 0,
            device_cpu_model: String::new(),
            device_cpu_arch: String::new(),
            device_cpu_freq_mhz: 0,
            device_ram_mb: 0,
            device_os_edition: String::new(),
            device_gpu: String::new(),
            device_touch: false,
            device_hostname: String::new(),
            device_timezone: String::new(),
            device_screens: Vec::new(),
            device_gpus: Vec::new(),
            device_changed: false,
            fetched_cloud_models: Vec::new(),
            fetching_models: false,
            fetch_rx: None,
            stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            fetch_started: None,
            onboarding,
            clarification_question: None,
            clarification_options: Vec::new(),
            clarification_cursor: 0,
            clarification_tx: None,
            welcome_message: String::from("How can I help you today?"),
            hf_search_results: Vec::new(),
            hf_searching: false,
        }
    }

    pub fn push_message(&mut self, role: &str, content: &str) {
        use chrono::Local;
        self.messages.push(ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Local::now().format("%H:%M").to_string(),
        });
        self.scroll_to_bottom();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 999999;
    }

    #[allow(dead_code)]
    pub fn push_thinking_log(&mut self, line: &str) {
        self.thinking_log.push(line.to_string());
    }

    pub fn replace_last_assistant(&mut self, content: &str) {
        if let Some(last) = self.messages.iter_mut().rev()
            .find(|m| m.role == "assistant") {
            last.content = content.to_string();
        }
        self.scroll_to_bottom();
    }

    pub fn tick(&mut self) {
        if !self.splash_done && self.splash_start.elapsed().as_millis() >= 1800 {
            self.splash_done = true;
        }
        if self.thinking {
            if let Some(start) = self.thinking_start {
                self.thinking_secs = start.elapsed().as_secs();
            }
        }
    }

    pub fn start_thinking(&mut self) {
        self.thinking = true;
        self.thinking_start = Some(std::time::Instant::now());
        self.thinking_secs = 0;
        self.thinking_log.clear();
    }

    pub fn stop_thinking(&mut self) {
        self.thinking = false;
        self.thinking_start = None;
    }

    pub fn conversation_title(&self) -> String {
        if !self.current_conversation_name.is_empty() {
            self.current_conversation_name.clone()
        } else {
            // Auto-name from first user message
            self.messages.iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.chars().take(30).collect())
                .unwrap_or_default()
        }
    }
}

fn load_conversations(conn: &Connection) -> Vec<Conversation> {
    // Group messages by session_id, get latest message per session
    let mut stmt = match conn.prepare(
        "SELECT session_id, MAX(created_at), content FROM messages
         WHERE role = 'user'
         GROUP BY session_id
         ORDER BY MAX(created_at) DESC
         LIMIT 50"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([], |r| {
        Ok(Conversation {
            id: r.get(0)?,
            name: String::new(),
            preview: r.get::<_, String>(2).unwrap_or_default()
                .chars().take(35).collect(),
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// Returns all keys for all providers: provider_id -> Vec<api_key>
pub fn load_provider_keys_multi(conn: &Connection) -> std::collections::HashMap<String, Vec<String>> {
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS provider_keys_v2 (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id TEXT NOT NULL,
            api_key TEXT NOT NULL,
            UNIQUE(provider_id, api_key)
        )",
        [],
    );
    // Migrate from old single-key table if it exists
    let _ = conn.execute(
        "INSERT OR IGNORE INTO provider_keys_v2 (provider_id, api_key)
         SELECT provider_id, api_key FROM provider_keys",
         [],
    );
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT provider_id, api_key FROM provider_keys_v2 ORDER BY id ASC"
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                map.entry(row.0).or_default().push(row.1);
            }
        }
    }
    map
}

/// Flat map for backward compat — returns first key per provider
pub fn load_provider_keys(conn: &Connection) -> std::collections::HashMap<String, String> {
    load_provider_keys_multi(conn)
        .into_iter()
        .filter_map(|(k, v)| v.into_iter().next().map(|key| (k, key)))
        .collect()
}

pub fn save_provider_key(conn: &Connection, provider_id: &str, api_key: &str) {
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS provider_keys_v2 (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id TEXT NOT NULL,
            api_key TEXT NOT NULL,
            UNIQUE(provider_id, api_key)
        )",
        [],
    );
    let _ = conn.execute(
        "INSERT OR IGNORE INTO provider_keys_v2 (provider_id, api_key) VALUES (?1, ?2)",
        rusqlite::params![provider_id, api_key],
    );
}

pub fn delete_provider_key(conn: &Connection, provider_id: &str, api_key: &str) {
    let _ = conn.execute(
        "DELETE FROM provider_keys_v2 WHERE provider_id = ?1 AND api_key = ?2",
        rusqlite::params![provider_id, api_key],
    );
}

pub fn save_active_key_index(conn: &Connection, provider_id: &str, index: usize) {
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS provider_key_active (
            provider_id TEXT PRIMARY KEY,
            key_index INTEGER NOT NULL DEFAULT 0
        )",
        [],
    );
    let _ = conn.execute(
        "INSERT INTO provider_key_active (provider_id, key_index) VALUES (?1, ?2)
         ON CONFLICT(provider_id) DO UPDATE SET key_index = excluded.key_index",
        rusqlite::params![provider_id, index as i64],
    );
}

pub fn load_active_key_indices(conn: &Connection) -> std::collections::HashMap<String, usize> {
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS provider_key_active (
            provider_id TEXT PRIMARY KEY,
            key_index INTEGER NOT NULL DEFAULT 0
        )",
        [],
    );
    let mut map = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT provider_id, key_index FROM provider_key_active"
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        }) {
            for row in rows.flatten() {
                map.insert(row.0, row.1 as usize);
            }
        }
    }
    map
}

pub fn estimate_token_limit(model: &str) -> u32 {
    let lower = model.to_lowercase();
    if lower.contains("gemma4") || lower.contains("gemma-4") { return 131072; }
    if lower.contains("gemma3") { return 128000; }
    if lower.contains("llama3.3") || lower.contains("llama-3.3") { return 131072; }
    if lower.contains("llama3.2") || lower.contains("llama-3.2") { return 131072; }
    if lower.contains("llama3.1") || lower.contains("llama-3.1") { return 131072; }
    if lower.contains("llama3") || lower.contains("llama-3") { return 8192; }
    if lower.contains("llama2") { return 4096; }
    if lower.contains("qwen2.5") { return 131072; }
    if lower.contains("qwen2") { return 32768; }
    if lower.contains("mistral") { return 32768; }
    if lower.contains("mixtral") { return 32768; }
    if lower.contains("deepseek") { return 65536; }
    if lower.contains("phi4") { return 16384; }
    if lower.contains("phi3") { return 4096; }
    if lower.contains("gpt-4o") { return 128000; }
    if lower.contains("gpt-4") { return 8192; }
    if lower.contains("gpt-3.5") { return 16385; }
    if lower.contains("claude-3-5") { return 200000; }
    if lower.contains("claude-3") { return 200000; }
    if lower.contains("claude-4") || lower.contains("claude-sonnet-4") || lower.contains("claude-opus-4") || lower.contains("claude-haiku-4") { return 200000; }
    if lower.contains("gemini-3") { return 1000000; }
    if lower.contains("gemini-2.5") { return 1000000; }
    if lower.contains("gemini-2") { return 1000000; }
    if lower.contains("gemini-1.5") { return 1000000; }
    if lower.contains("llava") { return 4096; }
    8192 // default
}