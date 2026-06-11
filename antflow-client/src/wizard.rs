// wizard.rs
// The first-run setup wizard.
// Runs automatically when setup_complete is false in the config.
// Guides the user step by step through every required setting.
// Every prompt includes "you can change this later with: antflow config <setting>".

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::ai::ollama;
use crate::ai::KNOWN_PROVIDERS;
use crate::config::{self, AppConfig, AiProviderConfig, StorageLocation};
use crate::device;
use crate::ui::console;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

// Runs the full wizard and returns a completed AppConfig and the resolved
// storage directory path.
pub async fn run(exe_dir: &Path) -> Result<(AppConfig, PathBuf)> {
    console::print_heading("Welcome to antflow");
    println!("This wizard will set up antflow in a few steps.");
    println!("Every setting can be changed later by running: antflow config\n");

    // -----------------------------------------------------------------------
    // Auto mode
    // -----------------------------------------------------------------------

    if console::confirm("Run in AUTO mode (configure everything automatically)?", false) {
        return auto_setup(exe_dir).await;
    }

    // -----------------------------------------------------------------------
    // Step 1: Storage location
    // -----------------------------------------------------------------------

    console::print_heading("Step 1 of 3 - Storage location");
    println!("Portable mode stores all files next to the antflow binary (ideal for USB drives).");
    println!("Global mode stores files in your user config directory.\n");

    let choice = console::menu(
        "Where should antflow store its data?",
        &[
            "Portable (next to the binary, USB-friendly)",
            "Global   (~/.config/antflow or AppData on Windows)",
        ],
    );

    let storage = if choice == 0 {
        StorageLocation::Portable
    } else {
        StorageLocation::Global
    };

    let storage_dir = config::resolve_storage_dir(&storage, exe_dir);
    println!("\nStorage directory: {}", storage_dir.display());
    println!("  (Change later with: antflow config storage)\n");

    // -----------------------------------------------------------------------
    // Step 2: AI provider
    // -----------------------------------------------------------------------

    console::print_heading("Step 2 of 3 - AI provider");
    let ai = setup_ai_provider().await?;

    // -----------------------------------------------------------------------
    // Step 3: Session memory
    // -----------------------------------------------------------------------

    console::print_heading("Step 3 of 3 - Session memory");
    println!("By default antflow remembers your conversation history between sessions.");
    let remember = console::confirm("Enable session memory?", true);
    println!("  (Change later with: antflow config memory)\n");

    // -----------------------------------------------------------------------
    // Assemble and save config
    // -----------------------------------------------------------------------

    let mut cfg = AppConfig::default();
    cfg.storage = storage;
    cfg.ai = ai;
    cfg.session.remember_sessions = remember;
    cfg.setup_complete = true;

    config::save(&storage_dir, &cfg)?;
    println!("\nSetup complete. Starting antflow...\n");

    Ok((cfg, storage_dir))
}

// ---------------------------------------------------------------------------
// Auto setup
// ---------------------------------------------------------------------------

// Tries to configure antflow with zero interaction.
// Preference order: running Ollama > installed Ollama > first known cloud provider.
async fn auto_setup(exe_dir: &Path) -> Result<(AppConfig, PathBuf)> {
    println!("\nRunning auto setup...");

    let storage = StorageLocation::Portable;
    let storage_dir = config::resolve_storage_dir(&storage, exe_dir);

    // Check if Ollama is already running locally.
    let base_url = "http://127.0.0.1:11434";
    let ai = if ollama::is_running(base_url).await {
        println!("  Ollama detected. Using local AI.");
        // Pick the first vision-capable model available.
        let models = ollama::list_models(base_url).await.unwrap_or_default();
        let vision_model = models.iter()
            .find(|m| ollama::is_vision_model(&m.name))
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "llava".to_string());

        AiProviderConfig {
            provider: "ollama".to_string(),
            base_url: base_url.to_string(),
            api_key: String::new(),
            model: vision_model,
            vision_capable: true,
        }
    } else {
        // No Ollama. Leave AI unconfigured; user will be prompted on first chat.
        println!("  No local AI detected. You can configure an AI provider by running: antflow config ai");
        AiProviderConfig::default()
    };

    let mut cfg = AppConfig::default();
    cfg.storage = storage;
    cfg.ai = ai;
    cfg.setup_complete = true;

    config::save(&storage_dir, &cfg)?;
    println!("  Auto setup complete.\n");

    Ok((cfg, storage_dir))
}

