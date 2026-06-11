#![cfg(any(feature = "gui", target_os = "android", target_os = "ios"))]

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use serde_json::Value as JsonValue;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::ai::{self, ChatMessage};
use crate::config::{self, AppConfig};
use crate::db;
use crate::executor::{self, StopFlag};
use crate::json_utils::clean_if_json;
use crate::redact::RedactPatterns;
use tauri::{Emitter, Manager};

pub struct AppState {
    pub storage_dir: Mutex<PathBuf>,
    pub session_id: Mutex<String>,
    pub stop_flag: StopFlag,
    pub cfg: Mutex<AppConfig>,
    pub history: Mutex<Vec<ChatMessage>>,
    pub token_limit: Mutex<usize>,
    pub answer_sender: Mutex<Option<mpsc::Sender<String>>>,
    pub result_receiver: Mutex<Option<tokio::sync::oneshot::Receiver<anyhow::Result<String>>>>,
    pub device_info_json: JsonValue,
    pub provider_keys_multi: Mutex<std::collections::HashMap<String, Vec<String>>>,
    pub active_downloads: Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>,
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub provider: String,
    pub provider_name: String,
    pub base_url: String,
    pub vision: bool,
    pub size_gb: f32,
    pub installed: bool,
}

#[derive(Serialize)]
pub struct ConvInfo {
    pub id: String,
    pub name: String,
    pub preview: String,
}

#[derive(Serialize, Clone)]
pub struct ChatTokenPayload {
    pub session_id: String,
    pub token: String,
}

