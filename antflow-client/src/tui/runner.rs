use anyhow::Result;
#[cfg(windows)]
#[allow(unused_imports)]
use std::os::windows::process::CommandExt;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use rusqlite::Connection;

use crate::ai;
use crate::config::{self, AppConfig};
use crate::db;
use crate::executor;
use crate::redact::RedactPatterns;
use super::app::{App, AppMode};
use super::events::{handle_events, AppEvent};
use super::ui;

// Debug logging macro (only active in debug builds)
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            let log_path = std::env::temp_dir().join("antflow_debug.log");
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true).append(true)
                .open(&log_path)
            {
                use std::io::Write;
                let _ = writeln!(f, $($arg)*);
            }
        }
    };
}

#[allow(dead_code)]
pub enum FetchResult {
    Local(Vec<crate::ai::ollama::OllamaModel>),
    Cloud(Vec<crate::tui::model_fetch::FetchedModel>),
    HfSearch { query: String, results: Vec<crate::ai::ollama::HfModelInfo> },
    ImportProgress { status: String, percent: u32 },
    ImportFinished { name: String },
    ImportFailed { name: String, error: String },
}

#[cfg(not(windows))]
fn isolate_console() {
}

#[cfg(windows)]
fn isolate_console() {
    unsafe {
        let _ = windows_sys::Win32::System::Console::AllocConsole();
    }
}