// ---------------------------------------------------------------------------
// AI provider sub-wizard
// ---------------------------------------------------------------------------

// Guides the user through choosing and configuring an AI provider.
async fn setup_ai_provider() -> Result<AiProviderConfig> {
    // Build the menu items dynamically from the KNOWN_PROVIDERS catalog
    // plus a local Ollama option and a custom option at the end.
    let mut items: Vec<String> = vec!["Local AI (Ollama - runs on this device, free)".to_string()];
    for p in KNOWN_PROVIDERS {
        items.push(format!("{} (cloud)", p.display_name));
    }
    items.push("Custom provider (enter your own endpoint)".to_string());

    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    let choice = console::menu("Choose an AI provider:", &refs);

    if choice == 0 {
        return setup_ollama().await;
    }

    // Offset 1 = first known provider, last item = custom.
    if choice - 1 < KNOWN_PROVIDERS.len() {
        let provider = &KNOWN_PROVIDERS[choice - 1];
        return setup_cloud_provider(provider.id, provider.display_name, provider.base_url, provider.recommended_model).await;
    }

    // Custom provider.
    setup_custom_provider().await
}

// ---------------------------------------------------------------------------
// Ollama setup
// ---------------------------------------------------------------------------

async fn setup_ollama() -> Result<AiProviderConfig> {
    let base_url = "http://127.0.0.1:11434";

    println!("\nChecking if Ollama is running...");

    if ollama::is_running(base_url).await {
        println!("  Ollama is running.");
        let model = pick_ollama_model(base_url).await?;

        return Ok(AiProviderConfig {
            provider: "ollama".to_string(),
            base_url: base_url.to_string(),
            api_key: String::new(),
            model: model.name.clone(),
            vision_capable: ollama::is_vision_model(&model.name),
        });
    }

    println!("  Ollama is not running or not installed.");
    let gpu = device::detect_gpu();
    println!("  Detected GPU: {}", gpu);
    println!("  Ollama download size: ~60 MB (installer). Models are downloaded separately.");

    let install_choice = console::menu(
        "How would you like to install Ollama?",
        &[
            "Global install (installs system-wide, leaves a trace)",
            "Portable install (installs next to antflow, USB-friendly)",
            "Skip (I will set up AI later with: antflow config ai)",
        ],
    );

    match install_choice {
        0 => {
            println!("  Opening the Ollama installer...");
            open::that(ollama::installer_url()).ok();
            println!("  After installing Ollama, run: antflow config ai");
        }
        1 => {
            println!("  Portable Ollama install is not yet automated.");
            println!("  Download manually from: {}", ollama::installer_url());
            println!("  Place the binary next to antflow and run: antflow config ai");
        }
        _ => {
            println!("  Skipping AI setup. Run: antflow config ai to set it up later.");
        }
    }

    Ok(AiProviderConfig::default())
}