#[derive(Serialize, Clone)]
pub struct AgentStepPayload {
    pub session_id: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct MsgInfo {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct TaskInfo {
    pub id: String,
    pub prompt: String,
    pub schedule: String,
    pub enabled: bool,
    pub next_run: String,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run(
    _cfg: AppConfig,
    _conn: Connection,
    storage_dir: PathBuf,
) -> Result<()> {
    // Auto-start Ollama if not running
    {
        let ollama_url = "http://127.0.0.1:11434".to_string();
        if !crate::ai::ollama::is_running(&ollama_url).await {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
                let exe_dir = exe_path.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf();
                let portable_ollama_path = exe_dir.join("ollama.exe");

                let mut cmd = if portable_ollama_path.exists() {
                    tokio::process::Command::new(portable_ollama_path)
                } else {
                    tokio::process::Command::new("ollama")
                };

                cmd.arg("serve")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());

                // Set OLLAMA_MODELS env variable if in Portable storage mode
                if _cfg.storage == crate::config::StorageLocation::Portable {
                    let models_dir = storage_dir.join("ollama_models");
                    let _ = std::fs::create_dir_all(&models_dir);
                    if let Some(s) = models_dir.to_str() {
                        cmd.env("OLLAMA_MODELS", s);
                        std::env::set_var("OLLAMA_MODELS", s);
                    }
                }

                #[cfg(windows)]
                {
                    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                }

                let _ = cmd.spawn();
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }
        }
    }

    let device_info = crate::device::collect()?;
    let system_prompt =
        crate::prompt::build_system_prompt_with_vision(&device_info, _cfg.ai.vision_capable);
    let session_id = uuid::Uuid::new_v4().to_string();
    let token_limit = crate::tui::app::estimate_token_limit(&_cfg.ai.model);
    let device_info_json = serde_json::to_value(&device_info).unwrap_or_default();
    let keys = crate::tui::app::load_provider_keys_multi(&_conn);

    let state = AppState {
        storage_dir: Mutex::new(storage_dir),
        session_id: Mutex::new(session_id),
        stop_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        cfg: Mutex::new(_cfg),
        history: Mutex::new(vec![ChatMessage::text("system", &system_prompt)]),
        token_limit: Mutex::new(token_limit as usize),
        answer_sender: Mutex::new(None),
        result_receiver: Mutex::new(None),
        device_info_json,
        provider_keys_multi: Mutex::new(keys),
        active_downloads: Mutex::new(std::collections::HashMap::new()),
    };

    tauri::Builder::default()
        .manage(state)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let mut run_bg = true;
                for _ in 0..50 {
                    if let Ok(cfg) = state.cfg.try_lock() {
                        run_bg = cfg.run_in_background;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                if run_bg {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    let _ = w.set_shadow(false);
                    let _ = w.set_maximizable(false);

                    // Load and set window icon programmatically to bypass Windows taskbar cache
                    let icon_bytes = include_bytes!("../icons/icon.png");
                    if let Ok(img) = image::load_from_memory(icon_bytes) {
                        let rgba = img.to_rgba8();
                        let (width, height) = rgba.dimensions();
                        let raw_samples = rgba.into_raw();
                        let icon = tauri::image::Image::new_owned(raw_samples, width, height);
                        let _ = w.set_icon(icon);
                    }

                    if let Ok(Some(monitor)) = w.current_monitor() {
                        let size = monitor.size();
                        let pos = monitor.position();
                        let scale_factor = monitor.scale_factor();
                        let physical_width = (700.0 * scale_factor) as u32;
                        let x = pos.x + ((size.width - physical_width) / 2) as i32;
                        
                        let physical_height = (130.0 * scale_factor) as u32;
                        let physical_offset = (60.0 * scale_factor) as u32;
                        let y = pos.y + (size.height - physical_height - physical_offset) as i32;
                        
                        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            run_prompt,
            answer_clarification,
            continue_after_clarification,
            get_current_session_id,
            stop_agent,
            set_window_size,
            set_window_dimensions,
            set_always_on_top,
            close_window,
            minimize_window,
            start_dragging,
            is_ollama_installed,
            toggle_maximize,
            get_config,
            list_models,
            switch_model,
            list_conversations,
            new_conversation,
            load_conversation,
            delete_conversation,
            rename_conversation,
            list_tasks,
            toggle_task,
            delete_task,
            get_provider_key,
            save_provider_key,
            set_session_memory,
            set_run_in_background,
            get_device_info,
            needs_setup,
            complete_setup,
            auto_setup,
            set_storage_location,
            list_ollama_models,
            pull_model,
            cancel_download,
            import_gguf_model,
            select_gguf_file,
            install_ollama,
            run_chat,
            verify_api_key,
            search_online_models,
            show_window,
            hide_window,
            get_welcome_greeting,
            get_context_tokens,
            start_voice_session,
            stop_voice_session,
            send_voice_chunk,
            is_window_maximized,
            is_mobile,
            open_app_settings,
            open_overlay_settings,
            open_assistant_settings,
            check_mobile_permissions,
            request_mobile_notification_permission,
            open_mobile_notification_settings,
            get_mobile_audio_devices,
            enter_pip_mode,
            start_mobile_local_ai_download,
            pause_mobile_local_ai_download,
            cancel_mobile_local_ai_download,
            get_mobile_local_ai_download_progress,
            read_local_model_file,
        ])
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("Tauri error: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_language_instruction(system_language: &str) -> Option<String> {
    let (is_auto, base_lang) = if system_language.starts_with("auto:") {
        (true, system_language.trim_start_matches("auto:").to_string())
    } else if system_language == "auto" {
        (true, "en".to_string())
    } else {
        (false, system_language.to_string())
    };

    let lang_prefix = base_lang.split('-').next().unwrap_or(&base_lang);
    
    // If it's explicitly set to English, we don't need a forcing system instruction
    if !is_auto && lang_prefix == "en" {
        return None;
    }

    let lang_name = match lang_prefix {
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ja" => "Japanese",
        "zh" => "Chinese",
        "ru" => "Russian",
        "ko" => "Korean",
        _ => "English",
    };

    if is_auto {
        Some(format!(
            "IMPORTANT: The user has set language preference to Auto-detect (Default system language is {}). \
             You MUST dynamically detect the language of the user's message, and respond strictly in that same language! \
             If they speak or type in Spanish, respond in Spanish. If they speak in French, respond in French. \
             If they speak in English, respond in English. If their message language is ambiguous or they speak in their default system locale, respond in {} strictly. \
             Your plan descriptions, summaries, and conversational responses must be in the detected language.",
            lang_name, lang_name
        ))
    } else {
        Some(format!(
            "IMPORTANT: The user has explicitly set their preferred language to {}. You MUST respond, explain, plan, reply, and message in {} strictly. All of your conversational replies, plan descriptions, summaries, and messages must be written in {}.",
            lang_name, lang_name, lang_name
        ))
    }
}

// ---------------------------------------------------------------------------
// run_prompt
// ---------------------------------------------------------------------------


#[tauri::command(rename_all = "snake_case")]
async fn run_prompt(
    prompt: String,
    system_language: Option<String>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> std::result::Result<JsonValue, String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    let sid = state.session_id.lock().await.clone();
    let _ = db::insert_message(&conn, &sid, "user", &prompt);

    state.stop_flag.store(true, Ordering::Relaxed);
    if let Some(s) = state.answer_sender.lock().await.take() {
        let _ = s.try_send("cancelled".to_string());
    }
    let _ = state.result_receiver.lock().await.take();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    state.stop_flag.store(false, Ordering::Relaxed);

    let (q_tx, mut q_rx) = mpsc::channel::<(String, Vec<String>)>(2);
    let (a_tx, a_rx) = mpsc::channel::<String>(2);
    let (res_tx, res_rx) = tokio::sync::oneshot::channel::<anyhow::Result<String>>();
    let (msg_tx, mut msg_rx) = mpsc::channel::<String>(64);
    let (token_tx, mut token_rx) = mpsc::channel::<String>(100);

    *state.answer_sender.lock().await = Some(a_tx);
    *state.result_receiver.lock().await = Some(res_rx);

    let cfg = state.cfg.lock().await.clone();
    let mut history = state.history.lock().await.clone();

    // Dynamic system language injection
    let lang = system_language.unwrap_or_else(|| "auto".to_string());
    if let Some(lang_instruction) = get_language_instruction(&lang) {
        history.insert(0, ChatMessage::text("system", &lang_instruction));
    }

    let session_id = state.session_id.lock().await.clone();
    let storage_dir = state.storage_dir.lock().await.clone();
    let stop = state.stop_flag.clone();
    let token_limit = *state.token_limit.lock().await;
    let p2 = prompt.clone();
    let sid2 = session_id.clone();

    let app2 = app_handle.clone();
    let sid_for_msg = session_id.clone();
    let _msg_fwd = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            let _ = app2.emit("status", AgentStepPayload {
                session_id: sid_for_msg.clone(),
                message: msg.clone(),
            });
            let _ = app2.emit("agent-step", AgentStepPayload {
                session_id: sid_for_msg.clone(),
                message: msg,
            });
        }
    });

    let app3 = app_handle.clone();
    let sid_for_token = session_id.clone();
    let tokens_streamed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let tokens_streamed_clone = tokens_streamed.clone();
    let _token_fwd = tokio::spawn(async move {
        while let Some(token) = token_rx.recv().await {
            tokens_streamed_clone.store(true, Ordering::Relaxed);
            let _ = app3.emit("chat-token", ChatTokenPayload {
                session_id: sid_for_token.clone(),
                token,
            });
        }
    });

    tokio::spawn(async move {
        let outcome = async {
            let patterns = RedactPatterns::new();
            let provider = ai::provider_for_config(&cfg.ai);
            executor::run_task(
                &p2, &sid2, history, provider.as_ref(),
                &storage_dir, &cfg, &patterns, stop, msg_tx, token_limit, q_tx, a_rx,
                Some(token_tx),
            )
            .await
        }
        .await;
        let _ = res_tx.send(outcome);
    });

    if let Some((question, options)) = q_rx.recv().await {
        return Ok(json!({
            "clarification": { "question": question, "options": options }
        }));
    }

    let rx = state.result_receiver.lock().await.take()
        .ok_or("Task already finished unexpectedly")?;

    match rx.await {
        Ok(Ok(response)) => {
            let user_msg = if prompt.trim_start().starts_with('[') {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&prompt) {
                    ChatMessage {
                        role: "user".to_string(),
                        content: val,
                    }
                } else {
                    ChatMessage::text("user", &prompt)
                }
            } else {
                ChatMessage::text("user", &prompt)
            };
            let friendly = clean_if_json(&response);
            state.history.lock().await.push(user_msg);
            state.history.lock().await.push(ChatMessage::text("assistant", &friendly));
            if let Ok(conn) = db::open(&*state.storage_dir.lock().await) {
                let _ = db::insert_message(&conn, &session_id, "assistant", &friendly);
            }

            // Stream response token-by-token if not already streamed during run_task
            if !tokens_streamed.load(Ordering::Relaxed) {
                let mut chunks = Vec::new();
                let mut current = String::new();
                for c in friendly.chars() {
                    current.push(c);
                    if c.is_whitespace() {
                        chunks.push(current);
                        current = String::new();
                    }
                }
                if !current.is_empty() {
                    chunks.push(current);
                }

                let stop = state.stop_flag.clone();
                let sid_clone = session_id.clone();
                for chunk in chunks {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = app_handle.emit("chat-token", ChatTokenPayload {
                        session_id: sid_clone.clone(),
                        token: chunk,
                    });
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
            }

            Ok(JsonValue::String(friendly))
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("Task was cancelled".to_string()),
    }
}

// ---------------------------------------------------------------------------
// answer_clarification
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn answer_clarification(
    answer: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let sender = state.answer_sender.lock().await.take()
        .ok_or("No clarification pending")?;
    sender.send(answer).await
        .map_err(|_| "Executor no longer waiting for clarification".to_string())
}

// ---------------------------------------------------------------------------
// continue_after_clarification
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn continue_after_clarification(
    session_id: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> std::result::Result<String, String> {
    let rx = state.result_receiver.lock().await.take()
        .ok_or("No pending task")?;

    match rx.await {
        Ok(Ok(response)) => {
            let friendly = clean_if_json(&response);
            state.history.lock().await.push(ChatMessage::text("assistant", &friendly));
            if let Ok(conn) = db::open(&*state.storage_dir.lock().await) {
                let _ = db::insert_message(&conn, &session_id, "assistant", &friendly);
            }

            // Stream response token-by-token
            let mut chunks = Vec::new();
            let mut current = String::new();
            for c in friendly.chars() {
                current.push(c);
                if c.is_whitespace() {
                    chunks.push(current);
                    current = String::new();
                }
            }
            if !current.is_empty() {
                chunks.push(current);
            }

            let stop = state.stop_flag.clone();
            let sid_clone = session_id;
            for chunk in chunks {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let _ = app_handle.emit("chat-token", ChatTokenPayload {
                    session_id: sid_clone.clone(),
                    token: chunk,
                });
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }

            Ok(friendly)
        }
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("Task was cancelled".to_string()),
    }
}

// ---------------------------------------------------------------------------
// stop_agent
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn stop_agent(state: tauri::State<'_, AppState>) -> std::result::Result<(), String> {
    state.stop_flag.store(true, Ordering::Relaxed);
    if let Some(s) = state.answer_sender.lock().await.take() {
        let _ = s.try_send("cancelled".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Window management
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn set_window_size(
    app_handle: tauri::AppHandle,
    expanded: bool,
) -> std::result::Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            let h = if expanded { 500.0 } else { 130.0 };
            w.set_size(tauri::LogicalSize::new(700.0, h))
                .map_err(|e| e.to_string())?;

            if let Ok(Some(monitor)) = w.current_monitor() {
                let size = monitor.size();
                let pos = monitor.position();
                let scale_factor = monitor.scale_factor();
                let physical_width = (700.0 * scale_factor) as u32;
                let x = pos.x + ((size.width - physical_width) / 2) as i32;
                
                let physical_height = (h * scale_factor) as u32;
                let physical_offset = (60.0 * scale_factor) as u32;
                let y = pos.y + (size.height - physical_height - physical_offset) as i32;
                
                let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
            }
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn set_window_dimensions(
    app_handle: tauri::AppHandle,
    width: f64,
    height: f64,
) -> std::result::Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            #[cfg(target_os = "windows")]
            {
                if let Ok(hwnd_wrapper) = w.hwnd() {
                    let hwnd = hwnd_wrapper.0 as *mut std::ffi::c_void;
                    if let Ok(Some(monitor)) = w.current_monitor() {
                        let size = monitor.size();
                        let pos = monitor.position();
                        let scale_factor = monitor.scale_factor();
                        let physical_width = (width * scale_factor) as u32;
                        let x = pos.x + ((size.width - physical_width) / 2) as i32;
                        
                        let physical_height = (height * scale_factor) as u32;
                        let physical_offset = (60.0 * scale_factor) as u32;
                        let y = pos.y + (size.height - physical_height - physical_offset) as i32;
                        
                        unsafe {
                            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                                hwnd,
                                std::ptr::null_mut(),
                                x,
                                y,
                                physical_width as i32,
                                physical_height as i32,
                                windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER |
                                windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE |
                                windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOOWNERZORDER |
                                windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSENDCHANGING,
                            );
                        }
                        return Ok(());
                    }
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                w.set_size(tauri::LogicalSize::new(width, height))
                    .map_err(|e| e.to_string())?;

                if let Ok(Some(monitor)) = w.current_monitor() {
                    let size = monitor.size();
                    let pos = monitor.position();
                    let scale_factor = monitor.scale_factor();
                    let physical_width = (width * scale_factor) as u32;
                    let x = pos.x + ((size.width - physical_width) / 2) as i32;
                    
                    let physical_height = (height * scale_factor) as u32;
                    let physical_offset = (60.0 * scale_factor) as u32;
                    let y = pos.y + (size.height - physical_height - physical_offset) as i32;
                    
                    let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                }
            }
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn set_always_on_top(
    app_handle: tauri::AppHandle,
    enabled: bool,
) -> std::result::Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            w.set_always_on_top(enabled).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn close_window(app_handle: tauri::AppHandle) -> std::result::Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            w.close().map_err(|e| e.to_string())?;
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        PENDING_ACTION.store(6, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn minimize_window(app_handle: tauri::AppHandle) -> std::result::Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            w.minimize().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn start_dragging(app_handle: tauri::AppHandle) -> std::result::Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            w.start_dragging().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn is_ollama_installed() -> std::result::Result<bool, String> {
    let cmd = if cfg!(target_os = "windows") { "ollama.exe" } else { "ollama" };
    
    // Check PATH
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            if path.join(cmd).exists() {
                return Ok(true);
            }
        }
    }
    
    // Check current exe directory
    if let Ok(exe_p) = std::env::current_exe() {
        if let Some(parent) = exe_p.parent() {
            if parent.join(cmd).exists() {
                return Ok(true);
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            if std::path::PathBuf::from(localappdata).join("Programs\\Ollama\\ollama.exe").exists() {
                return Ok(true);
            }
        }
        if std::path::PathBuf::from("C:\\Program Files\\Ollama\\ollama.exe").exists() {
            return Ok(true);
        }
        if std::path::PathBuf::from("C:\\Program Files (x86)\\Ollama\\ollama.exe").exists() {
            return Ok(true);
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/Applications/Ollama.app").exists()
            || std::path::Path::new("/usr/local/bin/ollama").exists()
            || std::path::Path::new("/usr/bin/ollama").exists()
        {
            return Ok(true);
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/usr/local/bin/ollama").exists()
            || std::path::Path::new("/usr/bin/ollama").exists()
        {
            return Ok(true);
        }
    }
    
    Ok(false)
}

#[tauri::command(rename_all = "snake_case")]
async fn open_app_settings() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(5, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn open_overlay_settings() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(1, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn open_assistant_settings() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(2, Ordering::Relaxed);
    }
    Ok(())
}


#[cfg(target_os = "windows")]
fn set_restore_dimensions(w: &tauri::WebviewWindow, width: f64, height: f64) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, SetWindowPlacement, WINDOWPLACEMENT
    };
    if let Ok(hwnd_wrapper) = w.hwnd() {
        let hwnd = hwnd_wrapper.0 as *mut std::ffi::c_void;
        unsafe {
            let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(hwnd as _, &mut placement) != 0 {
                if let Ok(Some(monitor)) = w.current_monitor() {
                    let size = monitor.size();
                    let pos = monitor.position();
                    let scale = monitor.scale_factor();
                    
                    let pw = (width * scale) as i32;
                    let ph = (height * scale) as i32;
                    
                    let x = pos.x + ((size.width as i32 - pw) / 2);
                    let offset = (60.0 * scale) as i32;
                    let y = pos.y + (size.height as i32 - ph - offset);
                    
                    placement.rcNormalPosition.left = x;
                    placement.rcNormalPosition.top = y;
                    placement.rcNormalPosition.right = x + pw;
                    placement.rcNormalPosition.bottom = y + ph;
                    
                    SetWindowPlacement(hwnd as _, &placement);
                }
            }
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn toggle_maximize(
    app_handle: tauri::AppHandle,
    maximize: bool,
) -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        if maximize {
            PENDING_ACTION.store(4, Ordering::Relaxed);
        } else {
            PENDING_ACTION.store(3, Ordering::Relaxed);
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            if maximize {
                let _ = w.set_maximizable(true);
                #[cfg(target_os = "windows")]
                set_restore_dimensions(&w, 520.0, 560.0);
                w.maximize().map_err(|e| e.to_string())?;
            } else {
                w.unmaximize().map_err(|e| e.to_string())?;
                let _ = w.set_maximizable(false);
                let h = 560.0f64;
                w.set_size(tauri::LogicalSize::new(520.0, h))
                    .map_err(|e| e.to_string())?;
                if let Ok(Some(monitor)) = w.current_monitor() {
                    let size = monitor.size();
                    let pos = monitor.position();
                    let scale = monitor.scale_factor();
                    let pw = (520.0 * scale) as u32;
                    let x = pos.x + ((size.width - pw) / 2) as i32;
                    let ph = (h * scale) as u32;
                    let offset = (60.0 * scale) as u32;
                    let y = pos.y + (size.height - ph - offset) as i32;
                    let _ = w.set_position(tauri::Position::Physical(
                        tauri::PhysicalPosition { x, y }
                    ));
                }
            }
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn is_window_maximized(
    app_handle: tauri::AppHandle,
) -> std::result::Result<bool, String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            return w.is_maximized().map_err(|e| e.to_string());
        }
    }
    Ok(false)
}

#[tauri::command(rename_all = "snake_case")]
fn is_mobile() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}



// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn get_config(state: tauri::State<'_, AppState>) -> std::result::Result<AppConfig, String> {
    Ok(state.cfg.lock().await.clone())
}

// ---------------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn list_models(
    skip_ollama: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<ModelInfo>, String> {
    use crate::ai::ollama::is_vision_model;

    let mut out: Vec<ModelInfo> = Vec::new();
    let mut ollama_url = {
        let cfg = state.cfg.lock().await;
        if cfg.ai.provider == "ollama" && !cfg.ai.base_url.is_empty() {
            cfg.ai.base_url.clone()
        } else {
            "http://127.0.0.1:11434".to_string()
        }
    };
    if ollama_url.contains("localhost") {
        ollama_url = ollama_url.replace("localhost", "127.0.0.1");
    }

    let skip = skip_ollama.unwrap_or(false);
    if !skip {
        if let Ok(ollama_models) = crate::ai::ollama::list_models(&ollama_url).await {
        for m in ollama_models {
            let v = crate::ai::ollama::is_vision_model(&m.name);
            out.push(ModelInfo {
                name: m.name,
                provider: "ollama".to_string(),
                provider_name: "Ollama".to_string(),
                base_url: ollama_url.clone(),
                vision: v,
                size_gb: m.size_bytes as f32 / (1024.0 * 1024.0 * 1024.0),
                installed: true,
            });
        }
    }
    }

    for pm in crate::tui::app::CLOUD_MODELS {
        out.push(ModelInfo {
            name: pm.model_id.to_string(),
            provider: pm.provider_id.to_string(),
            provider_name: pm.provider_name.to_string(),
            base_url: pm.base_url.to_string(),
            vision: pm.vision,
            size_gb: 0.0,
            installed: true,
        });
    }

    let installed_names: std::collections::HashSet<String> =
        out.iter().filter(|m| m.provider == "ollama").map(|m| m.name.clone()).collect();
    for k in crate::tui::app::OLLAMA_CATALOG {
        if !installed_names.contains(k.name) {
            out.push(ModelInfo {
                name: k.name.to_string(),
                provider: "ollama".to_string(),
                provider_name: "Ollama".to_string(),
                base_url: ollama_url.clone(),
                vision: k.vision,
                size_gb: k.size_gb,
                installed: false,
            });
        }
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        out.insert(0, ModelInfo {
            name: "Llama-3.2-1B-Instruct-q4f16_1-MLC".to_string(),
            provider: "ollama".to_string(),
            provider_name: "Local AI".to_string(),
            base_url: "http://127.0.0.1:11434".to_string(),
            vision: false,
            size_gb: 0.64,
            installed: true,
        });
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Model switching
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn switch_model(
    name: String,
    provider: String,
    base_url: String,
    vision: bool,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut cfg = state.cfg.lock().await.clone();
    cfg.ai.model = name.clone();
    cfg.ai.provider = provider.clone();
    let mut normalized_url = base_url.clone();
    if normalized_url.contains("localhost") {
        normalized_url = normalized_url.replace("localhost", "127.0.0.1");
    }
    cfg.ai.base_url = normalized_url;
    cfg.ai.vision_capable = if provider == "ollama" {
        crate::ai::ollama::is_vision_model(&name)
    } else {
        vision
    };

    let keys = state.provider_keys_multi.lock().await;
    if let Some(k) = keys.get(&provider).and_then(|v| v.first()) {
        cfg.ai.api_key = k.clone();
    }
    drop(keys);

    config::save(&*state.storage_dir.lock().await, &cfg).map_err(|e| e.to_string())?;
    *state.cfg.lock().await = cfg;
    *state.token_limit.lock().await = crate::tui::app::estimate_token_limit(&name) as usize;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn pull_model(
    name: String,
    base_url: Option<String>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let mut url = base_url.unwrap_or_else(|| {
        "http://127.0.0.1:11434".to_string()
    });
    if url.contains("localhost") {
        url = url.replace("localhost", "127.0.0.1");
    }
    
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut active = state.active_downloads.lock().await;
        active.insert(name.clone(), tx);
    }
    
    let app_handle_clone = app_handle.clone();
    let name_clone = name.clone();
    tokio::spawn(async move {
        let state = app_handle_clone.state::<AppState>();
        let client = reqwest::Client::new();
        let pull_url = format!("{}/api/pull", url.trim_end_matches('/'));
        let body = serde_json::json!({ "name": name_clone, "stream": true });
        
        let send_res = client.post(&pull_url).json(&body).send().await;
        let mut resp = match send_res {
            Ok(r) => r,
            Err(e) => {
                let mut active = state.active_downloads.lock().await;
                active.remove(&name_clone);
                let _ = app_handle_clone.emit("download-failed", serde_json::json!({ "model": name_clone, "error": e.to_string() }));
                return;
            }
        };
        
        if !resp.status().is_success() {
            let mut active = state.active_downloads.lock().await;
            active.remove(&name_clone);
            let text = resp.text().await.unwrap_or_else(|_| "Unknown HTTP error".to_string());
            let _ = app_handle_clone.emit("download-failed", serde_json::json!({ "model": name_clone, "error": text }));
            return;
        }
        
        let mut rx = rx;
        let mut buffer = String::new();
        loop {
            tokio::select! {
                _ = &mut rx => {
                    let mut active = state.active_downloads.lock().await;
                    active.remove(&name_clone);
                    let _ = app_handle_clone.emit("download-failed", serde_json::json!({ "model": name_clone, "error": "Canceled" }));
                    return;
                }
                res = resp.chunk() => {
                    match res {
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
                                        let percent = if total > 0 {
                                            (completed as f64 / total as f64 * 100.0) as u32
                                        } else {
                                            0
                                        };
                                        let payload = serde_json::json!({
                                            "model": name_clone,
                                            "status": status,
                                            "completed": completed,
                                            "total": total,
                                            "percent": percent,
                                        });
                                        let _ = app_handle_clone.emit("download-progress", payload);
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            break;
                        }
                        Err(e) => {
                            let mut active = state.active_downloads.lock().await;
                            active.remove(&name_clone);
                            let _ = app_handle_clone.emit("download-failed", serde_json::json!({ "model": name_clone, "error": e.to_string() }));
                            return;
                        }
                    }
                }
            }
        }
        
        {
            let mut active = state.active_downloads.lock().await;
            active.remove(&name_clone);
        }
        let _ = app_handle_clone.emit("download-finished", serde_json::json!({ "model": name_clone }));
    });
    
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn cancel_download(
    name: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut active = state.active_downloads.lock().await;
    if let Some(tx) = active.remove(&name) {
        let _ = tx.send(());
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn import_gguf_model(
    name: String,
    path: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let url = "http://127.0.0.1:11434".to_string();
    
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut active = state.active_downloads.lock().await;
        active.insert(name.clone(), tx);
    }
    
    let app_handle_clone = app_handle.clone();
    let name_clone = name.clone();
    let path_clone = path.clone();
    tokio::spawn(async move {
        let state = app_handle_clone.state::<AppState>();
        
        let mut resp = match crate::ai::ollama::create_model(&url, &name_clone, &path_clone).await {
            Ok(r) => r,
            Err(e) => {
                let mut active = state.active_downloads.lock().await;
                active.remove(&name_clone);
                let _ = app_handle_clone.emit("download-failed", serde_json::json!({ "model": name_clone, "error": e.to_string() }));
                return;
            }
        };
        
        let mut rx = rx;
        let mut buffer = String::new();
        loop {
            tokio::select! {
                _ = &mut rx => {
                    let mut active = state.active_downloads.lock().await;
                    active.remove(&name_clone);
                    let _ = app_handle_clone.emit("download-failed", serde_json::json!({ "model": name_clone, "error": "Canceled" }));
                    return;
                }
                res = resp.chunk() => {
                    match res {
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
                                        
                                        let payload = serde_json::json!({
                                            "model": name_clone,
                                            "status": status,
                                            "completed": completed,
                                            "total": total,
                                            "percent": percent,
                                        });
                                        let _ = app_handle_clone.emit("download-progress", payload);
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            break;
                        }
                        Err(e) => {
                            let mut active = state.active_downloads.lock().await;
                            active.remove(&name_clone);
                            let _ = app_handle_clone.emit("download-failed", serde_json::json!({ "model": name_clone, "error": e.to_string() }));
                            return;
                        }
                    }
                }
            }
        }
        
        {
            let mut active = state.active_downloads.lock().await;
            active.remove(&name_clone);
        }
        let _ = app_handle_clone.emit("download-finished", serde_json::json!({ "model": name_clone }));
    });
    
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn install_ollama(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let base_url = "http://127.0.0.1:11434";
    if crate::ai::ollama::is_running(base_url).await {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let has_ollama = std::process::Command::new("where")
            .arg("ollama")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if has_ollama {
            let _ = tokio::process::Command::new("ollama")
                .arg("serve")
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            
            for _ in 0..15 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if crate::ai::ollama::is_running(base_url).await {
                    return Ok(());
                }
            }
        }

        let is_portable = {
            let cfg = state.cfg.lock().await;
            cfg.storage == crate::config::StorageLocation::Portable
        };

        let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        let exe_dir = exe_path.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf();

        if is_portable {
            let installer_url = "https://github.com/ollama/ollama/releases/latest/download/ollama-windows-amd64.zip";
            let temp_dir = std::env::temp_dir();
            let zip_path = temp_dir.join("ollama_portable.zip");
            
            let _ = app_handle.emit("download-progress", serde_json::json!({
                "model": "Ollama",
                "status": "Connecting to GitHub download server...",
                "percent": 0
            }));

            let client = reqwest::Client::new();
            let mut response = client.get(installer_url).send().await.map_err(|e| e.to_string())?;
            
            let _ = app_handle.emit("download-progress", serde_json::json!({
                "model": "Ollama",
                "status": "Starting download (this is a large file, please wait)...",
                "percent": 0
            }));

            let total_size = response.content_length().unwrap_or(0);
            let mut downloaded: u64 = 0;
            
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&zip_path).await.map_err(|e| e.to_string())?;
            
            let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
            {
                let mut active = state.active_downloads.lock().await;
                active.insert("Ollama".to_string(), tx);
            }
            
            loop {
                tokio::select! {
                    _ = &mut rx => {
                        let mut active = state.active_downloads.lock().await;
                        active.remove("Ollama");
                        let _ = tokio::fs::remove_file(&zip_path).await;
                        return Err("Ollama download cancelled".to_string());
                    }
                    chunk_res = response.chunk() => {
                        match chunk_res {
                            Ok(Some(chunk)) => {
                                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                                downloaded += chunk.len() as u64;
                                
                                if total_size > 0 {
                                    let percent = ((downloaded as f64 / total_size as f64) * 100.0) as u32;
                                    let _ = app_handle.emit("download-progress", serde_json::json!({
                                        "model": "Ollama",
                                        "status": "Downloading Ollama (Portable)...",
                                        "percent": percent
                                    }));
                                }
                            }
                            Ok(None) => {
                                break;
                            }
                            Err(e) => {
                                let mut active = state.active_downloads.lock().await;
                                active.remove("Ollama");
                                let _ = tokio::fs::remove_file(&zip_path).await;
                                return Err(e.to_string());
                            }
                        }
                    }
                }
            }
            
            {
                let mut active = state.active_downloads.lock().await;
                active.remove("Ollama");
            }
            
            file.flush().await.map_err(|e| e.to_string())?;
            
            let _ = app_handle.emit("download-progress", serde_json::json!({
                "model": "Ollama",
                "status": "Extracting Ollama (Portable)...",
                "percent": 99
            }));
            
            let status = tokio::process::Command::new("powershell")
                .arg("-Command")
                .arg(format!("Expand-Archive -Path '{}' -DestinationPath '{}' -Force", zip_path.to_string_lossy(), exe_dir.to_string_lossy()))
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .status()
                .await
                .map_err(|e| e.to_string())?;
                
            let _ = tokio::fs::remove_file(&zip_path).await;
                
            if !status.success() {
                return Err("Ollama extraction returned an error status".to_string());
            }
            
            let portable_ollama_path = exe_dir.join("ollama.exe");
            let mut cmd = tokio::process::Command::new(portable_ollama_path);
            cmd.arg("serve")
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());

            let models_dir = exe_dir.join("ollama_models");
            let _ = std::fs::create_dir_all(&models_dir);
            if let Some(s) = models_dir.to_str() {
                cmd.env("OLLAMA_MODELS", s);
                std::env::set_var("OLLAMA_MODELS", s);
            }

            let _ = cmd.spawn();
            
            for _ in 0..15 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if crate::ai::ollama::is_running(base_url).await {
                    return Ok(());
                }
            }
            
            return Ok(());
        } else {
            let installer_url = "https://ollama.com/download/OllamaSetup.exe";
            let temp_dir = std::env::temp_dir();
            let installer_path = temp_dir.join("OllamaSetup.exe");
            
            let _ = app_handle.emit("download-progress", serde_json::json!({
                "model": "Ollama",
                "status": "Connecting to Ollama download server...",
                "percent": 0
            }));

            let client = reqwest::Client::new();
            let mut response = client.get(installer_url).send().await.map_err(|e| e.to_string())?;
            
            let _ = app_handle.emit("download-progress", serde_json::json!({
                "model": "Ollama",
                "status": "Starting download (this is a large file, please wait)...",
                "percent": 0
            }));

            let total_size = response.content_length().unwrap_or(0);
            let mut downloaded: u64 = 0;
            
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&installer_path).await.map_err(|e| e.to_string())?;
            
            let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
            {
                let mut active = state.active_downloads.lock().await;
                active.insert("Ollama".to_string(), tx);
            }
            
            loop {
                tokio::select! {
                    _ = &mut rx => {
                        let mut active = state.active_downloads.lock().await;
                        active.remove("Ollama");
                        let _ = tokio::fs::remove_file(&installer_path).await;
                        return Err("Ollama download cancelled".to_string());
                    }
                    chunk_res = response.chunk() => {
                        match chunk_res {
                            Ok(Some(chunk)) => {
                                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                                downloaded += chunk.len() as u64;
                                
                                if total_size > 0 {
                                    let percent = ((downloaded as f64 / total_size as f64) * 100.0) as u32;
                                    let _ = app_handle.emit("download-progress", serde_json::json!({
                                        "model": "Ollama",
                                        "status": "Downloading Ollama...",
                                        "percent": percent
                                    }));
                                }
                            }
                            Ok(None) => {
                                break;
                            }
                            Err(e) => {
                                let mut active = state.active_downloads.lock().await;
                                active.remove("Ollama");
                                let _ = tokio::fs::remove_file(&installer_path).await;
                                return Err(e.to_string());
                            }
                        }
                    }
                }
            }
            
            {
                let mut active = state.active_downloads.lock().await;
                active.remove("Ollama");
            }
            
            file.flush().await.map_err(|e| e.to_string())?;
            
            let _ = app_handle.emit("download-progress", serde_json::json!({
                "model": "Ollama",
                "status": "Installing Ollama...",
                "percent": 99
            }));
            
            let status = tokio::process::Command::new(&installer_path)
                .arg("/silent")
                .status()
                .await
                .map_err(|e| e.to_string())?;
                
            let _ = tokio::fs::remove_file(&installer_path).await;
                
            if !status.success() {
                return Err("Ollama installer returned an error status".to_string());
            }
            
            let _ = tokio::process::Command::new("ollama")
                .arg("serve")
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
                
            for _ in 0..15 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if crate::ai::ollama::is_running(base_url).await {
                    return Ok(());
                }
            }
            return Ok(());
        }
    }

    #[cfg(target_os = "macos")]
    {
        let status = tokio::process::Command::new("brew")
            .args(["install", "--cask", "ollama"])
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if status.success() {
            let _ = tokio::process::Command::new("ollama")
                .arg("serve")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            return Ok(());
        }
        return Err("Failed to install Ollama via Homebrew. Please download it manually from https://ollama.com".to_string());
    }

    #[cfg(target_os = "linux")]
    {
        let status = tokio::process::Command::new("sh")
            .arg("-c")
            .arg("curl -fsSL https://ollama.com/install.sh | sh")
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if status.success() {
            let _ = tokio::process::Command::new("ollama")
                .arg("serve")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            return Ok(());
        }
        return Err("Failed to install Ollama via official installer script. Please install it manually.".to_string());
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        return Err("Local Ollama server cannot be installed directly on mobile operating systems. On-device local models run in-process using WebGPU/WebLLM.".to_string());
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn select_gguf_file(
    app_handle: tauri::AppHandle,
    always_on_top: bool,
) -> std::result::Result<Option<String>, String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let app_handle_clone = app_handle.clone();
        app_handle.run_on_main_thread(move || {
            if always_on_top {
                if let Some(w) = app_handle_clone.get_webview_window("main") {
                    let _ = w.set_always_on_top(false);
                }
            }
            let res = rfd::FileDialog::new()
                .add_filter("GGUF Model", &["gguf"])
                .pick_file()
                .map(|f| f.to_string_lossy().to_string());
            if always_on_top {
                if let Some(w) = app_handle_clone.get_webview_window("main") {
                    let _ = w.set_always_on_top(true);
                }
            }
            let _ = tx.send(res);
        }).map_err(|e| e.to_string())?;
        let path = rx.await.map_err(|e| e.to_string())?;
        Ok(path)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = always_on_top;
        Ok(None)
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn run_chat(
    prompt: String,
    system_language: Option<String>,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> std::result::Result<String, String> {
    let cfg = state.cfg.lock().await.clone();
    let provider = crate::ai::provider_for_config(&cfg.ai);
    let mut history = state.history.lock().await.clone();
    history.push(crate::ai::ChatMessage::text("user", &prompt));

    // Dynamic system language injection
    let lang = system_language.unwrap_or_else(|| "auto".to_string());
    if let Some(lang_instruction) = get_language_instruction(&lang) {
        history.insert(0, crate::ai::ChatMessage::text("system", &lang_instruction));
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);
    let app = app_handle.clone();
    let sid = state.session_id.lock().await.clone();
    let fwd_handle = tokio::spawn(async move {
        while let Some(token) = rx.recv().await {
            let _ = app.emit("chat-token", ChatTokenPayload {
                session_id: sid.clone(),
                token,
            });
        }
    });

    let res = provider.chat_stream(history, Some(1000), tx).await.map_err(|e| e.to_string())?;
    let _ = fwd_handle.await;
    let reply = clean_if_json(&res.content);

    state.history.lock().await.push(crate::ai::ChatMessage::text("user", &prompt));
    state.history.lock().await.push(crate::ai::ChatMessage::text("assistant", &reply));

    if let Ok(conn) = crate::db::open(&*state.storage_dir.lock().await) {
        let sid = state.session_id.lock().await.clone();
        let _ = crate::db::insert_message(&conn, &sid, "user", &prompt);
        let _ = crate::db::insert_message(&conn, &sid, "assistant", &reply);
    }

    Ok(reply)
}

// ---------------------------------------------------------------------------
// Conversations
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn list_conversations(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<ConvInfo>, String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT m.session_id, MAX(m.created_at), m.content, meta.name
             FROM messages m
             LEFT JOIN conversation_metadata meta ON m.session_id = meta.session_id
             WHERE m.role = 'user'
             GROUP BY m.session_id
             ORDER BY MAX(m.created_at) DESC
             LIMIT 50",
        )
        .map_err(|e| e.to_string())?;

    let convs = stmt
        .query_map([], |r| {
            Ok(ConvInfo {
                id: r.get(0)?,
                name: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                preview: r.get::<_, String>(2).unwrap_or_default().chars().take(35).collect(),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(convs)
}

#[tauri::command(rename_all = "snake_case")]
async fn new_conversation(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<String, String> {
    let new_id = uuid::Uuid::new_v4().to_string();
    *state.session_id.lock().await = new_id.clone();
    let sys = state.history.lock().await.first().cloned();
    *state.history.lock().await = match sys {
        Some(s) => vec![s],
        None => Vec::new(),
    };
    Ok(new_id)
}

#[tauri::command(rename_all = "snake_case")]
async fn load_conversation(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<MsgInfo>, String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    let db_msgs = db::load_messages(&conn, &session_id, 200)
        .map_err(|e| e.to_string())?;

    let device_info = crate::device::collect().map_err(|e| e.to_string())?;
    let cfg = state.cfg.lock().await;
    let system_prompt = crate::prompt::build_system_prompt_with_vision(&device_info, cfg.ai.vision_capable);

    let mut history = vec![ChatMessage::text("system", &system_prompt)];
    for m in &db_msgs {
        let val = if m.content.trim().starts_with('[') || m.content.trim().starts_with('{') {
            serde_json::from_str(&m.content).unwrap_or_else(|_| serde_json::Value::String(m.content.clone()))
        } else {
            serde_json::Value::String(m.content.clone())
        };
        history.push(ChatMessage {
            role: m.role.clone(),
            content: val,
        });
    }

    *state.history.lock().await = history;
    *state.session_id.lock().await = session_id;

    let msgs = db_msgs.into_iter()
        .map(|m| MsgInfo { role: m.role, content: m.content })
        .collect();
    Ok(msgs)
}

#[tauri::command(rename_all = "snake_case")]
async fn delete_conversation(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    db::clear_session(&conn, &session_id).map_err(|e| e.to_string())?;
    let current = state.session_id.lock().await.clone();
    if current == session_id {
        *state.session_id.lock().await = uuid::Uuid::new_v4().to_string();
        let sys = state.history.lock().await.first().cloned();
        *state.history.lock().await = match sys {
            Some(s) => vec![s],
            None => Vec::new(),
        };
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn get_current_session_id(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<String, String> {
    Ok(state.session_id.lock().await.clone())
}

#[tauri::command(rename_all = "snake_case")]
async fn rename_conversation(
    session_id: String,
    name: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    db::set_conversation_name(&conn, &session_id, &name).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Scheduled tasks
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn list_tasks(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<TaskInfo>, String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    let tasks = db::load_tasks(&conn).map_err(|e| e.to_string())?;
    Ok(tasks
        .into_iter()
        .map(|t| TaskInfo {
            id: t.id,
            prompt: t.prompt,
            schedule: t.schedule,
            enabled: t.enabled,
            next_run: t.next_run.to_rfc3339(),
        })
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
async fn toggle_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    let tasks = db::load_tasks(&conn).map_err(|e| e.to_string())?;
    if let Some(t) = tasks.iter().find(|t| t.id == task_id) {
        db::set_task_enabled(&conn, &task_id, !t.enabled).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn delete_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    db::delete_task(&conn, &task_id).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Provider API keys
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn get_provider_key(
    provider_id: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Option<String>, String> {
    let keys = state.provider_keys_multi.lock().await;
    Ok(keys.get(&provider_id).and_then(|v| v.first().cloned()))
}

#[tauri::command(rename_all = "snake_case")]
async fn save_provider_key(
    provider_id: String,
    api_key: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let conn = db::open(&*state.storage_dir.lock().await).map_err(|e| e.to_string())?;
    crate::tui::app::save_provider_key(&conn, &provider_id, &api_key);
    let mut keys = state.provider_keys_multi.lock().await;
    keys.entry(provider_id).or_default().push(api_key);
    if let Some(v) = keys.values_mut().next() {
        v.dedup();
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn needs_setup(state: tauri::State<'_, AppState>) -> std::result::Result<bool, String> {
    let cfg = state.cfg.lock().await;
    Ok(!cfg.setup_complete)
}

#[tauri::command(rename_all = "snake_case")]
async fn complete_setup(state: tauri::State<'_, AppState>) -> std::result::Result<(), String> {
    let mut cfg = state.cfg.lock().await.clone();
    cfg.setup_complete = true;
    config::save(&*state.storage_dir.lock().await, &cfg).map_err(|e| e.to_string())?;
    *state.cfg.lock().await = cfg;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn auto_setup(state: tauri::State<'_, AppState>) -> std::result::Result<(), String> {
    let base_url = "http://127.0.0.1:11434";
    let ai = if crate::ai::ollama::is_running(base_url).await {
        let models = crate::ai::ollama::list_models(base_url).await.unwrap_or_default();
        let vision_model = models.iter()
            .find(|m| crate::ai::ollama::is_vision_model(&m.name))
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "llava".to_string());

        crate::config::AiProviderConfig {
            provider: "ollama".to_string(),
            base_url: base_url.to_string(),
            api_key: String::new(),
            model: vision_model,
            vision_capable: true,
        }
    } else {
        crate::config::AiProviderConfig::default()
    };

    let mut cfg = state.cfg.lock().await.clone();
    cfg.ai = ai;
    cfg.setup_complete = true;

    config::save(&*state.storage_dir.lock().await, &cfg).map_err(|e| e.to_string())?;
    *state.cfg.lock().await = cfg;
    Ok(())
}

async fn stop_ollama() {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = tokio::process::Command::new("taskkill");
        cmd.args(&["/F", "/IM", "ollama.exe"]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let _ = cmd.status().await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = tokio::process::Command::new("pkill");
        cmd.arg("ollama");
        let _ = cmd.status().await;
    }
    // Give it a brief moment to shut down and release resources/port
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}

#[tauri::command(rename_all = "snake_case")]
async fn set_storage_location(
    location: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let loc = match location.as_str() {
        "portable" => crate::config::StorageLocation::Portable,
        "global" => crate::config::StorageLocation::Global,
        _ => return Err("Invalid storage location".to_string()),
    };

    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = exe_path.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf();
    
    // Resolve the new storage directory
    let new_storage_dir = crate::config::resolve_storage_dir(&loc, &exe_dir);
    
    // Ensure the new directory exists
    std::fs::create_dir_all(&new_storage_dir).map_err(|e| e.to_string())?;

    // Load current config, update storage
    let current_dir = state.storage_dir.lock().await.clone();
    let mut cfg = state.cfg.lock().await.clone();
    cfg.storage = loc.clone();

    // Save the config to the new directory
    crate::config::save(&new_storage_dir, &cfg).map_err(|e| e.to_string())?;

    // Also, if the new location is global, write redirect config to the portable location (so next startup redirects)
    let provisional_storage = crate::config::resolve_storage_dir(&crate::config::StorageLocation::Portable, &exe_dir);
    crate::config::save(&provisional_storage, &cfg).map_err(|e| e.to_string())?;

    // Migrate database and current config from the old storage directory to the new storage directory if they exist and are different
    if current_dir != new_storage_dir {
        // Copy SQLite DB
        let old_db = current_dir.join("antflow.db");
        let new_db = new_storage_dir.join("antflow.db");
        if old_db.exists() {
            let _ = std::fs::copy(&old_db, &new_db);
        }
        
        // Copy config.toml
        let old_config = current_dir.join("config.toml");
        let new_config = new_storage_dir.join("config.toml");
        if old_config.exists() && !new_config.exists() {
            let _ = std::fs::copy(&old_config, &new_config);
        }
    }

    // Update AppState storage_dir and cfg
    *state.storage_dir.lock().await = new_storage_dir;
    *state.cfg.lock().await = cfg;

    // Terminate currently running Ollama so it can be restarted in the next step with correct environment
    stop_ollama().await;

    Ok(())
}

async fn start_ollama_if_installed(
    storage: &crate::config::StorageLocation,
    storage_dir: &std::path::Path,
) -> bool {
    if crate::ai::ollama::is_running("http://127.0.0.1:11434").await {
        return true;
    }
    let mut exe_path = "ollama".to_string();
    let mut found = false;
    
    let cmd = if cfg!(target_os = "windows") { "ollama.exe" } else { "ollama" };
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            let p = path.join(cmd);
            if p.exists() {
                exe_path = p.to_string_lossy().into_owned();
                found = true;
                break;
            }
        }
    }
    
    if !found {
        if let Ok(exe_p) = std::env::current_exe() {
            if let Some(parent) = exe_p.parent() {
                let p_exe = parent.join(cmd);
                if p_exe.exists() {
                    exe_path = p_exe.to_string_lossy().into_owned();
                    found = true;
                }
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        if !found {
            if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
                let p1 = std::path::PathBuf::from(localappdata).join("Programs\\Ollama\\ollama.exe");
                if p1.exists() {
                    exe_path = p1.to_string_lossy().into_owned();
                    found = true;
                }
            }
        }
        if !found {
            let p2 = std::path::PathBuf::from("C:\\Program Files\\Ollama\\ollama.exe");
            if p2.exists() {
                exe_path = p2.to_string_lossy().into_owned();
                found = true;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if !found {
            if std::path::Path::new("/Applications/Ollama.app/Contents/Resources/ollama").exists() {
                exe_path = "/Applications/Ollama.app/Contents/Resources/ollama".to_string();
                found = true;
            } else if std::path::Path::new("/usr/local/bin/ollama").exists() {
                exe_path = "/usr/local/bin/ollama".to_string();
                found = true;
            } else if std::path::Path::new("/usr/bin/ollama").exists() {
                exe_path = "/usr/bin/ollama".to_string();
                found = true;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if !found {
            if std::path::Path::new("/usr/local/bin/ollama").exists() {
                exe_path = "/usr/local/bin/ollama".to_string();
                found = true;
            } else if std::path::Path::new("/usr/bin/ollama").exists() {
                exe_path = "/usr/bin/ollama".to_string();
                found = true;
            }
        }
    }

    if found {
        let mut cmd = tokio::process::Command::new(exe_path);
        cmd.arg("serve");
        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // Set or remove OLLAMA_MODELS env variable depending on storage mode
        if storage == &crate::config::StorageLocation::Portable {
            let models_dir = storage_dir.join("ollama_models");
            let _ = std::fs::create_dir_all(&models_dir);
            if let Some(s) = models_dir.to_str() {
                cmd.env("OLLAMA_MODELS", s);
                std::env::set_var("OLLAMA_MODELS", s);
            }
        } else {
            cmd.env_remove("OLLAMA_MODELS");
            std::env::remove_var("OLLAMA_MODELS");
        }

        let _ = cmd.spawn();

        for _ in 0..75 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if crate::ai::ollama::is_running("http://127.0.0.1:11434").await {
                return true;
            }
        }
    }
    false
}

#[tauri::command(rename_all = "snake_case")]
async fn list_ollama_models(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<String>, String> {
    let mut ollama_url = {
        let cfg = state.cfg.lock().await;
        if cfg.ai.provider == "ollama" && !cfg.ai.base_url.is_empty() {
            cfg.ai.base_url.clone()
        } else {
            "http://127.0.0.1:11434".to_string()
        }
    };
    if ollama_url.contains("localhost") {
        ollama_url = ollama_url.replace("localhost", "127.0.0.1");
    }
    
    // Auto-start Ollama if it is installed but not running
    if ollama_url == "http://127.0.0.1:11434" || ollama_url == "http://localhost:11434" {
        let storage = {
            let cfg = state.cfg.lock().await;
            cfg.storage.clone()
        };
        let storage_dir = state.storage_dir.lock().await.clone();
        let _ = start_ollama_if_installed(&storage, &storage_dir).await;
    }

    match crate::ai::ollama::list_models(&ollama_url).await {
        Ok(models) => Ok(models.into_iter().map(|m| m.name).collect()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
async fn set_session_memory(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut cfg = state.cfg.lock().await.clone();
    cfg.session.remember_sessions = enabled;
    config::save(&*state.storage_dir.lock().await, &cfg).map_err(|e| e.to_string())?;
    *state.cfg.lock().await = cfg;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn set_run_in_background(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut cfg = state.cfg.lock().await.clone();
    cfg.run_in_background = enabled;
    config::save(&*state.storage_dir.lock().await, &cfg).map_err(|e| e.to_string())?;
    *state.cfg.lock().await = cfg;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn get_device_info(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<JsonValue, String> {
    Ok(state.device_info_json.clone())
}

#[tauri::command(rename_all = "snake_case")]
async fn verify_api_key(
    provider_id: String,
    base_url: String,
    api_key: String,
    model: String,
) -> std::result::Result<(), String> {
    let mut normalized_url = base_url.clone();
    if provider_id == "ollama" {
        if normalized_url.contains("localhost") {
            normalized_url = normalized_url.replace("localhost", "127.0.0.1");
        }
        // 1. Verify Ollama is running
        let provider = crate::ai::provider_for_config(&crate::config::AiProviderConfig {
            provider: provider_id.clone(),
            base_url: normalized_url.clone(),
            api_key: api_key.clone(),
            model: model.clone(),
            vision_capable: false,
        });
        provider.ping().await.map_err(|e| e.to_string())?;

        // 2. Verify that the model is downloaded/installed
        let installed = crate::ai::ollama::list_models(&normalized_url).await
            .map_err(|e| format!("Failed to list installed models: {}", e))?;
        
        let target = model.to_lowercase();
        let has_model = installed.iter().any(|m| {
            let name_lower = m.name.to_lowercase();
            name_lower == target 
            || format!("{}:latest", name_lower) == target 
            || name_lower == format!("{}:latest", target)
            || name_lower.split(':').next() == target.split(':').next()
        });

        if !has_model {
            return Err(format!("Model '{}' is not downloaded in Ollama. Please download it first.", model));
        }

        return Ok(());
    }

    let provider = crate::ai::provider_for_config(&crate::config::AiProviderConfig {
        provider: provider_id,
        base_url,
        api_key,
        model,
        vision_capable: false,
    });
    provider.ping().await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn search_online_models(
    query: String,
    _state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<ModelInfo>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let results = crate::ai::ollama::search_huggingface_gguf(&query).await
        .map_err(|e| e.to_string())?;

    let ollama_url = "http://127.0.0.1:11434".to_string();
    let installed_names: std::collections::HashSet<String> =
        match crate::ai::ollama::list_models(&ollama_url).await {
            Ok(models) => models.into_iter().map(|m| m.name.to_lowercase()).collect(),
            Err(_) => std::collections::HashSet::new(),
        };

    let mut out = Vec::new();
    for m in results {
        let hf_name = format!("hf.co/{}", m.id);
        let target_lower = hf_name.to_lowercase();
        
        let is_installed = installed_names.contains(&target_lower)
            || installed_names.contains(&format!("{}:latest", target_lower))
            || installed_names.iter().any(|name| name.contains(&m.id.to_lowercase()));

        let is_vision = m.id.to_lowercase().contains("vision")
            || m.id.to_lowercase().contains("llava")
            || m.tags.iter().any(|tag| tag.to_lowercase().contains("vision"));

        out.push(ModelInfo {
            name: hf_name,
            provider: "ollama".to_string(),
            provider_name: "Ollama (Hugging Face)".to_string(),
            base_url: ollama_url.clone(),
            vision: is_vision,
            size_gb: m.size_bytes as f32 / (1024.0 * 1024.0 * 1024.0),
            installed: is_installed,
        });
    }

    Ok(out)
}


#[tauri::command(rename_all = "snake_case")]
async fn show_window(app_handle: tauri::AppHandle) -> std::result::Result<(), String> {
    if let Some(w) = app_handle.get_webview_window("main") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn hide_window(app_handle: tauri::AppHandle) -> std::result::Result<(), String> {
    if let Some(w) = app_handle.get_webview_window("main") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn get_welcome_greeting(
    lang: Option<String>,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<String, String> {
    let cfg = state.cfg.lock().await.clone();
    if cfg.ai.provider.is_empty() || cfg.ai.model.is_empty() {
        return Ok("How can I help you today?".to_string());
    }
    let lang_str = lang.unwrap_or_else(|| "en".to_string());
    let greeting_fut = crate::ai::generate_welcome_greeting(&cfg.ai, &lang_str);
    match tokio::time::timeout(std::time::Duration::from_secs(5), greeting_fut).await {
        Ok(Ok(msg)) => Ok(msg),
        _ => Ok("How can I help you today?".to_string()),
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn get_context_tokens(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<usize, String> {
    let history = state.history.lock().await;
    let total_chars: usize = history.iter()
        .filter(|m| m.role != "system")
        .map(|m| {
            if let Some(s) = m.content.as_str() {
                s.len()
            } else {
                m.content.to_string().len()
            }
        })
        .sum();
    Ok(total_chars / 4)
}

#[tauri::command(rename_all = "snake_case")]
async fn start_voice_session(
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    crate::voice::start_session(app_handle).await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn stop_voice_session() -> std::result::Result<(), String> {
    crate::voice::stop_session().await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn send_voice_chunk(
    chunk: String,
) -> std::result::Result<(), String> {
    crate::voice::receive_chunk(chunk).await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn check_mobile_permissions(
    _app_handle: tauri::AppHandle,
) -> std::result::Result<JsonValue, String> {
    #[cfg(target_os = "android")]
    {
        if let Ok(guard) = get_mobile_permissions().lock() {
            if !guard.is_empty() {
                if let Ok(val) = serde_json::from_str::<JsonValue>(&guard) {
                    return Ok(val);
                }
            }
        }

        // Fallback: Shell command query!
        let mic_granted = true; // Webview handles actual recording check in JS
        
        let overlay_granted = {
            let op = std::process::Command::new("appops")
                .args(["get", "com.antflow.app", "SYSTEM_ALERT_WINDOW"])
                .output();
            if let Ok(o) = op {
                let s = String::from_utf8_lossy(&o.stdout);
                s.contains("allow")
            } else {
                false
            }
        };

        let is_assistant = {
            let op = std::process::Command::new("settings")
                .args(["get", "secure", "assistant"])
                .output();
            if let Ok(o) = op {
                let s = String::from_utf8_lossy(&o.stdout);
                s.contains("com.antflow.app")
            } else {
                false
            }
        };

        return Ok(serde_json::json!({
            "microphone": mic_granted,
            "overlay": overlay_granted,
            "assistant": is_assistant,
            "notifications": true
        }));
    }

    #[cfg(not(target_os = "android"))]
    {
        Ok(serde_json::json!({
            "microphone": true,
            "overlay": true,
            "assistant": true,
            "notifications": true
        }))
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn request_mobile_notification_permission() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(10, Ordering::Relaxed);
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn open_mobile_notification_settings() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(11, Ordering::Relaxed);
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn enter_pip_mode(
    app_handle: tauri::AppHandle,
) -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(3, Ordering::Relaxed);
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Some(w) = app_handle.get_webview_window("main") {
            w.minimize().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(target_os = "android")]
use std::sync::OnceLock;
#[cfg(target_os = "android")]
static MOBILE_PERMISSIONS: OnceLock<std::sync::Mutex<String>> = OnceLock::new();
#[cfg(target_os = "android")]
static PIP_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "android")]
static PENDING_ACTION: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_antflow_app_MainActivity_checkPendingActions(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jint {
    use std::sync::atomic::Ordering;
    PENDING_ACTION.swap(0, Ordering::Relaxed)
}

#[cfg(target_os = "android")]
fn get_mobile_permissions() -> &'static std::sync::Mutex<String> {
    MOBILE_PERMISSIONS.get_or_init(|| std::sync::Mutex::new(String::new()))
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_antflow_app_MainActivity_sendMobilePermissions(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    permissions: jni::objects::JString,
) {
    if let Ok(rust_str) = env.get_string(&permissions) {
        let rust_string: String = rust_str.into();
        if let Ok(mut guard) = get_mobile_permissions().lock() {
            *guard = rust_string;
        }
    }
}

#[cfg(target_os = "android")]
static MOBILE_AUDIO_DEVICES: OnceLock<std::sync::Mutex<String>> = OnceLock::new();

#[cfg(target_os = "android")]
fn get_mobile_audio_devices_mutex() -> &'static std::sync::Mutex<String> {
    MOBILE_AUDIO_DEVICES.get_or_init(|| std::sync::Mutex::new(String::new()))
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_antflow_app_MainActivity_sendMobileAudioDevices(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    devices: jni::objects::JString,
) {
    if let Ok(rust_str) = env.get_string(&devices) {
        let rust_string: String = rust_str.into();
        if let Ok(mut guard) = get_mobile_audio_devices_mutex().lock() {
            *guard = rust_string;
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn get_mobile_audio_devices(
    _app_handle: tauri::AppHandle,
) -> std::result::Result<serde_json::Value, String> {
    #[cfg(target_os = "android")]
    {
        if let Ok(guard) = get_mobile_audio_devices_mutex().lock() {
            if !guard.is_empty() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&guard) {
                    return Ok(val);
                }
            }
        }
        Ok(serde_json::json!([]))
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(serde_json::json!([]))
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_antflow_app_MainActivity_checkPipRequest(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jboolean {
    use std::sync::atomic::Ordering;
    if PIP_REQUESTED.swap(false, Ordering::Relaxed) {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Native Background AI Model Downloader JNI & Commands
// ---------------------------------------------------------------------------

#[cfg(target_os = "android")]
static DOWNLOAD_PERCENT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
#[cfg(target_os = "android")]
static DOWNLOAD_STATUS: OnceLock<std::sync::Mutex<String>> = OnceLock::new();
#[cfg(target_os = "android")]
static DOWNLOAD_BYTES_FETCHED: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
#[cfg(target_os = "android")]
static DOWNLOAD_BYTES_TOTAL: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

#[cfg(target_os = "android")]
fn get_download_status_mutex() -> &'static std::sync::Mutex<String> {
    DOWNLOAD_STATUS.get_or_init(|| std::sync::Mutex::new("Initializing...".to_string()))
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "C" fn Java_com_antflow_app_NativeBridge_updateDownloadProgress(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    percent: jni::sys::jint,
    status: jni::objects::JString,
    bytes_fetched: jni::sys::jlong,
    bytes_total: jni::sys::jlong,
) {
    use std::sync::atomic::Ordering;
    DOWNLOAD_PERCENT.store(percent, Ordering::Relaxed);
    DOWNLOAD_BYTES_FETCHED.store(bytes_fetched, Ordering::Relaxed);
    DOWNLOAD_BYTES_TOTAL.store(bytes_total, Ordering::Relaxed);
    if let Ok(rust_str) = env.get_string(&status) {
        if let Ok(mut guard) = get_download_status_mutex().lock() {
            *guard = rust_str.into();
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn start_mobile_local_ai_download() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        DOWNLOAD_PERCENT.store(0, Ordering::Relaxed);
        DOWNLOAD_BYTES_FETCHED.store(0, Ordering::Relaxed);
        if let Ok(mut guard) = get_download_status_mutex().lock() {
            *guard = "Connecting to download servers...".to_string();
        }
        PENDING_ACTION.store(7, Ordering::Relaxed);
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Native background downloader is only supported on Android".to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn pause_mobile_local_ai_download() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(8, Ordering::Relaxed);
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Native background downloader is only supported on Android".to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn cancel_mobile_local_ai_download() -> std::result::Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        PENDING_ACTION.store(9, Ordering::Relaxed);
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        Err("Native background downloader is only supported on Android".to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn get_mobile_local_ai_download_progress() -> std::result::Result<serde_json::Value, String> {
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::Ordering;
        let pct = DOWNLOAD_PERCENT.load(Ordering::Relaxed);
        let fetched = DOWNLOAD_BYTES_FETCHED.load(Ordering::Relaxed);
        let total = DOWNLOAD_BYTES_TOTAL.load(Ordering::Relaxed);
        let status = if let Ok(guard) = get_download_status_mutex().lock() {
            guard.clone()
        } else {
            "Running...".to_string()
        };
        Ok(serde_json::json!({
            "percent": pct,
            "status": status,
            "bytes_fetched": fetched,
            "bytes_total": total
        }))
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(serde_json::json!({
            "percent": 100,
            "status": "Completed",
            "bytes_fetched": 0,
            "bytes_total": 0
        }))
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn read_local_model_file(
    filename: String,
    app_handle: tauri::AppHandle,
) -> std::result::Result<Vec<u8>, String> {
    use tauri::Manager;
    let local_data = app_handle.path().app_local_data_dir()
        .map_err(|e| e.to_string())?;
    
    // Resolve: app_local_data_dir/llama_model/filename
    let path = local_data.join("llama_model").join(&filename);
    if !path.exists() {
        return Err(format!("Local file '{}' not found in cache", filename));
    }
    
    std::fs::read(&path).map_err(|e| e.to_string())
}


