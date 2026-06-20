// ai/localai.rs
// Implements management, execution, and integration of the LocalAI engine.
// Supports binary downloading, port scanning, background execution, and model downloads.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::OnceLock;
use tokio::sync::Mutex;
use reqwest::Client;
use tokio::io::AsyncWriteExt;

// Global active port for LocalAI. Defaults to 8080 but can be dynamically changed.
static ACTIVE_PORT: AtomicU16 = AtomicU16::new(8080);
static SERVER_PROCESS: OnceLock<Mutex<Option<tokio::process::Child>>> = OnceLock::new();

pub fn get_active_port() -> u16 {
    ACTIVE_PORT.load(Ordering::Relaxed)
}

pub fn set_active_port(port: u16) {
    ACTIVE_PORT.store(port, Ordering::Relaxed);
}

pub fn get_localai_base_url() -> String {
    format!("http://127.0.0.1:{}", get_active_port())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAiModel {
    pub name: String,
    pub category: String, // "chat" | "images" | "video" | "speaking" | "listening" | "music"
    pub size_gb: f32,
    pub vision: bool,
    pub installed: bool,
    pub download_url: String,
    pub filename: String,
    pub description: String,
}

// Predefined catalog of supported LocalAI models across the 6 categories.
pub fn get_catalog() -> Vec<LocalAiModel> {
    vec![
        LocalAiModel {
            name: "LocalAI-functioncall-llama3.2-3b-v0.5".to_string(),
            category: "chat".to_string(),
            size_gb: 2.0,
            vision: false,
            installed: false,
            download_url: "https://huggingface.co/mradermacher/LocalAI-functioncall-llama3.2-3b-v0.5-GGUF/resolve/main/LocalAI-functioncall-llama3.2-3b-v0.5.Q4_K_M.gguf".to_string(),
            filename: "LocalAI-functioncall-llama3.2-3b-v0.5.Q4_K_M.gguf".to_string(),
            description: "High performance local chat model with native function calling.".to_string(),
        },
        LocalAiModel {
            name: "stablediffusion-ggml".to_string(),
            category: "images".to_string(),
            size_gb: 1.6,
            vision: false,
            installed: false,
            download_url: "https://huggingface.co/gpustack/stable-diffusion-v1-5-GGUF/resolve/main/stable-diffusion-v1-5-Q4_0.gguf".to_string(),
            filename: "stable-diffusion-v1-5-Q4_0.gguf".to_string(),
            description: "Stable Diffusion v1.5 quantized GGUF image generator.".to_string(),
        },
        LocalAiModel {
            name: "animatediff".to_string(),
            category: "video".to_string(),
            size_gb: 1.8,
            vision: false,
            installed: false,
            download_url: "https://huggingface.co/guoyww/animatediff/resolve/main/mm_sd_v15_v2.ckpt".to_string(),
            filename: "mm_sd_v15_v2.ckpt".to_string(),
            description: "AnimateDiff local video generation model.".to_string(),
        },
        LocalAiModel {
            name: "kokoro".to_string(),
            category: "speaking".to_string(),
            size_gb: 0.35,
            vision: false,
            installed: false,
            download_url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model.onnx".to_string(),
            filename: "kokoro-82M.onnx".to_string(),
            description: "State-of-the-art TTS model for natural voice outputs.".to_string(),
        },
        LocalAiModel {
            name: "faster-whisper".to_string(),
            category: "listening".to_string(),
            size_gb: 0.15,
            vision: false,
            installed: false,
            download_url: "https://huggingface.co/systran/faster-whisper-tiny.en/resolve/main/model.bin".to_string(),
            filename: "whisper-tiny.en-q5.bin".to_string(),
            description: "Fast Whisper Speech to Text model for precise listening.".to_string(),
        },
        LocalAiModel {
            name: "musicgen".to_string(),
            category: "music".to_string(),
            size_gb: 2.2,
            vision: false,
            installed: false,
            download_url: "https://huggingface.co/facebook/musicgen-small/resolve/main/pytorch_model.bin".to_string(),
            filename: "musicgen-small.bin".to_string(),
            description: "MusicGen sound and music synthesis model.".to_string(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Server management
// ---------------------------------------------------------------------------

// Checks whether LocalAI is running and reachable on the active port.
pub async fn is_running() -> bool {
    let client = match Client::builder()
        .connect_timeout(std::time::Duration::from_millis(1000))
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format!("{}/v1/models", get_localai_base_url());
    client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

pub fn find_best_wsl_distro() -> Option<String> {
    let mut cmd = std::process::Command::new("wsl");
    cmd.args(&["-l", "-q"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = cmd.output();
    if let Ok(out) = output {
        let stdout = if out.stdout.len() >= 2 && out.stdout[0] == 0xFF && out.stdout[1] == 0xFE {
            let u16_data: Vec<u16> = out.stdout[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            String::from_utf16_lossy(&u16_data)
        } else {
            let raw_str = String::from_utf8_lossy(&out.stdout).to_string();
            raw_str.chars().filter(|&c| c != '\0' && c != '\r').collect()
        };

        for line in stdout.lines() {
            let name = line.trim();
            if !name.is_empty() && !name.contains("docker-desktop") && !name.contains("docker-desktop-data") {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn to_wsl_path(path: &Path) -> String {
    let path_str = path.to_string_lossy().to_string();
    let mut cmd = std::process::Command::new("wsl");
    
    let distro = find_best_wsl_distro();
    if let Some(ref d) = distro {
        cmd.args(&["-d", d, "wslpath", "-u", &path_str]);
    } else {
        cmd.args(&["wslpath", "-u", &path_str]);
    }
    
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output();
    if let Ok(out) = output {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }

    let path_forward = path_str.replace('\\', "/");
    if path_forward.len() >= 2 && path_forward.as_bytes()[1] == b':' {
        let drive = (path_forward.as_bytes()[0] as char).to_ascii_lowercase();
        format!("/mnt/{}{}", drive, &path_forward[2..])
    } else {
        path_forward
    }
}

pub fn is_docker_running() -> bool {
    let mut cmd = std::process::Command::new("docker");
    cmd.arg("ps");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

pub fn is_docker_pull_active() -> bool {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(&[
            "-Command",
            "Get-Process -Name docker -ErrorAction SilentlyContinue | Where-Object { $_.CommandLine -like '*localai/localai:v2.26.0*' } | Select-Object -ExpandProperty Id"
        ]);
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
        if let Ok(out) = cmd.output() {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            !stdout.is_empty()
        } else {
            false
        }
    }
    #[cfg(not(windows))]
    {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(&[
            "-c",
            "ps aux | grep -v grep | grep 'docker' | grep 'localai/localai:v2.26.0'"
        ]);
        if let Ok(out) = cmd.output() {
            !out.stdout.is_empty()
        } else {
            false
        }
    }
}

// Checks if the LocalAI binary is installed in the storage directory or globally on PATH.
pub async fn is_installed(storage_dir: &Path) -> bool {
    let check_file = |p: &Path| -> bool {
        if let Ok(meta) = p.metadata() {
            meta.is_file() && meta.len() > 0
        } else {
            false
        }
    };

    let has_native = if cfg!(target_os = "windows") {
        let native_binaries = vec!["local-ai.exe", "local-ai"];
        let mut found = false;
        for bin in &native_binaries {
            if check_file(&storage_dir.join(bin)) {
                found = true;
                break;
            }
        }
        if !found {
            if let Ok(exe_p) = std::env::current_exe() {
                if let Some(parent) = exe_p.parent() {
                    for bin in &native_binaries {
                        if check_file(&parent.join(bin)) {
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
        if !found {
            if let Some(paths) = std::env::var_os("PATH") {
                for path in std::env::split_paths(&paths) {
                    for bin in &native_binaries {
                        if check_file(&path.join(bin)) {
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
        found
    } else {
        false
    };

    if has_native {
        return true;
    }

    let has_linux_bin = if cfg!(target_os = "windows") {
        let mut found = check_file(&storage_dir.join("local-ai-Linux-x86_64"));
        if !found {
            if let Ok(exe_p) = std::env::current_exe() {
                if let Some(parent) = exe_p.parent() {
                    if check_file(&parent.join("local-ai-Linux-x86_64")) {
                        found = true;
                    }
                }
            }
        }
        if !found {
            if let Some(paths) = std::env::var_os("PATH") {
                for path in std::env::split_paths(&paths) {
                    if check_file(&path.join("local-ai-Linux-x86_64")) {
                        found = true;
                        break;
                    }
                }
            }
        }
        found
    } else {
        let mut found = false;
        let binaries = vec!["local-ai"];
        for bin in &binaries {
            if check_file(&storage_dir.join(bin)) {
                found = true;
                break;
            }
        }
        if !found {
            if let Ok(exe_p) = std::env::current_exe() {
                if let Some(parent) = exe_p.parent() {
                    for bin in &binaries {
                        if check_file(&parent.join(bin)) {
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
        if !found {
            if let Some(paths) = std::env::var_os("PATH") {
                for path in std::env::split_paths(&paths) {
                    for bin in &binaries {
                        if check_file(&path.join(bin)) {
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
        found
    };

    if has_linux_bin {
        #[cfg(windows)]
        {
            if find_best_wsl_distro().is_none() {
                return false;
            }
        }
        return true;
    }

    false
}

// Scans for a free TCP port starting at 8080.
pub fn find_free_port() -> u16 {
    let mut port = 8080;
    while port < 9000 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
        port += 1;
    }
    8080 // fallback
}

// Starts the LocalAI background process if it is not already running.
pub async fn start_server(storage_dir: &Path) -> Result<()> {
    if is_running().await {
        return Ok(());
    }

    // Check if the process is already running in memory
    {
        let mutex = SERVER_PROCESS.get_or_init(|| Mutex::new(None));
        let mut guard = mutex.lock().await;
        if let Some(ref mut child) = *guard {
            if let Ok(None) = child.try_wait() {
                return Ok(());
            }
        }
    }

    let free_port = find_free_port();
    set_active_port(free_port);

    let models_dir = storage_dir.join("localai_models");
    let _ = std::fs::create_dir_all(&models_dir);

    // Setup logging
    let log_path = storage_dir.join("localai.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path);

    let (stdout_cfg, stderr_cfg) = match log_file {
        Ok(f) => {
            if let Ok(f_err) = f.try_clone() {
                (std::process::Stdio::from(f), std::process::Stdio::from(f_err))
            } else {
                (std::process::Stdio::from(f), std::process::Stdio::null())
            }
        }
        Err(_) => (std::process::Stdio::null(), std::process::Stdio::null()),
    };

    // 2. WSL/Native startup logic (bypassing Docker)
    let check_file = |p: &Path| -> bool {
        if let Ok(meta) = p.metadata() {
            meta.is_file() && meta.len() > 0
        } else {
            false
        }
    };

    // Determine if we should run Linux binary via WSL2 or native binary
    let mut use_wsl = false;
    let mut wsl_bin_path = PathBuf::new();

    if cfg!(target_os = "windows") {
        if check_file(&storage_dir.join("local-ai-Linux-x86_64")) {
            use_wsl = true;
            wsl_bin_path = storage_dir.join("local-ai-Linux-x86_64");
        } else if let Ok(exe_p) = std::env::current_exe() {
            if let Some(parent) = exe_p.parent() {
                if check_file(&parent.join("local-ai-Linux-x86_64")) {
                    use_wsl = true;
                    wsl_bin_path = parent.join("local-ai-Linux-x86_64");
                }
            }
        }
    }

    let mut cmd = if use_wsl {
        let wsl_distro = find_best_wsl_distro();
        if wsl_distro.is_none() {
            let _ = std::fs::write(
                &log_path,
                "Error: No standard Linux distribution (like Ubuntu) found in WSL2.\n\
                 The active distribution 'docker-desktop' is a minimal Alpine VM managed by Docker Desktop and cannot run standard Linux binaries.\n\n\
                 To fix this:\n\
                 1. Open PowerShell or Command Prompt.\n\
                 2. Run: wsl --install -d Ubuntu\n\
                 3. Once installed, open 'Ubuntu' from the Start Menu to complete the initial setup (setting username/password).\n\
                 4. Restart Antflow and try again.\n"
            );
            anyhow::bail!("No standard Linux distribution found in WSL2. Check localai.log for setup instructions.");
        }
        let wsl_distro_name = wsl_distro.unwrap();
        let wsl_bin_str = to_wsl_path(&wsl_bin_path);
        let wsl_models_str = to_wsl_path(&models_dir);

        // Copy and chmod binary in WSL synchronously
        let mut cp_cmd = tokio::process::Command::new("wsl");
        cp_cmd.args(&[
            "-d",
            &wsl_distro_name,
            "sh",
            "-c",
            &format!("cp '{}' /tmp/local-ai && chmod +x /tmp/local-ai", wsl_bin_str)
        ]);
        #[cfg(windows)]
        {
            cp_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let _ = cp_cmd.status().await;

        let mut c = tokio::process::Command::new("wsl");
        c.args(&[
            "-d",
            &wsl_distro_name,
            "sh",
            "-c",
            &format!(
                "ADDRESS=127.0.0.1:{} PORT={} MODELS_PATH='{}' exec /tmp/local-ai",
                free_port,
                free_port,
                wsl_models_str
            )
        ]);
        c
    } else {
        // Fallback/native execution path
        let cmd_name = if cfg!(target_os = "windows") { "local-ai.exe" } else { "local-ai" };
        let bin_path = if check_file(&storage_dir.join(cmd_name)) {
            storage_dir.join(cmd_name)
        } else if let Ok(exe_p) = std::env::current_exe() {
            let parent = exe_p.parent().unwrap_or(Path::new("."));
            if check_file(&parent.join(cmd_name)) {
                parent.join(cmd_name)
            } else {
                PathBuf::from(cmd_name)
            }
        } else {
            PathBuf::from(cmd_name)
        };

        let mut c = tokio::process::Command::new(bin_path);
        c.env("ADDRESS", format!("127.0.0.1:{}", free_port));
        c.env("PORT", free_port.to_string());
        if let Some(s) = models_dir.to_str() {
            c.env("MODELS_PATH", s);
        }
        c
    };

    cmd.stdin(std::process::Stdio::null())
        .stdout(stdout_cfg)
        .stderr(stderr_cfg);

    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let child = cmd.spawn().context("Failed to spawn LocalAI process")?;

    let mutex = SERVER_PROCESS.get_or_init(|| Mutex::new(None));
    let mut guard = mutex.lock().await;
    *guard = Some(child);

    // Wait up to 3 seconds for it to start
    for _ in 0..15 {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if is_running().await {
            break;
        }
    }

    Ok(())
}

// Stops the running LocalAI background process.
pub async fn stop_server() {
    let mutex = SERVER_PROCESS.get_or_init(|| Mutex::new(None));
    let mut guard = mutex.lock().await;
    if let Some(mut child) = guard.take() {
        let _ = child.kill().await;
    }

    if is_docker_running() {
        let mut stop_cmd = std::process::Command::new("docker");
        stop_cmd.args(&["stop", "antflow-localai"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            stop_cmd.creation_flags(0x08000000);
        }
        let _ = stop_cmd.status();
    }
}

// ---------------------------------------------------------------------------
// Binary downloading
// ---------------------------------------------------------------------------

// Returns the LocalAI binary installer/executable download URL for the current platform.
pub fn get_installer_url() -> &'static str {
    if cfg!(target_os = "windows") {
        "https://github.com/mudler/LocalAI/releases/download/v2.26.0/local-ai-Linux-x86_64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "https://github.com/mudler/LocalAI/releases/download/v2.26.0/local-ai-Darwin-arm64"
        } else {
            "https://github.com/mudler/LocalAI/releases/download/v2.26.0/local-ai-Darwin-x86_64"
        }
    } else {
        if cfg!(target_arch = "aarch64") {
            "https://github.com/mudler/LocalAI/releases/download/v2.26.0/local-ai-Linux-arm64"
        } else {
            "https://github.com/mudler/LocalAI/releases/download/v2.26.0/local-ai-Linux-x86_64"
        }
    }
}

// Checks if a LocalAI model files are present in the models directory.
pub fn is_model_installed(storage_dir: &Path, model_name: &str) -> bool {
    let models_dir = storage_dir.join("localai_models");
    let config_path = models_dir.join(format!("{}.yaml", model_name));
    
    // Check if the YAML configuration file exists
    config_path.exists()
}

// Writes the YAML configuration file for a given model.
pub fn write_model_yaml(storage_dir: &Path, model_name: &str, category: &str, filename: &str) -> Result<()> {
    let models_dir = storage_dir.join("localai_models");
    let _ = std::fs::create_dir_all(&models_dir);
    let config_path = models_dir.join(format!("{}.yaml", model_name));
    
    let backend = match category {
        "chat" => "llama-cpp",
        "images" => "stablediffusion",
        "video" => "animatediff",
        "speaking" => "kokoro",
        "listening" => "faster-whisper",
        "music" => "musicgen",
        _ => "llama-cpp",
    };

    let yaml_content = format!(
        "name: {}\nbackend: {}\nparameters:\n  model: {}\n",
        model_name, backend, filename
    );

    std::fs::write(&config_path, yaml_content)
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

    Ok(())
}

pub async fn download_model_file<F>(
    storage_dir: &Path,
    model_name: &str,
    download_url: &str,
    filename: &str,
    mut rx: Option<tokio::sync::oneshot::Receiver<()>>,
    mut progress_callback: F,
) -> Result<()>
where
    F: FnMut(u32, &str, u64, u64) + Send + 'static,
{
    let models_dir = storage_dir.join("localai_models");
    std::fs::create_dir_all(&models_dir)?;
    let dest_path = models_dir.join(filename);

    let client = reqwest::Client::new();
    let mut downloaded_bytes = 0;

    let mut file = if dest_path.exists() {
        let meta = std::fs::metadata(&dest_path)?;
        downloaded_bytes = meta.len();
        tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(&dest_path)
            .await?
    } else {
        tokio::fs::File::create(&dest_path).await?
    };

    let mut req = client.get(download_url);
    if downloaded_bytes > 0 {
        req = req.header("Range", format!("bytes={}-", downloaded_bytes));
    }

    let mut response = req.send().await?;
    let status = response.status();

    let mut total_size = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        let content_range = response.headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|val| val.to_str().ok());

        let total = if let Some(range_str) = content_range {
            if let Some(slash_idx) = range_str.rfind('/') {
                range_str[slash_idx + 1..].parse::<u64>().ok()
            } else {
                None
            }
        } else {
            None
        };

        total.unwrap_or_else(|| {
            let chunk_len = response.content_length().unwrap_or(0);
            downloaded_bytes + chunk_len
        })
    } else {
        if downloaded_bytes > 0 {
            downloaded_bytes = 0;
            file = tokio::fs::File::create(&dest_path).await?;
        }
        response.content_length().unwrap_or(0)
    };

    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        progress_callback(100, "Already installed", downloaded_bytes, downloaded_bytes);
        return Ok(());
    }

    if !status.is_success() {
        anyhow::bail!("Download server returned HTTP status {}", status);
    }

    progress_callback(
        if total_size > 0 { ((downloaded_bytes as f64 / total_size as f64) * 100.0) as u32 } else { 0 },
        "Downloading...",
        downloaded_bytes,
        total_size
    );

    loop {
        if let Some(ref mut cancel_rx) = rx {
            if let Ok(_) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) = cancel_rx.try_recv() {
                anyhow::bail!("Download cancelled");
            }
        }

        tokio::select! {
            cancel_msg = async {
                if let Some(ref mut cancel_rx) = rx {
                    cancel_rx.await
                } else {
                    std::future::pending().await
                }
            } => {
                anyhow::bail!("Download cancelled");
            }
            chunk_res = response.chunk() => {
                match chunk_res {
                    Ok(Some(chunk)) => {
                        file.write_all(&chunk).await?;
                        downloaded_bytes += chunk.len() as u64;
                        let percent = if total_size > 0 {
                            ((downloaded_bytes as f64 / total_size as f64) * 100.0) as u32
                        } else {
                            0
                        };
                        progress_callback(percent, "Downloading...", downloaded_bytes, total_size);
                    }
                    Ok(None) => break,
                    Err(e) => anyhow::bail!("Connection error: {}", e),
                }
            }
        }
    }

    file.flush().await?;

    let catalog = get_catalog();
    if let Some(model_info) = catalog.iter().find(|m| m.name == model_name) {
        write_model_yaml(storage_dir, model_name, &model_info.category, &model_info.filename)?;
    }

    progress_callback(100, "Success", downloaded_bytes, total_size);
    Ok(())
}

pub async fn download_localai_binary<F>(
    storage_dir: &Path,
    mut rx: Option<tokio::sync::oneshot::Receiver<()>>,
    mut progress_callback: F,
) -> Result<()>
where
    F: FnMut(u32, &str, u64, u64) + Send + 'static,
{
    let url = get_installer_url();
    let cmd_name = if cfg!(target_os = "windows") { "local-ai-Linux-x86_64" } else { "local-ai" };
    let dest_path = storage_dir.join(cmd_name);

    let result = async {
        let client = reqwest::Client::new();
        let mut downloaded_bytes = 0;

        let mut file = if dest_path.exists() {
            let meta = std::fs::metadata(&dest_path)?;
            downloaded_bytes = meta.len();
            tokio::fs::OpenOptions::new()
                .write(true)
                .append(true)
                .open(&dest_path)
                .await?
        } else {
            tokio::fs::File::create(&dest_path).await?
        };

        let mut req = client.get(url);
        if downloaded_bytes > 0 {
            req = req.header("Range", format!("bytes={}-", downloaded_bytes));
        }

        let mut response = req.send().await?;
        let status = response.status();

        let mut total_size = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            let content_range = response.headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|val| val.to_str().ok());

            let total = if let Some(range_str) = content_range {
                if let Some(slash_idx) = range_str.rfind('/') {
                    range_str[slash_idx + 1..].parse::<u64>().ok()
                } else {
                    None
                }
            } else {
                None
            };

            total.unwrap_or_else(|| {
                let chunk_len = response.content_length().unwrap_or(0);
                downloaded_bytes + chunk_len
            })
        } else {
            if downloaded_bytes > 0 {
                downloaded_bytes = 0;
                file = tokio::fs::File::create(&dest_path).await?;
            }
            response.content_length().unwrap_or(0)
        };

        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            progress_callback(100, "Already installed", downloaded_bytes, downloaded_bytes);
            return Ok(());
        }

        if !status.is_success() {
            anyhow::bail!("Download server returned HTTP status {}", status);
        }

        progress_callback(
            if total_size > 0 { ((downloaded_bytes as f64 / total_size as f64) * 100.0) as u32 } else { 0 },
            "Downloading LocalAI binary...",
            downloaded_bytes,
            total_size
        );

        loop {
            if let Some(ref mut cancel_rx) = rx {
                if let Ok(_) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) = cancel_rx.try_recv() {
                    anyhow::bail!("Download cancelled");
                }
            }

            tokio::select! {
                cancel_msg = async {
                    if let Some(ref mut cancel_rx) = rx {
                        cancel_rx.await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    anyhow::bail!("Download cancelled");
                }
                chunk_res = response.chunk() => {
                    match chunk_res {
                        Ok(Some(chunk)) => {
                            file.write_all(&chunk).await?;
                            downloaded_bytes += chunk.len() as u64;
                            let percent = if total_size > 0 {
                                ((downloaded_bytes as f64 / total_size as f64) * 100.0) as u32
                            } else {
                                0
                            };
                            progress_callback(percent, "Downloading LocalAI binary...", downloaded_bytes, total_size);
                        }
                        Ok(None) => break,
                        Err(e) => anyhow::bail!("Connection error: {}", e),
                    }
                }
            }
        }

        file.flush().await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest_path, perms)?;
        }

        progress_callback(100, "Success", downloaded_bytes, total_size);
        Ok(())
    }.await;

    if result.is_err() {
        if dest_path.exists() {
            let _ = std::fs::remove_file(&dest_path);
        }
    }

    result
}

/// Deletes a LocalAI model's configuration and binary file.
pub async fn delete_model(storage_dir: &Path, model_name: &str) -> Result<()> {
    let models_dir = storage_dir.join("localai_models");
    let yaml_path = models_dir.join(format!("{}.yaml", model_name));
    
    // Find the model file path. First, check our catalog.
    let catalog = get_catalog();
    let filename = if let Some(m) = catalog.iter().find(|m| m.name == model_name) {
        Some(m.filename.clone())
    } else {
        // If not in catalog, read the YAML config file to extract the filename
        if yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                // Look for "model: <filename>" in yaml
                content.lines()
                    .find(|line| line.trim().starts_with("model:"))
                    .and_then(|line| {
                        let parts: Vec<&str> = line.splitn(2, ':').collect();
                        parts.get(1).map(|v| v.trim().to_string())
                    })
            } else {
                None
            }
        } else {
            None
        }
    };

    // Delete the YAML file
    if yaml_path.exists() {
        std::fs::remove_file(&yaml_path)
            .with_context(|| format!("Failed to delete YAML config: {}", yaml_path.display()))?;
    }

    // Delete the model binary file if we resolved the filename
    if let Some(fname) = filename {
        let model_file_path = models_dir.join(fname);
        if model_file_path.exists() {
            std::fs::remove_file(&model_file_path)
                .with_context(|| format!("Failed to delete model file: {}", model_file_path.display()))?;
        }
    }

    Ok(())
}