// Presents the user's installed Ollama models and lets them pick one.
async fn pick_ollama_model(base_url: &str) -> Result<ollama::OllamaModel> {
    let models = ollama::list_models(base_url).await?;

    if models.is_empty() {
        println!("  No models installed. A vision-capable model is required.");
        println!("  Recommended: llava (4.5 GB)");
        let download = console::confirm("Download llava now?", true);
        if download {
            let spinner = console::start_spinner("Downloading llava...");
            ollama::pull_model(base_url, "llava").await?;
            console::stop_spinner(spinner);
            return Ok(ollama::OllamaModel {
                name: "llava".to_string(),
                size_bytes: 0,
            });
        }
        anyhow::bail!("No model selected. Run: antflow config ai");
    }

    // List models, flagging non-vision ones.
    println!("\nInstalled models:");
    let mut display_items = Vec::new();
    for m in &models {
        let vision_tag = if ollama::is_vision_model(&m.name) {
            ""
        } else {
            "  [no vision support]"
        };
        display_items.push(format!("{} ({}){}", m.name, m.size_human(), vision_tag));
    }

    display_items.push("Download a different model...".to_string());

    let refs: Vec<&str> = display_items.iter().map(|s| s.as_str()).collect();
    let choice = console::menu("Select a model:", &refs);

    if choice < models.len() {
        let selected = &models[choice];
        if !ollama::is_vision_model(&selected.name) {
            println!("\n  Warning: {} does not support vision.", selected.name);
            println!("  antflow works best with a vision-capable model like llava or gemma3.");
            if !console::confirm("Use it anyway?", false) {
                anyhow::bail!("No model selected.");
            }
        }
        return Ok(selected.clone());
    }

    // Download a new model.
    let model_name = console::read_line("Model name to download (e.g. llava, gemma3, qwen2-vl): ");
    let spinner = console::start_spinner(&format!("Downloading {}...", model_name));
    ollama::pull_model(base_url, &model_name).await?;
    console::stop_spinner(spinner);

    Ok(ollama::OllamaModel {
        name: model_name,
        size_bytes: 0,
    })
}

// ---------------------------------------------------------------------------
// Cloud provider setup
// ---------------------------------------------------------------------------

async fn setup_cloud_provider(
    id: &str,
    display_name: &str,
    base_url: &str,
    recommended_model: &str,
) -> Result<AiProviderConfig> {
    println!("\nConfiguring {}...", display_name);
    println!("  Endpoint: {}", base_url);

    let api_key = console::read_line("  API key: ");
    let model_input = console::read_line(&format!(
        "  Model name [{}]: ",
        recommended_model
    ));
    let model = if model_input.is_empty() {
        recommended_model.to_string()
    } else {
        model_input
    };

    // Quick API key test.
    let spinner = console::start_spinner("Verifying API key...");
    let cfg = AiProviderConfig {
        provider: id.to_string(),
        base_url: base_url.to_string(),
        api_key: api_key.clone(),
        model: model.clone(),
        vision_capable: true,
    };
    let provider = crate::ai::provider_for_config(&cfg);
    let result = provider.ping().await;
    console::stop_spinner(spinner);

    match result {
        Ok(_) => println!("  API key verified."),
        Err(e) => {
            println!("  Warning: key check failed: {}", e);
            if !console::confirm("  Continue anyway?", false) {
                anyhow::bail!("API key rejected.");
            }
        }
    }

    println!("  (Change this later with: antflow config ai)");

    Ok(cfg)
}

// ---------------------------------------------------------------------------
// Custom provider setup
// ---------------------------------------------------------------------------

async fn setup_custom_provider() -> Result<AiProviderConfig> {
    println!("\nCustom provider setup");
    let base_url = console::read_line("  Base URL (e.g. https://my-server.example/v1): ");
    let api_key = console::read_line("  API key (leave empty if not required): ");
    let model = console::read_line("  Model name: ");
    let vision = console::confirm("  Does this model support vision (image input)?", false);

    let cfg = AiProviderConfig {
        provider: "custom".to_string(),
        base_url,
        api_key,
        model,
        vision_capable: vision,
    };

    let spinner = console::start_spinner("Verifying endpoint...");
    let provider = crate::ai::provider_for_config(&cfg);
    let result = provider.ping().await;
    console::stop_spinner(spinner);

    match result {
        Ok(_) => println!("  Endpoint verified."),
        Err(e) => {
            println!("  Warning: endpoint check failed: {}", e);
            if !console::confirm("  Continue anyway?", false) {
                anyhow::bail!("Endpoint check failed.");
            }
        }
    }

    println!("  (Change later with: antflow config ai)");
    Ok(cfg)
}