pub async fn run(cfg: AppConfig, conn: Connection, storage_dir: std::path::PathBuf) -> Result<()> {
    dbg_log!("=== TUI run() started ===");

    #[cfg(debug_assertions)]
    {
        let panic_path = std::env::temp_dir().join("antflow_panic.txt");
        std::panic::set_hook(Box::new(move |info| {
            let _ = std::fs::write(&panic_path, format!("{}", info));
        }));
        let log_path = std::env::temp_dir().join("antflow_debug.log");
        let _ = std::fs::write(&log_path, "");
        dbg_log!("=== panic hook set ===");
    }

    ctrlc::set_handler(|| {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);
        std::process::exit(0);
    }).ok();

    isolate_console();

    // Auto-start Ollama at launch if not running
    let mut installed_models: Vec<String> = Vec::new();
    // Always try to contact Ollama regardless of current provider
    {
        let ollama_url = "http://localhost:11434".to_string();
        if !crate::ai::ollama::is_running(&ollama_url).await {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                let mut cmd = tokio::process::Command::new("ollama");
                cmd.arg("serve")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                cmd.env("OLLAMA_NUM_PARALLEL", cfg.num_parallel.to_string());
                std::env::set_var("OLLAMA_NUM_PARALLEL", cfg.num_parallel.to_string());
                #[cfg(windows)]
                cmd.creation_flags(0x08000000);
                let _ = cmd.spawn();
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }
        }
        if let Ok(models) = crate::ai::ollama::list_models(&ollama_url).await {
            installed_models = models.into_iter().map(|m| m.name).collect();
        }
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let (welcome_tx, mut welcome_rx) = tokio::sync::mpsc::channel::<String>(1);
    let cfg_ai_clone = cfg.ai.clone();
    tokio::spawn(async move {
        if !cfg_ai_clone.provider.is_empty() && !cfg_ai_clone.model.is_empty() {
            let lang = crate::ai::get_system_lang();
            if let Ok(msg) = crate::ai::generate_welcome_greeting(&cfg_ai_clone, &lang).await {
                let _ = welcome_tx.send(msg).await;
            }
        }
    });
    let mut app = App::new(cfg.clone(), &conn, &session_id);
    app.installed_models = installed_models;
    // Build OllamaModelInfo list from catalog only (fast, no per-model API calls at startup)
    {
        use crate::tui::app::{OllamaModelInfo, OLLAMA_CATALOG};

        let mut infos: Vec<OllamaModelInfo> = Vec::new();
        let installed_set: std::collections::HashSet<String> = app.installed_models.iter().cloned().collect();

        for name in &app.installed_models {
            let known = OLLAMA_CATALOG.iter().find(|k| {
                name.starts_with(k.name) || k.name.starts_with(name.split(':').next().unwrap_or(name))
            });
            infos.push(OllamaModelInfo {
                name: name.clone(),
                size_gb: known.map(|k| k.size_gb).unwrap_or(0.0),
                vision: known.map(|k| k.vision)
                    .unwrap_or_else(|| crate::ai::ollama::is_vision_model(name)),
                installed: true,
});
        }

        for k in OLLAMA_CATALOG {
            if !installed_set.contains(k.name) {
                infos.push(OllamaModelInfo {
                    name: k.name.to_string(),
                    size_gb: k.size_gb,
                    vision: k.vision,
                    installed: false,
                });
            }
        }

        app.ollama_model_infos = infos;
    }

    // Load all saved provider keys from DB (multi-key)
    app.provider_keys_multi = crate::tui::app::load_provider_keys_multi(&conn);
    // Also ensure current config key is saved
    if !cfg.ai.api_key.is_empty() && !cfg.ai.provider.is_empty() && cfg.ai.provider != "ollama" {
        crate::tui::app::save_provider_key(&conn, &cfg.ai.provider, &cfg.ai.api_key);
        app.provider_keys_multi
            .entry(cfg.ai.provider.clone())
            .or_default()
            .push(cfg.ai.api_key.clone());
        // dedup
        app.provider_keys_multi.get_mut(&cfg.ai.provider).map(|v| v.dedup());
    }
    // Load persisted active key indices so the last-used key is active on restart
    app.provider_key_index = crate::tui::app::load_active_key_indices(&conn);
    // Rebuild flat map using the persisted or default index
    app.provider_keys = app.provider_keys_multi.iter()
        .filter_map(|(k, v)| {
            let idx = app.provider_key_index.get(k).copied().unwrap_or(0).min(v.len().saturating_sub(1));
            v.get(idx).map(|key| (k.clone(), key.clone()))
        })
        .collect();
    // Apply the active key to the current cfg if provider matches
    if let Some(keys) = app.provider_keys_multi.get(&app.cfg.ai.provider) {
        let idx = app.provider_key_index.get(&app.cfg.ai.provider).copied().unwrap_or(0).min(keys.len().saturating_sub(1));
        if let Some(key) = keys.get(idx) {
            if !key.is_empty() {
                app.cfg.ai.api_key = key.clone();
            }
        }
    }

    // Setup terminal and history
    let device_info = crate::device::collect()?;
    let system_prompt = crate::prompt::build_system_prompt(&device_info);
    app.device_screen_w = device_info.screen.width;
    app.device_screen_h = device_info.screen.height;
    app.device_screen_scale = device_info.screen.scale_factor;
    app.device_screens = device_info.screens.clone();
    app.device_gpus = device_info.gpus.clone();
    app.device_cpu_cores = device_info.cpu_cores;
    app.device_cpu_logical = device_info.cpu_logical_cores;
    app.device_cpu_model = device_info.cpu_model.clone();
    app.device_cpu_arch = device_info.cpu_arch.clone();
    app.device_cpu_freq_mhz = device_info.cpu_freq_mhz;
    app.device_ram_mb = device_info.ram_mb;
    app.device_os_edition = device_info.os_edition.clone();
    app.device_gpu = device_info.gpu.clone();
    app.device_touch = device_info.touch_capable;
    app.device_hostname = device_info.hostname.clone();
    app.device_timezone = device_info.timezone_offset.clone();
    crossterm::execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    ).ok();
    enable_raw_mode().ok();
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    let mut history: Vec<crate::ai::ChatMessage> = vec![
        crate::ai::ChatMessage::text("system", &system_prompt),
    ];
    let patterns = RedactPatterns::new();
    let mut last_model_search = String::new();
    let mut active_fetch_tx: Option<tokio::sync::mpsc::Sender<FetchResult>> = None;

    // Main event loop
    'main: loop {
        // Force-clear fetch after 8 seconds
        if app.fetching_models {
            if let Some(started) = app.fetch_started {
                if started.elapsed().as_secs() > 8 {
                    app.fetching_models = false;
                    app.fetch_rx = None;
                    app.fetch_started = None;
                }
            }
        }

        // Drain fetch results channel in main loop
        {
            let mut rx = app.fetch_rx.take();
            let mut cloud_models = None;
            let mut needs_clear = false;
            if let Some(ref mut r) = rx {
                while let Ok(result) = r.try_recv() {
                    match result {
                        FetchResult::Local(models) => {
                            let installed: std::collections::HashSet<String> =
                                models.iter().map(|m| m.name.clone()).collect();
                            for info in app.ollama_model_infos.iter_mut() {
                                info.installed = installed.contains(&info.name);
                            }
                            use crate::tui::app::{OllamaModelInfo, OLLAMA_CATALOG};
                            for m in &models {
                                if !app.ollama_model_infos.iter().any(|i| i.name == m.name) {
                                    let known = OLLAMA_CATALOG.iter()
                                        .find(|k| m.name.starts_with(k.name));
                                    app.ollama_model_infos.push(OllamaModelInfo {
                                        name: m.name.clone(),
                                        size_gb: known.map(|k| k.size_gb).unwrap_or(0.0),
                                        vision: known.map(|k| k.vision)
                                            .unwrap_or_else(|| crate::ai::ollama::is_vision_model(&m.name)),
                                        installed: true,
                                    });
                                }
                            }
                        }
                        FetchResult::Cloud(models) => {
                            cloud_models = Some(models);
                            needs_clear = true;
                        }
                        FetchResult::HfSearch { query, results } => {
                            if query == app.model_search {
                                app.hf_search_results = results;
                                app.hf_searching = false;
                            }
                        }
                        FetchResult::ImportProgress { status, percent } => {
                            app.status = format!("Importing: {} ({}%)", status, percent);
                        }
                        FetchResult::ImportFinished { name } => {
                            app.status = "Ready".to_string();
                            app.push_message("system", &format!("✓ Successfully imported GGUF model: {}", name));
                            app.installed_models.push(name.clone());
                            if let Some(info) = app.ollama_model_infos.iter_mut().find(|m| m.name == name) {
                                info.installed = true;
                            } else {
                                app.ollama_model_infos.push(crate::tui::app::OllamaModelInfo {
                                    name: name.clone(),
                                    size_gb: 0.0,
                                    vision: crate::ai::ollama::is_vision_model(&name),
                                    installed: true,
                                });
                            }
                        }
                        FetchResult::ImportFailed { name, error } => {
                            app.status = "Ready".to_string();
                            app.push_message("system", &format!("✗ GGUF import failed for {}: {}", name, error));
                        }
                    }
                }
            }
            if let Some(models) = cloud_models {
                app.fetched_cloud_models = models;
                app.fetching_models = false;
            }
            if needs_clear {
                rx = None;
                app.fetch_started = None;
            }
            app.fetch_rx = rx;
        }

        // If onboarding is on Ollama provider and models not yet fetched, fetch them.
        if app.mode == AppMode::Onboarding {
            if let Some(ob) = app.onboarding.as_mut() {
                if !ob.ollama_fetched
                    && ob.step == super::onboarding::OnboardingStep::Provider
                    && ob.current_provider().map(|p| p.id == "ollama").unwrap_or(false)
                {
                    ob.ollama_fetched = true;
                    let ollama_url = "http://localhost:11434".to_string();
                    if let Ok(models) = crate::ai::ollama::list_models(&ollama_url).await {
                        if let Some(ob2) = app.onboarding.as_mut() {
                            ob2.ollama_models = models.into_iter().map(|m| m.name).collect();
                        }
                    }
                }
            }
        }

        if let Ok(msg) = welcome_rx.try_recv() {
            app.welcome_message = msg;
        }

        app.tick();

        if app.device_changed {
            app.device_changed = false;
            let mut updated_device = device_info.clone();
            updated_device.screen = crate::device::ScreenInfo {
                width: app.device_screen_w,
                height: app.device_screen_h,
                scale_factor: app.device_screen_scale,
                is_primary: true,
            };
            updated_device.screens = app.device_screens.clone();
            let new_prompt = crate::prompt::build_system_prompt(&updated_device);
            if let Some(first) = history.first_mut() {
                *first = crate::ai::ChatMessage::text("system", &new_prompt);
            }
        }

        if app.mode == AppMode::ModelSelect {
            if app.model_search != last_model_search {
                last_model_search = app.model_search.clone();
                if app.model_search.trim().is_empty() {
                    app.hf_search_results.clear();
                    app.hf_searching = false;
                } else if let Some(ref tx) = active_fetch_tx {
                    app.hf_searching = true;
                    let tx_clone = tx.clone();
                    let query_clone = app.model_search.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        let results = crate::ai::ollama::search_huggingface_gguf(&query_clone).await.unwrap_or_default();
                        let _ = tx_clone.send(FetchResult::HfSearch { query: query_clone, results }).await;
                    });
                }
            }
        } else {
            last_model_search.clear();
        }

        terminal.draw(|f| ui::draw(f, &app))?;

        // Handle events
        match handle_events(&mut app)? {
            AppEvent::Quit => {
                disable_raw_mode()?;
                crossterm::execute!(std::io::stdout(), LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
                break 'main;
            }

            AppEvent::OnboardingSkip => {
                let ollama_url = "http://localhost:11434";
                let mut new_cfg = app.cfg.clone();
                if crate::ai::ollama::is_running(ollama_url).await {
                    let models = crate::ai::ollama::list_models(ollama_url).await.unwrap_or_default();
                    let model = models.iter()
                        .find(|m| crate::ai::ollama::is_vision_model(&m.name))
                        .or_else(|| models.first())
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| "llava".to_string());
                    new_cfg.ai.provider = "ollama".to_string();
                    new_cfg.ai.base_url = ollama_url.to_string();
                    new_cfg.ai.model = model;
                    new_cfg.ai.vision_capable = true;
                    new_cfg.ai.api_key = String::new();
                    app.push_message("system", "✓ Auto-configured with local Ollama.");
                } else {
                    app.push_message("system", "No Ollama found. Use /model to configure a provider.");
                }
                new_cfg.setup_complete = true;
                new_cfg.storage = crate::config::StorageLocation::Portable;
                config::save(&storage_dir, &new_cfg)?;
                app.cfg = new_cfg;
                app.onboarding = None;
                app.mode = AppMode::Chat;
            }

            AppEvent::OnboardingValidateKey => {
                if let Some(ob) = app.onboarding.as_mut() {
                    ob.validating = true;
                    ob.status_line = "Verifying key...".to_string();
                }
                terminal.draw(|f| ui::draw(f, &app))?;

                let (provider_id, base_url, model, key) = {
                    let ob = app.onboarding.as_ref().unwrap();
                    (
                        ob.current_provider().map(|p| p.id).unwrap_or("").to_string(),
                        ob.current_provider().map(|p| p.base_url).unwrap_or("").to_string(),
                        ob.selected_model_id().to_string(),
                        ob.api_key_input.clone(),
                    )
                };

                let result = super::api_helpers::test_api_key(
                    &provider_id, &base_url, &model, &key,
                ).await;

                if let Some(ob) = app.onboarding.as_mut() {
                    ob.validating = false;
                    match result {
                        Ok(()) => {
                            ob.key_status = super::onboarding::KeyStatus::Valid;
                            ob.status_line = "✓ Key accepted".to_string();
                            ob.commit_provider();
                            ob.step = super::onboarding::OnboardingStep::Memory;
                        }
                        Err(msg) => {
                            let msg_str = msg.to_string();
                            ob.key_status = super::onboarding::KeyStatus::Invalid(msg_str.clone());
                            ob.status_line = format!("✗ {}", msg_str);
                        }
                    }
                }
            }

            AppEvent::OnboardingDone => {
                let new_cfg = {
                    let ob = app.onboarding.as_ref().unwrap();
                    ob.build_config()
                };
                let new_storage = config::resolve_storage_dir(&new_cfg.storage, &storage_dir);
                config::save(&new_storage, &new_cfg)?;
                if !new_cfg.ai.api_key.is_empty() && new_cfg.ai.provider != "ollama" {
                    super::app::save_provider_key(&conn, &new_cfg.ai.provider, &new_cfg.ai.api_key);
                    app.provider_keys_multi
                        .entry(new_cfg.ai.provider.clone())
                        .or_default()
                        .push(new_cfg.ai.api_key.clone());
                }
                app.cfg = new_cfg;
                app.token_limit = super::app::estimate_token_limit(&app.cfg.ai.model);
                app.onboarding = None;
                app.mode = AppMode::Chat;
                app.push_message("system", "✓ Setup complete — Antflow is ready.");
            }

            AppEvent::OnboardingNext | AppEvent::OnboardingBack => {}

            AppEvent::Submit(raw_prompt) => {
                // Handle voice input trigger
                let prompt = if raw_prompt.starts_with("__VOICE__:") {
                    let secs: u64 = raw_prompt["__VOICE__:".len()..].parse().unwrap_or(5);
                    app.status = format!("Recording {}s... (speak now)", secs);
                    let result = tokio::task::spawn_blocking(move || {
                        crate::tools::voice_input(&serde_json::json!({"duration_secs": secs}))
                    }).await;
                    match result {
                        Ok(Ok(Some(text))) => {
                            app.status = "Ready".to_string();
                            text
                        }
                        _ => {
                            app.push_message("system", "(Voice input failed. Check ffmpeg is installed.)");
                            app.status = "Ready".to_string();
                            continue;
                        }
                    }
                } else {
                    raw_prompt
                };

                let _ = db::insert_message(&conn, &session_id, "user", &prompt);
                app.push_message("user", &prompt);
                app.start_thinking();
                app.push_message("assistant", "▋");
                app.status = "Thinking...".to_string();
                terminal.draw(|f| ui::draw(f, &app))?;

                app.stop_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                let stop = app.stop_flag.clone();
                let provider = ai::provider_for_config(&app.cfg.ai);
                let cfg_clone = app.cfg.clone();
                // Snapshot history before the call so the retry branch can reuse it
                // without the prompt being doubled (executor pushes it internally).
                let history_snapshot = history.clone();

                // Channel to receive AI's "message" field for display in chat
                let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel::<String>(4);
                let mut last_displayed_msg = String::new();
                let (clarif_q_tx, mut clarif_q_rx) = tokio::sync::mpsc::channel::<(String, Vec<String>)>(2);
                let (clarif_a_tx, clarif_a_rx) = tokio::sync::mpsc::channel::<String>(2);
                app.clarification_tx = Some(clarif_a_tx);

                let task_future = executor::run_task(
                    &prompt,
                    &session_id,
                    history.clone(),
                    provider.as_ref(),
                    &storage_dir,
                    &cfg_clone,
                    &patterns,
                    stop,
                    msg_tx,
                    app.token_limit as usize,
                    clarif_q_tx,
                    clarif_a_rx,
                    None,
                );
                tokio::pin!(task_future);

                let response = loop {
                    // Check for clarification request from executor
                    if let Ok((question, mut options)) = clarif_q_rx.try_recv() {
                        if !options.iter().any(|o| o.to_lowercase().contains("custom") || o.to_lowercase().contains("other")) {
                            options.push("Other (type custom)".to_string());
                        }
                        app.clarification_question = Some(question.clone());
                        app.clarification_options = options;
                        app.clarification_cursor = 0;
                        app.mode = AppMode::Clarification;
                        app.stop_thinking();
                        terminal.draw(|f| ui::draw(f, &app))?;
                    }

                    // Drain AI message channel — each new message gets its own bubble
                    while let Ok(ai_msg) = msg_rx.try_recv() {
                        if ai_msg != last_displayed_msg && !ai_msg.is_empty() {
                            let clean_msg = clean_user_facing_message(&ai_msg);
                            if clean_msg.is_empty() { continue; }
                            if last_displayed_msg.is_empty() {
                                app.replace_last_assistant(&clean_msg);
                            } else {
                                app.push_message("assistant", &clean_msg);
                            }
                            last_displayed_msg = ai_msg;
                            terminal.draw(|f| ui::draw(f, &app))?;
                        }
                    }

                    // Drain fetch results channel
                    let mut rx = app.fetch_rx.take();
                    let mut cloud_models = None;
                    let mut needs_clear = false;
                    if let Some(ref mut r) = rx {
                        while let Ok(result) = r.try_recv() {
                            match result {
                                FetchResult::Local(models) => {
                                    let installed: std::collections::HashSet<String> =
                                        models.iter().map(|m| m.name.clone()).collect();
                                    for info in app.ollama_model_infos.iter_mut() {
                                        info.installed = installed.contains(&info.name);
                                    }
                                    use crate::tui::app::{OllamaModelInfo, OLLAMA_CATALOG};
                                    for m in &models {
                                        if !app.ollama_model_infos.iter().any(|i| i.name == m.name) {
                                            let known = OLLAMA_CATALOG.iter()
                                                .find(|k| m.name.starts_with(k.name));
                                            app.ollama_model_infos.push(OllamaModelInfo {
                                                name: m.name.clone(),
                                                size_gb: known.map(|k| k.size_gb).unwrap_or(0.0),
                                                vision: known.map(|k| k.vision)
                                                    .unwrap_or_else(|| crate::ai::ollama::is_vision_model(&m.name)),
                                                installed: true,
                                            });
                                        }
                                    }
                                }
                                FetchResult::Cloud(models) => {
                                    cloud_models = Some(models);
                                    needs_clear = true;
                                }
                                FetchResult::HfSearch { query, results } => {
                                    if query == app.model_search {
                                        app.hf_search_results = results;
                                        app.hf_searching = false;
                                    }
                                }
                                FetchResult::ImportProgress { status, percent } => {
                                    app.status = format!("Importing: {} ({}%)", status, percent);
                                }
                                FetchResult::ImportFinished { name } => {
                                    app.status = "Ready".to_string();
                                    app.push_message("system", &format!("✓ Successfully imported GGUF model: {}", name));
                                    app.installed_models.push(name.clone());
                                    if let Some(info) = app.ollama_model_infos.iter_mut().find(|m| m.name == name) {
                                        info.installed = true;
                                    } else {
                                        app.ollama_model_infos.push(crate::tui::app::OllamaModelInfo {
                                            name: name.clone(),
                                            size_gb: 0.0,
                                            vision: crate::ai::ollama::is_vision_model(&name),
                                            installed: true,
                                        });
                                    }
                                }
                                FetchResult::ImportFailed { name, error } => {
                                    app.status = "Ready".to_string();
                                    app.push_message("system", &format!("✗ GGUF import failed for {}: {}", name, error));
                                }
                            }
                        }
                    }
                    if let Some(models) = cloud_models {
                        app.fetched_cloud_models = models;
                        app.fetching_models = false;
                    }
                    if needs_clear {
                        rx = None;
                        app.fetch_started = None;
                    }
                    app.fetch_rx = rx;

                    app.tick();
                    terminal.draw(|f| ui::draw(f, &app))?;

                    tokio::select! {
                        biased;
                        result = &mut task_future => {
                            // Check for rate limit and rotate key if needed
                            if let Err(e) = &result {
                                let provider_id = app.cfg.ai.provider.clone();
                                if crate::ai::is_rate_limit_error(e) {
                                    let keys_len = app.provider_keys_multi.get(&provider_id).map(|k| k.len()).unwrap_or(1);
                                    if keys_len > 1 {
                                        let current_idx = app.provider_key_index.get(&provider_id).copied().unwrap_or(0);
                                        let next_idx = (current_idx + 1) % keys_len;
                                        let next_key = app.provider_keys_multi.get(&provider_id).unwrap()[next_idx].clone();
                                        // Persist the new active index
                                        app.provider_key_index.insert(provider_id.clone(), next_idx);
                                        crate::tui::app::save_active_key_index(&conn, &provider_id, next_idx);
                                        // Apply to cfg and flat map
                                        app.cfg.ai.api_key = next_key.clone();
                                        app.provider_keys.insert(provider_id.clone(), next_key.clone());
                                        let mut new_cfg = app.cfg.clone();
                                        new_cfg.ai.api_key = next_key;
                                        let _ = config::save(&storage_dir, &new_cfg);
                                        app.cfg = new_cfg;
                                        app.push_message("system", &format!(
                                            "Rate limited — switched to key {}/{} for {} (saved)",
                                            next_idx + 1, keys_len, provider_id
                                        ));
                                        break Err(anyhow::anyhow!("__RETRY__"));
                                    }
                                }
                            }
                            break result;
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(16)) => {
                            loop {
                                match crossterm::event::poll(std::time::Duration::from_millis(0)) {
                                    Ok(true) => {
                                        if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                                            if key.kind == crossterm::event::KeyEventKind::Press {
                                                if app.mode == AppMode::Clarification {
                                                    match key.code {
                                                        crossterm::event::KeyCode::Up => {
                                                            if app.clarification_cursor > 0 {
                                                                app.clarification_cursor -= 1;
                                                            }
                                                        }
                                                        crossterm::event::KeyCode::Down => {
                                                            if app.clarification_cursor + 1 < app.clarification_options.len() {
                                                                app.clarification_cursor += 1;
                                                            }
                                                        }
                                                        crossterm::event::KeyCode::Enter => {
                                                            let selected = app.clarification_options
                                                                .get(app.clarification_cursor)
                                                                .cloned()
                                                                .unwrap_or_default();
                                                            let is_custom = selected.to_lowercase().contains("custom")
                                                                || selected.to_lowercase().contains("other");
                                                            app.clarification_question = None;
                                                            app.clarification_options.clear();
                                                            app.mode = AppMode::Chat;
                                                            if !is_custom {
                                                                app.push_message("user", &selected);
                                                                app.start_thinking();
                                                                if let Some(tx) = app.clarification_tx.as_ref() {
                                                                    let _ = tx.try_send(selected);
                                                                }
                                                            } else {
                                                                if let Some(tx) = app.clarification_tx.take() {
                                                                    let _ = tx.try_send("cancelled".to_string());
                                                                }
                                                                app.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                                            }
                                                        }
                                                        crossterm::event::KeyCode::Esc => {
                                                            app.clarification_question = None;
                                                            app.clarification_options.clear();
                                                            app.mode = AppMode::Chat;
                                                            if let Some(tx) = app.clarification_tx.take() {
                                                                let _ = tx.try_send("cancelled".to_string());
                                                            }
                                                            app.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                                            app.stop_thinking();
                                                        }
                                                        _ => {}
                                                    }
                                                } else {
                                                    match key.code {
                                                        crossterm::event::KeyCode::Esc => {
                                                            app.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                                            app.status = "Stopping...".to_string();
                                                            app.replace_last_assistant("(stopped)");
                                                            app.stop_thinking();
                                                        }
                                                        crossterm::event::KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                                            app.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                                            app.stop_thinking();
                                                            disable_raw_mode()?;
                                                            crossterm::execute!(std::io::stdout(), LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
                                                            return Ok(());
                                                        }
                                                        crossterm::event::KeyCode::Char('q') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                                            app.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                                            app.stop_thinking();
                                                            disable_raw_mode()?;
                                                            crossterm::execute!(std::io::stdout(), LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
                                                            return Ok(());
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => break,
                                }
                            }
                        }
                    }
                };

                app.stop_thinking();

                // Handle rate-limit retry by re-submitting with the new key
                let response = if matches!(&response, Err(e) if e.to_string() == "__RETRY__") {
                    app.start_thinking();
                    app.replace_last_assistant("▋");
                    app.status = "Retrying with rotated key...".to_string();
                    terminal.draw(|f| ui::draw(f, &app))?;
                    app.stop_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    let provider = ai::provider_for_config(&app.cfg.ai);
                    let stop = app.stop_flag.clone();
                    let (retry_msg_tx, mut retry_msg_rx) = tokio::sync::mpsc::channel::<String>(4);
                    let (retry_clarif_q_tx, _retry_clarif_q_rx) = tokio::sync::mpsc::channel::<(String, Vec<String>)>(2);
                    let (_retry_clarif_a_tx, retry_clarif_a_rx) = tokio::sync::mpsc::channel::<String>(2);
                    let result = executor::run_task(
                        &prompt,
                        &session_id,
                        history_snapshot.clone(),
                        provider.as_ref(),
                        &storage_dir,
                        &cfg_clone,
                        &patterns,
                        stop,
                        retry_msg_tx,
                        app.token_limit as usize,
                        retry_clarif_q_tx,
                        retry_clarif_a_rx,
                        None,
                    ).await;
                    // Drain messages from retry
                    let mut retry_last = String::new();
                    while let Ok(ai_msg) = retry_msg_rx.try_recv() {
                        if ai_msg != retry_last && !ai_msg.is_empty() {
                            if retry_last.is_empty() {
                                app.replace_last_assistant(&ai_msg);
                            } else {
                                app.push_message("assistant", &ai_msg);
                            }
                            retry_last = ai_msg;
                        }
                    }
                    app.stop_thinking();
                    result
                } else {
                    response
                };

                match response {
                    Ok(response) => {
                        let total_chars: usize = history.iter()
                            .map(|m| m.content.to_string().len())
                            .sum::<usize>() + prompt.len() + response.len();
                        app.tokens_used = (total_chars / 4) as u32;
                        app.token_limit = if app.token_limit == 0 { 8192 } else { app.token_limit };

                        if response.starts_with("__SCHEDULE__") {
                            app.status = "Task scheduled.".to_string();
                        } else {
                            app.replace_last_assistant(&response);
                            history.push(crate::ai::ChatMessage::text("user", &prompt));
                            history.push(crate::ai::ChatMessage::text("assistant", &response));
                            let _ = db::insert_message(&conn, &session_id, "assistant", &response);
                            app.status = "Ready".to_string();

                            if app.current_conversation_name.is_empty()
                                && history.iter().filter(|m| m.role == "user").count() == 1
                            {
                                let name: String = prompt.chars().take(30).collect();
                                app.current_conversation_name = name.clone();
                                if let Some(c) = app.conversations.iter_mut()
                                    .find(|c| c.id == session_id) {
                                    c.name = name;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let friendly = format_api_error(&e.to_string());
                        app.replace_last_assistant(&friendly);
                        app.status = "Error - see message".to_string();
                    }
                }
            }
            AppEvent::SwitchModel(model_name) => {
                use super::app::CLOUD_MODELS;

                if let Some(pm) = CLOUD_MODELS.iter().find(|p| p.model_id == model_name) {
                    // Also check current cfg key if provider matches
                    if !app.provider_keys_multi.contains_key(pm.provider_id)
                        && app.cfg.ai.provider == pm.provider_id
                        && !app.cfg.ai.api_key.is_empty() {
                        app.provider_keys_multi
                            .insert(pm.provider_id.to_string(), vec![app.cfg.ai.api_key.clone()]);
                    }
                    // Get the first/active key for this provider
                    if let Some(keys) = app.provider_keys_multi.get(pm.provider_id) {
                        let key = keys.first().cloned().unwrap_or_default();
                        let mut new_cfg = app.cfg.clone();
                        new_cfg.ai.api_key = key;
                        new_cfg.ai.model = pm.model_id.to_string();
                        new_cfg.ai.provider = pm.provider_id.to_string();
                        new_cfg.ai.base_url = pm.base_url.to_string();
                        new_cfg.ai.vision_capable = pm.vision;  // Use CLOUD_MODELS table's vision field
                        config::save(&storage_dir, &new_cfg)?;
                        app.cfg = new_cfg;
                        app.token_limit = crate::tui::app::estimate_token_limit(&pm.model_id);

                        app.recent_models.retain(|m| m != &model_name);
                        app.recent_models.insert(0, model_name.clone());
                        if app.recent_models.len() > 5 { app.recent_models.truncate(5); }

                        app.push_message("system", &format!("Switched to: {} (cached)", model_name));
                        app.status = "Ready".to_string();
                    } else {
                        app.pending_model = pm.model_id.to_string();
                        app.pending_provider = pm.provider_id.to_string();
                        app.pending_base_url = pm.base_url.to_string();
                        app.api_key_input.clear();
                        app.mode = AppMode::ApiKeyInput;
                    }
                } else {
                    let is_installed = app.ollama_model_infos.iter()
                        .find(|m| m.name == model_name || m.name == format!("{}:latest", model_name) || model_name.contains(&m.name))
                        .map(|m| m.installed)
                        .unwrap_or(false);

                    if !is_installed {
                        app.pending_download_model = model_name.clone();
                        app.mode = AppMode::DownloadConfirm;
                        terminal.draw(|f| ui::draw(f, &app))?;
                        continue;
                    }

                    let mut new_cfg = app.cfg.clone();
                    new_cfg.ai.model = model_name.clone();
                    new_cfg.ai.provider = "ollama".to_string();
                    new_cfg.ai.base_url = "http://localhost:11434".to_string();
                    new_cfg.ai.api_key = String::new();
                    new_cfg.ai.vision_capable = crate::ai::ollama::is_vision_model(&model_name);
                    config::save(&storage_dir, &new_cfg)?;
                    app.cfg = new_cfg;
                    app.token_limit = crate::tui::app::estimate_token_limit(&model_name);

                    app.recent_models.retain(|m| m != &model_name);
                    app.recent_models.insert(0, model_name.clone());
                    if app.recent_models.len() > 5 { app.recent_models.truncate(5); }

                    app.push_message("system", &format!("Switched to: {}", model_name));
                    app.status = "Ready".to_string();
                }
            }

            AppEvent::DownloadModel(model_name) => {
                app.push_message("system", &format!("⬇ Starting download: {} — this may take a few minutes...", model_name));
                app.status = format!("Downloading {}...", model_name);
                let model_clone = model_name.clone();
                tokio::spawn(async move {
                    let mut cmd = tokio::process::Command::new("ollama");
                    cmd.args(["pull", &model_clone])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null());
                    #[cfg(windows)]
                    cmd.creation_flags(0x08000000);
                    let _ = cmd.status().await;
                });
                if let Some(info) = app.ollama_model_infos.iter_mut().find(|m| m.name == model_name) {
                    info.installed = true;
                }
            }

            AppEvent::DeleteTask(id) => {
                if let Ok(tasks) = db::load_tasks(&conn) {
                    if let Some(task) = tasks.iter().find(|t| t.id.starts_with(&id)) {
                        let _ = db::delete_task(&conn, &task.id);
                        app.status = "Task deleted.".to_string();
                    }
                }
            }

            AppEvent::ToggleTask(id) => {
                if let Ok(tasks) = db::load_tasks(&conn) {
                    if let Some(task) = tasks.iter().find(|t| t.id.starts_with(&id)) {
                        let _ = db::set_task_enabled(&conn, &task.id, !task.enabled);
                        app.status = "Task updated.".to_string();
                    }
                }
            }

            AppEvent::Command(cmd) => {
                let trimmed = cmd.trim();
                let parts: Vec<&str> = trimmed.splitn(3, |c: char| c.is_whitespace()).filter(|s| !s.is_empty()).collect();
                let main_cmd = parts.first().copied().unwrap_or("");
                match main_cmd {
                    "/mode" => {
                        if parts.len() < 2 {
                            app.push_message("system", &format!("Current mode: {}. Usage: /mode <auto|chat|task>", app.cfg.mode));
                        } else {
                            let target_mode = parts[1].trim().to_lowercase();
                            if target_mode == "auto" || target_mode == "chat" || target_mode == "task" {
                                app.cfg.mode = target_mode.clone();
                                let _ = crate::config::save(&storage_dir, &app.cfg);
                                app.push_message("system", &format!("Agent mode changed to: {}", target_mode));
                            } else {
                                app.push_message("system", "Invalid mode. Choose from: auto, chat, task");
                            }
                        }
                    }
                    "/import" => {
                        if parts.len() < 3 {
                            app.push_message("system", "Usage: /import <name> <path_to_gguf>");
                        } else {
                            let import_name = parts[1].to_string();
                            let import_path = parts[2].trim().to_string();
                            
                            app.push_message("system", &format!("⬇ Starting local GGUF import for '{}' from '{}'...", import_name, import_path));
                            app.status = format!("Importing {}...", import_name);
                            
                            let ollama_url = "http://localhost:11434".to_string();
                            let (tx, rx) = tokio::sync::mpsc::channel(20);
                            active_fetch_tx = Some(tx.clone());
                            app.fetch_rx = Some(rx);
                            
                            let name_clone = import_name.clone();
                            let path_clone = import_path.clone();
                            tokio::spawn(async move {
                                let mut resp = match crate::ai::ollama::create_model(&ollama_url, &name_clone, &path_clone).await {
                                    Ok(r) => r,
                                    Err(e) => {
                                        let _ = tx.send(FetchResult::ImportFailed { name: name_clone, error: e.to_string() }).await;
                                        return;
                                    }
                                };
                                
                                let mut buffer = String::new();
                                loop {
                                    match resp.chunk().await {
                                        Ok(Some(chunk)) => {
                                            let text = String::from_utf8_lossy(&chunk);
                                            buffer.push_str(&text);
                                            while let Some(newline_idx) = buffer.find('\n') {
                                                let line = buffer[..newline_idx].trim().to_string();
                                                buffer = buffer[newline_idx + 1..].to_string();
                                                if !line.is_empty() {
                                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                                        let status = val["status"].as_str().unwrap_or("");
                                                        let completed = val["completed"].as_u64().unwrap_or(0);
                                                        let total = val["total"].as_u64().unwrap_or(0);
                                                        
                                                        let mut percent = if total > 0 {
                                                            (completed as f64 / total as f64 * 100.0) as u32
                                                        } else {
                                                            0
                                                        };
                                                        
                                                        if status == "success" {
                                                            percent = 100;
                                                        } else if percent == 0 {
                                                            percent = 50;
                                                        }
                                                        
                                                        let _ = tx.send(FetchResult::ImportProgress {
                                                            status: status.to_string(),
                                                            percent,
                                                        }).await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            break;
                                        }
                                        Err(e) => {
                                            let _ = tx.send(FetchResult::ImportFailed { name: name_clone, error: e.to_string() }).await;
                                            return;
                                        }
                                    }
                                }
                                
                                let _ = tx.send(FetchResult::ImportFinished { name: name_clone }).await;
                            });
                        }
                    }
                    "/new" => {
                        // Save current and start fresh
                        app.messages.clear();
                        app.current_conversation_name = String::new();
                        let new_id = uuid::Uuid::new_v4().to_string();
                        app.current_conversation_id = new_id;
                        history.truncate(1); // keep system prompt
                        app.tokens_used = 0;
                        app.push_message("system", "New conversation started.");
                    }
                    "/conversations" => {
                        app.conversations = load_conversations_runner(&conn);
                        app.mode = AppMode::ConversationList;
                    }
                    "/model" => {
                        app.model_cursor = 0;
                        app.model_search.clear();
                        app.mode = AppMode::ModelSelect;
                        app.fetching_models = true;
                        app.fetch_started = Some(std::time::Instant::now());

                        let ollama_url = "http://localhost:11434".to_string();
                        let mut keys = app.provider_keys.clone();
                        if !app.cfg.ai.api_key.is_empty() {
                            keys.entry(app.cfg.ai.provider.clone())
                                .or_insert_with(|| app.cfg.ai.api_key.clone());
                        }

                        let (tx, rx) = tokio::sync::mpsc::channel(20);
                        active_fetch_tx = Some(tx.clone());
                        app.fetch_rx = Some(rx);

                        tokio::spawn(async move {
                            let local_result = crate::ai::ollama::list_models(&ollama_url).await;
                            // Only send if we actually got models
                            let local_models = local_result.unwrap_or_default();
                            let _ = tx.send(FetchResult::Local(local_models)).await;

                            if !keys.is_empty() {
                                let cloud = crate::tui::model_fetch::fetch_all_known_providers(&keys).await;
                                  let _ = tx.send(FetchResult::Cloud(cloud)).await;
                            } else {
                                // No cloud keys - signal done
                                let _ = tx.send(FetchResult::Cloud(vec![])).await;
                            }
                        });
                    }
                    "/keys" => {
                        use crate::tui::model_fetch::PROVIDERS;
                        app.key_manager_provider = PROVIDERS.first()
                            .map(|p| p.id.to_string())
                            .unwrap_or_default();
                        app.key_manager_cursor = 0;
                        app.key_manager_adding = false;
                        app.key_manager_input.clear();
                        app.mode = AppMode::KeyManager;
                    }
                    "/tasks" => { app.mode = AppMode::TaskManager; }
                    "/help"  => { app.mode = AppMode::Help; }
                    "/clear" => {
                        app.messages.clear();
                        history.truncate(1);
                        app.tokens_used = 0;
                        app.push_message("system", "Chat cleared.");
                        let _ = db::clear_session(&conn, &session_id);
                    }
                    "/info" => {
                        let size = db::db_size_bytes(&storage_dir);
                        app.push_message("system", &format!(
                            "DB: {:.1}KB | ctx: {}/{} | {}/{}",
                            size as f64 / 1024.0,
                            app.tokens_used,
                            app.token_limit,
                            app.cfg.ai.provider,
                            app.cfg.ai.model
                        ));
                    }
                    "/quit" => {
                        if app.confirm_quit {
                            return Ok(());
                        } else {
                            app.confirm_quit = true;
                            app.push_message("system", "Type /quit again to confirm.");
                        }
                    }
                    _ => {}
                }
            },

            AppEvent::LoadConversation(conv_id) => {
                // Load messages for selected conversation
                let msgs = db::load_messages(&conn, &conv_id, 100).unwrap_or_default();
                app.messages.clear();
                history.truncate(1);
                for m in msgs {
                    app.push_message(&m.role, &m.content);
                    history.push(crate::ai::ChatMessage::text(&m.role, &m.content));
                }
                app.current_conversation_id = conv_id.clone();
                // Find name from conversations list
                if let Some(c) = app.conversations.iter().find(|c| c.id == conv_id) {
                    app.current_conversation_name = c.name.clone();
                }
                app.mode = AppMode::Chat;
                app.status = "Conversation loaded.".to_string();
            }

            AppEvent::RenameConversation => {
                app.current_conversation_name = app.rename_input.clone();
                app.mode = AppMode::Chat;
            }

            AppEvent::DeleteConversation => {
                // Read the id BEFORE removing from the vec
                if let Some(conv) = app.conversations.get(app.conversation_cursor) {
                    let id = conv.id.clone();
                    let _ = db::clear_session(&conn, &id);
                    app.conversations.retain(|c| c.id != id);
                    if app.conversation_cursor > 0 && app.conversation_cursor >= app.conversations.len() {
                        app.conversation_cursor = app.conversations.len().saturating_sub(1);
                    }
                    app.mode = AppMode::Chat;
                    app.status = "Conversation deleted.".to_string();
                }
            }

            AppEvent::ToggleThinkingExpand => {
                app.thinking_expanded = !app.thinking_expanded;
            }

            AppEvent::SetApiKey(key) => {
                app.status = "Validating API key...".to_string();
                app.mode = AppMode::ApiKeyInput;
                terminal.draw(|f| ui::draw(f, &app))?;

                let test_future = test_api_key(
                    &app.pending_provider,
                    &app.pending_base_url,
                    &app.pending_model,
                    &key,
                );

                // Redraw with timer while waiting so UI stays alive
                let result = {
                    tokio::pin!(test_future);
                    let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
                    loop {
                        tokio::select! {
                            res = &mut test_future => break res,
                            _ = interval.tick() => {
                                terminal.draw(|f| ui::draw(f, &app))?;
                            }
                        }
                    }
                };

                terminal.draw(|f| ui::draw(f, &app))?;

                match result {
                    Ok(()) => {
                        // Add to multi-key map (dedup)
                        app.provider_keys_multi
                            .entry(app.pending_provider.clone())
                            .or_default()
                            .push(key.clone());
                        if let Some(keys) = app.provider_keys_multi.get_mut(&app.pending_provider) {
                            keys.dedup();
                            // Set active index to the new key
                            if let Some(idx) = keys.iter().position(|k| k == &key) {
                                app.provider_key_index.insert(app.pending_provider.clone(), idx);
                            }
                        }
                        crate::tui::app::save_provider_key(&conn, &app.pending_provider, &key);

                        let vision = crate::tui::app::CLOUD_MODELS.iter()
                            .find(|p| p.model_id == app.pending_model.as_str())
                            .map(|p| p.vision)
                            .unwrap_or(false);

                        let mut new_cfg = app.cfg.clone();
                        new_cfg.ai.api_key = key;
                        new_cfg.ai.model = app.pending_model.clone();
                        new_cfg.ai.provider = app.pending_provider.clone();
                        new_cfg.ai.base_url = app.pending_base_url.clone();
                        new_cfg.ai.vision_capable = vision;
                        config::save(&storage_dir, &new_cfg)?;
                        app.cfg = new_cfg;
                        app.token_limit = crate::tui::app::estimate_token_limit(&app.pending_model);

                        let model = app.pending_model.clone();
                        app.recent_models.retain(|m| m != &model);
                        app.recent_models.insert(0, model.clone());
                        if app.recent_models.len() > 5 { app.recent_models.truncate(5); }

                        app.fetching_models = true;
                        terminal.draw(|f| ui::draw(f, &app))?;
                        let mut keys = app.provider_keys.clone();
                        keys.entry(app.pending_provider.clone())
                            .or_insert_with(|| app.cfg.ai.api_key.clone());
                        let fetched = crate::tui::model_fetch::fetch_all_known_providers(&keys).await;
                        app.fetched_cloud_models = fetched;
                        app.fetching_models = false;

                        app.thinking = false;
                        app.thinking_start = None;
                        app.thinking_secs = 0;
                        app.thinking_log.clear();
                        app.mode = AppMode::Chat;
                        app.messages.retain(|m| !(m.role == "assistant" && m.content == "▋"));
                        app.push_message("system", &format!(
                            "✓ Connected to {}/{} - ready to chat.",
                            app.pending_provider, model
                        ));
                        app.status = "Ready".to_string();
                        app.pending_model.clear();
                        app.pending_provider.clear();
                        app.pending_base_url.clear();
                    }
                    Err(friendly_err) => {
                        app.mode = AppMode::ApiKeyInput;
                        app.api_key_input.clear();
                        app.status = format!("Key rejected: {}", friendly_err);
                        terminal.draw(|f| ui::draw(f, &app))?;
                    }
                }
            }

            AppEvent::ModelsFetched(models) => {
                app.fetched_cloud_models = models;
                app.fetching_models = false;
            }

            AppEvent::AddProviderKey(provider_id, key) => {
                crate::tui::app::save_provider_key(&conn, &provider_id, &key);
                app.provider_keys_multi
                    .entry(provider_id.clone())
                    .or_default()
                    .push(key.clone());
                app.provider_keys_multi.get_mut(&provider_id).unwrap().dedup();
                app.provider_keys.insert(
                    provider_id.clone(),
                    app.provider_keys_multi[&provider_id][0].clone(),
                );
                app.key_manager_adding = false;
                app.key_manager_input.clear();
                app.status = format!("Key added for {}", provider_id);
            }

            AppEvent::DeleteProviderKey => {
                let provider_id = app.key_manager_provider.clone();
                let keys = app.provider_keys_multi
                    .entry(provider_id.clone())
                    .or_default();
                if app.key_manager_cursor < keys.len() {
                    let key = keys[app.key_manager_cursor].clone();
                    keys.remove(app.key_manager_cursor);
                    crate::tui::app::delete_provider_key(&conn, &provider_id, &key);
                    if app.key_manager_cursor > 0 { app.key_manager_cursor -= 1; }
                    if let Some(first) = app.provider_keys_multi
                        .get(&provider_id).and_then(|v| v.first()) {
                        app.provider_keys.insert(provider_id.clone(), first.clone());
                    } else {
                        app.provider_keys.remove(&provider_id);
                    }
                    app.status = format!("Key deleted for {}", provider_id);
                }
            }

            AppEvent::ClarificationAnswer(answer) => {
                if let Some(tx) = app.clarification_tx.take() {
                    let _ = tx.try_send(answer);
                }
            }

            AppEvent::Stop => {
                app.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                app.stop_thinking();
                app.status = "Stopped.".to_string();
                app.replace_last_assistant("(stopped)");
            }

            AppEvent::SystemMessage(msg) => {
                app.push_message("system", &msg);
            }

            AppEvent::ImportFinished(msg) => {
                app.push_message("system", &msg);
                app.status = "Ready".to_string();
            }

            _ => {}
        }
    }

    Ok(())
}

fn clean_user_facing_message(msg: &str) -> String {
    let trimmed = msg.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return String::new();
    }

    let msg = if let Some(idx) = msg.find("\nPlan:").or_else(|| msg.find("\nplan:")) {
        &msg[..idx]
    } else {
        msg
    };

    let cleaned: Vec<&str> = msg.lines()
        .filter(|line| {
            let t = line.trim();
            !(t.starts_with("[{\"tool\"") || t.starts_with("{\"tool\"")
              || t.starts_with("[{\\\"tool\\\"")
              || (t.starts_with("Brush size ") && t.contains(" at ("))
              || (t.len() > 200 && t.contains("Brush size")))
        })
        .collect();

    let result = cleaned.join("\n").trim().to_string();
    if result.len() < 3 { return String::new(); }
    result
}

fn load_conversations_runner(conn: &Connection) -> Vec<super::app::Conversation> {
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
        Ok(super::app::Conversation {
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

async fn test_api_key(provider: &str, base_url: &str, model: &str, api_key: &str) -> Result<(), String> {
    let client = reqwest::Client::new();

    let (url, body) = match provider {
        "anthropic" => (
            format!("{}/messages", base_url),
            serde_json::json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            }),
        ),
        _ => (
            format!("{}/chat/completions", base_url),
            serde_json::json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            }),
        ),
    };

    let mut req = client.post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json");

    if provider == "anthropic" {
        req = req
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");
    }

    let resp = req.json(&body).send().await
        .map_err(|e| format!("Network error: {}", e))?;

    let status = resp.status();
    if status == 401 {
        return Err("Invalid API key — authentication failed.".to_string());
    }
    if status == 403 {
        return Err("API key has no permission for this model.".to_string());
    }
    if status == 404 {
        return Err("Model not found. Check the model name is correct.".to_string());
    }
    if status == 429 {
        return Ok(());
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let msg = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            v["error"]["message"].as_str()
                .or(v["detail"].as_str())
                .or(v["message"].as_str())
                .unwrap_or("Unknown error")
                .to_string()
        } else {
            format!("HTTP {}", status)
        };
        return Err(msg);
    }
    Ok(())
}

fn format_api_error(raw: &str) -> String {
    let lower = raw.to_lowercase();

    if lower.contains("401") || lower.contains("unauthorized") || lower.contains("authentication") {
        return "✗ Authentication failed\n\nYour API key was rejected. Use /model to re-enter it.".to_string();
    }
    if lower.contains("403") || lower.contains("forbidden") {
        return "✗ Access denied\n\nYour API key doesn't have permission for this model.\nTry a different model or check your plan.".to_string();
    }
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many") {
        return "✗ Rate limit reached\n\nToo many requests. Wait a moment and try again.".to_string();
    }
    if lower.contains("quota") || lower.contains("billing") || lower.contains("insufficient_quota") {
        return "✗ Quota exceeded\n\nYou've run out of API credits. Check your billing at the provider's dashboard.".to_string();
    }
    if lower.contains("404") || lower.contains("model not found") || lower.contains("no such model") {
        return "✗ Model not found\n\nThis model doesn't exist or isn't available on your plan.\nUse /model to switch to a different one.".to_string();
    }
    if lower.contains("503") || lower.contains("overloaded") || lower.contains("unavailable") {
        return "✗ Service unavailable\n\nThe AI provider is temporarily overloaded. Try again in a moment.".to_string();
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return "✗ Request timed out\n\nThe server took too long to respond. Check your connection.".to_string();
    }
    if lower.contains("connection") || lower.contains("network") || lower.contains("connect") {
        return "✗ Connection failed\n\nCouldn't reach the AI provider. Check your internet connection.".to_string();
    }
    if lower.contains("context") && (lower.contains("length") || lower.contains("limit") || lower.contains("token")) {
        return "✗ Context too long\n\nThe conversation is too long for this model.\nUse /clear to start fresh or /model to switch to a model with a larger context.".to_string();
    }
    if lower.contains("content") && (lower.contains("filter") || lower.contains("policy") || lower.contains("safety")) {
        return "✗ Content filtered\n\nThe provider's safety filter blocked this request.\nTry rephrasing your message.".to_string();
    }
    if lower.contains("ollama") && lower.contains("not running") {
        return "✗ Ollama not running\n\nStart Ollama with: ollama serve".to_string();
    }

    let clean = raw
        .replace("Error: ", "")
        .replace("error: ", "")
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");

    format!("✗ Something went wrong\n\n{}", clean)
}