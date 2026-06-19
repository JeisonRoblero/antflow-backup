use crate::ai::{AiProvider, ChatMessage};
use crate::config::AppConfig;
use crate::db;
use crate::json_utils::{strip_fences, extract_json, extract_best_json, clean_if_json};
use crate::redact::RedactPatterns;
use crate::search;
use crate::ui::console;
use crate::screenshot::{capture_redacted, to_base64};

use anyhow::{Context, Result};

use serde_json::Value;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub type StopFlag = Arc<std::sync::atomic::AtomicBool>;

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

// Max chars kept from a single tool output before truncation.
const TOOL_OUTPUT_MAX_CHARS: usize = 2000;

// Completed CGAE cycles kept as raw messages; older cycles are summarised.
const RAW_CYCLES_KEPT: usize = 1;

pub fn new_stop_flag() -> StopFlag {
    Arc::new(std::sync::atomic::AtomicBool::new(false))
}

fn truncate_tool_output(output: &str) -> String {
    let char_count = output.chars().count();
    if char_count <= TOOL_OUTPUT_MAX_CHARS {
        return output.to_string();
    }
    let truncated: String = output.chars().take(TOOL_OUTPUT_MAX_CHARS).collect();
    format!(
        "{}\n[...+{} chars truncated]",
        truncated,
        char_count - TOOL_OUTPUT_MAX_CHARS
    )
}

/// Extract a web search query from a chat-mode response that contains a JSON block
/// like `{"web_search": "query string"}`.
fn extract_chat_search_query(response: &str) -> Option<String> {
    // Try to find {"web_search": "..."} pattern
    if let Some(pos) = response.find("\"web_search\"") {
        // Find the colon after "web_search"
        let after = &response[pos + 12..];
        if let Some(colon_pos) = after.find(':') {
            let after_colon = after[colon_pos + 1..].trim_start();
            // Find the opening quote
            if after_colon.starts_with('"') {
                let value_start = &after_colon[1..];
                if let Some(end_quote) = value_start.find('"') {
                    let query = &value_start[..end_quote];
                    if !query.is_empty() {
                        return Some(query.to_string());
                    }
                }
            }
        }
    }
    None
}

fn detect_search_intent(prompt: &str) -> bool {
    let prompt_lower = prompt.to_lowercase();
    
    // Check if the prompt contains words indicative of searching/researching/real-time info
    let search_keywords = [
        "search", "find", "google", "lookup", "look up", "research", "investigate",
        "weather", "news", "current", "latest", "recent", "today", "yesterday",
        "busca", "buscar", "encuentra", "encontrar", "investiga", "investigar",
        "averigua", "averiguar", "noticias", "clima", "tiempo", "reciente", "actual",
        "partido", "mundial", "resultado", "marcador", "goleador", "campeon", "quien gano",
        "who won", "winner", "score", "versus", "vs", "pelicula", "movie", "show",
        "dolar", "euro", "precio", "price", "stock", "quien es", "quién es",
        "quien fue", "quién fue", "que es", "qué es", "que paso", "qué pasó",
        "tell me about", "dime sobre", "hablame de", "háblame de", "saber sobre"
    ];
    
    // Clean prompt to check for exact greetings
    let cleaned = prompt_lower.trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c == '¡' || c == '¿' || c.is_whitespace());
        
    let exact_greetings = [
        "hello", "hi", "hey", "hola", "buenos dias", "buenas noches", "buenas tardes",
        "how are you", "como estas", "cómo estás", "thank you", "thanks", "gracias"
    ];
    
    if exact_greetings.iter().any(|g| cleaned == *g) {
        return false;
    }
    
    search_keywords.iter().any(|kw| prompt_lower.contains(kw))
}

async fn extract_search_query(prompt: &str, provider: &dyn AiProvider) -> String {
    let extract_prompt = format!(
        "You are a search query optimizer. Given this user message, extract or write the single best Google/DuckDuckGo search query to find the requested information.\n\
         User message: \"{}\"\n\
         Reply with ONLY the search query. No quotes, no markdown, no other explanation or text.",
        prompt
    );
    
    if let Ok(resp) = provider.chat(
        vec![
            ChatMessage::text("system", "You are a search query optimizer. Output only the query text."),
            ChatMessage::text("user", &extract_prompt)
        ],
        Some(60),
    ).await {
        let clean = resp.content.trim().trim_matches('"').trim().to_string();
        if !clean.is_empty() {
            return clean;
        }
    }
    
    // Fallback: clean the prompt yourself
    let mut clean = prompt.to_string();
    let prefixes = [
        "hola", "hello", "hi", "investiga sobre", "investigar sobre", "investiga",
        "busca sobre", "buscar sobre", "busca", "buscar", "search for", "search about",
        "search", "find out about", "find out", "tell me about", "dime sobre",
        "hablame de", "háblame de"
    ];
    let lower = clean.to_lowercase();
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            clean = clean[prefix.len()..].trim().to_string();
            break;
        }
    }
    if clean.is_empty() {
        prompt.to_string()
    } else {
        clean
    }
}

fn parse_exit_code(output: &str) -> Option<i32> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("exit_code:") {
            return rest.trim().parse::<i32>().ok();
        }
    }
    None
}

