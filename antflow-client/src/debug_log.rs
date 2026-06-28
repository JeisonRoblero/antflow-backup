use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

static DEBUG_LOG_MUTEX: Mutex<()> = Mutex::new(());
static LAST_LOG_AT: Mutex<Option<Instant>> = Mutex::new(None);

fn antflow_debug_log_path() -> PathBuf {
    std::env::temp_dir().join("antflow_debug.log")
}

fn session_debug_log_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../debug-317755.log")
}

fn format_log_line(message: &str) -> String {
    let now = Instant::now();
    let delta_ms = LAST_LOG_AT
        .lock()
        .ok()
        .and_then(|mut last| {
            let delta = last.map(|t| now.duration_since(t).as_millis()).unwrap_or(0);
            *last = Some(now);
            Some(delta)
        })
        .unwrap_or(0);

    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    format!("[{ts} +{delta_ms}ms] {message}")
}

pub fn append_line(line: &str) {
    if let Ok(_guard) = DEBUG_LOG_MUTEX.lock() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(antflow_debug_log_path())
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

pub fn append_formatted(args: std::fmt::Arguments<'_>) {
    append_line(&format_log_line(&format!("{}", args)));
}

// #region agent log
pub fn session_log(
    location: &str,
    message: &str,
    data: serde_json::Value,
    hypothesis_id: &str,
    run_id: &str,
) {
    let payload = serde_json::json!({
        "sessionId": "317755",
        "runId": run_id,
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": chrono::Utc::now().timestamp_millis(),
    });
    if let Ok(_guard) = DEBUG_LOG_MUTEX.lock() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(session_debug_log_path())
        {
            let _ = writeln!(f, "{}", payload);
        }
    }
}
// #endregion

#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        $crate::debug_log::append_formatted(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! trace_log {
    ($($arg:tt)*) => {
        $crate::debug_log::append_formatted(format_args!($($arg)*));
    };
}
