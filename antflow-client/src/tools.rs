// tools.rs
// Implements every automation action the AI can request.

use anyhow::{Context, Result};
use serde_json::Value;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

macro_rules! dbg_log {
    ($($arg:tt)*) => {
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

macro_rules! trace_log {
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

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn json_int(v: &Value, key: &str) -> Result<i32> {
    v.get(key)
        .and_then(|x| x.as_i64())
        .map(|x| x as i32)
        .with_context(|| format!("Missing or invalid integer param: '{}'", key))
}

fn json_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

// ===========================================================================
// DESKTOP PLATFORMS IMPLEMENTATION
// ===========================================================================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use arboard::Clipboard;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use enigo::{
    Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings,
};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn mouse_move(params: &Value) -> Result<Option<String>> {
    let x = json_int(params, "x")?;
    let y = json_int(params, "y")?;
    trace_log!("[mouse_move] Coordinates: x={}, y={}", x, y);
    let mut e = Enigo::new(&Settings::default()).context("Failed to initialize input driver")?;
    e.move_mouse(x, y, Coordinate::Abs).context("Mouse move failed")?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn mouse_click(params: &Value) -> Result<Option<String>> {
    let button_str = json_str(params, "button").unwrap_or("left");
    let button = parse_button(button_str)?;
    let x = params.get("x").and_then(|v| v.as_i64());
    let y = params.get("y").and_then(|v| v.as_i64());
    trace_log!("[mouse_click] Button: {}, Coordinates: {:?}", button_str, x.zip(y));
    let mut e = Enigo::new(&Settings::default()).context("Failed to initialize input driver")?;
    if let (Some(x), Some(y)) = (params.get("x").and_then(|v| v.as_i64()), params.get("y").and_then(|v| v.as_i64())) {
        e.move_mouse(x as i32, y as i32, Coordinate::Abs).context("Mouse move failed")?;
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    e.button(button, Direction::Click).context("Mouse click failed")?;
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn mouse_double_click(params: &Value) -> Result<Option<String>> {
    let x = params.get("x").and_then(|v| v.as_i64());
    let y = params.get("y").and_then(|v| v.as_i64());
    trace_log!("[mouse_double_click] Coordinates: {:?}", x.zip(y));
    let mut e = Enigo::new(&Settings::default()).context("Failed to initialize input driver")?;
    if let (Some(x), Some(y)) = (params.get("x").and_then(|v| v.as_i64()), params.get("y").and_then(|v| v.as_i64())) {
        e.move_mouse(x as i32, y as i32, Coordinate::Abs).context("Mouse move failed")?;
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    e.button(Button::Left, Direction::Click)?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    e.button(Button::Left, Direction::Click)?;
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn mouse_drag(params: &Value) -> Result<Option<String>> {
    let from_x = json_int(params, "from_x")?;
    let from_y = json_int(params, "from_y")?;
    let to_x = json_int(params, "to_x")?;
    let to_y = json_int(params, "to_y")?;
    let button_str = json_str(params, "button").unwrap_or("left");
    let button = parse_button(button_str)?;
    trace_log!("[mouse_drag] Button: {}, From: ({},{}), To: ({},{})", button_str, from_x, from_y, to_x, to_y);

    let mut e = Enigo::new(&Settings::default()).context("Failed to initialize input driver")?;
    e.move_mouse(from_x, from_y, Coordinate::Abs)?;
    e.button(button, Direction::Press)?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    e.move_mouse(to_x, to_y, Coordinate::Abs)?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    e.button(button, Direction::Release)?;
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn mouse_scroll(params: &Value) -> Result<Option<String>> {
    let amount = json_int(params, "amount")?;
    trace_log!("[mouse_scroll] Amount: {}", amount);
    let mut e = Enigo::new(&Settings::default()).context("Failed to initialize input driver")?;
    e.scroll(amount, enigo::Axis::Vertical).context("Scroll failed")?;
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn key_press(params: &Value) -> Result<Option<String>> {
    let key_name = json_str(params, "key").context("key_press requires a 'key' param")?;
    let modifiers = params.get("modifiers").and_then(|m| m.as_array()).cloned().unwrap_or_default();
    trace_log!("[key_press] Key: {}, Modifiers: {:?}", key_name, modifiers);

    let mut e = Enigo::new(&Settings::default()).context("Failed to initialize input driver")?;

    for modifier in &modifiers {
        if let Some(m) = modifier.as_str() {
            let k = parse_modifier(m)?;
            e.key(k, Direction::Press)?;
        }
    }

    let main_key = parse_key(key_name)?;
    e.key(main_key, Direction::Click)?;

    for modifier in modifiers.iter().rev() {
        if let Some(m) = modifier.as_str() {
            let k = parse_modifier(m)?;
            e.key(k, Direction::Release)?;
        }
    }
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn type_text(params: &Value) -> Result<Option<String>> {
    let text = json_str(params, "text").context("type_text requires a 'text' param")?;
    trace_log!("[type_text] Typing text: '{}'", text);
    let mut e = Enigo::new(&Settings::default()).context("Failed to initialize input driver")?;
    e.text(text).context("Type text failed")?;
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn clipboard_set(params: &Value) -> Result<Option<String>> {
    let text = json_str(params, "text").context("clipboard_set requires a 'text' param")?.to_string();
    trace_log!("[clipboard_set] Writing text to clipboard: '{}'", text);
    let mut cb = Clipboard::new().context("Failed to open clipboard")?;
    cb.set_text(text).context("Failed to write to clipboard")?;
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn clipboard_get(_params: &Value) -> Result<Option<String>> {
    let mut cb = Clipboard::new().context("Failed to open clipboard")?;
    let text = cb.get_text().context("Failed to read clipboard")?;
    trace_log!("[clipboard_get] Read text from clipboard: '{}'", text);
    Ok(Some(text))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn open_url(params: &Value) -> Result<Option<String>> {
    let url = json_str(params, "url").context("open_url requires a 'url' param")?;
    dbg_log!("[open_url] Launching URL: '{}'", url);
    open::that(url).context("Failed to open URL")?;
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn send_keys_to_app(params: &Value) -> Result<Option<String>> {
    let keys = json_str(params, "keys").context("send_keys_to_app requires a 'keys' param")?;
    let app_name = params.get("app_name").and_then(|v| v.as_str()).unwrap_or("");

    if cfg!(target_os = "windows") {
        let script = if app_name.is_empty() {
            format!(
                r#"Add-Type -AssemblyName Microsoft.VisualBasic
Add-Type -AssemblyName System.Windows.Forms
$hwnd = [Microsoft.VisualBasic.Interaction]::AppActivate((Get-Process | Where-Object {{$_.MainWindowTitle -ne ''}} | Select-Object -First 1).Id)
[System.Windows.Forms.SendKeys]::SendWait('{}')"#,
                keys.replace("'", "''")
            )
        } else {
            format!(
                r#"Add-Type -AssemblyName Microsoft.VisualBasic
$proc = Get-Process | Where-Object {{$_.MainWindowTitle -like '*{}*'}} | Select-Object -First 1
if ($proc) {{ [Microsoft.VisualBasic.Interaction]::AppActivate($proc.Id) }}
Start-Sleep -Milliseconds 200
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait('{}')"#,
                app_name,
                keys.replace("'", "''")
            )
        };
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        cmd.output()
            .context("SendKeys failed")?;
    } else if cfg!(target_os = "macos") {
        let script = format!(r#"tell application "System Events" to keystroke "{}""#, keys);
        std::process::Command::new("osascript").arg("-e").arg(&script).output().context("AppleScript keystroke failed")?;
    }
    Ok(None)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn voice_input(params: &Value) -> Result<Option<String>> {
    let duration = params.get("duration_secs").and_then(|v| v.as_u64()).unwrap_or(5);
    let api_key = params.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
    let tmp = std::env::temp_dir().join("antflow_voice.wav");
    let tmp_str = tmp.to_string_lossy().to_string();

    let recorded = if cfg!(target_os = "windows") {
        let script = format!("ffmpeg -f dshow -i audio='Microphone' -t {} -ar 16000 -ac 1 '{}' -y 2>$null", duration, tmp_str);
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", &script]);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        cmd.output().is_ok()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("ffmpeg").args(["-f", "avfoundation", "-i", ":0", "-t", &duration.to_string(), "-ar", "16000", "-ac", "1", &tmp_str, "-y"]).output().is_ok()
    } else {
        std::process::Command::new("ffmpeg").args(["-f", "alsa", "-i", "default", "-t", &duration.to_string(), "-ar", "16000", "-ac", "1", &tmp_str, "-y"]).output().is_ok()
    };

    if !recorded || !tmp.exists() {
        anyhow::bail!("Audio recording failed");
    }

    let transcript = transcribe_audio(&tmp_str, api_key)?;
    Ok(Some(transcript))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn transcribe_audio(wav_path: &str, api_key: &str) -> Result<String> {
    let mut cmd = std::process::Command::new("whisper");
    cmd.args([wav_path, "--model", "base", "--output-format", "txt", "--output-dir", &std::env::temp_dir().to_string_lossy()]);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let local = cmd.output();

    if let Ok(out) = local {
        if out.status.success() {
            let txt_path = std::path::Path::new(wav_path).with_extension("txt");
            if let Ok(text) = std::fs::read_to_string(&txt_path) {
                return Ok(text.trim().to_string());
            }
        }
    }

    if !api_key.is_empty() {
        let client = reqwest::blocking::Client::new();
        let file_bytes = std::fs::read(wav_path)?;
        let part = reqwest::blocking::multipart::Part::bytes(file_bytes).file_name("audio.wav").mime_str("audio/wav")?;
        let form = reqwest::blocking::multipart::Form::new().part("file", part).text("model", "whisper-1");
        let resp = client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()?;
        let json: serde_json::Value = resp.json()?;
        if let Some(text) = json["text"].as_str() {
            return Ok(text.to_string());
        }
    }
    anyhow::bail!("Transcription failed")
}


// ===========================================================================
// ANDROID PLATFORM IMPLEMENTATION
// ===========================================================================

#[cfg(target_os = "android")]
pub fn mouse_move(_params: &Value) -> Result<Option<String>> {
    // Mouse hover coordinates aren't natively meaningful on standard touchscreens.
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn mouse_click(params: &Value) -> Result<Option<String>> {
    let x = json_int(params, "x")?;
    let y = json_int(params, "y")?;

    let status = std::process::Command::new("input")
        .args(["tap", &x.to_string(), &y.to_string()])
        .status()
        .context("Failed to run Android system input utility")?;

    if !status.success() {
        anyhow::bail!("Android input tap command failed with exit status {:?}", status.code());
    }
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn mouse_double_click(params: &Value) -> Result<Option<String>> {
    let x = json_int(params, "x")?;
    let y = json_int(params, "y")?;

    std::process::Command::new("input").args(["tap", &x.to_string(), &y.to_string()]).status().ok();
    std::thread::sleep(std::time::Duration::from_millis(120));
    let status = std::process::Command::new("input")
        .args(["tap", &x.to_string(), &y.to_string()])
        .status()
        .context("Failed to run Android system input utility")?;

    if !status.success() {
        anyhow::bail!("Android double tap failed");
    }
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn mouse_drag(params: &Value) -> Result<Option<String>> {
    let from_x = json_int(params, "from_x")?;
    let from_y = json_int(params, "from_y")?;
    let to_x = json_int(params, "to_x")?;
    let to_y = json_int(params, "to_y")?;

    // Swipe performs a drag action between coordinates. Standard duration is 300ms.
    let status = std::process::Command::new("input")
        .args(["swipe", &from_x.to_string(), &from_y.to_string(), &to_x.to_string(), &to_y.to_string(), "300"])
        .status()
        .context("Failed to run Android system input swipe utility")?;

    if !status.success() {
        anyhow::bail!("Android drag/swipe failed");
    }
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn mouse_scroll(params: &Value) -> Result<Option<String>> {
    let amount = json_int(params, "amount")?;
    
    // Simulate scroll using a drag/swipe down (scroll up) or up (scroll down) at screen center.
    // Standard screen middle estimate: (540, 1100).
    let from_y = 1100;
    let to_y = 1100 - (amount * 8); // Scaled scroll drag distance
    
    let status = std::process::Command::new("input")
        .args(["swipe", "540", &from_y.to_string(), "540", &to_y.to_string(), "200"])
        .status()
        .context("Failed to run Android system input swipe scroll utility")?;

    if !status.success() {
        anyhow::bail!("Android scroll swipe failed");
    }
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn key_press(params: &Value) -> Result<Option<String>> {
    let key = json_str(params, "key").context("key_press requires a 'key' param")?;
    
    let key_lower = key.to_lowercase();
    let keycode = match key_lower.as_str() {
        "enter" | "return" => "KEYCODE_ENTER",
        "backspace" => "KEYCODE_BACKSPACE",
        "home" => "KEYCODE_HOME",
        "back" => "KEYCODE_BACK",
        "space" => "KEYCODE_SPACE",
        "tab" => "KEYCODE_TAB",
        "escape" | "esc" => "KEYCODE_ESCAPE",
        _ => key,
    };

    let status = std::process::Command::new("input")
        .args(["keyevent", keycode])
        .status()
        .context("Failed to run Android system input keyevent utility")?;

    if !status.success() {
        anyhow::bail!("Android keyevent failed for key '{}'", key);
    }
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn type_text(params: &Value) -> Result<Option<String>> {
    let text = json_str(params, "text").context("type_text requires a 'text' param")?;

    let status = std::process::Command::new("input")
        .args(["text", text])
        .status()
        .context("Failed to run Android system input text utility")?;

    if !status.success() {
        anyhow::bail!("Android text typing failed");
    }
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn clipboard_set(_params: &Value) -> Result<Option<String>> {
    anyhow::bail!("Direct clipboard write is not supported natively via Android terminal.")
}

#[cfg(target_os = "android")]
pub fn clipboard_get(_params: &Value) -> Result<Option<String>> {
    anyhow::bail!("Direct clipboard read is not supported natively via Android terminal.")
}

#[cfg(target_os = "android")]
pub fn open_url(params: &Value) -> Result<Option<String>> {
    let url = json_str(params, "url").context("open_url requires a 'url' param")?;

    // Android Activity Manager Intent View launch
    let status = std::process::Command::new("am")
        .args(["start", "-a", "android.intent.action.VIEW", "-d", url])
        .status()
        .context("Failed to launch Android activity manager")?;

    if !status.success() {
        anyhow::bail!("Android am start failed to open URL '{}'", url);
    }
    Ok(None)
}

#[cfg(target_os = "android")]
pub fn send_keys_to_app(_params: &Value) -> Result<Option<String>> {
    anyhow::bail!("send_keys_to_app is not supported on Android. Use standard type_text or key_press instead.")
}

#[cfg(target_os = "android")]
pub fn voice_input(_params: &Value) -> Result<Option<String>> {
    anyhow::bail!("Audio input capture is handled natively via WebView Web Speech integration on mobile devices.")
}


// ===========================================================================
// IOS PLATFORM IMPLEMENTATION
// ===========================================================================

#[cfg(target_os = "ios")]
pub fn mouse_move(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Mouse automation is not supported on iOS due to platform security sandbox restrictions.") }
#[cfg(target_os = "ios")]
pub fn mouse_click(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Mouse automation is not supported on iOS due to platform security sandbox restrictions.") }
#[cfg(target_os = "ios")]
pub fn mouse_double_click(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Mouse automation is not supported on iOS due to platform security sandbox restrictions.") }
#[cfg(target_os = "ios")]
pub fn mouse_drag(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Mouse automation is not supported on iOS due to platform security sandbox restrictions.") }
#[cfg(target_os = "ios")]
pub fn mouse_scroll(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Mouse automation is not supported on iOS due to platform security sandbox restrictions.") }
#[cfg(target_os = "ios")]
pub fn key_press(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Keyboard automation is not supported on iOS due to platform security sandbox restrictions.") }
#[cfg(target_os = "ios")]
pub fn type_text(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Keyboard automation is not supported on iOS due to platform security sandbox restrictions.") }
#[cfg(target_os = "ios")]
pub fn clipboard_set(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Clipboard write is not supported on iOS via terminal.") }
#[cfg(target_os = "ios")]
pub fn clipboard_get(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Clipboard read is not supported on iOS via terminal.") }
#[cfg(target_os = "ios")]
pub fn open_url(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Direct browser launching is not supported on iOS via terminal.") }
#[cfg(target_os = "ios")]
pub fn send_keys_to_app(_params: &Value) -> Result<Option<String>> { anyhow::bail!("send_keys_to_app is not supported on iOS.") }
#[cfg(target_os = "ios")]
pub fn voice_input(_params: &Value) -> Result<Option<String>> { anyhow::bail!("Voice input is handled natively via WebView Web Speech integration on mobile devices.") }


// ===========================================================================
// GLOBAL SHELL UTILITIES
// ===========================================================================

// Runs a shell command and optionally returns its stdout.
pub fn run_command(params: &Value, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<Option<String>> {
    let command = json_str(params, "command").context("run_command requires a 'command' param")?;
    let capture = params.get("capture_output").and_then(|v| v.as_bool()).unwrap_or(false);
    trace_log!("[run_command] executing command='{}' (capture={})", command, capture);

    let (shell, flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let trimmed_lower = command.trim_start().to_lowercase();
    let is_launch_cmd = trimmed_lower.starts_with("start ")          // Windows
        || trimmed_lower.starts_with("open ")                        // macOS/iOS-friendly
        || trimmed_lower.starts_with("xdg-open ")                    // Linux
        || trimmed_lower.ends_with(" &")                             // background
        || {
            let first_token = trimmed_lower.split_whitespace().next().unwrap_or("");
            matches!(first_token,
                "firefox" | "chromium" | "google-chrome" | "gedit" | "kate" | "mousepad"
                | "libreoffice" | "soffice" | "gimp" | "inkscape" | "vlc" | "evince"
                | "nautilus" | "dolphin" | "thunar" | "code" | "subl" | "atom"
                | "gnome-terminal" | "konsole" | "xterm" | "tilix"
            )
        };

    let output = if is_launch_cmd {
        let mut cmd = std::process::Command::new(shell);
        cmd.arg(flag)
            .arg(command)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        cmd.spawn()
            .context("Failed to launch app")?;
        dbg_log!("[run_command] command treated as background launch, returned immediately");
        return Ok(None);
    } else {
        let mut cmd = std::process::Command::new(shell);
        let run_cmd = if cfg!(target_os = "windows") {
            format!("chcp 65001 > nul && {}", command)
        } else {
            command.to_string()
        };
        cmd.arg(flag)
            .arg(&run_cmd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let mut child = cmd.spawn()
            .context("Failed to run command")?;

        let timeout = std::time::Duration::from_secs(10);
        let start = std::time::Instant::now();
        loop {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = child.kill();
                dbg_log!("[run_command] process terminated via StopFlag");
                anyhow::bail!("Command execution cancelled by user.");
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.stdout.take();
                        let _ = child.stderr.take();
                        dbg_log!("[run_command] command timed out after 10s");
                        if capture {
                            return Ok(Some("[process still running after 10s — treated as background app]".to_string()));
                        }
                        return Ok(None);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => return Err(e.into()),
            }
        }
        child.wait_with_output().context("Failed to collect output")?
    };

    if capture {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = format!(
            "exit_code: {}\nstdout: {}\nstderr: {}",
            output.status.code().unwrap_or(-1),
            stdout.trim(),
            if stderr.is_empty() { "none".to_string() } else { stderr.trim().to_string() }
        );
        trace_log!("[run_command] command output: {}", combined);
        Ok(Some(combined))
    } else {
        dbg_log!("[run_command] command success (output not captured)");
        Ok(None)
    }
}

fn escape_powershell_double_quoted(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '`' => {
                escaped.push('`');
                escaped.push('`');
            }
            '$' => {
                escaped.push('`');
                escaped.push('$');
            }
            '"' => {
                escaped.push('`');
                escaped.push('"');
            }
            _ => {
                escaped.push(c);
            }
        }
    }
    escaped
}

fn sanitize_powershell_script(script: &str) -> String {
    let mut result = script.to_string();

    // Regex for $data = "..." followed by either $rows = $data or $data -replace
    if let Ok(re1) = regex::Regex::new(r#"(?is)(^\s*|\;\s*|\r?\n\s*)(\$data\s*=\s*")([\s\S]*?)("\s*(?:\r?\n|;)\s*(?:\$rows|\$data\s*-replace))"#) {
        result = re1.replace_all(&result, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let var_assign = &caps[2];
            let content = &caps[3];
            let suffix = &caps[4];
            let escaped = escape_powershell_double_quoted(content);
            format!("{}{}{}{}", prefix, var_assign, escaped, suffix)
        }).to_string();
    }

    // Regex for $d.Content.Text = "..." followed by $d.SaveAs2
    if let Ok(re2) = regex::Regex::new(r#"(?is)(^\s*|\;\s*|\r?\n\s*)(\$d\.Content\.Text\s*=\s*")([\s\S]*?)("\s*(?:\r?\n|;)\s*\$d\.SaveAs2)"#) {
        result = re2.replace_all(&result, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let var_assign = &caps[2];
            let content = &caps[3];
            let suffix = &caps[4];
            let escaped = escape_powershell_double_quoted(content);
            format!("{}{}{}{}", prefix, var_assign, escaped, suffix)
        }).to_string();
    }

    result
}

pub fn run_script(params: &Value, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<Option<String>> {
    let script = json_str(params, "script").context("run_script requires a 'script' param")?;
    let capture = params.get("capture_output").and_then(|v| v.as_bool()).unwrap_or(true);
    trace_log!("[run_script] executing script (capture={}):\n--- SCRIPT START ---\n{}\n--- SCRIPT END ---", capture, script);

    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("powershell");
        let sanitized_script = sanitize_powershell_script(script);
        let full_script = format!("$ErrorActionPreference = 'Stop'; [Console]::InputEncoding = [System.Text.Encoding]::UTF8; [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [System.Text.Encoding]::UTF8; {}", sanitized_script);
        c.args(["-NoProfile", "-NonInteractive", "-Command", &full_script]);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", script]);
        c
    };

    cmd.stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn().context("Failed to spawn script process")?;

    let timeout = std::time::Duration::from_secs(12);
    let start = std::time::Instant::now();
    let mut timed_out = false;

    loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = child.kill();
            dbg_log!("[run_script] script terminated via StopFlag");
            anyhow::bail!("Script execution cancelled by user.");
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(e.into()),
        }
    }

    if timed_out {
        dbg_log!("[run_script] script execution timed out after 12s");
        if capture {
            return Ok(Some("exit_code: 1\nstdout: \nstderr: Error: Script execution timed out after 12 seconds. Do NOT use blocking operations (like -Wait or synchronous loops) unless they exit quickly. If starting a GUI application, run it in the background and do not wait for it to close.".to_string()));
        } else {
            return Ok(None);
        }
    }

    let output = child.wait_with_output().context("Failed to collect script output")?;

    if capture {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = format!(
            "exit_code: {}\nstdout: {}\nstderr: {}",
            output.status.code().unwrap_or(-1),
            stdout.trim(),
            if stderr.is_empty() { "none".to_string() } else { stderr.trim().to_string() }
        );
        trace_log!("[run_script] script result:\n{}", combined);
        Ok(Some(combined))
    } else {
        dbg_log!("[run_script] script success (output not captured)");
        Ok(None)
    }
}


// ===========================================================================
// VALUE PARSERS (DESKTOP MODIFIERS)
// ===========================================================================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn parse_button(s: &str) -> Result<Button> {
    match s.to_lowercase().as_str() {
        "left" => Ok(Button::Left),
        "right" => Ok(Button::Right),
        "middle" => Ok(Button::Middle),
        other => anyhow::bail!("Unknown mouse button: '{}'", other),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn parse_key(s: &str) -> Result<Key> {
    if s.chars().count() == 1 {
        return Ok(Key::Unicode(s.chars().next().unwrap()));
    }
    match s.to_lowercase().as_str() {
        "return" | "enter" => Ok(Key::Return),
        "tab" => Ok(Key::Tab),
        "space" => Ok(Key::Space),
        "escape" | "esc" => Ok(Key::Escape),
        "backspace" => Ok(Key::Backspace),
        "delete" | "del" => Ok(Key::Delete),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "pageup" => Ok(Key::PageUp),
        "pagedown" => Ok(Key::PageDown),
        "left" => Ok(Key::LeftArrow),
        "right" => Ok(Key::RightArrow),
        "up" => Ok(Key::UpArrow),
        "down" => Ok(Key::DownArrow),
        "f1" => Ok(Key::F1),  "f2" => Ok(Key::F2),
        "f3" => Ok(Key::F3),  "f4" => Ok(Key::F4),
        "f5" => Ok(Key::F5),  "f6" => Ok(Key::F6),
        "f7" => Ok(Key::F7),  "f8" => Ok(Key::F8),
        "f9" => Ok(Key::F9),  "f10" => Ok(Key::F10),
        "f11" => Ok(Key::F11),"f12" => Ok(Key::F12),
        other => anyhow::bail!("Unknown key: '{}'. Use single characters or named keys.", other),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn parse_modifier(s: &str) -> Result<Key> {
    match s.to_lowercase().as_str() {
        "ctrl" | "control" => Ok(Key::Control),
        "shift" => Ok(Key::Shift),
        "alt" => Ok(Key::Alt),
        "meta" | "super" | "cmd" | "win" => Ok(Key::Meta),
        other => anyhow::bail!("Unknown modifier key: '{}'", other),
    }
}

pub fn read_file(params: &Value) -> Result<Option<String>> {
    let path_str = params.get("path")
        .and_then(|x| x.as_str())
        .with_context(|| "Missing parameter 'path'")?;
    
    let start_line = params.get("start_line").and_then(|v| v.as_i64());
    let end_line = params.get("end_line").and_then(|v| v.as_i64());

    let content = std::fs::read_to_string(path_str)
        .with_context(|| format!("Failed to read file: {}", path_str))?;

    if start_line.is_none() && end_line.is_none() {
        return Ok(Some(content));
    }

    let start = start_line.unwrap_or(1).max(1) as usize;
    let end = end_line.unwrap_or(i64::MAX).max(1) as usize;

    let lines: Vec<&str> = content.lines().collect();
    if start > lines.len() {
        return Ok(Some(format!("[Range starting line {} exceeds total lines {}]", start, lines.len())));
    }

    let end_limit = end.min(lines.len());
    if start > end_limit {
        return Ok(Some(format!("[Invalid range: start_line {} is greater than end_line {}]", start, end_limit)));
    }

    let sliced = &lines[start - 1..end_limit];
    Ok(Some(sliced.join("\n")))
}

pub fn str_replace(params: &Value) -> Result<Option<String>> {
    let path_str = params.get("path")
        .and_then(|x| x.as_str())
        .with_context(|| "Missing parameter 'path'")?;
    
    let find_str = params.get("find")
        .and_then(|x| x.as_str())
        .with_context(|| "Missing parameter 'find'")?;

    let replace_str = params.get("replace")
        .and_then(|x| x.as_str())
        .with_context(|| "Missing parameter 'replace'")?;

    let content = std::fs::read_to_string(path_str)
        .with_context(|| format!("Failed to read file: {}", path_str))?;

    let count = content.matches(find_str).count();
    if count == 0 {
        anyhow::bail!("Target string to replace not found in the file: '{}'", find_str);
    }
    if count > 1 {
        anyhow::bail!("Target string to replace is not unique (found {} occurrences) in: '{}'", count, find_str);
    }

    let new_content = content.replace(find_str, replace_str);
    std::fs::write(path_str, new_content)
        .with_context(|| format!("Failed to write updated contents back to: {}", path_str))?;

    Ok(Some("String replaced successfully.".to_string()))
}