fn parse_stderr(output: &str) -> String {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("stderr:") {
            let s = rest.trim();
            if s != "none" && !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

fn output_looks_like_error(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("exception") ||
    lower.contains("error:") ||
    lower.contains("cannot find") ||
    lower.contains("not recognized") ||
    lower.contains("not found") ||
    lower.contains("access is denied") ||
    lower.contains("failed") ||
    lower.contains("no such file") ||
    lower.contains("is not") ||
    lower.contains("cannot be") ||
    lower.contains("unexpected token")
}

fn screenshot_hash(b64: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    b64.hash(&mut h);
    h.finish()
}

fn is_app_launch(tool_name: &str, params: &Value) -> bool {
    if tool_name != "run_command" && tool_name != "run_script" {
        return false;
    }
    let cmd = params["command"].as_str()
        .or(params["script"].as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();
    cmd.starts_with("start ")
        || cmd.starts_with("start-process ")
        || cmd.starts_with("open -a ")
        || cmd.starts_with("xdg-open ")
        || cmd.ends_with(" &")
}

fn make_verify_step(tool_name: &str, params: &Value, platform: &str) -> Option<Value> {
    let cmd = params["command"].as_str()
        .or(params["script"].as_str())
        .unwrap_or("")
        .trim();

    let exe = if cmd.to_lowercase().starts_with("start ") {
        cmd[6..].trim().split_whitespace().next()?.to_string()
    } else if cmd.to_lowercase().starts_with("open -a ") {
        cmd[8..].trim().trim_matches('"').to_string()
    } else if cmd.to_lowercase().starts_with("open ") {
        cmd[5..].trim().split_whitespace().next()?.to_string()
    } else {
        cmd.split_whitespace().next()?.trim_end_matches(" &").to_string()
    };

    if exe.is_empty() { return None; }

    let check_cmd = match platform {
        "windows" => format!("where {}", exe),
        "macos"   => format!("which \"{}\" 2>/dev/null || ls /Applications | grep -i \"{}\"", exe, exe),
        _         => format!("which {} 2>/dev/null || command -v {}", exe, exe),
    };

    Some(serde_json::json!({
        "tool": "run_command",
        "params": { "command": check_cmd, "capture_output": true },
        "delay_ms": 0
    }))
}

fn get_known_fix(script_content: &str, stderr_text: &str) -> Option<String> {
    let stderr_lower = stderr_text.to_lowercase();
    let script_lower = script_content.to_lowercase();

    if script_content.contains("System.Drawing.Graphics(") {
        Some(format!(
            "The script used `New-Object System.Drawing.Graphics(...)` which does not exist. \
             System.Drawing.Graphics cannot be constructed directly. \
             The correct PowerShell pattern to draw and save an image is:\n\
````powershell\n\
             Add-Type -AssemblyName System.Drawing\n\
             $bmp = New-Object System.Drawing.Bitmap(800, 600)\n\
             $g = [System.Drawing.Graphics]::FromImage($bmp)\n\
             $g.Clear([System.Drawing.Color]::White)\n\
             # draw here with $g.FillRectangle(...) etc\n\
             $bmp.Save('$env:USERPROFILE\\drawing.png')\n\
             $g.Dispose(); $bmp.Dispose()\n\
             Start-Process '$env:USERPROFILE\\drawing.png'\n\
```\n\
             Write a complete working script using this pattern to accomplish the original task. \
             Output ONLY the JSON plan with the full corrected run_script."
        ))
    } else if stderr_lower.contains("no se reconoce") || stderr_lower.contains("not recognized") {
        let cmd = script_content.lines().next().unwrap_or("");
        let lower_cmd = cmd.to_lowercase();
        let is_powershell = lower_cmd.contains("get-") || lower_cmd.contains("start-") || lower_cmd.contains("new-object") || lower_cmd.contains("set-location") || lower_cmd.contains("write-") || lower_cmd.contains("out-") || lower_cmd.contains("where-object") || lower_cmd.contains("foreach-object");
        if is_powershell {
            Some(format!(
                "The command '{}' was not recognized. \
                 You attempted to run a PowerShell cmdlet or syntax inside the `run_command` tool (which runs in CMD.exe and does not support PowerShell). \
                 To run PowerShell cmdlets or scripts (like Get-Process, Start-Process, New-Object), you MUST use the `run_script` tool instead of `run_command`. \
                 Output ONLY a corrected JSON plan.",
                cmd
            ))
        } else {
            Some(format!(
                "The command '{}' is not recognized. \
                 On Windows use PowerShell commands not cmd commands. \
                 For checking if a program exists use: `Get-Command mspaint -ErrorAction SilentlyContinue`. \
                 Output ONLY a corrected JSON plan.",
                cmd
            ))
        }
    } else if stderr_lower.contains("argumentlist") && (stderr_lower.contains("null") || stderr_lower.contains("vacío") || stderr_lower.contains("empty") || stderr_lower.contains("argumento")) {
        Some(format!(
            "Start-Process failed: The ArgumentList parameter cannot be empty or null (e.g. ArgumentList ''). \
             If you do not need arguments, completely omit the -ArgumentList parameter. \
             If you want to open a URL in the default browser, do NOT run Chrome/Edge by full path. \
             Instead, simply run: `Start-Process 'https://www.youtube.com'` directly. \
             Correct your command and reply with a new JSON plan."
        ))
    } else if (stderr_lower.contains("redirectstandardoutput") || stderr_lower.contains("redirectstandarderror")) && 
              (stderr_lower.contains("null") || stderr_lower.contains("vacío") || stderr_lower.contains("empty") || stderr_lower.contains("argumento")) {
        Some(format!(
            "Start-Process failed: The -RedirectStandardOutput or -RedirectStandardError parameter cannot be null or empty. \
             Do NOT pass `$null` directly to these parameters in Start-Process. \
             Instead, omit these parameters entirely, or redirect output using PowerShell pipeline operators (e.g. `Start-Process ... | Out-Null` or `Start-Process ... > $null`). \
             Correct your command and reply with a new JSON plan."
        ))
    } else if script_lower.contains("set-location") && (stderr_lower.contains("force") || stderr_lower.contains("coincida con el nombre") || stderr_lower.contains("no parameter matches")) {
        Some(format!(
            "Set-Location failed: Set-Location does not have a `-Force` parameter. \
             Please run `Set-Location -Path '<path>'` without `-Force`. \
             Correct your command and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("clase no registrada") || stderr_lower.contains("classnotreg") || stderr_lower.contains("80040154") || stderr_lower.contains("comobject") {
        Some(format!(
            "COM object creation failed (Class not registered / CLSID failure). \
             Do NOT use legacy COM objects like `New-Object -ComObject WebBrowser.Browser` or `InternetExplorer.Application` to automate web browsing or navigate URLs. \
             Instead, launch a browser process using `Start-Process chrome` or `Start-Process msedge`, or use the `open_url` tool, or write a standard GUI interaction script. \
             Correct your approach and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("system.windows.forms.application") || stderr_lower.contains("typenotfound") || stderr_lower.contains("no se encuentra el tipo [system.windows.forms") {
        Some(format!(
            "The type [System.Windows.Forms.Application] was not found. \
             Do NOT use `System.Windows.Forms` or GUI windows forms API in PowerShell scripts, as they are not supported in this headless/non-interactive shell environment. \
             Automate browsers or applications via command line arguments, standard keyboard shortcuts, or GUI mouse tools. \
             Correct your approach and reply with a new JSON plan."
        ))
    } else if (stderr_lower.contains("no se reconoce como nombre de un cmdlet") || 
               stderr_lower.contains("is not recognized as the name of a cmdlet") || 
               stderr_lower.contains("is not recognized as a cmdlet") || 
               stderr_lower.contains("objectnotfound") ||
               stderr_lower.contains("commandnotfoundexception")) && 
              (script_lower.contains("type_text") || script_lower.contains("key_press") || 
               script_lower.contains("press_key") || script_lower.contains("press key") ||
               script_lower.contains("mouse_click") || script_lower.contains("mouse_move") || 
               script_lower.contains("mouse_drag") || script_lower.contains("open_url") || 
               script_lower.contains("clipboard_set")) {
        Some(format!(
            "PowerShell syntax error: You attempted to call a tool name (like type_text, key_press, etc.) as a PowerShell command. \
             Tool names are JSON actions in the `plan` array and cannot be executed directly inside PowerShell scripts. \
             To type text, press keys, or click, add them as separate JSON tool entries in your plan (e.g. `{{\"tool\":\"type_text\",\"params\":{{\"text\":\"...\"}}}}`). \
             Do NOT call them as commands in `run_script`. \
             Correct your plan structure and reply with a new JSON plan."
        ))
    } else if (stderr_lower.contains("no se reconoce como nombre de un cmdlet") || 
               stderr_lower.contains("is not recognized as the name of a cmdlet") || 
               stderr_lower.contains("is not recognized as a cmdlet") || 
               stderr_lower.contains("objectnotfound")) && 
              (stderr_lower.contains(".processname") || stderr_lower.contains(".mainwindowhandle") || stderr_lower.contains(".id") || script_lower.contains("processname") || script_lower.contains("mainwindowhandle")) {
        Some(format!(
            "PowerShell syntax error: A property name or dot-access (like `.ProcessName`) was not recognized. \
             This usually happens when you double-quote a script block containing `$_.ProcessName` (e.g. \"... $_.ProcessName ...\"), causing PowerShell to expand `$_` to an empty string before execution, leaving `.ProcessName`. \
             To prevent premature expansion, use single quotes `'` around your script block, or escape the dollar sign like ``$_.ProcessName` inside double quotes. \
             Correct your script and reply with a new JSON plan."
        ))
    } else if (stderr_lower.contains("aplicación asociada") || 
               stderr_lower.contains("no application is associated") || 
               stderr_lower.contains("invalidoperationexception")) && 
              script_lower.contains("-verb") {
        Some(format!(
            "Start-Process failed: The specified verb (e.g. `-Verb Run`) is invalid or not associated with the executable or protocol. \
             On Windows, do NOT use `-Verb Run` or `-Verb RunAs` when opening standard applications, files, or web URLs unless you specifically require administrator privilege elevation (RunAs). \
             Simply omit the `-Verb` parameter (e.g. run `Start-Process chrome` or `Start-Process 'https://www.youtube.com'`). \
             Correct your command and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("parámetro 'argument'") || stderr_lower.contains("parameter name 'argument'") {
        Some(format!(
            "Start-Process failed: The parameter `-Argument` does not exist. \
             The correct parameter name is `-ArgumentList`. \
             For example: `Start-Process chrome -ArgumentList 'https://www.youtube.com'`. \
             Correct your command and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("wait-seconds") || stderr_lower.contains("waitseconds") {
        Some(format!(
            "Start-Process failed: The parameter `-Wait-Seconds` does not exist. \
             To wait for a process to exit, use the `-Wait` parameter without specifying seconds. \
             If you want to introduce a delay or timeout in PowerShell, use the `Start-Sleep -Seconds 5` cmdlet. \
             Correct your command and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("verb") || stderr_lower.contains("asociada con el archivo") || stderr_lower.contains("no application is associated") {
        Some(format!(
            "Start-Process failed: Invalid -Verb or missing file association. \
             Do NOT use custom or invalid verbs (like RunWindows, RunShell, or unclosed quotes like RunAs\"). \
             To run a standard application or open a URL, simply use: `Start-Process chrome` or `Start-Process 'https://www.youtube.com'`. \
             Correct your command and reply with a new JSON plan."
        ))
    } else if script_lower.contains("start-process") && (stderr_lower.contains("no se encuentra") || stderr_lower.contains("not find") || stderr_lower.contains("cannot find")) {
        Some(format!(
            "Start-Process failed: The executable path was not found. \
             Do NOT guess or hardcode complex system paths like 'C:\\Program Files\\...\\chrome.exe' or '.app' shortcuts. \
             Instead, use standard system aliases: `Start-Process chrome` or `Start-Process msedge` or open the website URL directly using `Start-Process 'https://www.youtube.com'`. \
             Correct your command and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("falta la cadena en el terminador") || stderr_lower.contains("terminatorexpectedatendofstring") {
        Some(format!(
            "Syntax error: Unterminated string literal. Check your single and double quotes to ensure they are properly balanced. \
             Correct your script and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("debe proporcionar") || stderr_lower.contains("después del operador") || stderr_lower.contains("value expression was expected") || stderr_lower.contains("after the operator") {
        Some(format!(
            "Syntax error: A value expression was expected or an unexpected token occurred. This is often caused by nesting quotes of the same type (e.g. using single quotes inside a single-quoted string) which breaks the parsing. \
             Make sure to balance your single/double quotes correctly, or escape them. \
             Correct your script and reply with a new JSON plan."
        ))
    } else if stderr_lower.contains("falta una expresión") || stderr_lower.contains("missingexpression") {
        Some(format!(
            "Syntax error: Missing expression (often caused by trailing commas, unfinished piping, or syntax errors like passing comma-separated lists instead of arrays). \
             Correct your script and reply with a new JSON plan."
        ))
    } else if script_lower.contains("registerforapplicationevents") || script_lower.contains("applicationcanvasrender") {
        Some(format!(
            "The script used `RegisterForApplicationEvents` or `ApplicationCanvasRender` which do not exist in the PowerShell host. \
             You cannot draw on applications by registering event handlers on the PowerShell host UI. \
             To draw on Windows Paint, you must either: \
             1) Use mouse_drag actions to draw on the screen canvas manually using pixel coordinates (after taking a screenshot to find them). \
             2) Or use `System.Drawing` to create and draw on a Bitmap programmatically, save it as a PNG, and open it. \
             Correct your approach and reply with a new JSON plan."
        ))
    } else if (script_lower.contains("system.drawing") || script_lower.contains("drawellipse") || script_lower.contains("drawline")) && 
              (stderr_lower.contains("falta") || stderr_lower.contains("missing") || stderr_lower.contains("token") || stderr_lower.contains("argumento") || stderr_lower.contains("argument")) {
        Some(format!(
            "PowerShell .NET syntax error: You made a syntax error in your System.Drawing code (e.g., mismatched parentheses, leading commas like `(,$pen,...)`, or unquoted colors like `Black` instead of `[System.Drawing.Color]::Black`). \
             The correct syntax to initialize a Pen and draw an ellipse/line is: \
             `$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::Red, 2)` \
             `$g.DrawEllipse($pen, 100, 100, 200, 200)` \
             `$g.DrawLine($pen, 100, 100, 200, 200)` \
             (Note: No leading commas before variables in method calls). \
             Correct your script and reply with a new JSON plan."
        ))
    } else {
        None
    }
}

fn summarise_cycle(cycle_msgs: &[ChatMessage]) -> String {
    let step_count = cycle_msgs.iter()
        .filter(|m| m.role == "user" && m.content.as_str().map(|s| s.starts_with("Command output:")).unwrap_or(false))
        .count();
    format!("[cycle] {} tool outputs kept in context", step_count)
}

const KEEP_RECENT: usize = 6;

fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter()
        .map(|m| {
            if let Some(s) = m.content.as_str() {
                s.len() / 4
            } else if let Some(arr) = m.content.as_array() {
                let mut sum = 0;
                for part in arr {
                    if let Some(t_val) = part.get("text") {
                        if let Some(t) = t_val.as_str() {
                            sum += t.len() / 4;
                        }
                    } else if part.get("image_url").is_some() {
                        sum += 1000;
                    } else {
                        sum += part.to_string().len() / 4;
                    }
                }
                sum
            } else {
                m.content.to_string().len() / 4
            }
        })
        .sum()
}

fn track_action_repetition(
    tool_name: &str,
    params: &Value,
    recent_action_hashes: &mut std::collections::VecDeque<u64>,
) -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    
    let is_mouse = matches!(tool_name, "mouse_click" | "mouse_move" | "mouse_drag" | "mouse_double_click");
    tool_name.hash(&mut h);
    if !is_mouse {
        params.to_string().hash(&mut h);
    }
    let hash = h.finish();
    
    recent_action_hashes.push_back(hash);
    if recent_action_hashes.len() > 10 {
        recent_action_hashes.pop_front();
    }
    
    recent_action_hashes.iter().filter(|&&h2| h2 == hash).count()
}

async fn compress_context(
    messages: &[ChatMessage],
    provider: &dyn AiProvider,
    original_prompt: &str,
    msg_tx: &tokio::sync::mpsc::Sender<String>,
) -> Vec<ChatMessage> {
    let system = messages.first().cloned();
    let rest = if messages.len() > 1 { &messages[1..] } else { return messages.to_vec() };

    if rest.len() <= KEEP_RECENT {
        return messages.to_vec();
    }

    let (to_compress, recent) = rest.split_at(rest.len() - KEEP_RECENT);

    let history_text: String = to_compress.iter()
        .filter_map(|m| {
            if let Some(s) = m.content.as_str() {
                Some(format!("[{}]: {}", m.role, &s[..s.len().min(300)]))
            } else if let Some(arr) = m.content.as_array() {
                let txt = arr.iter()
                    .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");
                Some(format!("[{}]: {}", m.role, &txt[..txt.len().min(300)]))
            } else {
                let serialized = m.content.to_string();
                Some(format!("[{}]: {}", m.role, &serialized[..serialized.len().min(300)]))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let compact_prompt = format!(
        "Create a concise handoff summary of this agent session. Preserve ALL of:\n\
         - What was accomplished and what failed (keep failures — they matter)\n\
         - Every file path, command, error message that was seen\n\
         - Current task state: done / in-progress / pending\n\
         - Key facts about the environment or user preferences\n\
         Original task: \"{}\"\n\
         History:\n{}\n\
         Reply with ONLY the summary. No preamble. Max 400 words.",
        original_prompt, history_text
    );

    let _ = msg_tx.try_send("Compacting context to save tokens...".to_string());

    let summary = match provider.chat(
        vec![ChatMessage::text("user", &compact_prompt)],
        Some(600),
    ).await {
        Ok(r) => r.content.trim().to_string(),
        Err(_) => format!("[Context compressed — original task: {}]", original_prompt),
    };

    dbg_log!("[compress] {} messages → summary ({} chars)", to_compress.len(), summary.len());

    let mut result = Vec::new();
    if let Some(sys) = system {
        result.push(sys);
    }
    result.push(ChatMessage::text("system",
        &format!("[MEMORY — earlier context compressed]\n{}", summary)
    ));
    result.extend_from_slice(recent);
    result
}

// ---------------------------------------------------------------------------
// CGAE loop: Context → Goal → Act → Evaluate
// ---------------------------------------------------------------------------

fn is_drawing_prompt(prompt: &str) -> bool {
    let p = prompt.to_lowercase();
    (p.contains("draw") || p.contains("paint") || p.contains("sketch") || p.contains("doodle"))
        && !p.contains("open") && !p.contains("install")
}

fn extract_drawing_subject(prompt: &str) -> String {
    let p = prompt.to_lowercase();
    for prefix in &["draw ", "draw me ", "draw a ", "draw an ", "paint ", "paint a ", "paint an ", "sketch "] {
        if let Some(rest) = p.find(prefix).map(|i| &prompt[i + prefix.len()..]) {
            let subject = rest.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
            if !subject.is_empty() { return subject; }
        }
    }
    prompt.to_string()
}

fn make_drawing_script(subject: &str, output_path: &str) -> String {
    let subject_lower = subject.to_lowercase();

    let drawing_code = if subject_lower.contains("duck") {
        r#"
$g.FillEllipse($bodyBrush, 200, 280, 280, 180)
$g.FillEllipse($bodyBrush, 360, 200, 130, 110)
$g.FillEllipse($blackBrush, 445, 220, 18, 18)
$pts = [System.Drawing.PointF[]]@(
    [System.Drawing.PointF]::new(490, 248),
    [System.Drawing.PointF]::new(540, 240),
    [System.Drawing.PointF]::new(540, 262),
    [System.Drawing.PointF]::new(490, 268))
$g.FillPolygon($billBrush, $pts)
$g.FillEllipse($wingBrush, 220, 300, 160, 90)
$pts2 = [System.Drawing.PointF[]]@(
    [System.Drawing.PointF]::new(200, 320),
    [System.Drawing.PointF]::new(140, 270),
    [System.Drawing.PointF]::new(160, 350))
$g.FillPolygon($bodyBrush, $pts2)
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::Orange, 5)
$g.DrawLine($pen, 280, 460, 240, 490)
$g.DrawLine($pen, 280, 460, 290, 495)
$g.DrawLine($pen, 360, 460, 320, 490)
$g.DrawLine($pen, 360, 460, 370, 495)
"#
    } else if subject_lower.contains("ambulance") {
        r#"
$g.FillRectangle($whiteBrush, 100, 260, 420, 160)
$g.FillRectangle($whiteBrush, 420, 220, 120, 200)
$g.FillRectangle($redBrush, 210, 280, 70, 20)
$g.FillRectangle($redBrush, 235, 260, 20, 60)
$g.FillRectangle($blueBrush, 430, 235, 80, 60)
$g.FillEllipse($blackBrush, 130, 395, 80, 80)
$g.FillEllipse($blackBrush, 400, 395, 80, 80)
$g.FillEllipse($whiteBrush2, 148, 413, 44, 44)
$g.FillEllipse($whiteBrush2, 418, 413, 44, 44)
$g.FillRectangle($redBrush, 430, 200, 40, 25)
$g.FillRectangle($blueBrush, 472, 200, 40, 25)
$outlinePen = New-Object System.Drawing.Pen([System.Drawing.Color]::Black, 2)
$g.DrawRectangle($outlinePen, 100, 260, 420, 160)
$g.DrawRectangle($outlinePen, 420, 220, 120, 200)
"#
    } else if subject_lower.contains("house") || subject_lower.contains("home") {
        r#"
$g.FillRectangle($wallBrush, 150, 280, 320, 200)
$roofPts = [System.Drawing.PointF[]]@(
    [System.Drawing.PointF]::new(100, 280),
    [System.Drawing.PointF]::new(310, 120),
    [System.Drawing.PointF]::new(520, 280))
$g.FillPolygon($roofBrush, $roofPts)
$g.FillRectangle($doorBrush, 270, 380, 80, 100)
$g.FillRectangle($blueBrush, 170, 310, 70, 60)
$g.FillRectangle($blueBrush, 380, 310, 70, 60)
"#
    } else if subject_lower.contains("cat") {
        r#"
$g.FillEllipse($bodyBrush, 180, 280, 240, 170)
$g.FillEllipse($bodyBrush, 230, 160, 160, 140)
$earPts1 = [System.Drawing.PointF[]]@(
    [System.Drawing.PointF]::new(240, 175),
    [System.Drawing.PointF]::new(215, 115),
    [System.Drawing.PointF]::new(270, 150))
$g.FillPolygon($bodyBrush, $earPts1)
$earPts2 = [System.Drawing.PointF[]]@(
    [System.Drawing.PointF]::new(380, 175),
    [System.Drawing.PointF]::new(405, 115),
    [System.Drawing.PointF]::new(350, 150))
$g.FillPolygon($bodyBrush, $earPts2)
$g.FillEllipse($blackBrush, 262, 195, 22, 22)
$g.FillEllipse($blackBrush, 336, 195, 22, 22)
$nosePts = [System.Drawing.PointF[]]@(
    [System.Drawing.PointF]::new(305, 248),
    [System.Drawing.PointF]::new(315, 238),
    [System.Drawing.PointF]::new(325, 248))
$g.FillPolygon($noseBrush, $nosePts)
$tailPen = New-Object System.Drawing.Pen($bodyBrush.Color, 12)
$g.DrawArc($tailPen, 350, 300, 160, 160, 180, 200)
"#
    } else {
        r#"
$g.FillEllipse($bodyBrush, 150, 130, 320, 320)
$g.FillEllipse($blackBrush, 235, 220, 40, 40)
$g.FillEllipse($blackBrush, 345, 220, 40, 40)
$smilePen = New-Object System.Drawing.Pen([System.Drawing.Color]::Black, 8)
$g.DrawArc($smilePen, 210, 260, 200, 120, 10, 160)
"#
    };

    format!(r#"Add-Type -AssemblyName System.Drawing

$bmp = New-Object System.Drawing.Bitmap(620, 540)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::White)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias

$bodyBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255,210,160,60))
$whiteBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$whiteBrush2= New-Object System.Drawing.SolidBrush([System.Drawing.Color]::LightGray)
$blackBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::Black)
$redBrush   = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::Red)
$blueBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::SkyBlue)
$billBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::Orange)
$wingBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255,180,130,40))
$wallBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::Wheat)
$roofBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::Brown)
$doorBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::SaddleBrown)
$noseBrush  = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::Pink)

{}

$g.Dispose()
$bmp.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "SAVED: {}"
"#, drawing_code, output_path, output_path)
}

async fn chat_with_retry(
    provider: &dyn crate::ai::AiProvider,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    msg_tx: &tokio::sync::mpsc::Sender<String>,
) -> anyhow::Result<crate::ai::AiResponse> {
    use std::time::Duration;

    const MIN_INTER_CALL_MS: u64 = 500;
    const MAX_RETRIES: u32 = 5;
    const BASE_DELAY_MS: u64 = 1_000;
    const MAX_DELAY_MS: u64 = 60_000;

    tokio::time::sleep(Duration::from_millis(MIN_INTER_CALL_MS)).await;

    let mut attempt = 0u32;
    loop {
        match provider.chat(messages.clone(), max_tokens).await {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                let err_str = e.to_string().to_lowercase();

                let is_rate_limit = err_str.contains("429")
                    || err_str.contains("rate limit")
                    || err_str.contains("rate_limit")
                    || err_str.contains("too many requests")
                    || err_str.contains("resource_exhausted")
                    || err_str.contains("quota");

                let is_transient = is_rate_limit
                    || err_str.contains("503")
                    || err_str.contains("502")
                    || err_str.contains("overloaded")
                    || err_str.contains("timeout")
                    || err_str.contains("unavailable");

                let is_fatal = err_str.contains("401")
                    || err_str.contains("403")
                    || err_str.contains("404")
                    || err_str.contains("invalid api key")
                    || err_str.contains("unauthorized");

                if is_fatal || !is_transient || attempt >= MAX_RETRIES {
                    return Err(e);
                }

                attempt += 1;

                let exp_delay = BASE_DELAY_MS
                    .saturating_mul(1u64 << attempt.min(6))
                    .min(MAX_DELAY_MS);
                let jitter = rand::random::<u64>() % exp_delay.max(1);
                let wait_ms = jitter.max(BASE_DELAY_MS);

                let label = if is_rate_limit { "Rate limited" } else { "Transient error" };
                let msg = format!(
                    "{} — retry {}/{} in {:.1}s...",
                    label, attempt, MAX_RETRIES,
                    wait_ms as f64 / 1000.0
                );
                dbg_log!("[chat_with_retry] {} err='{}' wait={}ms", label, err_str, wait_ms);
                let _ = msg_tx.try_send(msg);

                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }
        }
    }
}

fn generate_acknowledgment(prompt: &str) -> String {
    let lower = prompt.to_lowercase();
    let is_spanish = lower.contains("escribe") || lower.contains("hola") || lower.contains("crea") || lower.contains("busca") || lower.contains("abre") || lower.contains("borra") || lower.contains("elimina") || lower.contains("en un") || lower.contains("un ") || lower.contains("por favor");
    if is_spanish {
        format!("¡Por supuesto! Entendido, procederé con la tarea: \"{}\"...\n\n", prompt)
    } else {
        format!("Sure, I will do this! Starting the task: \"{}\"...\n\n", prompt)
    }
}

pub async fn run_task(
    prompt: &str,
    session_id: &str,
    history: Vec<ChatMessage>,
    provider: &dyn AiProvider,
    storage_dir: &Path,
    cfg: &AppConfig,
    patterns: &RedactPatterns,
    stop: StopFlag,
    msg_tx: tokio::sync::mpsc::Sender<String>,
    token_limit: usize,
    clarification_tx: tokio::sync::mpsc::Sender<(String, Vec<String>)>,
    clarification_rx: tokio::sync::mpsc::Receiver<String>,
    token_tx: Option<tokio::sync::mpsc::Sender<String>>,
) -> Result<String> {
    let mut clarification_rx = clarification_rx;
    let conn = db::open(storage_dir)?;
    let mut messages = history;
    let mut search_widgets: Vec<String> = Vec::new();
    let mut performed_searches: Vec<(String, Vec<crate::search::SearchResult>)> = Vec::new();
    let mut all_search_images: Vec<crate::search::SearchImage> = Vec::new();

    let vision_capable = if cfg.ai.provider == "ollama" {
        crate::ai::ollama::is_vision_model(&cfg.ai.model)
    } else {
        cfg.ai.vision_capable
    };

    let mut user_steps = vec![
        "Preparing workspace".to_string(),
        "Executing steps".to_string(),
        "Verifying final results".to_string(),
    ];
    let mut ack = String::new();

    let _ = msg_tx.send("Analyzing request...".to_string()).await;

    // Fast dynamic conversational acknowledgment & progress steps query!
    let ack_prompt = format!(
        "You are a computer automation assistant. The user wants you to do: \"{}\".\n\
         Generate a friendly, concise, conversational acknowledgment in the user's language (e.g. Spanish if the user query is in Spanish) stating that you will perform the task.\n\
         Also, list 3 to 4 high-level, user-friendly progress steps for this task (e.g. 'Opening Microsoft Word', 'Typing text', 'Saving document'). Do not include technical agent architecture words like 'routing', 'translation', 'LLM', or 'cycles'.\n\
         Reply with ONLY a JSON object in this format:\n\
         {{\n  \"acknowledgment\": \"<concise acknowledgment>\",\n  \"steps\": [\"Step 1\", \"Step 2\", \"Step 3\"]\n}}",
        prompt
    );

    if let Ok(resp) = provider.chat(vec![ChatMessage::text("user", &ack_prompt)], Some(250)).await {
        if let Some(clean_json) = crate::json_utils::extract_best_json(&resp.content) {
            if let Ok(val) = serde_json::from_str::<Value>(&clean_json) {
                if let Some(a) = val["acknowledgment"].as_str() {
                    ack = a.to_string();
                }
                if let Some(arr) = val["steps"].as_array() {
                    let parsed: Vec<String> = arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    if !parsed.is_empty() {
                        user_steps = parsed;
                    }
                }
            }
        }
    }

    if ack.is_empty() {
        ack = generate_acknowledgment(prompt);
    }

    // Stream the AI's dynamic acknowledgment to the user!
    if let Some(ref tx) = token_tx {
        let words = ack.split_whitespace();
        for (i, word) in words.enumerate() {
            let space = if i > 0 { " " } else { "" };
            let _ = tx.send(format!("{}{}", space, word)).await;
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
        let _ = tx.send("\n\n".to_string()).await;
    }

    if !user_steps.is_empty() {
        let _ = msg_tx.send(format!("{}...", user_steps[0])).await;
    }

    let mut english_prompt = prompt.to_string();
    if !prompt.is_empty() {
        let translate_prompt = format!(
            "Translate the following user query to English. Return ONLY the English translation, no other text or explanation:\n\"{}\"",
            prompt
        );
        let msg = ChatMessage::text("user", &translate_prompt);
        if let Ok(resp) = provider.chat(vec![msg], Some(100)).await {
            let clean = resp.content.trim().trim_matches('"').trim().to_string();
            if !clean.is_empty() {
                english_prompt = clean;
            }
        }
    }

    let prompt_lower = english_prompt.to_lowercase();
    let is_drawing = prompt_lower.contains("draw") || prompt_lower.contains("paint")
        || prompt_lower.contains("sketch") || prompt_lower.contains("doodle")
        || prompt_lower.contains("heart");

    // --- SEMANTIC ROUTER ---
    let threshold = 0.45;
    let (matched_skill, mut matched_tools) = crate::ai::router::route_prompt(&english_prompt, threshold);

    if let Some((ref skill, _)) = matched_skill {
        let cache = crate::ai::router::get_router_cache();
        let needed_tools = match skill.id.as_str() {
            "creative_image_editing" | "ui_spatial_drag_and_drop" => {
                vec!["mouse_click", "mouse_drag", "mouse_move", "key_press", "type_text", "screenshot", "task_complete"]
            }
            "misc_web_search" => {
                vec!["open_url", "web_search", "screenshot", "mouse_click", "type_text", "task_complete"]
            }
            "office_word_document_creation" | "business_financial_reporting" => {
                vec!["run_script", "run_command", "type_text", "key_press", "screenshot", "task_complete"]
            }
            _ => {
                vec!["run_script", "run_command", "screenshot", "task_complete"]
            }
        };
        for tool_id in needed_tools {
            if !matched_tools.iter().any(|(t, _)| t.id == tool_id) {
                if let Some(tool) = cache.tools.iter().find(|t| t.id == tool_id) {
                    matched_tools.push((tool.clone(), 0.1));
                }
            }
        }
    }

    if matched_tools.is_empty() {
        let cache = crate::ai::router::get_router_cache();
        let prompt_lower = prompt.to_lowercase();
        let is_gui_task = prompt_lower.contains("click") || prompt_lower.contains("clic")
            || prompt_lower.contains("drag") || prompt_lower.contains("arrastra")
            || prompt_lower.contains("draw") || prompt_lower.contains("paint") || prompt_lower.contains("dibujar") || prompt_lower.contains("pintar")
            || prompt_lower.contains("type") || prompt_lower.contains("write") || prompt_lower.contains("escribe")
            || prompt_lower.contains("key") || prompt_lower.contains("tecla") || prompt_lower.contains("word")
            || prompt_lower.contains("excel") || prompt_lower.contains("mspaint") || prompt_lower.contains("notepad");
        
        for tool in &cache.tools {
            let is_fallback = if is_gui_task {
                tool.id == "run_script"
                    || tool.id == "run_command"
                    || tool.id == "screenshot"
                    || tool.id == "task_complete"
                    || tool.id == "ask_clarification"
                    || tool.id == "mouse_click"
                    || tool.id == "mouse_drag"
                    || tool.id == "mouse_move"
                    || tool.id == "key_press"
                    || tool.id == "type_text"
            } else {
                tool.id == "run_script"
                    || tool.id == "run_command"
                    || tool.id == "screenshot"
                    || tool.id == "task_complete"
                    || tool.id == "ask_clarification"
            };
            if is_fallback {
                matched_tools.push((tool.clone(), 0.0));
            }
        }
    }

    if let Some((ref skill, score)) = matched_skill {
        dbg_log!("[ROUTER] Matched skill/tool '{}' with score {}", skill.id, score);
    }
    for (tool, score) in &matched_tools {
        dbg_log!("[ROUTER] Matched skill/tool '{}' with score {}", tool.id, score);
    }

    if let Ok(device_info) = crate::device::collect() {
        let mut corrected_system_prompt = crate::prompt::build_system_prompt_with_vision(&device_info, vision_capable, Some(&english_prompt));

        // Format dynamic tool definitions
        let tools_list_str = if matched_tools.is_empty() {
            "No tools are currently available for this task. Respond conversationally to the user.".to_string()
        } else {
            let mut parts = Vec::new();
            for (tool, _) in &matched_tools {
                let mut params_obj = serde_json::Map::new();
                if let Some(properties) = tool.schema_json.get("parameters").and_then(|p| p.get("properties")).and_then(|p| p.as_object()) {
                    for (prop_name, prop_val) in properties {
                        if let Some(ty) = prop_val.get("type").and_then(|t| t.as_str()) {
                            if let Some(enum_vals) = prop_val.get("enum").and_then(|e| e.as_array()) {
                                let enum_strs: Vec<String> = enum_vals.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect();
                                params_obj.insert(prop_name.clone(), serde_json::Value::String(format!("enum: {:?}", enum_strs)));
                            } else {
                                params_obj.insert(prop_name.clone(), serde_json::Value::String(ty.to_string()));
                            }
                        }
                    }
                }
                let params_str = serde_json::Value::Object(params_obj).to_string();
                parts.push(format!("{}:{} — {}", tool.id, params_str, tool.description));
            }
            parts.join("\n")
        };

        // Replace the TOOLS section in system prompt
        if let Some(tools_start) = corrected_system_prompt.find("TOOLS\n") {
            if let Some(response_start) = corrected_system_prompt.find("\nRESPONSE (task)") {
                if tools_start < response_start {
                    let before = &corrected_system_prompt[..tools_start + 6];
                    let after = &corrected_system_prompt[response_start..];
                    corrected_system_prompt = format!("{}{}\n{}", before, tools_list_str, after);
                }
            }
        }

        // Prepend skill instructions if matched
        if let Some((ref skill, _)) = matched_skill {
            let skill_instructions = if skill.full_content.starts_with("---") {
                if let Some(end_idx) = skill.full_content[3..].find("---") {
                    skill.full_content[3 + end_idx + 3..].trim().to_string()
                } else {
                    skill.full_content.clone()
                }
            } else {
                skill.full_content.clone()
            };
            corrected_system_prompt = format!("{}\n\n{}", skill_instructions, corrected_system_prompt);
        }

        let mut found_system = false;
        for msg in &mut messages {
            if msg.role == "system" {
                if let Some(text) = msg.content.as_str() {
                    if text.starts_with("You are antflow") || text.contains("You are a precise, cross-platform spatial automation agent") {
                        msg.content = serde_json::Value::String(corrected_system_prompt.clone());
                        dbg_log!("[run_task] Corrected system prompt vision_capable to {}", vision_capable);
                        found_system = true;
                        break;
                    }
                }
            }
        }
        if !found_system {
            messages.insert(0, ChatMessage::text("system", &corrected_system_prompt));
        }
    }

    if token_limit > 0 {
        let used = estimate_tokens(&messages);
        if used > (token_limit * 85 / 100) {
            dbg_log!("[compress] triggering: used≈{} limit={}", used, token_limit);
            messages = compress_context(&messages, provider, prompt, &msg_tx).await;
        }
    }

    let user_msg = if prompt.trim_start().starts_with('[') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(prompt) {
            ChatMessage {
                role: "user".to_string(),
                content: val,
            }
        } else {
            ChatMessage::text("user", prompt)
        }
    } else {
        ChatMessage::text("user", prompt)
    };
    messages.push(user_msg);
    let mut is_chat_mode = false;
    {
        let prompt_lower = prompt.to_lowercase();

        // Anti-task patterns: if the user explicitly says they want a chat response,
        // override task keywords. Checked BEFORE task keywords.
        let anti_task_phrases: &[&str] = &[
            "no action", "just chat", "just answer", "just tell me", "just respond",
            "solo responde", "solo dime", "solo contesta", "ninguna accion", "sin accion",
            "no ejecutes", "no hagas nada", "solo por aqui", "en el chat", "por el chat",
            "don't do anything", "no need to do", "no action required", "no action needed",
            "just here", "chat only", "conversational", "no task",
            "damelo aqui", "responde aqui", "dime aqui",
        ];
        let is_anti_task = anti_task_phrases.iter().any(|p| prompt_lower.contains(p));

        let task_keywords: &[&str] = &[
            "draw", "paint", "sketch", "doodle",
            "dibuja", "dibujar", "pinta", "pintar",
            "open", "launch", "start",
            "abre", "abrir", "lanza", "lanzar",
            "close", "quit", "exit", "kill",
            "cierra", "cerrar", "quita", "quitar", "sale", "salir", "mata", "matar",
            "save", "delete", "remove", "rename",
            "guarda", "guardar", "borra", "borrar", "elimina", "eliminar", "renombra", "renombrar",
            "install", "execute", "command", "script", "terminal", "powershell", "bash", "cmd",
            "instala", "instalar", "ejecuta", "ejecutar", "comando", "consola",
            "click", "press", "drag", "scroll",
            "clic", "presiona", "arrastra", "arrastrar",
            "take a screenshot", "screenshot", "captura de pantalla", "pantallazo",
            "play", "pause", "mute",
            "reproduce", "reproducir", "pausa", "pausar", "silencia", "silenciar",
            "resize", "crop", "rotate",
            "redimensiona", "redimensionar", "recorta", "recortar", "rota", "rotar",
            "zip", "unzip", "compress", "extract",
            "comprime", "comprimir", "descomprime", "descomprimir", "extrae", "extraer",
            "notepad", "bloc de notas", "wordpad",
            "excel", "powerpoint",
        ];

        let is_obvious_task = if cfg.mode == "chat" {
            false
        } else if cfg.mode == "task" {
            true
        } else if is_anti_task {
            false
        } else {
            task_keywords.iter().any(|kw| {
                // Use word boundary matching to avoid false positives
                let kw_lower = kw.to_lowercase();
                if kw_lower.contains(' ') {
                    // Multi-word keywords: use simple contains
                    prompt_lower.contains(&kw_lower)
                } else {
                    // Single-word keywords: check word boundaries
                    prompt_lower.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|word| word == kw_lower)
                }
            })
        };

        let exact_chat_patterns: &[&str] = &[
            "hello", "hi there", "good morning", "good evening", "good night",
            "how are you", "what's up", "whats up",
            "thank you", "thanks", "thx",
            "hi", "hey", "hola", "buenos dias", "buenas noches", "buenas tardes", "yo", "sup", "wassup",
            "bye", "goodbye", "adios", "ciao",
        ];
        let starts_with_chat_patterns: &[&str] = &[
            "what is your name", "who are you", "what can you do",
        ];
        let is_obvious_chat = if cfg.mode == "chat" {
            true
        } else if cfg.mode == "task" {
            false
        } else {
            if is_obvious_task {
                false
            } else {
                let trimmed = prompt_lower.trim()
                    .trim_matches(|c: char| c.is_ascii_punctuation() || c == '¡' || c == '¿' || c.is_whitespace());
                exact_chat_patterns.iter().any(|p| trimmed == *p) ||
                starts_with_chat_patterns.iter().any(|p| trimmed.starts_with(p))
            }
        };

        if is_obvious_task {
            dbg_log!("[gate] keyword match → TASK (skipping AI gate): '{}'", prompt);
        } else if is_obvious_chat {
            let _ = msg_tx.send("Preparing response...".to_string()).await;
            dbg_log!("[gate] keyword match → CHAT: '{}'", prompt);
            let mut chat_history = messages.clone();

            let mut search_context = String::new();
            if detect_search_intent(prompt) {
                let query = extract_search_query(prompt, provider).await;
                dbg_log!("[chat_mode] Pre-flight search detected search intent. Query: '{}'", query);
                let _ = msg_tx.send(format!("Searching web for: \"{}\"", query)).await;

                // Perform search
                let mut is_net_err = false;
                let (result, results_list, images_list) = match search::search_detailed(&query).await {
                    Ok((text, list, imgs)) => (text, list, imgs),
                    Err(e) => {
                        let msg = handle_search_error(&e);
                        if msg.contains("No internet connection") {
                            is_net_err = true;
                        }
                        (msg, vec![], vec![])
                    }
                };

                if !is_net_err {
                    // Send search widget to UI
                    let html = format_search_widget(&query, &results_list, &images_list);
                    if let Some(ref tx) = token_tx {
                        let _ = tx.send(html.clone()).await;
                    }
                    performed_searches.push((query.clone(), results_list.clone()));
                    for img in &images_list {
                        if !all_search_images.iter().any(|existing| existing.url == img.url) {
                            all_search_images.push(img.clone());
                        }
                    }
                }

                // Format search context
                let mut search_info = format!("Search results for '{}':\n{}\n\nFound web pages:\n", query, result);
                for res in &results_list {
                    search_info.push_str(&format!("- Title: \"{}\", URL: {}\n", res.title, res.url));
                }
                if !images_list.is_empty() {
                    search_info.push_str("\nFound images:\n");
                    for img in &images_list {
                        search_info.push_str(&format!("- Image URL: {}\n", img.url));
                    }
                }
                search_info.push_str("\n(Use the above search results to reply to the user. Always include relevant links and images using markdown.)");
                search_context = search_info;
            }

            // Build a chat-oriented system prompt to overwrite any task system prompt instructions!
            let chat_system = if search_context.is_empty() {
                format!(
                    "You are antflow, a helpful conversational AI assistant. Respond in the user's language using rich Markdown.\n\
                     Current date/time: {}\n\
                     Be concise, accurate, and helpful. Do NOT output JSON.\n\
                     When presenting list items with descriptions (such as web search findings, lists of options, or recommendations), structure them cleanly. Keep bullet points compact. Put the description inline after a colon or nested on a new line using exactly 2 spaces of indentation, for example:\n\
                     - **Option A**: Description here.\n\
                     Or:\n\
                     - **Option A**\n\
                       Description indented here.\n\
                     Do NOT leave empty lines between a bullet point and its indented description.",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z")
                )
            } else {
                format!(
                    "You are antflow, a helpful conversational AI assistant. Respond in the user's language using rich Markdown.\n\
                     Current date/time: {}\n\
                     Use the following search results to answer the user's query. You must include the relevant search links and markdown images from the context in your response:\n\
                     {}\n\n\
                     Be detailed, accurate, and helpful. Do NOT output JSON.\n\
                     When presenting list items with descriptions (such as web search findings, lists of options, or recommendations), structure them cleanly. Keep bullet points compact. Put the description inline after a colon or nested on a new line using exactly 2 spaces of indentation, for example:\n\
                     - **Option A**: Description here.\n\
                     Or:\n\
                     - **Option A**\n\
                       Description indented here.\n\
                     Do NOT leave empty lines between a bullet point and its indented description.",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
                    search_context
                )
            };

            // Extract language instruction if present in any system message
            let mut lang_instruction = String::new();
            for msg in &chat_history {
                if msg.role == "system" {
                    if let Some(text) = msg.content.as_str() {
                        if !text.starts_with("You are antflow") && !text.contains("You are a precise, cross-platform spatial automation agent") {
                            lang_instruction = text.to_string();
                        }
                    }
                }
            }

            // Remove all system messages from chat_history to purge spatial agent rules in Chat mode
            chat_history.retain(|msg| msg.role != "system");

            // Build final unified chat system prompt with language instruction
            let mut final_chat_system = chat_system.clone();
            if !lang_instruction.is_empty() {
                final_chat_system = format!("{}\n\nImportant Instruction: {}", final_chat_system, lang_instruction);
            }

            // Insert single clean chat system prompt at index 0
            chat_history.insert(0, ChatMessage::text("system", &final_chat_system));

            if let Some(last_msg) = chat_history.last_mut() {
                let chat_prompt = if search_context.is_empty() {
                    format!(
                        "{}\n\n(System note: Respond conversationally. Reply in the user's language. Be brief and helpful. No JSON, no tools.)",
                        prompt
                    )
                } else {
                    format!(
                        "User query: {}\n\nSearch context:\n{}\n\n(System note: Respond conversationally based on the search context. Reply in the user's language. Be detailed yet concise. No JSON, no tools. You must include the relevant search links and markdown images from the context in your response.)",
                        prompt, search_context
                    )
                };
                last_msg.content = serde_json::Value::String(chat_prompt);
            }

            let chat_res = if let Some(tx) = token_tx.clone() {
                provider.chat_stream(chat_history, Some(1000), tx).await
            } else {
                provider.chat(chat_history, Some(1000)).await
            };

            if let Ok(resp) = chat_res {
                let raw_reply = resp.content.trim().to_string();
                if !raw_reply.is_empty() && !raw_reply.eq_ignore_ascii_case("TASK") {
                    let reply = clean_if_json(&raw_reply);
                    return Ok(prepend_search_context(&reply, &performed_searches, &all_search_images));
                }
            }
        } else {
            dbg_log!("[gate] Running AI gate classifier for: '{}'", prompt);
            let mut history_str = String::new();
            if messages.len() > 1 {
                let start_idx = if messages.len() > 4 { messages.len() - 4 } else { 0 };
                for msg in &messages[start_idx..messages.len() - 1] {
                    if msg.role == "system" {
                        continue;
                    }
                    if let Some(content_str) = msg.content.as_str() {
                        let truncated = if content_str.len() > 300 {
                            format!("{}...", &content_str[..300])
                        } else {
                            content_str.to_string()
                        };
                        history_str.push_str(&format!("{}: {}\n", msg.role.to_uppercase(), truncated));
                    }
                }
            }

            let gate_prompt = if history_str.is_empty() {
                format!(
                    "Classify this user message as either TASK or CHAT.\n\
                     TASK = Requires GUI/desktop automation (clicks, drags, keyboard key presses, taking screenshots, controlling windows outside this application).\n\
                     CHAT = Anything else that runs internally or in the background, including conversation, greetings, web searches, internet research, and executing background commands/scripts/programs that do not require GUI interaction or taking screenshots.\n\
                     User message: \"{}\"\n\
                     Reply with ONLY one word: TASK or CHAT",
                    prompt
                )
            } else {
                format!(
                    "Classify this user message as either TASK or CHAT.\n\
                     TASK = Requires GUI/desktop automation (clicks, drags, keyboard key presses, taking screenshots, controlling windows outside this application).\n\
                     CHAT = Anything else that runs internally or in the background, including conversation, greetings, web searches, internet research, and executing background commands/scripts/programs that do not require GUI interaction or taking screenshots.\n\n\
                     Recent conversation history:\n\
                     {}\n\
                     User message: \"{}\"\n\
                     Reply with ONLY one word: TASK or CHAT",
                    history_str, prompt
                )
            };
            let ai_says_task = if cfg.mode == "chat" {
                false
            } else if cfg.mode == "task" {
                true
            } else {
                match provider.chat(
                    vec![
                        ChatMessage::text("system", "You are a classifier. You must reply with either TASK or CHAT based on the user's message."),
                        ChatMessage::text("user", &gate_prompt)
                    ],
                    Some(50),
                ).await {
                    Ok(resp) => {
                        let content = resp.content.trim();
                        if content.is_empty() {
                            dbg_log!("[gate] AI gate returned empty content. Defaulting to TASK.");
                            true
                        } else {
                            let res = content.to_uppercase().contains("TASK");
                            dbg_log!("[gate] AI gate classification result: '{}' -> classified as: {}", content, if res { "TASK" } else { "CHAT" });
                            res
                        }
                    }
                    Err(e) => {
                        dbg_log!("[gate] AI gate failed: {:?}. Defaulting to TASK.", e);
                        true
                    }
                }
            };

            if !ai_says_task {
                dbg_log!("[gate] AI gate → CHAT: '{}'", prompt);
                is_chat_mode = true;
            } else {
                dbg_log!("[gate] AI gate → TASK: '{}'", prompt);
            }
        }
    }

    if is_chat_mode {
        dbg_log!("[chat_mode] Handling as chat with optional web search");
        let _ = msg_tx.send("Preparing response...".to_string()).await;

        let mut chat_history = messages.clone();

        let mut search_context = String::new();
        if detect_search_intent(prompt) {
            let query = extract_search_query(prompt, provider).await;
            dbg_log!("[chat_mode] Pre-flight search detected search intent. Query: '{}'", query);
            let _ = msg_tx.send(format!("Searching web for: \"{}\"", query)).await;

            // Perform search
            let mut is_net_err = false;
            let (result, results_list, images_list) = match search::search_detailed(&query).await {
                Ok((text, list, imgs)) => (text, list, imgs),
                Err(e) => {
                    let msg = handle_search_error(&e);
                    if msg.contains("No internet connection") {
                        is_net_err = true;
                    }
                    (msg, vec![], vec![])
                }
            };

            if !is_net_err {
                // Send search widget to UI
                let html = format_search_widget(&query, &results_list, &images_list);
                if let Some(ref tx) = token_tx {
                    let _ = tx.send(html.clone()).await;
                }
                performed_searches.push((query.clone(), results_list.clone()));
                for img in &images_list {
                    if !all_search_images.iter().any(|existing| existing.url == img.url) {
                        all_search_images.push(img.clone());
                    }
                }
            }

            let mut search_info = format!("Search results for '{}':\n{}\n\nFound web pages:\n", query, result);
            for res in &results_list {
                search_info.push_str(&format!("- Title: \"{}\", URL: {}\n", res.title, res.url));
            }
            if !images_list.is_empty() {
                search_info.push_str("\nFound images:\n");
                for img in &images_list {
                    search_info.push_str(&format!("- Image URL: {}\n", img.url));
                }
            }
            search_info.push_str("\n(Use the above search results to reply to the user. Always include relevant links and images using markdown.)");
            search_context = search_info;
        }

        // Build a chat-oriented system prompt
        let chat_system = if search_context.is_empty() {
            format!(
                "You are antflow, a helpful AI assistant. Respond conversationally in the user's language using rich Markdown.\n\
                 You have access to web_search to look up current information. If you need to search, include a JSON block like:\n\
                 ```json\n{{\"web_search\": \"your search query\"}}\n```\n\
                 in your response, and the system will perform the search and ask you to provide a final answer.\n\
                 Current date/time: {}\n\
                 Be concise, accurate, and helpful. Never output raw JSON as your entire response.\n\
                 When presenting list items with descriptions (such as web search findings, lists of options, or recommendations), structure them cleanly. Keep bullet points compact. Put the description inline after a colon or nested on a new line using exactly 2 spaces of indentation, for example:\n\
                 - **Option A**: Description here.\n\
                 Or:\n\
                 - **Option A**\n\
                   Description indented here.\n\
                 Do NOT leave empty lines between a bullet point and its indented description.",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z")
            )
        } else {
            format!(
                "You are antflow, a helpful AI assistant. Respond conversationally in the user's language using rich Markdown.\n\
                 Current date/time: {}\n\
                 Use the following search results to answer the user's query. You must include the relevant search links and markdown images from the context in your response:\n\
                 {}\n\n\
                 Be detailed, accurate, and helpful.\n\
                 When presenting list items with descriptions (such as web search findings, lists of options, or recommendations), structure them cleanly. Keep bullet points compact. Put the description inline after a colon or nested on a new line using exactly 2 spaces of indentation, for example:\n\
                 - **Option A**: Description here.\n\
                 Or:\n\
                 - **Option A**\n\
                   Description indented here.\n\
                 Do NOT leave empty lines between a bullet point and its indented description.",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
                search_context
            )
        };

        // Extract language instruction if present in any system message
        let mut lang_instruction = String::new();
        for msg in &chat_history {
            if msg.role == "system" {
                if let Some(text) = msg.content.as_str() {
                    if !text.starts_with("You are antflow") && !text.contains("You are a precise, cross-platform spatial automation agent") {
                        lang_instruction = text.to_string();
                    }
                }
            }
        }

        // Remove all system messages from chat_history to purge spatial agent rules in Chat mode
        chat_history.retain(|msg| msg.role != "system");

        // Build final unified chat system prompt with language instruction
        let mut final_chat_system = chat_system.clone();
        if !lang_instruction.is_empty() {
            final_chat_system = format!("{}\n\nImportant Instruction: {}", final_chat_system, lang_instruction);
        }

        // Insert single clean chat system prompt at index 0
        chat_history.insert(0, ChatMessage::text("system", &final_chat_system));

        // First attempt: get a conversational response
        let chat_res = if let Some(tx) = token_tx.clone() {
            provider.chat_stream(chat_history.clone(), Some(1000), tx).await
        } else {
            provider.chat(chat_history.clone(), Some(1000)).await
        };

        if let Ok(resp) = chat_res {
            let raw_reply = resp.content.trim().to_string();

            // Check if the model requested a web search (as fallback)
            let search_pattern = "\"web_search\"";
            if raw_reply.contains(search_pattern) && search_context.is_empty() {
                // Try to extract the search query
                if let Some(query) = extract_chat_search_query(&raw_reply) {
                    dbg_log!("[chat_mode] Model requested web search: '{}'", query);
                    let _ = msg_tx.send(format!("Searching web for: \"{}\"", query)).await;

                    // Perform search
                    let mut is_net_err = false;
                    let (result, results_list, images_list) = match search::search_detailed(&query).await {
                        Ok((text, list, imgs)) => (text, list, imgs),
                        Err(e) => {
                            let msg = handle_search_error(&e);
                            if msg.contains("No internet connection") {
                                is_net_err = true;
                            }
                            (msg, vec![], vec![])
                        }
                    };

                    if !is_net_err {
                        // Send search widget to UI
                        let html = format_search_widget(&query, &results_list, &images_list);
                        if let Some(ref tx) = token_tx {
                            let _ = tx.send(html.clone()).await;
                        }
                        performed_searches.push((query.clone(), results_list.clone()));
                        for img in &images_list {
                            if !all_search_images.iter().any(|existing| existing.url == img.url) {
                                all_search_images.push(img.clone());
                            }
                        }
                    }

                    // Feed search results back to get final answer
                    let mut search_info = format!("Search results for '{}':\n{}\n\nFound web pages:\n", query, result);
                    for res in &results_list {
                        search_info.push_str(&format!("- Title: \"{}\", URL: {}\n", res.title, res.url));
                    }
                    if !images_list.is_empty() {
                        search_info.push_str("\nFound images:\n");
                        for img in &images_list {
                            search_info.push_str(&format!("- Image URL: {}\n", img.url));
                        }
                    }
                    search_info.push_str("\n(Now provide a natural conversational answer based on these search results. Include relevant links and images using markdown.)");

                    chat_history.push(ChatMessage::text("assistant", &raw_reply));
                    chat_history.push(ChatMessage::text("user", &search_info));

                    let final_res = if let Some(tx) = token_tx.clone() {
                        provider.chat_stream(chat_history, Some(1000), tx).await
                    } else {
                        provider.chat(chat_history, Some(1000)).await
                    };

                    if let Ok(final_resp) = final_res {
                        let final_reply = clean_if_json(&final_resp.content);
                        return Ok(prepend_search_context(&final_reply, &performed_searches, &all_search_images));
                    }
                }
            }

            // No search needed or search extraction failed — return direct response
            if !raw_reply.is_empty() && !raw_reply.eq_ignore_ascii_case("TASK") {
                let reply = clean_if_json(&raw_reply);
                return Ok(prepend_search_context(&reply, &performed_searches, &all_search_images));
            }
        }
        // If chat response failed, fall through to CGAE as fallback
        dbg_log!("[chat_mode] Chat response failed, falling through to CGAE");
    }

    // Drawing fast-path removed — CGAE handles all tasks including drawing.

    const MAX_CYCLES: usize = 35;
    const MAX_STEPS: usize = 100;
    const MAX_VERIFY: usize = 3;  // Reduced to avoid double screenshot round-trips

    let mut cycles = 0usize;
    let mut total_steps = 0usize;
    let mut cycle_starts: Vec<usize> = Vec::new();
    let mut recent_action_hashes: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
    let mut same_action_count: usize = 0;
    let mut circuit_breaker_count: usize = 0;
    let mut zero_output_cycles: usize = 0;
    let mut noop_script_count: usize = 0;
    let mut last_screenshot_hash: Option<u64> = None;
    let mut no_screen_change_count: usize = 0;

    'cgae: loop {
        if stop.load(Ordering::Relaxed) {
            return Ok("Task stopped by user.".to_string());
        }
        if cycles >= MAX_CYCLES {
            return Ok(prepend_search_context(&format!("Reached maximum cycles ({MAX_CYCLES}). Last known state saved."), &performed_searches, &all_search_images));
        }
        if total_steps >= MAX_STEPS {
            return Ok(prepend_search_context(&format!("Reached maximum steps ({MAX_STEPS})."), &performed_searches, &all_search_images));
        }
        cycles += 1;
        cycle_starts.push(messages.len());
        dbg_log!("[cycle {}] starting (total_steps={})", cycles, total_steps);

        if cycles >= 3 {
            let screenshot_only_cycles = cycle_starts.windows(2)
                .rev()
                .take(2)
                .filter(|w| {
                    let (start, end) = (w[0], w[1]);
                    messages[start..end].iter()
                        .filter(|m| m.role == "assistant")
                        .any(|m| m.content.as_str()
                            .map(|s| s.contains("\"screenshot\"") && !s.contains("\"run_command\"") && !s.contains("\"mouse_drag\""))
                            .unwrap_or(false))
                        && !messages[start..end].iter()
                            .any(|m| m.role == "user" && m.content.as_str()
                                .map(|s| s.starts_with("Command output:") || s.starts_with("Output:"))
                                .unwrap_or(false))
                })
                .count();
            let last_two_screenshot_only = screenshot_only_cycles >= 2;
            if last_two_screenshot_only {
                dbg_log!("[cycle {}] SCREENSHOT LOOP DETECTED — injecting screenshot + forced act", cycles);
                if vision_capable {
                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                // Focus the most recently launched window before screenshot
                                if cfg!(target_os = "windows") {
                                    let _ = execute_tool_async("run_script".to_string(), serde_json::json!({
                                        "script": "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate((Get-Process | Where-Object {$_.MainWindowTitle -ne ''} | Sort-Object StartTime -Descending | Select-Object -First 1).Id) | Out-Null",
                                        "capture_output": false
                                    })).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                }
                                if let Ok(img) = take_screenshot(patterns, prompt).await {
                        messages.push(ChatMessage::with_image(
                            &format!(
                                "[SCREEN] Task: \"{}\". Pick ONE element, get its x/y from this image, output JSON action NOW. \
                                 No prose. No screenshots. Example: {{\"plan\":[{{\"tool\":\"mouse_click\",\"params\":{{\"x\":500,\"y\":300,\"button\":\"left\"}},\"delay_ms\":200}}],\"message\":\"clicking canvas\",\"summary\":\"act\"}}",
                                prompt
                            ),
                            &img,
                        ));
                    }
                } else {
                    messages.push(ChatMessage::text("user",
                        "[LOOP BREAK] You must act now. Use run_script with actual executable PowerShell code."
                    ));
                }
            }
        }

        // Inject a todo reminder at the start of each cycle so the model never
        // loses sight of the original goal (Manus "recitation" pattern).
        // Remove any previous todo reminder first to keep context clean.
        messages.retain(|m| {
            !(m.role == "system" && m.content.as_str().map(|s| s.starts_with("[TODO]")).unwrap_or(false))
        });
        let drawing_tip = if is_drawing {
            "\nFor drawing tasks: Do NOT try to use RegisterForApplicationEvents or interact programmatically with Paint's canvas. \
             Either use mouse_drag manually with coordinates, or use System.Drawing to create a Bitmap, draw shapes, save it as PNG, and open it."
        } else {
            ""
        };
        messages.push(ChatMessage::text("system", &format!(
            "[TODO] Original task: \"{}\". Cycles used: {}/{}. Steps used: {}/{}. \
             GROUNDING RULE: Take screenshot → describe UI elements with pixel coords → act with those coords. \
             type_text is ONLY for typing real text into text fields. \
             run_script must contain ONLY executable PowerShell/bash code, never English descriptions. \
             {} \
             Never guess coordinates. Never narrate instead of acting. \
             If stuck: take a fresh screenshot, read the UI, pick exact pixel coords, then act.",
            prompt, cycles, MAX_CYCLES, total_steps, MAX_STEPS, drawing_tip
        )));

        // Keep only the last 2 screenshots in context — older ones bloat
        // the context window and cause models to break schema entirely.
        let mut messages_for_ai = {
            let mut cleaned: Vec<ChatMessage> = Vec::new();
            let total_images = messages.iter().filter(|m| m.has_image()).count();
            let mut image_idx = 0usize;
            for msg in &messages {
                if msg.has_image() {
                    image_idx += 1;
                    if image_idx <= total_images.saturating_sub(2) {
                        cleaned.push(ChatMessage::text(
                            &msg.role,
                            "[screenshot removed from context to save space]",
                        ));
                        continue;
                    }
                }
                cleaned.push(msg.clone());
            }
            cleaned
        };

        if token_limit > 0 {
            let used = estimate_tokens(&messages_for_ai);
            if used > (token_limit * 85 / 100) {
                dbg_log!("[compress] triggering inside loop: used≈{} limit={}", used, token_limit);
                messages = compress_context(&messages_for_ai, provider, prompt, &msg_tx).await;
                // Re-build messages_for_ai from compressed history
                messages_for_ai = {
                    let mut cleaned: Vec<ChatMessage> = Vec::new();
                    let total_images = messages.iter().filter(|m| m.has_image()).count();
                    let mut image_idx = 0usize;
                    for msg in &messages {
                        if msg.has_image() {
                            image_idx += 1;
                            if image_idx <= total_images.saturating_sub(2) {
                                cleaned.push(ChatMessage::text(
                                    &msg.role,
                                    "[screenshot removed from context to save space]",
                                ));
                                continue;
                            }
                        }
                        cleaned.push(msg.clone());
                    }
                    cleaned
                };
            }
        }

        let default_step = "Executing action".to_string();
        let step_text = if cycles.saturating_sub(1) < user_steps.len() {
            &user_steps[cycles.saturating_sub(1)]
        } else {
            user_steps.last().unwrap_or(&default_step)
        };
        let _ = msg_tx.send(format!("{}...", step_text)).await;
        let response = chat_with_retry(provider, messages_for_ai, Some(2048), &msg_tx).await?;
        let raw = response.content.trim();
        dbg_log!("[cycle {}] raw AI response: {}", cycles, raw);
        let content = strip_fences(raw);

        {
            let content_lower = content.to_lowercase();
            let at_paren_count = content.matches(" at (").count();
            let brush_size_count = content_lower.matches("brush size ").count();
            let is_hallucinated_coords = !vision_capable && (at_paren_count >= 5 || brush_size_count >= 3);

            if is_hallucinated_coords {
                dbg_log!("[cycle {}] HALLUCINATION DETECTED: {} 'at (' patterns, {} 'brush size' patterns — injecting correction",
                    cycles, at_paren_count, brush_size_count);
                messages.push(ChatMessage::text("user",
                    "STOP. Your last response contained fabricated UI coordinates that do not reflect the real screen. \
                     DO NOT describe or list UI elements with coordinates. \
                     Instead, use run_script with PowerShell to accomplish the task programmatically. \
                     For drawing tasks on Windows: use run_script to automate Paint via PowerShell, \
                     or use System.Drawing to create the image directly. \
                     Output ONLY a JSON plan with run_script steps. No coordinate lists."
                ));
                continue 'cgae;
            }
        }

        db::insert_message(&conn, session_id, "assistant", &content)?;

        let json_content = extract_best_json(&content);
        dbg_log!("[cycle {}] extracted json: {:?}", cycles, json_content);

        let val: Value = match json_content {
            None => {
                messages.push(ChatMessage::text("user",
                    "Your last response was plain text or wrong schema. Reply with ONLY this exact JSON shape:\n\
                     {\"message\":\"what you do\",\"plan\":[{\"tool\":\"mouse_click\",\"params\":{\"x\":500,\"y\":300,\"button\":\"left\"},\"delay_ms\":200}],\"summary\":\"...\"}\n\
                     x and y are REQUIRED inside params for any mouse tool. No prose, no markdown, no action field."));
                continue 'cgae;
            }
            Some(jc) => match serde_json::from_str::<Value>(&jc) {
                Ok(v) => {
                    let mut v = normalize_agent_json(v);
                    // Normalize legacy/wrong schema: {"action":"click","x":N,"y":N}
                    v = if v.get("plan").is_none() && v.get("tool").is_none() {
                        if let (Some(x), Some(y)) = (v["x"].as_i64(), v["y"].as_i64()) {
                            let button = v["button"].as_str().unwrap_or("left");
                            serde_json::json!({
                                "message": "clicking coordinate",
                                "plan": [{"tool": "mouse_click", "params": {"x": x, "y": y, "button": button}, "delay_ms": 200}],
                                "summary": "click"
                            })
                        } else {
                            v
                        }
                    } else { v };
                    // Short-circuit: if AI decided this is pure chat, skip CGAE entirely.
                    if v["intent"].as_str() == Some("chat") {
                        let reply = v["message"].as_str().unwrap_or("").to_string();
                        if !reply.is_empty() {
                            return Ok(prepend_search_context(&reply, &performed_searches, &all_search_images));
                        }
                    }
                    // Send the AI's status message every cycle so user always sees what's happening
                    let ui_msg = v["message"].as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("Working on it...");
                    let _ = msg_tx.try_send(ui_msg.to_string());
                    v
                }
                Err(e) => {
                    dbg_log!("[cycle {}] JSON parse error: {}", cycles, e);
                    db::insert_message(&conn, session_id, "system",
                        &format!("JSON parse failed: {e}\nRaw: {}", &content[..content.len().min(200)])
                    ).ok();
                    messages.push(ChatMessage::text("user", &format!(
                        "Your response was not valid JSON: {e}. \
                         Reply with ONLY a raw JSON object starting with {{ — no markdown, no explanation, no backticks."
                    )));
                    continue 'cgae;
                }
            }
        };

        // SCHEMA VERIFICATION
        if !val.is_object() || (val["plan"].as_array().is_none() && val["tool"].as_str().is_none()) {
            dbg_log!("[cycle {}] Invalid JSON schema: missing 'plan' or 'tool'", cycles);
            messages.push(ChatMessage::text("user",
                "Your response is a JSON object but is missing the REQUIRED 'plan' (array of steps) or 'tool' (string) fields. \
                 If you are executing a task, you MUST output a JSON object with this exact shape:\n\
                 {\"message\":\"explanation\",\"plan\":[{\"tool\":\"run_script\",\"params\":{\"script\":\"...\"}}],\"summary\":\"...\"}\n\
                 Do NOT output alternative schemas like 'steps' or 'action' or plain text. Ensure you reply with the exact keys ('plan', 'tool', 'params')."
            ));
            continue 'cgae;
        }

        let mut failed: Option<(usize, String)> = None;
        let mut early_done: Option<String> = None;
        let mut plan_summary = val["summary"].as_str().unwrap_or("Plan executed.").to_string();
        let mut steps: Vec<Value> = Vec::new();
        let mut has_task_complete = false;

        // PRE-EXECUTION VALIDATION (DEACTION pattern)
        if let Some(plan) = val["plan"].as_array() {
            let mut validation_errors: Vec<String> = Vec::new();

            if plan.len() > 10 {
                validation_errors.push(format!(
                    "Your plan contains {} steps, which is too long. Keep plans very short (max 5-6 steps) to stay within the total task limit, and re-plan after taking screenshots to verify screen state in between.",
                    plan.len()
                ));
            }

            for (i, step) in plan.iter().enumerate() {
                let tool = step["tool"].as_str().unwrap_or("");
                let params = &step["params"];

                match tool {
                    "mouse_click" | "mouse_move" | "mouse_double_click" => {
                        if params.get("x").and_then(|v| v.as_i64()).is_none()
                            || params.get("y").and_then(|v| v.as_i64()).is_none() {
                            validation_errors.push(format!("step {}: {} missing x/y", i+1, tool));
                        }
                    }
                    "mouse_drag" => {
                        if params.get("from_x").is_none() || params.get("from_y").is_none()
                            || params.get("to_x").is_none() || params.get("to_y").is_none() {
                            validation_errors.push(format!("step {}: mouse_drag missing from_x/from_y/to_x/to_y", i+1));
                        }
                    }
                    "run_script" => {
                        let script = params["script"].as_str().unwrap_or("");
                        if script.is_empty() {
                            validation_errors.push(format!("step {}: run_script has empty script", i+1));
                        }
                        if script.lines().all(|l| l.trim().is_empty() || l.trim().starts_with('#') || l.trim().starts_with("//")) {
                            validation_errors.push(format!("step {}: run_script is comments only", i+1));
                        }
                    }
                    "type_text" => {
                        let text = params["text"].as_str().unwrap_or("");
                        let looks_like_instruction =
                            text.contains("click") || text.contains("select")
                            || text.contains("open") || text.contains("draw")
                            || text.contains("tool") || text.contains("Paint.")
                            || text.contains("Canvas") || text.contains("cmd /")
                            || text.contains("Start-Process") || text.contains("mspaint")
                            || text.contains("powershell") || text.contains("Clear()")
                            || (text.starts_with('(') && text.contains(',') && text.ends_with(')'))
                            || text.contains("\\")
                            || text.starts_with('[');
                        if looks_like_instruction {
                            validation_errors.push(format!(
                                "step {}: type_text is for typing REAL text into fields only, \
                                 not commands or instructions. Rejected: '{}'. \
                                 Use run_script for commands, mouse_click for UI interaction.",
                                i+1, &text[..text.len().min(60)]
                            ));
                        }
                    }
                    "image_analysis" | "take_screenshot" | "browser_action" => {
                        validation_errors.push(format!("step {}: '{}' is not a valid tool", i+1, tool));
                    }
                    _ => {}
                }
            }

            if !validation_errors.is_empty() {
                let error_list = validation_errors.join("; ");
                dbg_log!("[cycle {}] PRE-EXEC VALIDATION FAILED: {}", cycles, error_list);
                messages.push(ChatMessage::text("user", &format!(
                    "Your plan has validation errors and cannot execute: {}. \
                     Fix these issues and reply with a corrected plan. \
                     Valid tools: run_command, run_script, screenshot, mouse_click{{x,y,button}}, \
                     mouse_move{{x,y}}, mouse_drag{{from_x,from_y,to_x,to_y}}, \
                     mouse_scroll, key_press, type_text, clipboard_set, open_url, task_complete.",
                    error_list
                )));
                continue 'cgae;
            }
        }

        // Handle single-shot tools (no plan array)
        if let Some(tool_name) = val["tool"].as_str() {
            match tool_name {
                "task_complete" => {
                    let summary = val["params"]["summary"].as_str().unwrap_or(&plan_summary).to_string();
                    return Ok(prepend_search_context(&summary, &performed_searches, &all_search_images));
                }
                "ask_clarification" => {
                    let question = val["params"]["question"].as_str().unwrap_or("").to_string();
                    let options: Vec<String> = val["params"]["options"].as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    let _ = clarification_tx.send((question.clone(), options)).await;
                    let answer = loop {
                        if stop.load(Ordering::Relaxed) {
                            return Ok("Task stopped by user.".to_string());
                        }
                        if let Ok(ans) = clarification_rx.try_recv() {
                            break ans;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    };
                    db::insert_message(&conn, session_id, "user", &answer)?;
                    messages.push(ChatMessage::text("user", &answer));
                    continue 'cgae;
                }
                "web_search" => {
                    let query = val["params"]["query"].as_str().unwrap_or("").to_string();
                    let mut is_net_err = false;
                    let (result, results_list, images_list) = match search::search_detailed(&query).await {
                        Ok((text, list, imgs)) => (text, list, imgs),
                        Err(e) => {
                            let msg = handle_search_error(&e);
                            if msg.contains("No internet connection") {
                                is_net_err = true;
                            }
                            (msg, vec![], vec![])
                        }
                    };

                    if !is_net_err {
                        let html = format_search_widget(&query, &results_list, &images_list);

                        // Send the search widget HTML directly to the token stream so it renders dynamically in real-time
                        if let Some(ref tx) = token_tx {
                            let _ = tx.send(html.clone()).await;
                        }

                        search_widgets.push(html.clone());
                        performed_searches.push((query.clone(), results_list.clone()));
                        for img in &images_list {
                            if !all_search_images.iter().any(|existing| existing.url == img.url) {
                                all_search_images.push(img.clone());
                            }
                        }
                    }

                    // Send JSON to thought process so it is displayed in the thought process accordion
                    // Commented out to prevent duplicate search widget in Thought Process
                    /*
                    let _ = msg_tx.send(format!("JSON_SEARCH:{}", serde_json::json!({
                        "type": "web_search",
                        "query": query,
                        "results": results_list,
                        "images": images_list,
                    }).to_string())).await;
                    */

                    let mut search_info = format!("Search results for '{}':\n{}\n\nFound web pages:\n", query, result);
                    for res in &results_list {
                        search_info.push_str(&format!("- Title: \"{}\", URL: {}\n", res.title, res.url));
                    }
                    if !images_list.is_empty() {
                        search_info.push_str("\nFound images:\n");
                        for img in &images_list {
                            search_info.push_str(&format!("- Image URL: {}\n", img.url));
                        }
                    }
                    search_info.push_str("\n(System instruction: Please include the most relevant pages/links you searched, and render the images using markdown `![image](url)` if they are available and relevant to the user query.)");
                    messages.push(ChatMessage::text("user", &search_info));
                    continue 'cgae;
                }
                "screenshot" => {
                    if !vision_capable {
                        messages.push(ChatMessage::text("user",
                            "Screenshot requested but this model has no vision capability."));
                        continue 'cgae;
                    }
                    let img = take_screenshot(patterns, prompt).await?;
                    messages.push(ChatMessage::with_image("Current screen:", &img));
                    continue 'cgae;
                }
                other => {
                    dbg_log!("[cycle {}] single tool: {}", cycles, other);
                    let out = execute_tool_async(other.to_string(), val["params"].clone()).await;
                    let _ = db::log_action(&conn, session_id, other, &val["params"].to_string(), None);
                    match out {
                        Ok(o) => {
                            total_steps += 1;
                            // Fingerprint: for mouse actions, hash tool only (coords vary but mean no progress)
                            // For other tools, hash tool+args
                            same_action_count = track_action_repetition(other, &val["params"], &mut recent_action_hashes);
                            let fb = o.map(|s| format!("Output:\n{}", truncate_tool_output(&s)))
                                .unwrap_or_else(|| "Done.".to_string());
                            messages.push(ChatMessage::text("user", &fb));
                        }
                        Err(e) => {
                            messages.push(ChatMessage::text("user",
                                &format!("Tool '{other}' failed: {e}.")));
                        }
                    }
                    continue 'cgae;
                }
            }
        }

        if let Some(plan_steps) = val["plan"].as_array() {
            steps = plan_steps.iter().cloned().collect();
            let mut last_mouse_pos: Option<(i64, i64)> = None;
            for i in 0..steps.len() {
                if stop.load(Ordering::Relaxed) {
                    return Ok("Task stopped by user.".to_string());
                }
                if total_steps >= MAX_STEPS {
                    return Ok(prepend_search_context(&format!("Reached maximum steps ({MAX_STEPS})."), &performed_searches, &all_search_images));
                }

                let step = &steps[i];
                let tool_name = step["tool"].as_str().unwrap_or("");
                let mut params = step["params"].clone();
                let delay_ms = step["delay_ms"].as_u64().unwrap_or(0) as u64;

                if tool_name == "mouse_move" {
                    if let (Some(x), Some(y)) = (params["x"].as_i64(), params["y"].as_i64()) {
                        last_mouse_pos = Some((x, y));
                    }
                }
                if matches!(tool_name, "mouse_click" | "mouse_double_click") {
                    if params.get("x").and_then(|v| v.as_i64()).is_none() {
                        if let Some((lx, ly)) = last_mouse_pos {
                            if let Some(obj) = params.as_object_mut() {
                                obj.insert("x".to_string(), serde_json::json!(lx));
                                obj.insert("y".to_string(), serde_json::json!(ly));
                            }
                        }
                    }
                }

                dbg_log!("[cycle {} step {}/{}] tool='{}' params={}", cycles, i+1, steps.len(), tool_name, params);

                let step_desc = match tool_name {
                    "mouse_click" => {
                        let x = params["x"].as_i64().unwrap_or(0);
                        let y = params["y"].as_i64().unwrap_or(0);
                        let button = params["button"].as_str().unwrap_or("left");
                        format!("Clicking {} button at ({}, {})", button, x, y)
                    }
                    "mouse_double_click" => {
                        let x = params["x"].as_i64().unwrap_or(0);
                        let y = params["y"].as_i64().unwrap_or(0);
                        let button = params["button"].as_str().unwrap_or("left");
                        format!("Double clicking {} button at ({}, {})", button, x, y)
                    }
                    "mouse_move" => {
                        let x = params["x"].as_i64().unwrap_or(0);
                        let y = params["y"].as_i64().unwrap_or(0);
                        format!("Moving mouse to ({}, {})", x, y)
                    }
                    "mouse_drag" => {
                        let from_x = params["from_x"].as_i64().unwrap_or(0);
                        let from_y = params["from_y"].as_i64().unwrap_or(0);
                        let to_x = params["to_x"].as_i64().unwrap_or(0);
                        let to_y = params["to_y"].as_i64().unwrap_or(0);
                        format!("Dragging from ({}, {}) to ({}, {})", from_x, from_y, to_x, to_y)
                    }
                    "run_script" => {
                        let script = params["script"].as_str().unwrap_or("");
                        let first_line = script.lines().next().unwrap_or("").trim();
                        let line_trunc = if first_line.len() > 60 {
                            format!("{}...", &first_line[..60])
                        } else {
                            first_line.to_string()
                        };
                        format!("Running script: {}", line_trunc)
                    }
                    "run_command" => {
                        let cmd = params["command"].as_str().unwrap_or("");
                        let first_line = cmd.lines().next().unwrap_or("").trim();
                        let line_trunc = if first_line.len() > 60 {
                            format!("{}...", &first_line[..60])
                        } else {
                            first_line.to_string()
                        };
                        format!("Running command: {}", line_trunc)
                    }
                    "screenshot" => "Taking screenshot...".to_string(),
                    "type_text" => {
                        let text = params["text"].as_str().unwrap_or("");
                        let text_trunc = if text.len() > 40 {
                            format!("{}...", &text[..40])
                        } else {
                            text.to_string()
                        };
                        format!("Typing text: \"{}\"", text_trunc)
                    }
                    "key_press" => {
                        let key = params["key"].as_str().unwrap_or("");
                        format!("Pressing key: {}", key)
                    }
                    "web_search" => {
                        let query = params["query"].as_str().unwrap_or("");
                        format!("Searching web for: \"{}\"", query)
                    }
                    "ask_clarification" => "Asking for clarification...".to_string(),
                    "task_complete" => "Task complete!".to_string(),
                    other => format!("Executing tool: {}", other),
                };
                let _ = msg_tx.try_send(format!("Step {}/{}: {}", i + 1, steps.len(), step_desc));

                if tool_name == "run_script" {
                    if let Some(obj) = params.as_object_mut() {
                        obj.insert("capture_output".to_string(), serde_json::json!(true));
                    }

                    if cfg!(target_os = "windows") {
                        let script = params["script"].as_str().unwrap_or("");
                        if script.contains("[Win32]") && !script.contains("Add-Type") {
                            let patched = format!(r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {{
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}}
"@
{}"#, script);
                            if let Some(obj) = params.as_object_mut() {
                                obj.insert("script".to_string(), serde_json::json!(patched));
                            }
                            dbg_log!("[cycle {} step {}] AUTO-PATCHED Win32 Add-Type header", cycles, i+1);
                        }
                    }

                    {
                        let script = params["script"].as_str().unwrap_or("").to_string();
                        if let Some(corrected) = resolve_app_command(&script) {
                            dbg_log!("[cycle {} step {}] APP COMMAND CORRECTED: '{}' -> '{}'",
                                cycles, i+1, script, corrected);
                            if let Some(obj) = params.as_object_mut() {
                                obj.insert("script".to_string(), serde_json::json!(corrected));
                            }
                        }
                    }

                    let script = params["script"].as_str().unwrap_or("");
                    let is_noop = script.lines()
                        .all(|l| l.trim().is_empty() || l.trim().starts_with('#') || l.trim().starts_with("//"));
                    if is_noop {
                        noop_script_count += 1;
                        dbg_log!("[cycle {} step {}] NO-OP SCRIPT (count={}): {}", cycles, i+1, noop_script_count, script);
                        let feedback = if noop_script_count >= 3 {
                            "CRITICAL: You have sent a comments-only script 3 times in a row. \
                             You MUST write real executable code. \
                             Windows example: `New-Item -Path 'C:\\test.txt' -ItemType File`. \
                             Linux example: `echo hello > /tmp/test.txt`. \
                             No English descriptions. Only runnable code lines."
                        } else {
                            "Your script contained only comments and did nothing. \
                             Write actual executable code, not comments or descriptions."
                        };
                        messages.push(ChatMessage::text("user", feedback));
                        failed = Some((i, format!("no-op script attempt {}", noop_script_count)));
                        break;
                    } else {
                        noop_script_count = 0;
                    }
                }

                // Reject mouse_click on canvas when task involves drawing — must use mouse_drag
                if tool_name == "mouse_click" {
                    let x = params["x"].as_i64().unwrap_or(0);
                    let y = params["y"].as_i64().unwrap_or(0);
                    let is_drawing_task = is_drawing;
                    let on_canvas = y > 150 && x > 50;
                    if is_drawing_task && on_canvas && same_action_count >= 2 {
                        messages.push(ChatMessage::text("user",
                            "You keep clicking the canvas but clicking does not draw anything. \
                             To draw you MUST use mouse_drag with from_x/from_y/to_x/to_y. \
                             Example to draw a line: {\"tool\":\"mouse_drag\",\"params\":{\"from_x\":300,\"from_y\":300,\"to_x\":600,\"to_y\":300}}"
                        ));
                        failed = Some((i, "clicking canvas does not draw — use mouse_drag".to_string()));
                        break;
                    }
                }

                match tool_name {
                "task_complete" => {
                    // Don't trust task_complete from inside a plan — force visual verification
                    dbg_log!("[cycle {} step {}] task_complete in plan - deferring to visual eval", cycles, i+1);
                    plan_summary = step["params"]["summary"].as_str().unwrap_or(&plan_summary).to_string();
                    has_task_complete = true;
                    break;
                }
                _ if is_app_launch(tool_name, &params) => {
                    let cmd_str = params["command"].as_str()
                        .or(params["script"].as_str()).unwrap_or("");
                    let exe = if cmd_str.to_lowercase().starts_with("start ") {
                        cmd_str[6..].trim().split_whitespace().next().unwrap_or("").to_lowercase()
                    } else if cmd_str.to_lowercase().starts_with("start-process ") {
                        cmd_str[14..].trim().trim_matches('"').trim_matches('\'')
                            .split_whitespace().next().unwrap_or("").to_lowercase()
                    } else if cmd_str.to_lowercase().starts_with("open -a ") {
                        cmd_str[8..].trim().trim_matches('"').to_lowercase()
                    } else {
                        cmd_str.split_whitespace().next().unwrap_or("").to_lowercase()
                    };
                    let exe_clean = exe.trim_end_matches(".exe").trim_matches('"').trim_matches('\'').to_string();
                    if !exe_clean.is_empty() {
                        let already_running = if cfg!(target_os = "windows") {
                            execute_tool_async("run_script".to_string(), serde_json::json!({
                                "script": format!("if (Get-Process -Name '{}' -ErrorAction SilentlyContinue) {{ 'is_active' }} else {{ 'inactive' }}", exe_clean),
                                "capture_output": true
                            })).await.ok().flatten().map(|o| o.contains("is_active")).unwrap_or(false)
                        } else {
                            execute_tool_async("run_command".to_string(), serde_json::json!({
                                "command": format!("pgrep -x '{}' > /dev/null 2>&1 && echo is_active || echo inactive", exe_clean),
                                "capture_output": true
                            })).await.ok().flatten().map(|o| o.contains("is_active")).unwrap_or(false)
                        };
                        if already_running {
                            dbg_log!("[cycle {} step {}] {} already running — focusing", cycles, i+1, exe_clean);
                            if cfg!(target_os = "windows") {
                                let focus_script = format!(r#"
$p = Get-Process -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($p) {{
    Add-Type -AssemblyName Microsoft.VisualBasic
    [Microsoft.VisualBasic.Interaction]::AppActivate($p.Id) | Out-Null
    Start-Sleep -Milliseconds 300
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinFocus {{
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}}
"@
    $hwnd = [WinFocus]::GetForegroundWindow()
    [WinFocus]::ShowWindow($hwnd, 3) | Out-Null
    Write-Output "focused"
}}"#, exe_clean.trim_end_matches(".exe"));
                                let focus_result = execute_tool_async("run_script".to_string(), serde_json::json!({
                                    "script": focus_script,
                                    "capture_output": true
                                })).await;
                                let focused = focus_result.ok().flatten()
                                    .map(|o| o.contains("focused")).unwrap_or(false);
                                if focused {
                                    messages.push(ChatMessage::text("user",
                                        &format!("'{}' was already running and is now focused and maximized. Proceed with the task.", exe_clean)
                                    ));
                                } else {
                                    messages.push(ChatMessage::text("user",
                                        &format!("'{}' is already running. It has been focused. Proceed with the task.", exe_clean)
                                    ));
                                }
                            } else {
                                messages.push(ChatMessage::text("user",
                                    &format!("'{}' is already running and focused. Proceed with the task.", exe_clean)
                                ));
                            }
                            continue;
                        }
                    }

                    // Auto-inject a verify step if the previous step wasn't already a check.
                    let prev_was_check = i > 0 && {
                        let prev = &steps[i - 1];
                        let prev_cmd = prev["params"]["command"].as_str().unwrap_or("");
                        prev_cmd.contains("where ") || prev_cmd.contains("which ")
                            || prev_cmd.contains("command -v")
                    };
                    let platform = if cfg!(target_os = "windows") { "windows" }
                        else if cfg!(target_os = "macos") { "macos" }
                        else { "linux" };
                    if !prev_was_check {
                        if let Some(verify) = make_verify_step(tool_name, &params, platform) {
                            let check_result = execute_tool_async(
                                "run_command".to_string(),
                                verify["params"].clone(),
                            ).await;
                            if let Ok(Some(out)) = check_result {
                                let out_lower = out.to_lowercase();
                                if out_lower.contains("not found") || out_lower.contains("no such")
                                    || out_lower.trim().is_empty() {
                                    let msg = format!(
                                        "App check failed: {}. Re-plan using an alternative.",
                                        truncate_tool_output(&out)
                                    );
                                    messages.push(ChatMessage::text("user", &msg));
                                    failed = Some((i, msg));
                                    break;
                                }
                                messages.push(ChatMessage::text("user",
                                    &format!("Verified: {}", truncate_tool_output(&out))
                                ));
                            }
                        }
                    }
                    let result = execute_tool_async(tool_name.to_string(), params.clone()).await;
                    let _ = db::log_action(&conn, session_id, tool_name, &params.to_string(), None);
                    match result {
                        Ok(out) => {
                            total_steps += 1;
                            // Fingerprint this action
                            same_action_count = track_action_repetition(tool_name, &params, &mut recent_action_hashes);
                            if let Some(ref o) = out {
                                dbg_log!("[cycle {} step {}] output: {}", cycles, i+1, truncate_tool_output(o));

                                let exit_code = parse_exit_code(o);
                                let stderr_text = parse_stderr(o);
                                let looks_bad = output_looks_like_error(o);
                                let hard_fail = exit_code.map(|c| c != 0).unwrap_or(false);

                                if hard_fail || (!stderr_text.is_empty() && looks_bad) {
                                    let script_content = params["script"].as_str().unwrap_or("");
                                    let known_fix = get_known_fix(script_content, &stderr_text);

                                    let diag_prompt = if let Some(fix) = known_fix {
                                        fix
                                    } else {
                                        format!(
                                            "The last command failed with exit_code={} and this error:\n\"{}\"\n\
                                             Before writing a new plan, answer in ONE sentence: \
                                             what exactly went wrong and why? \
                                             Then immediately provide a corrected plan as JSON.",
                                            exit_code.unwrap_or(-1),
                                            if stderr_text.is_empty() {
                                                truncate_tool_output(o)
                                            } else {
                                                stderr_text.clone()
                                            }
                                        )
                                    };
                                    dbg_log!("[cycle {} step {}] HARD FAIL exit={:?} stderr='{}'",
                                        cycles, i+1, exit_code, stderr_text);
                                    messages.push(ChatMessage::text("user", &format!(
                                        "Command output:\n{}", truncate_tool_output(o)
                                    )));
                                    messages.push(ChatMessage::text("user", &diag_prompt));
                                    failed = Some((i, format!(
                                        "exit_code={:?} stderr={}",
                                        exit_code,
                                        if stderr_text.is_empty() { "none" } else { &stderr_text }
                                    )));
                                    break;
                                }

                                messages.push(ChatMessage::text("user", &format!(
                                    "Command output:\n{}", truncate_tool_output(o)
                                )));
                            } else {
                                dbg_log!("[cycle {} step {}] output: (none)", cycles, i+1);
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms.max(2500))).await;
                        }
                        Err(e) => {
                            dbg_log!("[cycle {} step {}] FAILED: {}", cycles, i+1, e);
                            failed = Some((i, e.to_string()));
                            break;
                        }
                    }

                    continue;
                }
                "screenshot" => {
                    if !vision_capable {
                        messages.push(ChatMessage::text("user",
                            "Screenshot requested but this model has no vision capability. \
                             Continue planning based on expected state after previous steps."
                        ));
                        continue;
                    }
                    let img = take_screenshot(patterns, prompt).await?;

                    let new_hash = screenshot_hash(&img);
                    let screen_changed = last_screenshot_hash
                        .map(|old| old != new_hash)
                        .unwrap_or(true);
                    last_screenshot_hash = Some(new_hash);

                    if !screen_changed && i > 0 {
                        no_screen_change_count += 1;
                        dbg_log!("[cycle {} step {}] SCREEN UNCHANGED (count={})",
                            cycles, i+1, no_screen_change_count);
                    } else {
                        no_screen_change_count = 0;
                    }

                    let remaining = steps.len().saturating_sub(i + 1);

                    let ctx = if !screen_changed && no_screen_change_count >= 2 {
                        format!(
                            "HARD STOP: Screen is IDENTICAL for {} consecutive screenshots. \
                             Your mouse/keyboard actions are having ZERO effect. \
                             You MUST respond with ONLY a run_script plan that: \
                             1) Gets the title of the currently focused window \
                             2) Lists running processes to confirm the target app is open \
                             3) Reports findings so you can re-plan with real data. \
                             Example: {{\"plan\":[{{\"tool\":\"run_script\",\"params\":{{\"script\":\
                             \"(Get-Process | Where-Object {{$_.MainWindowTitle -ne ''}} | \
                             Select-Object Name,MainWindowTitle | Format-List) | Out-String\",\
                             \"capture_output\":true}},\"delay_ms\":0}}]}}\
                             Original task: \"{}\". No mouse actions until you have shell output.",
                            no_screen_change_count, prompt
                        )
                    } else {
                        format!(
                            "Screenshot taken mid-plan after {} steps. \
                             Original task: \"{}\". \
                             FIRST describe every visible UI element with pixel coordinates. \
                             THEN produce a new plan using those exact coordinates. \
                             {} steps were remaining in the old plan — re-plan from what you see now.",
                            i, prompt, remaining
                        )
                    };

                    messages.push(ChatMessage::with_image(&ctx, &img));
                    continue;
                }
                "web_search" => {
                    let query = params["query"].as_str().unwrap_or("").to_string();
                    match search::search_detailed(&query).await {
                        Ok((text_result, results, images)) => {
                            let html = format_search_widget(&query, &results, &images);
                            if let Some(ref tx) = token_tx {
                                let _ = tx.send(html.clone()).await;
                            }
                            search_widgets.push(html);
                            performed_searches.push((query.clone(), results.clone()));
                            for img in &images {
                                if !all_search_images.iter().any(|existing| existing.url == img.url) {
                                    all_search_images.push(img.clone());
                                }
                            }

                            let _payload = serde_json::json!({
                                "type": "web_search",
                                "query": query,
                                "results": results,
                                "images": images,
                            });
                            // Commented out to prevent duplicate search widget in Thought Process
                            // let _ = msg_tx.try_send(format!("JSON_SEARCH:{}", _payload.to_string()));
                            let mut search_info = format!("Search results for '{}':\n{}\n\nFound web pages:\n", query, text_result);
                            for res in &results {
                                search_info.push_str(&format!("- Title: \"{}\", URL: {}\n", res.title, res.url));
                            }
                            if !images.is_empty() {
                                search_info.push_str("\nFound images:\n");
                                for img in &images {
                                    search_info.push_str(&format!("- Image URL: {}\n", img.url));
                                }
                            }
                            search_info.push_str("\n(System instruction: Please include the most relevant pages/links you searched, and render the images using markdown `![image](url)` if they are available and relevant to the user query.)");
                            messages.push(ChatMessage::text("user", &search_info));
                        }
                        Err(e) => {
                            let err_msg = handle_search_error(&e);
                            let _payload = serde_json::json!({
                                "type": "web_search",
                                "query": query,
                                "results": Vec::<search::SearchResult>::new(),
                                "images": Vec::<search::SearchImage>::new(),
                                "error": err_msg,
                            });
                            // Commented out to prevent duplicate search widget in Thought Process
                            // let _ = msg_tx.try_send(format!("JSON_SEARCH:{}", _payload.to_string()));
                            messages.push(ChatMessage::text("user", &format!("Search results:\n{err_msg}")));
                        }
                    }
                }
                "ask_clarification" => {
                    let question = params["question"].as_str().unwrap_or("").to_string();
                    let options: Vec<String> = params["options"].as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    let _ = clarification_tx.send((question.clone(), options)).await;
                    let answer = loop {
                        if stop.load(Ordering::Relaxed) {
                            return Ok("Task stopped by user.".to_string());
                        }
                        if let Ok(ans) = clarification_rx.try_recv() {
                            break ans;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    };
                    db::insert_message(&conn, session_id, "user", &answer)?;
                    messages.push(ChatMessage::text("user", &answer));
                }
                _ => {
                    // If this is an app-launch command, auto-maximize after the delay
                    let is_launch = {
                        let cmd = params["command"].as_str().unwrap_or("").trim().to_lowercase();
                        tool_name == "run_command" && (
                            cmd.starts_with("start ")
                            || cmd.starts_with("open ")
                            || cmd.starts_with("xdg-open ")
                            || cmd.ends_with(" &")
                        )
                    };

                    let mut final_params = params.clone();
                    let is_spatial = tool_name == "mouse_click"
                        || tool_name == "mouse_double_click"
                        || tool_name == "mouse_drag";

                    if is_spatial && vision_capable {
                        let mut points = Vec::new();
                        if let (Some(x), Some(y)) = (params.get("x").and_then(|v| v.as_i64()), params.get("y").and_then(|v| v.as_i64())) {
                            points.push((x as i32, y as i32));
                        } else if let (Some(from_x), Some(from_y), Some(to_x), Some(to_y)) = (
                            params.get("from_x").and_then(|v| v.as_i64()),
                            params.get("from_y").and_then(|v| v.as_i64()),
                            params.get("to_x").and_then(|v| v.as_i64()),
                            params.get("to_y").and_then(|v| v.as_i64()),
                        ) {
                            points.push((from_x as i32, from_y as i32));
                            points.push((to_x as i32, to_y as i32));
                        }

                        if !points.is_empty() {
                            dbg_log!("[VERIFICATION] Starting visual verification for spatial tool '{}' at {:?}", tool_name, points);
                            let _ = msg_tx.send("Verifying screen...".to_string()).await;
                            
                            if let Ok(overlay_png) = crate::screenshot::capture_with_overlay(patterns, &points) {
                                let b64_img = crate::screenshot::to_base64(&overlay_png);
                                let plan_msg = val["message"].as_str().unwrap_or("");
                                let verify_prompt = format!(
                                    "OVERALL GOAL: \"{prompt}\"\n\
                                     INTENDED ACTION: \"{plan_msg}\"\n\
                                     TOOL: '{tool_name}' with parameters: '{params}'\n\n\
                                     The red marker in the screenshot shows the target position on screen.\n\
                                     Is the red marker exactly over the intended target for this action?\n\
                                     - If YES, reply: {{\"status\": \"execute\"}}\n\
                                     - If NO, find the correct coordinates on screen and reply with the corrected coordinates:\n\
                                       {{\"status\": \"adjust\", \"x\": corrected_x, \"y\": corrected_y}}\n\
                                       (or for drag: {{\"status\": \"adjust\", \"from_x\": adj_from_x, \"from_y\": adj_from_y, \"to_x\": adj_to_x, \"to_y\": adj_to_y}}).\n\n\
                                     Only raw JSON, no other text."
                                );
                                let verify_msg = ChatMessage::with_image(&verify_prompt, &b64_img);
                                let mut verify_history = messages.clone();
                                verify_history.push(verify_msg);
                                if let Ok(resp) = provider.chat(verify_history, Some(150)).await {
                                    let resp_text = resp.content.trim();
                                    dbg_log!("[VERIFICATION] Response from model: {}", resp_text);
                                    let clean_json = crate::json_utils::clean_if_json(resp_text);
                                    if let Ok(verify_val) = serde_json::from_str::<Value>(&clean_json) {
                                        let status = verify_val.get("status").and_then(|s| s.as_str()).unwrap_or("");
                                        if status == "execute" {
                                            dbg_log!("[VERIFICATION] Model confirmed coordinates.");
                                        } else if status == "adjust" {
                                            dbg_log!("[VERIFICATION] Model requested coordinate adjustment.");
                                            let adj_x = verify_val.get("x").and_then(|v| v.as_i64());
                                            let adj_y = verify_val.get("y").and_then(|v| v.as_i64());
                                            let adj_from_x = verify_val.get("from_x").and_then(|v| v.as_i64());
                                            let adj_from_y = verify_val.get("from_y").and_then(|v| v.as_i64());
                                            let adj_to_x = verify_val.get("to_x").and_then(|v| v.as_i64());
                                            let adj_to_y = verify_val.get("to_y").and_then(|v| v.as_i64());

                                            let is_suspicious = {
                                                let is_coord_suspicious = |cx: Option<i64>, cy: Option<i64>| {
                                                    match (cx, cy) {
                                                        (Some(x), Some(y)) => x < 30 && y < 30,
                                                        _ => false,
                                                    }
                                                };
                                                is_coord_suspicious(adj_x, adj_y) || is_coord_suspicious(adj_from_x, adj_from_y)
                                            };

                                            if is_suspicious {
                                                dbg_log!("[VERIFICATION] Ignored suspicious top-left coordinate adjustment: x={:?}, y={:?}, from_x={:?}, from_y={:?}", adj_x, adj_y, adj_from_x, adj_from_y);
                                            } else {
                                                if let Some(x) = adj_x { final_params["x"] = serde_json::json!(x); }
                                                if let Some(y) = adj_y { final_params["y"] = serde_json::json!(y); }
                                                if let Some(fx) = adj_from_x { final_params["from_x"] = serde_json::json!(fx); }
                                                if let Some(fy) = adj_from_y { final_params["from_y"] = serde_json::json!(fy); }
                                                if let Some(tx) = adj_to_x { final_params["to_x"] = serde_json::json!(tx); }
                                                if let Some(ty) = adj_to_y { final_params["to_y"] = serde_json::json!(ty); }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let result = execute_tool_async(tool_name.to_string(), final_params.clone()).await;
                    let _ = db::log_action(&conn, session_id, tool_name, &final_params.to_string(), None);
                    match result {
                        Ok(out) => {
                            total_steps += 1;
                            // Fingerprint this action
                            same_action_count = track_action_repetition(tool_name, &params, &mut recent_action_hashes);
                            if let Some(ref o) = out {
                                dbg_log!("[cycle {} step {}] output: {}", cycles, i+1, truncate_tool_output(o));
                                messages.push(ChatMessage::text("user", &format!("Command output:\n{}", truncate_tool_output(o))));

                                if tool_name == "run_command" || tool_name == "run_script" {
                                    let exit_code = parse_exit_code(o);
                                    let stderr_text = parse_stderr(o);
                                    let looks_bad = output_looks_like_error(o);
                                    let hard_fail = exit_code.map(|c| c != 0).unwrap_or(false);

                                    if hard_fail || (!stderr_text.is_empty() && looks_bad) {
                                        let script_content = params["script"].as_str().or(params["command"].as_str()).unwrap_or("");
                                        let known_fix = get_known_fix(script_content, &stderr_text);

                                        let diag_prompt = if let Some(fix) = known_fix {
                                            fix
                                        } else {
                                            format!(
                                                "The last command failed with exit_code={} and this error:\n\"{}\"\n\
                                                 Before writing a new plan, answer in ONE sentence: \
                                                 what exactly went wrong and why? \
                                                 Then immediately provide a corrected plan as JSON.",
                                                exit_code.unwrap_or(-1),
                                                if stderr_text.is_empty() {
                                                    truncate_tool_output(o)
                                                } else {
                                                    stderr_text.clone()
                                                }
                                            )
                                        };
                                        dbg_log!("[cycle {} step {}] HARD FAIL exit={:?} stderr='{}'",
                                            cycles, i+1, exit_code, stderr_text);
                                        messages.push(ChatMessage::text("user", &diag_prompt));
                                        failed = Some((i, format!(
                                            "exit_code={:?} stderr={}",
                                            exit_code,
                                            if stderr_text.is_empty() { "none" } else { &stderr_text }
                                        )));
                                        break;
                                    }
                                }
                            } else {
                                dbg_log!("[cycle {} step {}] output: (none)", cycles, i+1);
                            }
                            // Wait for app to appear before maximizing
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms.max(2500))).await;

                            // Auto-maximize: use run_script with P/Invoke for real maximize
                            // Win+Up only snaps, not maximizes on Windows
                            if is_launch {
                                let maximize_result = if cfg!(target_os = "windows") {
                                    execute_tool_async("run_script".to_string(), serde_json::json!({
                                        "script": r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
"@
$hwnd = [Win32]::GetForegroundWindow()
[Win32]::ShowWindow($hwnd, 3) | Out-Null
"#,
                                        "capture_output": false
                                    })).await
                                } else if cfg!(target_os = "macos") {
                                    execute_tool_async("run_script".to_string(), serde_json::json!({
                                        "script": "osascript -e 'tell application \"System Events\" to set frontmost of first process whose frontmost is true to true'",
                                        "capture_output": false
                                    })).await
                                } else {
                                    execute_tool_async("run_script".to_string(), serde_json::json!({
                                        "script": "wmctrl -r :ACTIVE: -b add,maximized_vert,maximized_horz 2>/dev/null || true",
                                        "capture_output": false
                                    })).await
                                };
                                let _ = maximize_result;
                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            }

                            if stop.load(Ordering::Relaxed) {
                                return Ok("Task stopped by user.".to_string());
                            }
                        }
                        Err(e) => {
                            failed = Some((i, e.to_string()));
                            break;
                        }
                    }
                }
            }
            }
        }

        if let Some((fi, err)) = failed {
            dbg_log!("[cycle {}] plan failed at step {}/{}: {}", cycles, fi+1, steps.len(), err);
            let _ = msg_tx.try_send("Adjusting actions...".to_string());
            let msg = format!("Step {}/{} failed: {err}. Reply with a corrected plan.", fi + 1, steps.len());
            db::insert_message(&conn, session_id, "user", &msg)?;
            messages.push(ChatMessage::text("user", &msg));
            continue 'cgae;
        }

        // Circuit breaker: same action 3+ times in a row = force strategy change
        if same_action_count >= 3 {
            same_action_count = 0;
            zero_output_cycles = 0;
            recent_action_hashes.clear();
            circuit_breaker_count += 1;
            dbg_log!("[cycle {}] CIRCUIT BREAKER: same action repeated 3x (count={})", cycles, circuit_breaker_count);
            if circuit_breaker_count >= 3 {
                return Ok(prepend_search_context("Task aborted: Agent got stuck in a repetition loop and could not proceed.", &performed_searches, &all_search_images));
            }
            messages.retain(|m| {
                !(m.role == "system" && m.content.as_str()
                    .map(|s| s.starts_with("[CIRCUIT BREAKER]")).unwrap_or(false))
            });
            messages.push(ChatMessage::text("system",
                "[CIRCUIT BREAKER] You have repeated the exact same action 3 times with no progress. \
                 This approach is NOT working. You MUST try a completely different strategy. \
                 Options: use run_script with capture_output:true to accomplish the task \
                 programmatically, try different coordinates, use keyboard shortcuts instead \
                 of clicking, or use a different tool entirely. Do NOT repeat what you just did."
            ));
        }

        let last_cycle_had_output = messages.iter().rev()
            .take(10)
            .any(|m| m.role == "user" && m.content.as_str()
                .map(|s| s.starts_with("Command output:") || s.starts_with("Output:")
                      || s.starts_with("Verified:") || s.starts_with("Search results:"))
                .unwrap_or(false));

        if !vision_capable && !last_cycle_had_output {
            zero_output_cycles += 1;
        } else {
            zero_output_cycles = 0;
        }

        if zero_output_cycles >= 3 {
            zero_output_cycles = 0;
            dbg_log!("[cycle {}] ZERO-OUTPUT CIRCUIT BREAKER fired", cycles);
            messages.retain(|m| {
                !(m.role == "system" && m.content.as_str()
                    .map(|s| s.starts_with("[ZERO-OUTPUT BREAKER]")).unwrap_or(false))
            });
            messages.push(ChatMessage::text("system",
                "[ZERO-OUTPUT BREAKER] You have taken actions 3 cycles in a row with no \
                 observable output. You may be planning instead of executing, using tools \
                 that silently fail, or clicking things with no effect. \
                 You MUST use run_script with capture_output:true RIGHT NOW so you can \
                 see what is actually happening on the system. \
                 Do not take any mouse or keyboard action until you have shell output to work from."
            ));
        }

        if let Some(done_msg) = early_done {
            dbg_log!("[cycle {}] task_complete: {}", cycles, done_msg);
            return Ok(prepend_search_context(&done_msg, &performed_searches, &all_search_images));
        }

        if !vision_capable {
            if has_task_complete {
                return Ok(prepend_search_context(&plan_summary, &performed_searches, &all_search_images));
            } else {
                continue 'cgae;
            }
        }

        // After plan completes, take one screenshot and re-enter loop so AI sees real state
        if vision_capable && failed.is_none() && early_done.is_none() {
            if let Ok(img) = take_screenshot(patterns, prompt).await {
                dbg_log!("[cycle {}] post-plan screenshot injected", cycles);

                let new_hash = screenshot_hash(&img);
                let screen_changed = last_screenshot_hash
                    .map(|old| old != new_hash)
                    .unwrap_or(true);
                last_screenshot_hash = Some(new_hash);

                let post_ctx = if has_task_complete {
                    format!(
                        "VERIFICATION CYCLE: You have executed the plan steps and called `task_complete`. \
                         A screenshot of the desktop has been taken. \
                         Verify if the task is completed (e.g. Word/Excel/Notepad is open and the target text is visible, or the action has succeeded). \
                         If it has succeeded, you MUST immediately reply with a JSON plan containing ONLY `task_complete` to exit the loop. \
                         Do NOT open any more windows or repeat any typing/clicking if the target text is already there. \
                         Original task: \"{}\"",
                        prompt
                    )
                } else if !screen_changed {
                    no_screen_change_count += 1;
                    dbg_log!("[cycle {}] post-plan screen UNCHANGED (count={})",
                        cycles, no_screen_change_count);
                    format!(
                        "WARNING: The screen is IDENTICAL to before your last action. \
                         Your action had NO visible effect. Task: \"{}\". \
                         Do NOT repeat what you just did. \
                         Switch strategy: use run_script with capture_output:true \
                         to verify system state, or try keyboard shortcuts, \
                         or a completely different approach. \
                         Describe the screen, then give a new plan.",
                        prompt
                    )
                } else {
                    no_screen_change_count = 0;
                    format!(
                        "This is the screen RIGHT NOW after your last action. Task: \"{}\". \
                         Look carefully — did your action have a visible effect? \
                         If NO visible evidence your action worked, try something different. \
                         Do NOT repeat the same action. Describe what you see, then plan next step.",
                        prompt
                    )
                };

                messages.push(ChatMessage::with_image(&post_ctx, &img));
                continue 'cgae;
            }
        }

        // EVALUATE: critic uses a stripped context — system prompt + screenshot only.
        // This prevents the actor's failed reasoning from anchoring the verdict.
        for verify_attempt in 0..MAX_VERIFY {
            if stop.load(Ordering::Relaxed) {
                return Ok("Task stopped by user.".to_string());
            }

            if !vision_capable {
                return Ok(prepend_search_context(&plan_summary, &performed_searches, &all_search_images));
            }

            let img = take_screenshot(patterns, prompt).await?;

            let new_hash = screenshot_hash(&img);
            last_screenshot_hash = Some(new_hash);

            let eval_msg = format!(
                "EVALUATION {}/{}: The original task was: \"{prompt}\". \
                 Look ONLY at this screenshot. \
                 — If the task result is VISIBLY PRESENT on screen right now: \
                   {{\"tool\":\"task_complete\",\"params\":{{\"summary\":\"describe exactly what you see\"}}}}\
                 — If you cannot see clear evidence of completion: \
                   {{\"plan\":[...]}} with one concrete corrective action using pixel coords from this image. \
                 Do NOT assume success from what previous steps said. Only what you SEE now. \
                 Only raw JSON, no markdown.",
                verify_attempt + 1, MAX_VERIFY
            );

            let critic_messages = vec![
                messages.first().cloned()
                    .unwrap_or_else(|| ChatMessage::text("system", "You are a task evaluator.")),
                ChatMessage::with_image(&eval_msg, &img),
            ];

            let eval_resp = provider.chat(critic_messages, Some(1200)).await?;
            let eval_content = strip_fences(eval_resp.content.trim());
            let eval_json = extract_json(&eval_content);

            if let Ok(ev) = serde_json::from_str::<Value>(&eval_json) {
                if ev["tool"].as_str() == Some("task_complete") {
                    let summary = ev["params"]["summary"].as_str().unwrap_or(&plan_summary).to_string();
                    return Ok(prepend_search_context(&summary, &performed_searches, &all_search_images));
                }
                if ev["plan"].is_array() {
                    messages.push(ChatMessage::text("user", "Executing corrected plan now."));
                    messages.push(ChatMessage::text("assistant", &eval_content));
                    continue 'cgae;
                }
                // Got valid JSON but neither task_complete nor plan — treat as done
                break;
            }
            // Invalid JSON from eval — retry if attempts remain, otherwise return summary
            if verify_attempt + 1 >= MAX_VERIFY {
                break;
            }
            messages.push(ChatMessage::text("user",
                "Your evaluation response was not valid JSON. Reply with ONLY a raw JSON object."
            ));
        }

        return Ok(prepend_search_context(&plan_summary, &performed_searches, &all_search_images));
    }
}

fn get_target_app_and_text(prompt: &str) -> (Option<&'static str>, Option<String>) {
    let prompt_lower = prompt.to_lowercase();
    
    let target_app = if prompt_lower.contains("word") || prompt_lower.contains("winword") {
        Some("word")
    } else if prompt_lower.contains("excel") {
        Some("excel")
    } else if prompt_lower.contains("notepad") || prompt_lower.contains("bloc de notas") || prompt_lower.contains("wordpad") {
        Some("notepad")
    } else if prompt_lower.contains("paint") || prompt_lower.contains("mspaint") || prompt_lower.contains("dibujar") {
        Some("paint")
    } else {
        None
    };

    let mut target_text = None;
    if let Some(start_idx) = prompt.find('"') {
        if let Some(end_idx) = prompt[start_idx + 1..].find('"') {
            target_text = Some(prompt[start_idx + 1..start_idx + 1 + end_idx].to_string());
        }
    }
    if target_text.is_none() {
        if let Some(start_idx) = prompt.find('\'') {
            if let Some(end_idx) = prompt[start_idx + 1..].find('\'') {
                target_text = Some(prompt[start_idx + 1..start_idx + 1 + end_idx].to_string());
            }
        }
    }
    if target_text.is_none() {
        if let Some(start_idx) = prompt.find('«') {
            if let Some(end_idx) = prompt[start_idx + 1..].find('»') {
                target_text = Some(prompt[start_idx + 1..start_idx + 1 + end_idx].to_string());
            }
        }
    }

    (target_app, target_text)
}

#[cfg(not(windows))]
fn autofocus_target_app(_prompt: &str) {}

async fn take_screenshot(patterns: &RedactPatterns, _prompt: &str) -> Result<String> {
    hide_console_window();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    let png = capture_redacted(patterns)?;
    show_console_window();
    Ok(to_base64(&png))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn hide_console_window() {}

#[cfg(not(windows))]
fn hide_console_window() {}

#[cfg(windows)]
fn show_console_window() {}

#[cfg(not(windows))]
fn show_console_window() {}

// ---------------------------------------------------------------------------
// Tool dispatcher
// ---------------------------------------------------------------------------

fn execute_tool(tool: &str, params: &Value) -> Result<Option<String>> {
    use crate::tools;
    match tool {
        "maximize" | "AppActivate" | "app_activate" | "focus_window" => {
            let script = r#"
$proc = Get-Process | Where-Object { $_.MainWindowTitle -ne '' -and $_.MainWindowTitle -notlike '*PowerShell*' -and $_.MainWindowTitle -notlike '*antflow*' } | Sort-Object StartTime -Descending | Select-Object -First 1
if ($proc) {
    Add-Type -AssemblyName Microsoft.VisualBasic
    [Microsoft.VisualBasic.Interaction]::AppActivate($proc.Id) | Out-Null
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinApi {
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
"@
    Start-Sleep -Milliseconds 300
    $hwnd = [WinApi]::GetForegroundWindow()
    [WinApi]::ShowWindow($hwnd, 3) | Out-Null
    Write-Output "Focused: $($proc.MainWindowTitle)"
} else { Write-Output "No window found" }
"#;
            tools::run_script(&serde_json::json!({"script": script, "capture_output": true}))
        }
        "mouse_move" => tools::mouse_move(params),
        "mouse_click" => tools::mouse_click(params),
        "mouse_double_click" => tools::mouse_double_click(params),
        "mouse_drag" => tools::mouse_drag(params),
        "mouse_scroll" => tools::mouse_scroll(params),
        "key_press" => tools::key_press(params),
        "type_text" => tools::type_text(params),
        "run_command" => tools::run_command(params),
        "run_script" => tools::run_script(params),
        "send_keys_to_app" => tools::send_keys_to_app(params),
        "clipboard_set" => tools::clipboard_set(params),
        "clipboard_get" => tools::clipboard_get(params),
        "open_url" => tools::open_url(params),
        "voice_input" => tools::voice_input(params),
        unknown => {
            anyhow::bail!(
                "Tool '{}' does not exist. Available tools: run_command, run_script, \
                 screenshot, mouse_click, mouse_move, mouse_drag, mouse_scroll, \
                 key_press, type_text, clipboard_set, open_url, task_complete. \
                 Use screenshot to see the screen, then mouse_click with x/y coords to act.",
                unknown
            )
        }
    }
}

fn get_screen_bounds() -> (i64, i64) {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn GetSystemMetrics(nIndex: i32) -> i32;
        }
        let w = unsafe { GetSystemMetrics(0) } as i64;
        let h = unsafe { GetSystemMetrics(1) } as i64;
        if w > 0 && h > 0 { return (w, h); }
    }
    (1920, 1080)
}

fn resolve_app_command(script: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let s = script.trim().to_lowercase();
        let store_apps = [
            ("ms-paint", "ms-paint:"),
            ("paint", "mspaint.exe"),
            ("mspaint", "mspaint.exe"),
            ("notepad", "notepad.exe"),
            ("calculator", "calc.exe"),
            ("calc", "calc.exe"),
            ("wordpad", "write.exe"),
            ("explorer", "explorer.exe"),
            ("cmd", "cmd.exe"),
        ];
        let is_start_process = s.contains("start-process") || s.starts_with("start ");
        if !is_start_process { return None; }
        for (wrong_name, correct_cmd) in &store_apps {
            if s.contains(wrong_name) && !s.contains(correct_cmd) {
                let corrected = if correct_cmd.ends_with(':') {
                    format!("Start-Process '{}'", correct_cmd)
                } else {
                    format!("Start-Process '{}'", correct_cmd)
                };
                return Some(corrected);
            }
        }
    }
    None
}

async fn execute_tool_async(tool: String, mut params: Value) -> Result<Option<String>> {
    match tool.as_str() {
        "mouse_click" | "mouse_move" | "mouse_double_click" => {
            let has_x = params.get("x").and_then(|v| v.as_i64()).is_some();
            let has_y = params.get("y").and_then(|v| v.as_i64()).is_some();
            if !has_x || !has_y {
                anyhow::bail!(
                    "mouse action '{}' rejected: missing x/y coordinates. \
                     Take a screenshot first, then use exact pixel coords from it.",
                    tool
                );
            }
            if let Some(obj) = params.as_object_mut() {
                let (max_x, max_y) = get_screen_bounds();
                if let Some(x) = obj.get("x").and_then(|v| v.as_i64()) {
                    let clamped = x.max(0).min(max_x as i64 - 1);
                    if clamped != x {
                        dbg_log!("COORD CLAMP x: {} -> {}", x, clamped);
                        obj.insert("x".to_string(), serde_json::json!(clamped));
                    }
                }
                if let Some(y) = obj.get("y").and_then(|v| v.as_i64()) {
                    let clamped = y.max(0).min(max_y as i64 - 1);
                    if clamped != y {
                        dbg_log!("COORD CLAMP y: {} -> {}", y, clamped);
                        obj.insert("y".to_string(), serde_json::json!(clamped));
                    }
                }
            }
        }
        "mouse_drag" => {
            let ok = params.get("from_x").and_then(|v| v.as_i64()).is_some()
                && params.get("from_y").and_then(|v| v.as_i64()).is_some()
                && params.get("to_x").and_then(|v| v.as_i64()).is_some()
                && params.get("to_y").and_then(|v| v.as_i64()).is_some();
            if !ok {
                anyhow::bail!(
                    "mouse_drag rejected: missing from_x/from_y/to_x/to_y. \
                     Take a screenshot first, then use exact pixel coords from it.",
                );
            }
            if let Some(obj) = params.as_object_mut() {
                let (max_x, max_y) = get_screen_bounds();
                for key in &["from_x", "to_x"] {
                    if let Some(v) = obj.get(*key).and_then(|v| v.as_i64()) {
                        let c = v.max(0).min(max_x as i64 - 1);
                        if c != v { obj.insert(key.to_string(), serde_json::json!(c)); }
                    }
                }
                for key in &["from_y", "to_y"] {
                    if let Some(v) = obj.get(*key).and_then(|v| v.as_i64()) {
                        let c = v.max(0).min(max_y as i64 - 1);
                        if c != v { obj.insert(key.to_string(), serde_json::json!(c)); }
                    }
                }
            }
        }
        _ => {}
    }

    let tool_clone = tool.clone();
    let params_clone = params.clone();
    tokio::task::spawn_blocking(move || {
        execute_tool(&tool_clone, &params_clone)
    }).await.context("Tool thread panicked")?
}

fn normalize_agent_json(mut v: Value) -> Value {
    // 1. Rename "steps" or "actions" to "plan" if "plan" is missing
    if v.get("plan").is_none() {
        if let Some(obj) = v.as_object_mut() {
            if let Some(steps_val) = obj.remove("steps") {
                obj.insert("plan".to_string(), steps_val);
            } else if let Some(actions_val) = obj.remove("actions") {
                obj.insert("plan".to_string(), actions_val);
            }
        }
    }

    // 2. Rename top-level "parameters" to "params" if "params" is missing
    if v.get("params").is_none() && v.get("parameters").is_some() {
        if let Some(obj) = v.as_object_mut() {
            if let Some(params_val) = obj.remove("parameters") {
                obj.insert("params".to_string(), params_val);
            }
        }
    }

    // 3. Top-level "action" -> "tool"
    if v.get("tool").is_none() && v.get("action").is_some() {
        if let Some(act) = v["action"].as_str() {
            if act != "task" && act != "execute" { // ignore generic intents
                v["tool"] = serde_json::json!(act);
            }
        }
    }

    // 4. If it has a plan array, normalize each step
    if let Some(plan) = v["plan"].as_array_mut() {
        // Retain only objects to filter out plain text elements (like "results" string)
        plan.retain(|step| step.is_object());

        for step in plan {
            if step.is_object() {
                // Rename "parameters" to "params" inside the step if "params" is missing
                if step.get("params").is_none() && step.get("parameters").is_some() {
                    if let Some(obj) = step.as_object_mut() {
                        if let Some(params_val) = obj.remove("parameters") {
                            obj.insert("params".to_string(), params_val);
                        }
                    }
                }

                // Map action -> tool if tool is missing
                if step.get("tool").is_none() && step.get("action").is_some() {
                    step["tool"] = step["action"].clone();
                }

                let tool = step["tool"].as_str().unwrap_or("").to_string();

                // If params is missing or is not an object, let's initialize it or extract it
                if step.get("params").is_none() || !step["params"].is_object() {
                    let mut params_obj = serde_json::Map::new();

                    // If params is a string, check if we can map it to script/command/text/url
                    if let Some(params_str) = step["params"].as_str() {
                        if tool == "run_script" {
                            params_obj.insert("script".to_string(), serde_json::json!(params_str));
                        } else if tool == "run_command" {
                            params_obj.insert("command".to_string(), serde_json::json!(params_str));
                        } else if tool == "type_text" {
                            params_obj.insert("text".to_string(), serde_json::json!(params_str));
                        } else if tool == "open_url" {
                            params_obj.insert("url".to_string(), serde_json::json!(params_str));
                        }
                    } else {
                        // Extract fields directly from step
                        if tool == "run_script" && step.get("script").is_some() {
                            params_obj.insert("script".to_string(), step["script"].clone());
                        } else if tool == "run_command" && step.get("command").is_some() {
                            params_obj.insert("command".to_string(), step["command"].clone());
                        } else if tool == "type_text" && step.get("text").is_some() {
                            params_obj.insert("text".to_string(), step["text"].clone());
                        } else if tool == "open_url" && step.get("url").is_some() {
                            params_obj.insert("url".to_string(), step["url"].clone());
                        } else if matches!(tool.as_str(), "mouse_click" | "mouse_move" | "mouse_double_click") {
                            if step.get("x").is_some() { params_obj.insert("x".to_string(), step["x"].clone()); }
                            if step.get("y").is_some() { params_obj.insert("y".to_string(), step["y"].clone()); }
                            if step.get("button").is_some() { params_obj.insert("button".to_string(), step["button"].clone()); }
                        } else if tool == "mouse_drag" {
                            if step.get("from_x").is_some() { params_obj.insert("from_x".to_string(), step["from_x"].clone()); }
                            if step.get("from_y").is_some() { params_obj.insert("from_y".to_string(), step["from_y"].clone()); }
                            if step.get("to_x").is_some() { params_obj.insert("to_x".to_string(), step["to_x"].clone()); }
                            if step.get("to_y").is_some() { params_obj.insert("to_y".to_string(), step["to_y"].clone()); }
                        }
                    }

                    if !params_obj.is_empty() {
                        step["params"] = Value::Object(params_obj);
                    }
                } else if step["params"].is_object() {
                    // Even if params is an object, if script/command/etc. is missing but present at step level, merge them
                    let script_val = step.get("script").cloned();
                    let command_val = step.get("command").cloned();
                    let text_val = step.get("text").cloned();
                    let url_val = step.get("url").cloned();

                    if let Some(params_obj) = step["params"].as_object_mut() {
                        if tool == "run_script" && !params_obj.contains_key("script") {
                            if let Some(sv) = script_val {
                                params_obj.insert("script".to_string(), sv);
                            }
                        } else if tool == "run_command" && !params_obj.contains_key("command") {
                            if let Some(cv) = command_val {
                                params_obj.insert("command".to_string(), cv);
                            }
                        } else if tool == "type_text" && !params_obj.contains_key("text") {
                            if let Some(tv) = text_val {
                                params_obj.insert("text".to_string(), tv);
                            }
                        } else if tool == "open_url" && !params_obj.contains_key("url") {
                            if let Some(uv) = url_val {
                                params_obj.insert("url".to_string(), uv);
                            }
                        }
                        
                        // Heal command/script parameter mismatches
                        if tool == "run_script" {
                            if (!params_obj.contains_key("script") || params_obj["script"].as_str().unwrap_or("").is_empty())
                                && params_obj.contains_key("command") {
                                if let Some(cmd) = params_obj.get("command").cloned() {
                                    params_obj.insert("script".to_string(), cmd);
                                }
                            }
                        } else if tool == "run_command" {
                            if (!params_obj.contains_key("command") || params_obj["command"].as_str().unwrap_or("").is_empty())
                                && params_obj.contains_key("script") {
                                if let Some(scr) = params_obj.get("script").cloned() {
                                    params_obj.insert("command".to_string(), scr);
                                }
                            }
                        }
                    }
                }
            }
        }
    } else if v.get("tool").is_some() {
        // Single tool invocation format
        let tool = v["tool"].as_str().unwrap_or("").to_string();
        if v.get("params").is_none() || !v["params"].is_object() {
            let mut params_obj = serde_json::Map::new();

            if let Some(params_str) = v["params"].as_str() {
                if tool == "run_script" {
                    params_obj.insert("script".to_string(), serde_json::json!(params_str));
                } else if tool == "run_command" {
                    params_obj.insert("command".to_string(), serde_json::json!(params_str));
                } else if tool == "type_text" {
                    params_obj.insert("text".to_string(), serde_json::json!(params_str));
                } else if tool == "open_url" {
                    params_obj.insert("url".to_string(), serde_json::json!(params_str));
                }
            } else {
                if tool == "run_script" && v.get("script").is_some() {
                    params_obj.insert("script".to_string(), v["script"].clone());
                } else if tool == "run_command" && v.get("command").is_some() {
                    params_obj.insert("command".to_string(), v["command"].clone());
                } else if tool == "type_text" && v.get("text").is_some() {
                    params_obj.insert("text".to_string(), v["text"].clone());
                } else if tool == "open_url" && v.get("url").is_some() {
                    params_obj.insert("url".to_string(), v["url"].clone());
                }
            }

            if !params_obj.is_empty() {
                v["params"] = Value::Object(params_obj);
            }
        }
    }

    v
}

fn handle_search_error(e: &anyhow::Error) -> String {
    let err_msg = e.to_string();
    let err_lower = err_msg.to_lowercase();
    let is_network_error = err_lower.contains("dns")
        || err_lower.contains("connection")
        || err_lower.contains("timeout")
        || err_lower.contains("connect")
        || err_lower.contains("resolv")
        || err_lower.contains("temporary failure")
        || err_lower.contains("offline")
        || err_lower.contains("unreachable")
        || err_lower.contains("network")
        || err_lower.contains("reqwest")
        || err_lower.contains("http");

    if is_network_error {
        "Search failed: No internet connection is currently available on this device. \
         You MUST immediately inform the user: 'I have no internet connection currently, but based on my knowledge...' \
         and answer or proceed using offline capabilities only. DO NOT attempt to search the web again."
            .to_string()
    } else {
        format!("Search error: {}", err_msg)
    }
}

pub fn format_search_widget(query: &str, results: &[crate::search::SearchResult], images: &[crate::search::SearchImage]) -> String {
    let mut html = format!(
        "<div class=\"web-search-box\">\n\
           <div class=\"web-search-header\">\n\
             <svg class=\"web-search-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10\"></circle><line x1=\"2\" y1=\"12\" x2=\"22\" y2=\"12\"></line><path d=\"M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z\"></path></svg>\n\
             <span class=\"web-search-title\">Searched the web for <span class=\"web-search-q\">\"{}\"</span></span>\n\
             <span class=\"web-search-count\">{} results</span>\n\
             <svg class=\"web-search-chevron\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><polyline points=\"6 9 12 15 18 9\"></polyline></svg>\n\
           </div>\n\
           <div class=\"web-search-results hidden\">\n",
        query, results.len()
    );

    for item in results {
        let b64_url = to_base64(item.url.as_bytes());
        let b64_title = to_base64(item.title.as_bytes());
        let b64_domain = to_base64(item.domain.as_bytes());
        let b64_snippet = to_base64(item.snippet.as_bytes());
        html.push_str(&format!(
            "    <div class=\"web-search-item\" data-url=\"{}\" data-title=\"{}\" data-domain=\"{}\" data-snippet=\"{}\">\n\
               <img class=\"web-search-favicon\" src=\"https://icons.duckduckgo.com/ip3/{}.ico\" onerror=\"this.style.display='none'\">\n\
               <a class=\"web-search-link\" href=\"{}\" target=\"_blank\">{}</a>\n\
               <span class=\"web-search-domain\">{}</span>\n\
             </div>\n",
            b64_url, b64_title, b64_domain, b64_snippet,
            item.domain, item.url, item.title, item.domain
        ));
    }
    
    html.push_str("  </div>\n");

    // Add image gallery if images are present (at least 5 if available, showing whatever is found)
    if !images.is_empty() {
        html.push_str("  <div class=\"web-search-images-gallery\">\n");
        // User requested minimum 5 images if available (we will show up to 10 if found)
        for img in images.iter().take(10) {
            html.push_str(&format!(
                "    <div class=\"web-search-image-card\">\n\
                   <img class=\"web-search-img\" src=\"{}\" alt=\"Search result\" onerror=\"this.closest('.web-search-image-card').style.display='none';\" />\n\
                   <span class=\"web-search-img-domain\">{}</span>\n\
                 </div>\n",
                img.url, img.domain
            ));
        }
        html.push_str("  </div>\n");
        html.push_str("  <span class=\"web-search-images-caption\">Results from the web</span>\n");
    }

    html.push_str("</div>\n\n");
    html
}

pub fn format_master_search_widget(
    searches: &[(String, Vec<crate::search::SearchResult>)],
    images: &[crate::search::SearchImage],
) -> String {
    let mut html = String::new();
    html.push_str(
        "<div class=\"web-search-box\">\n\
           <div class=\"web-search-header\">\n\
             <svg class=\"web-search-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10\"></circle><line x1=\"2\" y1=\"12\" x2=\"22\" y2=\"12\"></line><path d=\"M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z\"></path></svg>\n\
             <span class=\"web-search-title\">Searched the web</span>\n\
             <svg class=\"web-search-chevron\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><polyline points=\"6 9 12 15 18 9\"></polyline></svg>\n\
           </div>\n\
           <div class=\"web-search-results master-search-results hidden\">\n"
    );

    for (query, results_list) in searches {
        html.push_str(&format!(
            "    <div class=\"web-search-query-container\">\n\
                   <div class=\"web-search-query-header\">\n\
                     <svg class=\"web-search-query-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10\"></circle><line x1=\"2\" y1=\"12\" x2=\"22\" y2=\"12\"></line><path d=\"M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z\"></path></svg>\n\
                     <span class=\"web-search-query-text\">{}</span>\n\
                     <span class=\"web-search-query-count\">{} results</span>\n\
                   </div>\n\
                   <div class=\"web-search-query-results-box\">\n",
            query, results_list.len()
        ));

        for item in results_list {
            let b64_url = to_base64(item.url.as_bytes());
            let b64_title = to_base64(item.title.as_bytes());
            let b64_domain = to_base64(item.domain.as_bytes());
            let b64_snippet = to_base64(item.snippet.as_bytes());
            html.push_str(&format!(
                "      <div class=\"web-search-item\" data-url=\"{}\" data-title=\"{}\" data-domain=\"{}\" data-snippet=\"{}\">\n\
                         <img class=\"web-search-favicon\" src=\"https://icons.duckduckgo.com/ip3/{}.ico\" onerror=\"this.style.display='none'\">\n\
                         <a class=\"web-search-link\" href=\"{}\" target=\"_blank\">{}</a>\n\
                         <span class=\"web-search-domain\">{}</span>\n\
                       </div>\n",
                b64_url, b64_title, b64_domain, b64_snippet,
                item.domain, item.url, item.title, item.domain
            ));
        }

        html.push_str(
            "    </div>\n\
               </div>\n"
        );
    }

    html.push_str("  </div>\n"); // Close web-search-results
    
    html.push_str(
        "  <div class=\"web-search-done\">\n\
             <svg class=\"web-search-done-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M22 11.08V12a10 10 0 1 1-5.93-9.14\"></path><polyline points=\"22 4 12 14.01 9 11.01\"></polyline></svg>\n\
             <span class=\"web-search-done-text\">Search Completed</span>\n\
           </div>\n"
    );

    if !images.is_empty() {
        html.push_str("  <div class=\"web-search-images-gallery\">\n");
        for img in images.iter().take(10) {
            html.push_str(&format!(
                "    <div class=\"web-search-image-card\">\n\
                       <img class=\"web-search-img\" src=\"{}\" alt=\"Search result\" onerror=\"this.closest('.web-search-image-card').style.display='none';\" />\n\
                       <span class=\"web-search-img-domain\">{}</span>\n\
                     </div>\n",
                img.url, img.domain
            ));
        }
        html.push_str("  </div>\n");
        html.push_str("  <span class=\"web-search-images-caption\">Results from the web</span>\n");
    }

    html.push_str("</div>\n\n");
    html
}

fn prepend_search_context(
    response: &str,
    searches: &[(String, Vec<crate::search::SearchResult>)],
    images: &[crate::search::SearchImage],
) -> String {
    if searches.is_empty() {
        response.to_string()
    } else {
        let widget = format_master_search_widget(searches, images);
        format!("{}{}", widget, response)
    }
}