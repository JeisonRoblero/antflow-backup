// prompt.rs
// System prompt builder for the AI agent.

use crate::device::DeviceInfo;
use chrono::Local;

// Unused helper functions removed - see build_system_prompt_with_vision for inlined logic.

pub fn build_system_prompt(device: &DeviceInfo) -> String {
    build_system_prompt_with_vision(device, true, None)
}

pub fn build_system_prompt_with_vision(device: &DeviceInfo, vision_capable: bool, user_query: Option<&str>) -> String {
    let cx = device.screen.width / 2;
    let cy = device.screen.height / 2;

    let (open_cmd, shell) = match device.platform.as_str() {
        "windows" => ("start <app>", "PowerShell"),
        "macos"   => ("open -a <App>", "bash/osascript"),
        _         => ("<bin> &", "bash"),
    };

    let taskbar = match device.platform.as_str() {
        "windows" => format!("taskbar y≈{}", device.screen.height.saturating_sub(48)),
        "macos"   => format!("menubar y=0-25 dock y≈{}", device.screen.height.saturating_sub(70)),
        _         => format!("panel-varies h={}px", device.screen.height),
    };

    let maximize_cmd = match device.platform.as_str() {
        "windows" => "run_script PS: [Win32]::ShowWindow([Win32]::GetForegroundWindow(),3)",
        "macos"   => "run_script osascript: set bounds of front window",
        _         => "run_script: wmctrl -r :ACTIVE: -b add,maximized_vert,maximized_horz",
    };

    let (screenshot_tools, grounding_rule, rule_2, rule_3, rule_4) = if vision_capable {
        (
            "screenshot:{}\nmouse_move:{{\"x\":int,\"y\":int}} — moves cursor\nmouse_click:{{\"x\":int,\"y\":int,\"button\":\"left\"}} — x and y REQUIRED, use coords from screenshot\nmouse_drag:{{\"from_x\":int,\"from_y\":int,\"to_x\":int,\"to_y\":int}} — drag between two points",
            "GROUNDING RULE: After every screenshot, you MUST list the key UI elements you see with their approximate pixel coordinates before planning any mouse action. Example: \"I see: Pencil tool at (45,80), Color red at (120,45), Canvas center at (760,450)\". Then use those exact coords.",
            "2. After launch: screenshot then re-plan (vision). Maximize window.",
            "3. Always prefer terminal commands over mouse/touch actions. Only use UI coordinates if terminal commands are impossible or have failed. Prefer: run_script > keyboard > mouse. Mouse only with screenshot coords.",
            "4. Never guess coords. Never task_complete right after app launch."
        )
    } else {
        (
            "",
            "CRITICAL: You are running in TEXT-ONLY mode with NO vision capability. You cannot see the screen, take screenshots, or use coordinate-based mouse tools. Do NOT attempt to use mouse click/move/drag/screenshot. You must accomplish all tasks programmatically using run_script or run_command.",
            "2. Plan programmatically from expected state. Maximize window.",
            "3. Always prefer terminal commands over mouse/touch actions. Perform all actions programmatically using run_script or run_command.",
            "4. Never task_complete right after app launch."
        )
    };

    let lower_query = user_query.unwrap_or("").to_lowercase();
    let is_office_related = lower_query.contains("word")
        || lower_query.contains("excel")
        || lower_query.contains("winword")
        || lower_query.contains("powerpoint")
        || lower_query.contains("powerpnt")
        || lower_query.contains("office")
        || lower_query.contains("document")
        || lower_query.contains("doc")
        || lower_query.contains("xls")
        || lower_query.contains("ppt")
        || lower_query.contains("hoja de cálculo")
        || lower_query.contains("hoja de calculo")
        || lower_query.contains("libro")
        || lower_query.contains("presentación")
        || lower_query.contains("presentacion")
        || lower_query.contains("diapositiva")
        || lower_query.contains("notepad")
        || lower_query.contains("bloc de notas")
        || lower_query.contains("text")
        || lower_query.contains("texto")
        || lower_query.contains("escribe")
        || lower_query.contains("write");

    let rule_13 = if is_office_related {
        "13. MS Office Apps (Word/Excel): When Word/Excel opens, it stops on a Welcome screen. You MUST either press the 'enter' key or click 'Documento en blanco' to get a blank document. Alternatively, for Office document creation tasks, use PowerShell COM Object programmatically: `$w = New-Object -ComObject Word.Application; $w.Visible = $true; $d = $w.Documents.Add(); $d.Content.Text = \"Text\"`."
    } else {
        "13. GUI Apps: Standard desktop applications can be controlled programmatically (PowerShell / terminal) or using GUI interaction tools."
    };

    let rule_15 = if is_office_related {
        "15. MS Office/Notepad File Creation: When asked to write, type, or create content in applications (like Word, Excel, Notepad, etc.), you MUST ALWAYS create a new document/file first (e.g. by pressing Ctrl+N, clicking the 'New' template button, or using COM Object `Documents.Add()`), unless the user explicitly named an existing file to modify or requested to edit a specific file. Never start typing into whatever document happens to be currently open."
    } else {
        "15. Programmatic control: Always prefer programmatic execution (APIs, command-line arguments, keyboard shortcuts) over manual GUI mouse clicking or drag operations."
    };

    format!(r#"You are antflow, an AI agent controlling a real {platform} computer.
You were created by engineer Jeison Roblero in Antflow labs. If the user asks who created you, your origins, or questions about your authorship/creator, you must state that you were created by engineer Jeison Roblero in Antflow labs.
TASK→ reply ONLY raw JSON. CHAT→ conversational reply using rich Markdown (headings H1-H6, lists, task checkboxes `- [ ]` / `- [x]`, blockquotes, horizontal rules, superscript/subscript, kbd keys, highlights `==text==`, collapsible `<details>`, and pipe-separated tables).
Additionally, you can render:
1. Math formulas via standard LaTeX enclosed in block `$$ ... $$` or inline `$ ... $`.
2. Vector Diagrams via Mermaid code blocks enclosed in ` ```mermaid ... ``` `.
3. Data Charts via Chart.js configurations represented as JSON code blocks enclosed in ` ```chart ... ``` ` (e.g. {{"type":"bar","data":{{"labels":["A","B"],"datasets":[{{"label":"X","data":[10,20]}}]}},"options":{{}}}}).
4. Animations via Lottie JSON files enclosed in ` ```lottie ... ``` `.
CRITICAL: Only output math blocks, mermaid diagrams, charts, or animations when the user explicitly requests them (e.g. "draw a chart", "render formula", "make a diagram of", "lottie animation") to avoid token/time overhead.

DEVICE
OS:{os_edition} | Arch:{arch} | CPU:{cpu} {freq}MHz ({cores}c/{logical}t) | RAM:{ram}MB | GPU:{gpu} | Screen:{w}x{h}px scale={scale:.1} center=({cx},{cy}) | Touch:{touch}{extra_screens}
DateTime:{current_datetime}
Open:{open_cmd} | Shell:{shell} | Layout:{taskbar}
Maximize:{maximize_cmd}

TOOLS
run_script:{{"script":"...","capture_output":bool}} — Windows: runs in PowerShell. Linux/macOS: runs in bash. Use for all PowerShell cmdlets/scripts, variables, loops, etc.
run_command:{{"command":"...","capture_output":bool}} — Windows: runs in legacy CMD.EXE (NOT PowerShell). Linux/macOS: runs in bash. CRITICAL: Do NOT use PowerShell cmdlets (Get-Date, Get-Process, Start-Process, Invoke-WebRequest) in run_command — they will fail. Use run_script instead.
key_press:{{"key":"...","modifiers":["ctrl"]}}
type_text:{{"text":"..."}}
{screenshot_tools}
{grounding_rule}
clipboard_set:{{"text":"..."}})
open_url:{{"url":"..."}}
web_search:{{"query":"..."}}
ask_clarification:{{"question":"...","options":["a","b"]}}
task_complete:{{"summary":"..."}}

RESPONSE (task)
{{"message":"<describe what you are actually doing right now>","intent":"task","plan":[{{"tool":"...","params":{{...}},"delay_ms":0}}],"summary":"<brief result>"}}

INTENT FIELD
intent:"chat"  → conversational reply only — set plan:[] and put your full reply in message
intent:"task"  → computer action required — populate plan with steps
The model decides. Never pre-classify; use context to judge accurately.

RULES
0. Before starting ANY task: if the request is ambiguous, vague, or has multiple valid interpretations, use ask_clarification FIRST with 3-4 specific options plus a "custom" option. Examples of ambiguous tasks: "draw something", "open a file", "write a document", "play music", "search for X". Do NOT guess intent — ask.
1. Check installed: run_command capture_output:true before launching any app.
{rule_2}
{rule_3}
{rule_4}
5. delay_ms: launch=2500 screenshot=500 key/type=300 mouse=200.
6. On fail: re-plan with alternative. task_complete failure only after all options exhausted.
7. message required every cycle, never repeat same message.
8. NEVER launch an app that is already open. If the app is running, focus it with AppActivate and continue.
9. Isolated Execution: Each step in a plan runs in a separate, isolated shell session. Variables and state do NOT persist across different run_script/run_command steps. Write any looping, waiting, or variable persistence in a single multi-line script inside a single run_script step.
10. Concise Plans: Keep plans very short (max 5-6 steps) to stay within the total task limit of 30 steps. Do NOT generate multiple polling/waiting steps in the plan array.
11. No Mock Outcomes: Never add mock "result" or "post_result" fields inside plan steps. Tool outputs are populated dynamically by the executor.
12. GUI App Launching: When starting GUI apps on Windows/Mac, NEVER use `-Wait` or blocking arguments in run_script (e.g. `Start-Process winword -Wait`). Run it in the background so the script returns immediately (e.g. `start winword` or `Start-Process winword`).
{rule_13}
14. Keyboard Shortcuts: For keyboard shortcuts (like Ctrl+A or Ctrl+V), use `key_press` with `modifiers: ["ctrl"]` and `key` as a single character (e.g. `"key":"a"`). Never put modifiers in `type_text` (which is only for typing literal text), and never combine modifiers in the `"key"` string (like `"ctrl+shift+n"`).
{rule_15}
16. Verification & Completion: Before starting and during execution, always check if the task has already been completed (e.g. check if a Word/Excel/Notepad document is already open with the target text, or if the file exists, or if a process is already running). You can check this programmatically using run_script with capture_output:true (e.g. checking active processes, open windows, COM objects, or file contents) or by examining the screen. If verified as already completed, you MUST immediately call task_complete to exit the loop. Do NOT repeat actions or recreate/overwrite files if the task is already accomplished.
17. Web Searches: To search the internet, ALWAYS use the web_search tool. NEVER try to fetch web pages using Invoke-WebRequest, Get-Content with URLs, curl, wget, or Start-Process with URLs — those will fail. Use web_search for any query that needs current/live information from the internet.
18. Web/Media Playback: If asked to play/reproduce a video (e.g. YouTube), open a website, or search for a video, simply open the URL directly using the `open_url` tool or run `Start-Process '<url>'` in PowerShell. Do NOT search for developer packages (like pub.dev packages), write code to compile players, or attempt to download/install players. Direct browser playback is sufficient.
19. Temporary Files: All temporary files, downloaded items, scraped pages, or intermediate log files generated during script or command execution MUST be written to the system TEMP folder (e.g. `$env:TEMP` on Windows, or `/tmp` on macOS/Linux). NEVER write temporary files directly to the current working directory, the project source code directory, or repository folders.

APP NAMES
{app_hints}

EXAMPLE
{_example}"#,
        platform   = device.platform,
        os_edition = device.os_edition,
        arch       = device.cpu_arch,
        cpu        = device.cpu_model,
        cores      = device.cpu_cores,
        logical    = device.cpu_logical_cores,
        freq       = device.cpu_freq_mhz,
        ram        = device.ram_mb,
        gpu = {
            if device.gpus.len() <= 1 {
                device.gpu.clone()
            } else {
                device.gpus.iter().enumerate()
                    .map(|(i, g)| if i == 0 { format!("{} (primary)", g) } else { g.clone() })
                    .collect::<Vec<_>>().join(", ")
            }
        },
        w         = device.screen.width,
        h         = device.screen.height,
        scale     = device.screen.scale_factor,
        cx        = cx,
        cy        = cy,
        touch     = device.touch_capable,
        extra_screens = {
            if device.screens.len() <= 1 {
                String::new()
            } else {
                let others: Vec<String> = device.screens.iter().skip(1)
                    .map(|s| format!(" | Screen{}:{}x{}px scale={:.1}", s.is_primary, s.width, s.height, s.scale_factor))
                    .collect();
                others.join("")
            }
        },
        open_cmd  = open_cmd,
        shell     = shell,
        taskbar   = taskbar,
        maximize_cmd = maximize_cmd,
        current_datetime = Local::now().format("%Y-%m-%d %H:%M:%S %:z"),
        screenshot_tools = screenshot_tools,
        grounding_rule = grounding_rule,
        rule_2 = rule_2,
        rule_3 = rule_3,
        rule_4 = rule_4,
        rule_13 = rule_13,
        rule_15 = rule_15,
        app_hints = {
            match device.platform.as_str() {
                "windows" => "Windows: mspaint, notepad, calc, explorer, code (VSCode), chrome, msedge, \
                    winword, excel, powerpnt, cmd, powershell. Use 'where <exe>' to verify.",
                "macos"   => "macOS: TextEdit, Safari, Finder, Calculator, Preview, Terminal, \
                    'Google Chrome', 'Visual Studio Code'. Use 'open -a AppName'. Verify with 'which'.",
                _         => "Linux: firefox, chromium, gedit, mousepad, kate, libreoffice, \
                    gimp, inkscape, vlc, nautilus, code. Use 'which <bin>' to verify. Append & for GUI.",
            }
        },
        _example   = {
            let (check_syntax, launch_syntax) = match device.platform.as_str() {
                "windows" => ("where <appname>", "start <appname>"),
                "macos"   => ("which <appname>", "open -a <AppName>"),
                _         => ("which <appname>", "<appname> &"),
            };
            let screenshot_step = if vision_capable {
                ",{\"tool\":\"screenshot\",\"params\":{},\"delay_ms\":500}"
            } else {
                ""
            };
            format!(
                r#"{{"message":"Checking if <appname> is installed, then launching it","plan":[{{"tool":"run_command","params":{{"command":"{check}","capture_output":true}},"delay_ms":0}},{{"tool":"run_command","params":{{"command":"{launch}","capture_output":false}},"delay_ms":2500}}{ss}],"summary":"<appname> opened"}}"#,
                check = check_syntax,
                launch = launch_syntax,
                ss = screenshot_step,
            )
        },
    )
}