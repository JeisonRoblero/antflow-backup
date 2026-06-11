pub mod ai;
pub mod config;
pub mod db;
pub mod device;
pub mod executor;
pub mod prompt;
pub mod redact;
pub mod scheduler;
pub mod screenshot;
pub mod search;
pub mod tools;
pub mod json_utils;
pub mod ui;
pub mod tui;
pub mod gui;
pub mod wizard;

#[cfg(any(feature = "gui", target_os = "android", target_os = "ios"))]
pub mod voice;

use anyhow::Result;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::ai::ChatMessage;
use crate::config::AppConfig;
use crate::executor::new_stop_flag;
use crate::redact::RedactPatterns;
use crate::ui::console;

pub async fn run_main() -> Result<()> {
    let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));

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
    #[cfg(debug_assertions)]
    {
        let log_path = std::env::temp_dir().join("antflow_debug.log");
        let _ = std::fs::write(&log_path, "");
    }
    
    let exe_dir = exe_path.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf();
    let args: Vec<String> = env::args().skip(1).collect();
    let subcommand = args.first().map(|s| s.as_str()).unwrap_or("");
    let is_tui_arg = subcommand == "tui" 
        || args.contains(&"--tui".to_string()) 
        || args.contains(&"cli".to_string())
        || subcommand == "info" 
        || subcommand == "tasks" 
        || subcommand == "setup" 
        || subcommand == "config" 
        || subcommand == "stop" 
        || subcommand == "clear" 
        || subcommand == "undo" 
        || subcommand == "model" 
        || subcommand == "help" 
        || args.contains(&"-h".to_string()) 
        || args.contains(&"--help".to_string());

    #[cfg(windows)]
    {
        if is_tui_arg {
            unsafe {
                windows_sys::Win32::System::Console::AttachConsole(0xFFFFFFFF);
            }
        }
    }


    let provisional_storage = config::resolve_storage_dir(&config::StorageLocation::Portable, &exe_dir);
    let mut cfg = config::load(&provisional_storage).unwrap_or_default();

    let storage_dir = config::resolve_storage_dir(&cfg.storage, &exe_dir);
    cfg = config::load(&storage_dir).unwrap_or_default();

    // Auto-heal/validate Ollama vision_capable setting on startup
    if cfg.ai.provider == "ollama" && !cfg.ai.model.is_empty() {
        let expected_vision = crate::ai::ollama::is_vision_model(&cfg.ai.model);
        if cfg.ai.vision_capable != expected_vision {
            cfg.ai.vision_capable = expected_vision;
            let _ = config::save(&storage_dir, &cfg);
        }
    }

    match subcommand {
        "setup" => {
            cfg.setup_complete = false;
            config::save(&storage_dir, &cfg)?;
            println!("Setup reset. Run 'antflow' to launch the TUI onboarding wizard.");
            return Ok(());
        }
        "stop" => {
            println!("Sending stop signal to background processes...");
            println!("  (Background scheduler will stop on its next poll cycle)");
            return Ok(());
        }
        "config" => {
            println!("To change settings, run the wizard again: antflow setup");
            return Ok(());
        }
        "tasks" => {
            let conn = db::open(&storage_dir)?;
            let subcmd = args.get(1).map(|s| s.as_str()).unwrap_or("list");
            let target_id = args.get(2).map(|s| s.as_str()).unwrap_or("");
            match subcmd {
                "delete" => {
                    if target_id.is_empty() {
                        println!("Usage: antflow tasks delete <id>");
                    } else {
                        db::delete_task(&conn, target_id)?;
                        println!("Task {} deleted.", target_id);
                    }
                }
                "pause" => {
                    if target_id.is_empty() {
                        println!("Usage: antflow tasks pause <id>");
                    } else {
                        db::set_task_enabled(&conn, target_id, false)?;
                        println!("Task {} paused.", target_id);
                    }
                }
                "resume" => {
                    if target_id.is_empty() {
                        println!("Usage: antflow tasks resume <id>");
                    } else {
                        db::set_task_enabled(&conn, target_id, true)?;
                        println!("Task {} resumed.", target_id);
                    }
                }
                _ => {
                    scheduler::print_task_list(&conn)?;
                    println!("\nUsage:");
                    println!("  antflow tasks delete <id>");
                    println!("  antflow tasks pause  <id>");
                    println!("  antflow tasks resume <id>");
                }
            }
            return Ok(());
        }
        "clear" => {
            let conn = db::open(&storage_dir)?;
            if let Some(session_id) = args.get(1) {
                db::clear_session(&conn, session_id)?;
                println!("Session {} cleared.", session_id);
            } else {
                println!("Usage: antflow clear <session_id>");
                println!("Session IDs appear in the prompt: antflow [xxxxxxxx]>");
            }
            return Ok(());
        }
        "undo" => {
            let conn = db::open(&storage_dir)?;
            let limit = args.get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(10);
            let mut stmt = conn.prepare(
                "SELECT action_type, params_json, created_at FROM undo_log ORDER BY created_at DESC LIMIT ?1"
            )?;
            let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            println!("\nRecent actions (undo log):");
            println!("{:<4} {:<20} {:<20} {}", "#", "Action", "Time", "Params");
            println!("{}", "-".repeat(80));
            for (i, row) in rows.enumerate() {
                let (action, params, time) = row?;
                let params_preview: String = params.chars().take(40).collect();
                let time_short = if time.len() >= 19 { &time[..19] } else { &time };
                println!("{:<4} {:<20} {:<20} {}", i + 1, action, time_short, params_preview);
            }
            return Ok(());
        }
        "info" => {
            let conn = db::open(&storage_dir)?;
            let size = db::db_size_bytes(&storage_dir);
            println!("\n=== antflow info ===");
            println!("Storage     : {}", storage_dir.display());
            println!("Database    : {:.1} KB", size as f64 / 1024.0);
            println!("Provider    : {}", if cfg.ai.provider.is_empty() { "not configured" } else { &cfg.ai.provider });
            println!("Model       : {}", if cfg.ai.model.is_empty() { "not configured" } else { &cfg.ai.model });
            println!("Vision      : {}", cfg.ai.vision_capable);
            println!("Scheduler   : {}", if cfg.scheduler.enabled { "enabled" } else { "disabled" });
            println!("Memory      : {}", if cfg.session.remember_sessions { "enabled" } else { "disabled" });
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0)).unwrap_or(0);
            println!("Messages    : {}", count);
            return Ok(());
        }
        "model" => {
            let subcmd = args.get(1).map(|s| s.as_str()).unwrap_or("show");
            match subcmd {
                "show" | "list" => {
                    println!("Current model : {}", if cfg.ai.model.is_empty() { "not configured" } else { &cfg.ai.model });
                    println!("Provider      : {}", if cfg.ai.provider.is_empty() { "not configured" } else { &cfg.ai.provider });
                    if cfg.ai.provider == "ollama" {
                        println!("\nInstalled Ollama models:");
                        match crate::ai::ollama::list_models(&cfg.ai.base_url).await {
                            Ok(models) => {
                                for m in &models {
                                    let active = if m.name == cfg.ai.model { " <-- active" } else { "" };
                                    println!("  {} ({}){}", m.name, m.size_human(), active);
                                }
                            }
                            Err(e) => println!("  Could not reach Ollama: {}", e),
                        }
                    }
                }
                "set" => {
                    if let Some(model_name) = args.get(2) {
                        cfg.ai.model = model_name.clone();
                        cfg.ai.vision_capable = crate::ai::ollama::is_vision_model(model_name);
                        config::save(&storage_dir, &cfg)?;
                        println!("Model set to: {}", model_name);
                        println!("Vision      : {}", cfg.ai.vision_capable);
                    } else {
                        println!("Usage: antflow model set <model_name>");
                        println!("Example: antflow model set llama3.2");
                    }
                }
                "pull" => {
                    if let Some(model_name) = args.get(2) {
                        if cfg.ai.provider != "ollama" {
                            println!("Pull only works with Ollama provider.");
                        } else {
                            let spinner = console::start_spinner(&format!("Downloading {}...", model_name));
                            match crate::ai::ollama::pull_model(&cfg.ai.base_url, model_name).await {
                                Ok(_) => {
                                    console::stop_spinner(spinner);
                                    println!("Downloaded {}. Run: antflow model set {}", model_name, model_name);
                                }
                                Err(e) => {
                                    console::stop_spinner(spinner);
                                    println!("Download failed: {}", e);
                                }
                            }
                        }
                    } else {
                        println!("Usage: antflow model pull <model_name>");
                        println!("Example: antflow model pull qwen2.5:7b");
                    }
                }
                _ => {
                    println!("Usage:");
                    println!("  antflow model           Show current model");
                    println!("  antflow model set <name> Switch to a different model");
                    println!("  antflow model pull <name> Download a new Ollama model");
                }
            }
            return Ok(());
        }
        "help" | "--help" | "-h" => {
            println!("\nantflow - automation agent");
            println!("\nUSAGE:");
            println!("  antflow                        Start the agent");
            println!("  antflow setup                  Re-run setup wizard");
            println!("  antflow info                   Show config and stats");
            println!("  antflow tasks                  List scheduled tasks");
            println!("  antflow tasks delete <id>      Delete a task");
            println!("  antflow tasks pause  <id>      Pause a task");
            println!("  antflow tasks resume <id>      Resume a task");
            println!("  antflow clear <session_id>     Clear session history");
            println!("  antflow undo [N]               Show last N actions (default 10)");
            println!("  antflow stop                   Stop background scheduler");
            println!("  antflow model                  Show current model");
            println!("  antflow model set <name>       Switch model");
            println!("  antflow model pull <name>      Download Ollama model");
            return Ok(());
        }
        _ => {}
    }

    dbg_log!("storage_dir: {}", storage_dir.display());
    dbg_log!("cfg.ai.provider: '{}'", cfg.ai.provider);
    dbg_log!("cfg.ai.model: '{}'", cfg.ai.model);
    dbg_log!("cfg.ai.vision_capable: {}", cfg.ai.vision_capable);
    dbg_log!("cfg.setup_complete: {}", cfg.setup_complete);
    let conn = db::open(&storage_dir)?;
    dbg_log!("db opened ok");

    let conn_shared = Arc::new(Mutex::new(conn));

    let device_info = device::collect()?;
    dbg_log!("device info ok: platform={} os={} screen={}x{} scale={:.1} touch={} vision={}",
        device_info.platform,
        device_info.os_name,
        device_info.screen.width,
        device_info.screen.height,
        device_info.screen.scale_factor,
        device_info.touch_capable,
        cfg.ai.vision_capable,
    );
    dbg_log!("cpu={} cores={} ram={}MB",
        device_info.cpu_model,
        device_info.cpu_cores,
        device_info.ram_mb,
    );
    let system_prompt = prompt::build_system_prompt_with_vision(&device_info, cfg.ai.vision_capable);

    let run_tui_mode = subcommand == "tui" || args.contains(&"--tui".to_string()) || args.contains(&"cli".to_string());
    
    #[cfg(not(any(feature = "gui", target_os = "android", target_os = "ios")))]
    let is_gui = false;
    #[cfg(any(feature = "gui", target_os = "android", target_os = "ios"))]
    let is_gui = !run_tui_mode;

    if !is_gui && cfg.ai.provider.is_empty() {
        console::print_error(
            "No AI provider configured. Run: antflow config ai"
        );
        return Ok(());
    }

    let _provider = ai::provider_for_config(&cfg.ai);
    let _patterns = RedactPatterns::new();

    let stop_flag = new_stop_flag();
    let stop_for_scheduler = stop_flag.clone();
    let (sched_tx, _sched_rx) = tokio::sync::mpsc::channel::<String>(32);

    if cfg.scheduler.enabled {
        let conn_clone = conn_shared.clone();
        let poll = cfg.scheduler.poll_interval_secs;
        tokio::spawn(async move {
            scheduler::run_scheduler_loop(conn_clone, poll, stop_for_scheduler, sched_tx).await;
        });
    }

    let session_id = Uuid::new_v4().to_string();
    let _session_short = &session_id[..8];

    let system_msg = ChatMessage::text("system", &system_prompt);

    let mut base_history: Vec<ChatMessage> = {
        let guard = conn_shared.lock().await;
        if cfg.session.remember_sessions {
            db::load_messages(&guard, &session_id, cfg.session.max_history_messages)
                .unwrap_or_default()
                .into_iter()
                .map(|m| ChatMessage::text(&m.role, &m.content))
                .collect()
        } else {
            Vec::new()
        }
    };

    base_history.insert(0, system_msg);

    let conn = Arc::try_unwrap(conn_shared)
        .map_err(|_| anyhow::anyhow!("Could not unwrap connection"))?
        .into_inner();

    #[cfg(any(feature = "gui", target_os = "android", target_os = "ios"))]
    {
        let run_tui_mode = subcommand == "tui" || args.contains(&"--tui".to_string()) || args.contains(&"cli".to_string());
        if run_tui_mode {
            dbg_log!("launching TUI...");
            tui::runner::run(cfg, conn, storage_dir).await?;
            dbg_log!("TUI exited cleanly");
        } else {
            dbg_log!("launching GUI...");
            gui::run(cfg, conn, storage_dir).await?;
            dbg_log!("GUI exited cleanly");
        }
    }

    #[cfg(not(any(feature = "gui", target_os = "android", target_os = "ios")))]
    {
        dbg_log!("launching TUI...");
        tui::runner::run(cfg, conn, storage_dir).await?;
        dbg_log!("TUI exited cleanly");
    }

    println!("\nGoodbye.");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let storage_dir = std::path::PathBuf::from("/data/data/com.antflow.app/files");
        std::fs::create_dir_all(&storage_dir).ok();

        let mut cfg = config::load(&storage_dir).unwrap_or_default();
        if cfg.ai.provider.is_empty() && !cfg.setup_complete {
            cfg.storage = config::StorageLocation::Portable;
            cfg.setup_complete = false;
            cfg.ai.provider = "".to_string();
            cfg.ai.model = "".to_string();
            cfg.ai.vision_capable = false;
            let _ = config::save(&storage_dir, &cfg);
        }

        let conn = db::open(&storage_dir).expect("Failed to open SQLite database");

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async {
                let _ = gui::run(cfg, conn, storage_dir).await;
            });
    }
}
